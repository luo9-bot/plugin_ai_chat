use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;
use tracing::debug;

/// AI 记忆审查提示词
const REVIEW_PROMPT: &str = r#"你是一个记忆管理助手。审查以下记忆列表和最近的对话，决定是否需要整理。

返回 JSON（不要输出其他内容）:
{
  "action": "keep" | "consolidate" | "update",
  "reason": "原因",
  "updates": [
    {"old_content": "原内容", "new_content": "新内容", "importance": "permanent|important|normal"}
  ],
  "removes": ["要删除的记忆内容"],
  "adds": [{"content": "新记忆", "importance": "permanent|important|normal"}]
}

判断标准:
- 重复或相似的记忆应该合并
- 已经不正确的记忆应该更新或删除
- 最近对话中的新信息应该添加为记忆
- 过时的记忆应该删除
- 如果记忆都正确且不需要改动，返回 action: "keep""#;

/// AI 记忆提取提示词
const EXTRACT_PROMPT: &str = r#"分析以下对话，提取值得长期记忆的信息。

返回 JSON 数组格式（不要输出其他内容）:
[{"content":"记忆内容","importance":"permanent|important|normal"}]

重要性判断标准:
- permanent (永久): 用户明确要求记住的信息，如"记住xxx"、"别忘了xxx"
- important (重要): 用户的个人信息（姓名、生日、喜好、住址、职业等）、重要经历、情感表达
- normal (普通): 一般性对话中值得记录的有趣内容、观点、经历

规则:
- 只提取有长期价值的信息，不要记录琐碎内容
- 记忆内容应该简洁明了，一句话概括
- 如果没有值得记忆的内容，返回空数组 []
- 不要重复已有的记忆

示例:
用户: "我叫小明，生日是3月15日"
→ [{"content":"用户叫小明","importance":"important"},{"content":"用户生日是3月15日","importance":"permanent"}]

用户: "今天天气真好"
→ []"#;

/// AI 摘要提示词
const SUMMARIZE_PROMPT: &str = r#"将以下对话历史总结为一段简洁的摘要，提取关键话题和重要信息。

返回纯文本摘要（一句话到三句话），不要输出其他内容。

示例:
用户: 我最近在学吉他
AI: 那很棒啊！学了多久了？
用户: 才一周，手指好痛
→ 用户最近开始学吉他，刚一周，还在适应期。"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Importance {
    Permanent,
    Important,
    Normal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub content: String,
    pub importance: Importance,
    pub created: u64,
    pub last_accessed: u64,
    pub access_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserMemory {
    pub entries: Vec<MemoryEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MemoryStore {
    pub users: HashMap<String, UserMemory>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn memory_path() -> std::path::PathBuf {
    crate::config::data_dir().join("memory.json")
}

impl MemoryStore {
    fn load() -> Self {
        let path = memory_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = memory_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }

    fn get_user_mut(&mut self, user_id: u64) -> &mut UserMemory {
        self.users
            .entry(user_id.to_string())
            .or_insert_with(UserMemory::default)
    }
}

/// 初始化时调用：返回有记忆的用户数量
pub fn load_user_count() -> usize {
    let store = MemoryStore::load();
    store.users.len()
}

pub fn add(user_id: u64, content: &str, importance: Importance) {
    let mut store = MemoryStore::load();
    let now = now_secs();
    let user = store.get_user_mut(user_id);

    if let Some(entry) = user.entries.iter_mut().find(|e| e.content == content) {
        entry.last_accessed = now;
        entry.access_count += 1;
        if importance == Importance::Permanent {
            entry.importance = Importance::Permanent;
        }
        debug!(user_id, content, "memory: updated existing entry");
        store.save();
        return;
    }

    debug!(user_id, content, ?importance, "memory: added new entry");
    user.entries.push(MemoryEntry {
        content: content.to_string(),
        importance,
        created: now,
        last_accessed: now,
        access_count: 1,
    });
    store.save();
}

pub fn forget(user_id: u64, pattern: &str) -> Vec<String> {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let mut archived = Vec::new();
    let mut remaining = Vec::new();
    for entry in user.entries.drain(..) {
        if entry.content.contains(pattern) {
            archived.push(entry);
        } else {
            remaining.push(entry);
        }
    }
    user.entries = remaining;
    let removed = archived.len();
    if removed > 0 {
        crate::archive::archive_long_term_memory(user_id, archived);
    }
    store.save();
    if removed > 0 {
        vec![format!("已遗忘 {} 条记忆", removed)]
    } else {
        vec!["没有找到匹配的记忆".to_string()]
    }
}

/// 修正用户记忆：根据 old 模糊匹配，替换为 new (new 为空则删除)
/// 返回修正的条数
pub fn correct(user_id: u64, old: &str, new: &str) -> usize {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let now = now_secs();
    let mut count = 0;

    if new.is_empty() {
        // 删除匹配的记忆
        let mut archived = Vec::new();
        let mut remaining = Vec::new();
        for entry in user.entries.drain(..) {
            if entry.content.contains(old) {
                archived.push(entry);
                count += 1;
            } else {
                remaining.push(entry);
            }
        }
        user.entries = remaining;
        if !archived.is_empty() {
            crate::archive::archive_long_term_memory(user_id, archived);
        }
    } else {
        // 更新匹配的记忆
        for entry in &mut user.entries {
            if entry.content.contains(old) {
                entry.content = new.to_string();
                entry.last_accessed = now;
                count += 1;
            }
        }
    }

    if count > 0 {
        store.save();
        debug!(user_id, old, new, count, "memory: corrected entries");
    }
    count
}

pub fn forget_all(user_id: u64) {
    let mut store = MemoryStore::load();
    if let Some(user) = store.users.remove(&user_id.to_string()) {
        if !user.entries.is_empty() {
            crate::archive::archive_long_term_memory(user_id, user.entries);
        }
    }
    store.save();
}

pub fn get_context(user_id: u64) -> String {
    let store = MemoryStore::load();
    let user = match store.users.get(&user_id.to_string()) {
        Some(u) => u,
        None => return String::new(),
    };
    if user.entries.is_empty() {
        return String::new();
    }

    let cfg = &crate::config::get().memory;
    let now = now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut lines = Vec::new();

    for entry in &user.entries {
        if entry.importance == Importance::Normal
            && now.saturating_sub(entry.last_accessed) > normal_expire
        {
            continue;
        }
        let tag = match entry.importance {
            Importance::Permanent => "[永久]",
            Importance::Important => "[重要]",
            Importance::Normal => {
                if now.saturating_sub(entry.last_accessed) > important_fade {
                    "[淡忘]"
                } else {
                    ""
                }
            }
        };
        lines.push(format!("- {}{}", tag, entry.content));
    }

    if lines.is_empty() {
        return String::new();
    }
    format!("# 关于用户的记忆\n{}", lines.join("\n"))
}

/// 获取群组级别的记忆上下文 (包含群内所有参与者的记忆)
/// 解决问题: 用户A提及用户B时，AI需要知道B是谁
pub fn get_group_context(group_id: u64, exclude_user: u64) -> String {
    // 从工作记忆获取群内参与者列表
    let participants = crate::working_memory::get_participants(group_id);
    if participants.is_empty() {
        return String::new();
    }

    let store = MemoryStore::load();
    let cfg = &crate::config::get().memory;
    let now = now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut user_blocks = Vec::new();

    for user_id in &participants {
        if *user_id == exclude_user {
            continue; // 排除当前用户（已有独立记忆上下文）
        }
        if let Some(user_mem) = store.users.get(&user_id.to_string()) {
            let mut lines = Vec::new();
            for entry in &user_mem.entries {
                if entry.importance == Importance::Normal
                    && now.saturating_sub(entry.last_accessed) > normal_expire
                {
                    continue;
                }
                let tag = match entry.importance {
                    Importance::Permanent => "[永久]",
                    Importance::Important => "[重要]",
                    Importance::Normal => {
                        if now.saturating_sub(entry.last_accessed) > important_fade {
                            "[淡忘]"
                        } else {
                            ""
                        }
                    }
                };
                lines.push(format!("  - {}{}", tag, entry.content));
            }
            if !lines.is_empty() {
                user_blocks.push(format!("用户{}:\n{}", user_id, lines.join("\n")));
            }
        }
    }

    if user_blocks.is_empty() {
        return String::new();
    }
    format!("# 群内其他成员的记忆\n{}", user_blocks.join("\n"))
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

    let result = crate::ai::analyze(EXTRACT_PROMPT, &content);
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

pub fn check_forget_command(user_id: u64, message: &str) -> Option<String> {
    let forget_patterns = ["忘掉", "忘记", "不要记", "别记"];
    for pattern in &forget_patterns {
        if message.contains(pattern) {
            let content = message
                .replace(pattern, "")
                .replace("我刚才说的", "")
                .replace("刚才说的", "")
                .replace("刚才说", "")
                .trim()
                .to_string();
            if content.is_empty() {
                let mut store = MemoryStore::load();
                let user = store.get_user_mut(user_id);
                let mut archived = Vec::new();
                let mut remaining = Vec::new();
                for entry in user.entries.drain(..) {
                    if entry.importance == Importance::Permanent {
                        remaining.push(entry);
                    } else {
                        archived.push(entry);
                    }
                }
                user.entries = remaining;
                if !archived.is_empty() {
                    crate::archive::archive_long_term_memory(user_id, archived);
                }
                store.save();
                return Some("已清除近期记忆".to_string());
            } else {
                let result = forget(user_id, &content);
                return Some(result.join("\n"));
            }
        }
    }
    None
}

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
    let result = crate::ai::analyze(SUMMARIZE_PROMPT, &conversation_text);
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
        let recent_history = crate::with_state(|s| {
            let ctx = s.get_or_create_context(0, user_id);
            ctx.history.iter()
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

        let result = crate::ai::analyze(REVIEW_PROMPT, &context);
        match result {
            Ok(raw) => {
                let json_str = if let Some(start) = raw.find('{') {
                    if let Some(end) = raw[start..].find('}') {
                        &raw[start..start + end + 1]
                    } else { continue; }
                } else { continue; };

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
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
                                let now = now_secs();
                                user.entries.push(MemoryEntry {
                                    content: content.to_string(),
                                    importance,
                                    created: now,
                                    last_accessed: now,
                                    access_count: 1,
                                });
                            }
                        }
                    }

                    store.save();
                    debug!(user_id = user_id_str, action, "memory: review completed");
                }
            }
            Err(e) => {
                debug!(user_id = user_id_str, error = %e, "memory: review AI error");
            }
        }
    }
}
