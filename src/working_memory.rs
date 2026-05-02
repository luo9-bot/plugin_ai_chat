use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub user_id: u64,
    pub content: String,
    pub timestamp: u64,
    pub bot_replied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupMemory {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WorkingMemoryStore {
    pub groups: HashMap<String, GroupMemory>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn memory_path() -> std::path::PathBuf {
    crate::config::data_dir().join("working_memory.json")
}

impl WorkingMemoryStore {
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
}

/// 记录一条群聊消息 (无论是否回复)
pub fn record(group_id: u64, user_id: u64, content: &str, bot_replied: bool) {
    if group_id == 0 { return; }
    let mut store = WorkingMemoryStore::load();
    let group = store.groups.entry(group_id.to_string()).or_default();
    group.entries.push(Entry {
        user_id,
        content: content.to_string(),
        timestamp: now_secs(),
        bot_replied,
    });
    // 每群最多保留 200 条
    if group.entries.len() > 200 {
        let drain_count = group.entries.len() - 200;
        group.entries.drain(0..drain_count);
    }
    store.save();
}

/// 标记某用户最近的消息为已回复
pub fn mark_replied(group_id: u64, user_id: u64) {
    if group_id == 0 { return; }
    let mut store = WorkingMemoryStore::load();
    if let Some(group) = store.groups.get_mut(&group_id.to_string()) {
        // 标记该用户最近一条未回复的消息
        if let Some(entry) = group.entries.iter_mut().rev().find(|e| e.user_id == user_id && !e.bot_replied) {
            entry.bot_replied = true;
        }
    }
    store.save();
}

/// 获取某群最近的消息
pub fn get_recent(group_id: u64, max_age_secs: u64, max_count: usize) -> Vec<Entry> {
    let store = WorkingMemoryStore::load();
    let now = now_secs();
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            group.entries.iter()
                .rev()
                .filter(|e| now.saturating_sub(e.timestamp) < max_age_secs)
                .take(max_count)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        })
        .unwrap_or_default()
}

/// 获取指定时间戳之后的群聊消息
pub fn get_since(group_id: u64, since_timestamp: u64, max_count: usize) -> Vec<Entry> {
    let store = WorkingMemoryStore::load();
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            group.entries.iter()
                .filter(|e| e.timestamp > since_timestamp)
                .take(max_count)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// 获取格式化的群聊工作记忆上下文 (用于 AI 决策)
pub fn get_context(group_id: u64, max_age_secs: u64) -> String {
    let entries = get_recent(group_id, max_age_secs, 30);
    if entries.is_empty() {
        return String::new();
    }
    // 合并同一用户的连续消息
    let mut lines: Vec<String> = Vec::new();
    let mut iter = entries.iter().peekable();
    while let Some(first) = iter.next() {
        let tag = if first.bot_replied { "[已回复]" } else { "" };
        let mut block = first.content.clone();
        while let Some(next) = iter.peek() {
            if next.user_id == first.user_id && next.bot_replied == first.bot_replied {
                block.push('\n');
                block.push_str(&next.content);
                iter.next();
            } else {
                break;
            }
        }
        lines.push(format!("[user_id:{}]{} {}", first.user_id, tag, block));
    }
    format!("# 群聊工作记忆 (最近消息)\n{}", lines.join("\n"))
}

/// 清理过期的工作记忆 (归档后删除)
pub fn cleanup(max_age_secs: u64) {
    let mut store = WorkingMemoryStore::load();
    let now = now_secs();
    let mut to_archive = Vec::new();

    for (group_id_str, group) in store.groups.iter_mut() {
        let group_id: u64 = group_id_str.parse().unwrap_or(0);
        let mut remaining = Vec::new();
        for entry in group.entries.drain(..) {
            if now.saturating_sub(entry.timestamp) >= max_age_secs {
                to_archive.push((group_id, entry));
            } else {
                remaining.push(entry);
            }
        }
        group.entries = remaining;
    }

    // 移除空群
    store.groups.retain(|_, g| !g.entries.is_empty());

    if !to_archive.is_empty() {
        crate::archive::archive_working_memory(to_archive);
        store.save();
    }
}

/// 返回有工作记忆的群数量 (用于启动日志)
pub fn group_count() -> usize {
    let store = WorkingMemoryStore::load();
    store.groups.len()
}

/// 获取某群最近参与过的用户列表 (用于群组记忆互通)
pub fn get_participants(group_id: u64) -> Vec<u64> {
    let store = WorkingMemoryStore::load();
    let now = now_secs();
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            let mut users: Vec<u64> = group.entries.iter()
                .filter(|e| now.saturating_sub(e.timestamp) < 7200) // 最近2小时
                .map(|e| e.user_id)
                .collect();
            users.sort_unstable();
            users.dedup();
            users
        })
        .unwrap_or_default()
}
