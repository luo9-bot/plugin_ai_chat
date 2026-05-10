use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;


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

fn memory_path() -> std::path::PathBuf {
    crate::config::data_dir().join("memory.json")
}

impl MemoryStore {
    pub(crate) fn load() -> Self {
        let path = memory_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub(crate) fn save(&self) {
        let path = memory_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }

    pub(crate) fn get_user_mut(&mut self, user_id: u64) -> &mut UserMemory {
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
