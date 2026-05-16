use tracing::debug;

use super::operations::add;
use super::store::{Importance, MemoryStore};

/// 最近关键词记忆注入记录，防止刷屏
static RECENT_KEYWORD_EXTRACTS: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> = std::sync::Mutex::new(None);

const KEYWORD_COOLDOWN_SECS: u64 = 600; // 同关键词模式 10 分钟内不重复存储

/// 检查同类型的关键词记忆是否在冷却中
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

/// 检查新记忆是否与现有记忆矛盾（如 "我叫小红" vs 已有的 "我叫小明"）
/// 如果矛盾，返回 true 且不存储新内容
fn is_contradictory(user_id: u64, content: &str) -> bool {
    // 只检测个人信息类记忆（我是/我叫/我喜欢/我不喜欢等）
    let prefixes = ["我叫", "我是", "我的名字", "我住", "我在"];
    let has_prefix = prefixes.iter().any(|p| content.contains(p));
    if !has_prefix {
        return false;
    }

    let store = MemoryStore::load();
    let user = match store.users.get(&user_id.to_string()) {
        Some(u) => u,
        None => return false,
    };

    // 提取新信息的类型标签（如 "我叫" 后面的内容）
    let new_info_type = prefixes.iter().find(|p| content.contains(*p)).copied().unwrap_or("");
    if new_info_type.is_empty() {
        return false;
    }

    // 提取新信息的具体内容
    let new_value = content.split(new_info_type).nth(1).unwrap_or("").trim();

    for entry in &user.entries {
        // 检查是否有相同类型但不同内容的已有记忆
        if entry.content.contains(new_info_type)
            && !entry.content.contains(new_value)
        {
            let existing_value = entry.content.split(new_info_type).nth(1).unwrap_or("").trim();
            // 如果已有记忆是重要/永久的，新矛盾信息大概率是捣乱
            debug!(user_id, existing = %existing_value, new = %new_value, "memory: contradictory info detected, skipping");
            return true;
        }
    }
    false
}

/// AI 驱动的记忆提取 (发送回复后调用)
///
/// 分析用户消息和 AI 回复的完整上下文，提取值得记忆的信息
pub fn ai_extract(user_id: u64, user_message: &str, ai_reply: &str, history: &[(String, String)]) {
    // 构建分析上下文: 最近几轮对话 + 当前轮
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
            // 尝试从回复中提取 JSON 数组
            let json_str = if let Some(start) = raw.find('[') {
                if let Some(end) = raw[start..].find(']') {
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
                        let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let importance_str = item.get("importance").and_then(|v| v.as_str()).unwrap_or("normal");
                        if content.is_empty() {
                            continue;
                        }
                        let importance = match importance_str {
                            "permanent" => Importance::Permanent,
                            "important" => Importance::Important,
                            _ => Importance::Normal,
                        };
                        add(user_id, content, importance);
                    }
                }
        }
        Err(e) => {
            debug!(user_id, error = %e, "memory: AI extraction failed, falling back to keyword");
            // fallback: 关键词提取
            extract_memory_from_keyword(user_id, user_message);
        }
    }
}

/// 关键词记忆提取 (fallback)
fn extract_memory_from_keyword(user_id: u64, message: &str) {
    let patterns: &[(&str, Importance)] = &[
        ("记住", Importance::Permanent),
        ("记着", Importance::Permanent),
        ("别忘了", Importance::Permanent),
        ("不要忘", Importance::Permanent),
    ];
    for (keyword, importance) in patterns {
        if let Some(pos) = message.find(keyword) {
            let after = &message[pos + keyword.len()..].trim();
            if !after.is_empty()
                && !is_keyword_on_cooldown(user_id, keyword)
                && !is_contradictory(user_id, after)
            {
                add(user_id, after, importance.clone());
                mark_keyword_extracted(user_id, keyword);
            }
        }
    }

    let auto_patterns: &[(&str, Importance)] = &[
        ("我叫", Importance::Important),
        ("我的名字", Importance::Important),
        ("我喜欢", Importance::Important),
        ("我不喜欢", Importance::Important),
        ("我讨厌", Importance::Important),
        ("我的生日", Importance::Permanent),
        ("我生日", Importance::Permanent),
        ("我住", Importance::Important),
        ("我在", Importance::Important),
        ("我是", Importance::Important),
    ];
    for (keyword, importance) in auto_patterns {
        if let Some(pos) = message.find(keyword)
            && !is_keyword_on_cooldown(user_id, keyword)
        {
            let info = &message[pos..];
            if info.len() > keyword.len() + 1 && !is_contradictory(user_id, info) {
                add(user_id, info, importance.clone());
                mark_keyword_extracted(user_id, keyword);
            }
        }
    }
}

/// 兼容旧调用 (已弃用，使用 ai_extract)
pub fn extract_memory_from_message(user_id: u64, message: &str) {
    let patterns: &[(&str, Importance)] = &[
        ("记住", Importance::Permanent),
        ("记着", Importance::Permanent),
        ("别忘了", Importance::Permanent),
        ("不要忘", Importance::Permanent),
    ];
    for (keyword, importance) in patterns {
        if let Some(pos) = message.find(keyword) {
            let after = &message[pos + keyword.len()..].trim();
            if !after.is_empty()
                && !is_keyword_on_cooldown(user_id, keyword)
                && !is_contradictory(user_id, after)
            {
                add(user_id, after, importance.clone());
                mark_keyword_extracted(user_id, keyword);
            }
        }
    }

    let auto_patterns: &[(&str, Importance)] = &[
        ("我叫", Importance::Important),
        ("我的名字", Importance::Important),
        ("我喜欢", Importance::Important),
        ("我不喜欢", Importance::Important),
        ("我讨厌", Importance::Important),
        ("我的生日", Importance::Permanent),
        ("我生日", Importance::Permanent),
        ("我住", Importance::Important),
        ("我在", Importance::Important),
        ("我是", Importance::Important),
    ];
    for (keyword, importance) in auto_patterns {
        if let Some(pos) = message.find(keyword)
            && !is_keyword_on_cooldown(user_id, keyword)
        {
            let info = &message[pos..];
            if info.len() > keyword.len() + 1 && !is_contradictory(user_id, info) {
                add(user_id, info, importance.clone());
                mark_keyword_extracted(user_id, keyword);
            }
        }
    }
}
