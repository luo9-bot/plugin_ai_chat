use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

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

fn memory_path() -> std::path::PathBuf {
    crate::config::data_dir().join("working_memory.json")
}

impl WorkingMemoryStore {
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
}
