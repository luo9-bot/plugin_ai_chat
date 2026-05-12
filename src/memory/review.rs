use tracing::{debug, info};

use super::operations::add;
use super::store::{Importance, MemoryEntry, MemoryStore};

pub fn auto_summarize(user_id: u64, history: &[(String, String)]) {
    let threshold = crate::config::get().memory.auto_summarize_threshold;
    if history.len() < threshold {
        return;
    }

    // 构建对话文本
    let conversation: Vec<String> = history
        .iter()
        .rev()
        .take(10)
        .map(|(role, content)| format!("[{}] {}", role, content))
        .collect();
    let conversation_text = conversation.join("\n");

    // 尝试 AI 摘要
    let result = crate::ai::analyze(crate::prompt::PromptManager::get().raw("memory_summarize"), &conversation_text);
    match result {
        Ok(summary) => {
            let summary = summary.trim();
            if !summary.is_empty() && summary.len() > 10 {
                add(user_id, &format!("曾谈论: {}", summary), Importance::Normal);
            }
        }
        Err(_) => {
            // fallback: 简单拼接
            let recent: Vec<&str> = history
                .iter()
                .rev()
                .take(6)
                .filter(|(role, _)| role == "user")
                .map(|(_, content)| content.as_str())
                .collect();
            if recent.len() >= 3 {
                let summary = recent.join("; ");
                if summary.len() > 20 {
                    add(user_id, &format!("曾谈论: {}", summary), Importance::Normal);
                }
            }
        }
    }
}

/// AI 驱动的记忆审查 (定期调用，整合和修正所有用户的记忆)
pub fn ai_review_all() {
    info!("memory_review: 开始审查所有用户记忆");
    let store = MemoryStore::load();

    for (user_id_str, user_memory) in &store.users {
        if user_memory.entries.is_empty() {
            continue;
        }

        let user_id: u64 = match user_id_str.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };

        // 构建审查上下文: 现有记忆
        let memories_text: Vec<String> = user_memory.entries.iter().map(|e| {
            let imp = match e.importance {
                Importance::Permanent => "永久",
                Importance::Important => "重要",
                Importance::Normal => "普通",
            };
            format!("- [{}] {}", imp, e.content)
        }).collect();

        // 获取最近对话历史 (取私聊上下文作为代表)
        let recent_history = crate::read_shared_state(|s| {
            s.get_history_clone(0, user_id).iter()
                .rev()
                .take(8)
                .map(|(role, content)| format!("[{}]: {}", role, content))
                .collect::<Vec<_>>()
        });

        let mut context_parts = Vec::new();
        context_parts.push(format!("# 现有记忆\n{}", memories_text.join("\n")));
        if !recent_history.is_empty() {
            context_parts.push(format!("# 最近对话\n{}", recent_history.join("\n")));
        }
        let context = context_parts.join("\n\n");

        let result = crate::ai::analyze_with_tools(
            crate::prompt::PromptManager::get().raw("memory_review"),
            &context,
            &[crate::ai::memory_review_tool()],
            Some(serde_json::json!("auto"))
        );
        match result {
            Ok(parsed) => {
                let action = parsed.get("action").and_then(|v| v.as_str()).unwrap_or("keep");
                if action == "keep" {
                    continue;
                }

                let mut store = MemoryStore::load();
                let user = store.get_user_mut(user_id);

                // 删除记忆
                if let Some(removes) = parsed.get("removes").and_then(|v| v.as_array()) {
                    for remove in removes {
                        if let Some(content) = remove.as_str() {
                            user.entries.retain(|e| !e.content.contains(content));
                        }
                    }
                }

                // 更新记忆
                if let Some(updates) = parsed.get("updates").and_then(|v| v.as_array()) {
                    for update in updates {
                        let old = update.get("old_content").and_then(|v| v.as_str()).unwrap_or("");
                        let new = update.get("new_content").and_then(|v| v.as_str()).unwrap_or("");
                        let imp_str = update.get("importance").and_then(|v| v.as_str()).unwrap_or("normal");
                        if old.is_empty() || new.is_empty() { continue; }
                        let importance = match imp_str {
                            "permanent" => Importance::Permanent,
                            "important" => Importance::Important,
                            _ => Importance::Normal,
                        };
                        if let Some(entry) = user.entries.iter_mut().find(|e| e.content.contains(old)) {
                            entry.content = new.to_string();
                            entry.importance = importance;
                        }
                    }
                }

                // 添加新记忆
                if let Some(adds) = parsed.get("adds").and_then(|v| v.as_array()) {
                    for item in adds {
                        let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let imp_str = item.get("importance").and_then(|v| v.as_str()).unwrap_or("normal");
                        if content.is_empty() { continue; }
                        let importance = match imp_str {
                            "permanent" => Importance::Permanent,
                            "important" => Importance::Important,
                            _ => Importance::Normal,
                        };
                        if !user.entries.iter().any(|e| e.content == content) {
                            let now = crate::util::now_secs();
                            user.entries.push(MemoryEntry {
                                content: content.to_string(),
                                importance,
                                created: now,
                                last_accessed: now,
                                access_count: 1,
                                embedding: None,
                            });
                        }
                    }
                }

                store.save();
                debug!(user_id = user_id_str, action, "memory: review completed");
            }
            Err(e) => {
                debug!(user_id = user_id_str, error = %e, "memory: review AI error");
            }
        }
    }
}
