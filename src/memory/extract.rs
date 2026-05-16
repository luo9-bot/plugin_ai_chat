use tracing::debug;

use super::operations::add;
use super::store::{Importance, MemoryStore};

/// 最近关键词记忆注入记录，防止刷屏
static RECENT_KEYWORD_EXTRACTS: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> = std::sync::Mutex::new(None);
const KEYWORD_COOLDOWN_SECS: u64 = 600;

fn is_keyword_on_cooldown(user_id: u64, keyword: &str) -> bool {
    let key = format!("{}:{}", user_id, keyword);
    let guard = RECENT_KEYWORD_EXTRACTS.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(ts) = map.get(&key)
    {
        return crate::util::now_secs().saturating_sub(*ts) < KEYWORD_COOLDOWN_SECS;
    }
    false
}

fn mark_keyword_extracted(user_id: u64, keyword: &str) {
    let key = format!("{}:{}", user_id, keyword);
    let mut guard = RECENT_KEYWORD_EXTRACTS.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    map.insert(key, crate::util::now_secs());
}

/// 临时内容过滤：短消息、问候语、客套话跳过 LLM 提取
fn looks_ephemeral(message: &str) -> bool {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return true;
    }
    let ephemeral_markers = ["哈哈", "好的", "收到", "嗯嗯", "晚安", "早安", "拜拜", "谢谢", "在吗", "？", "ok", "OK", "好", "嗯", "?", "？"];
    if trimmed.chars().count() <= 8 && ephemeral_markers.iter().any(|m| trimmed.contains(m)) {
        return true;
    }
    false
}

/// 检查新记忆是否与现有记忆矛盾（如 "我叫小红" vs 已有的 "我叫小明"）
fn is_contradictory(user_id: u64, content: &str) -> bool {
    let prefixes = ["我叫", "我是", "我的名字", "我住", "我在", "我喜欢", "我不喜欢", "我讨厌"];
    let has_prefix = prefixes.iter().any(|p| content.contains(p));
    if !has_prefix {
        return false;
    }

    let store = MemoryStore::load();
    let user = match store.users.get(&user_id.to_string()) {
        Some(u) => u,
        None => return false,
    };

    let new_info_type = prefixes.iter().find(|p| content.contains(*p)).copied().unwrap_or("");
    if new_info_type.is_empty() {
        return false;
    }
    let new_value = content.split(new_info_type).nth(1).unwrap_or("").trim();

    for entry in &user.entries {
        if entry.content.contains(new_info_type) && !entry.content.contains(new_value) {
            let existing_value = entry.content.split(new_info_type).nth(1).unwrap_or("").trim();
            debug!(user_id, existing = %existing_value, new = %new_value, "memory: contradictory info detected, skipping");
            return true;
        }
    }
    false
}

/// AI 驱动的记忆提取（LLM 主驱动，关键词仅作为最后防线）
///
/// 流程:
/// 1. 临时内容过滤 → 跳过无意义的问候/客套
/// 2. LLM 提取 → 分析完整对话上下文，提取事实（含永久性判断）
/// 3. LLM 失败 → 关键词兜底（仅"记住"指令）
///
/// Permanent 由 LLM 判断，不硬编码任何关键词为永久。
pub fn ai_extract(user_id: u64, user_message: &str, ai_reply: &str, history: &[(String, String)]) {
    // 临时内容过滤：短消息/问候/客套不浪费 LLM 调用
    if looks_ephemeral(user_message) {
        debug!(user_id, "memory: ephemeral message, skipping extraction");
        return;
    }

    let mut context_parts = Vec::new();
    let recent: Vec<_> = history.iter().rev().take(6).collect();
    for (role, content) in recent.iter().rev() {
        context_parts.push(format!("[{}] {}", role, content));
    }
    context_parts.push(format!("[user] {}", user_message));
    context_parts.push(format!("[assistant] {}", ai_reply));
    let content = context_parts.join("\n");

    let result = crate::ai::analyze(crate::prompt::PromptManager::get().raw("memory_extract"), &content);
    match result {
        Ok(raw) => {
            let json_str = if let Some(start) = raw.find('[') {
                if let Some(end) = raw[start..].rfind(']') {
                    &raw[start..start + end + 1]
                } else {
                    "[]"
                }
            } else {
                "[]"
            };

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str)
                && let Some(arr) = parsed.as_array() {
                    for item in arr {
                        let c = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let importance_str = item.get("importance").and_then(|v| v.as_str()).unwrap_or("normal");
                        if c.is_empty() {
                            continue;
                        }
                        // Permanent 完全由 LLM 判断，不硬编码关键词
                        let importance = match importance_str {
                            "permanent" => Importance::Permanent,
                            "important" => Importance::Important,
                            _ => Importance::Normal,
                        };
                        add(user_id, c, importance);
                    }
                } else {
                    // LLM 返回了空数组或无效 JSON → 不降级到关键词，直接跳过
                    debug!(user_id, "memory: LLM returned no valid facts, skipping extraction");
                }
        }
        Err(e) => {
            debug!(user_id, error = %e, "memory: AI extraction failed, falling back to keyword");
            // LLM 彻底失败时，关键词做最后防线（仅"记住"指令）
            fallback_keyword(user_id, user_message);
        }
    }
}

/// 关键词最后防线：仅当 LLM 完全失败时才触发
///
/// 关键词提取只处理用户明确的"记住"指令，
/// 不自动从"我叫""我是"等猜测记忆（这些应该由 LLM 处理）。
fn fallback_keyword(user_id: u64, message: &str) {
    // 唯一的关键词：用户明确说"记住"
    if let Some(pos) = message.find("记住") {
        let after = &message[pos + 2..].trim();
        if !after.is_empty()
            && !is_keyword_on_cooldown(user_id, "记住")
            && !is_contradictory(user_id, after)
        {
            // 关键词兜底只存 Normal（LLM 缺席时不假设重要性）
            add(user_id, after, Importance::Normal);
            mark_keyword_extracted(user_id, "记住");
        }
    }
}

/// 兼容旧调用 (仅供内部不使用 ai_extract 的场景)
pub fn extract_memory_from_message(user_id: u64, message: &str) {
    if looks_ephemeral(message) {
        return;
    }
    // 同样只处理"记住"
    if let Some(pos) = message.find("记住") {
        let after = &message[pos + 2..].trim();
        if !after.is_empty()
            && !is_keyword_on_cooldown(user_id, "记住")
            && !is_contradictory(user_id, after)
        {
            add(user_id, after, Importance::Normal);
            mark_keyword_extracted(user_id, "记住");
        }
    }
}
