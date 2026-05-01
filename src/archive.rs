use serde::{Deserialize, Serialize};
use std::fs;
use std::time::SystemTime;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn archive_path() -> std::path::PathBuf {
    crate::config::data_dir().join("archive.json")
}

// ── 数据结构 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedWorkingMemory {
    pub group_id: u64,
    pub user_id: u64,
    pub content: String,
    pub timestamp: u64,
    pub bot_replied: bool,
    pub archived_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedLongTermMemory {
    pub user_id: u64,
    pub content: String,
    pub importance: String,
    pub created: u64,
    pub archived_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ArchiveStore {
    pub working_memory: Vec<ArchivedWorkingMemory>,
    pub long_term: Vec<ArchivedLongTermMemory>,
}

impl ArchiveStore {
    fn load() -> Self {
        let path = archive_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = archive_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}

// ── 归档操作 ─────────────────────────────────────────────────

/// 归档过期的群聊工作记忆
pub fn archive_working_memory(entries: Vec<(u64, crate::working_memory::Entry)>) {
    if entries.is_empty() {
        return;
    }
    let mut store = ArchiveStore::load();
    let now = now_secs();
    for (group_id, entry) in entries {
        store.working_memory.push(ArchivedWorkingMemory {
            group_id,
            user_id: entry.user_id,
            content: entry.content,
            timestamp: entry.timestamp,
            bot_replied: entry.bot_replied,
            archived_at: now,
        });
    }
    store.save();
}

/// 归档长期记忆 (用户遗忘或过期时)
pub fn archive_long_term_memory(user_id: u64, entries: Vec<crate::memory::MemoryEntry>) {
    if entries.is_empty() {
        return;
    }
    let mut store = ArchiveStore::load();
    let now = now_secs();
    for entry in entries {
        let importance = match entry.importance {
            crate::memory::Importance::Permanent => "permanent",
            crate::memory::Importance::Important => "important",
            crate::memory::Importance::Normal => "normal",
        };
        store.long_term.push(ArchivedLongTermMemory {
            user_id,
            content: entry.content,
            importance: importance.to_string(),
            created: entry.created,
            archived_at: now,
        });
    }
    store.save();
}

/// 返回归档统计 (用于启动日志)
pub fn stats() -> (usize, usize) {
    let store = ArchiveStore::load();
    (store.working_memory.len(), store.long_term.len())
}
