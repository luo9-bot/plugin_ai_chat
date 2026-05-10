use tracing::debug;

use super::operations::add;
use super::store::Importance;

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

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(arr) = parsed.as_array() {
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
            if !after.is_empty() {
                add(user_id, after, importance.clone());
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
        if let Some(pos) = message.find(keyword) {
            let info = &message[pos..];
            if info.len() > keyword.len() + 1 {
                add(user_id, info, importance.clone());
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
            if !after.is_empty() {
                add(user_id, after, importance.clone());
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
        if let Some(pos) = message.find(keyword) {
            let info = &message[pos..];
            if info.len() > keyword.len() + 1 {
                add(user_id, info, importance.clone());
            }
        }
    }
}
