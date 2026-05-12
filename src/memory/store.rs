use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// 记忆重要性
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Importance {
    Permanent,
    Important,
    Normal,
}

/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub content: String,
    pub importance: Importance,
    pub created: u64,
    pub last_accessed: u64,
    pub access_count: u32,
}

/// 用户记忆集合
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserMemory {
    pub entries: Vec<MemoryEntry>,
}

/// 记忆存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryStore {
    pub users: HashMap<String, UserMemory>,
}

/// 全局缓存，持锁防竞态
static STORE: Mutex<Option<MemoryStore>> = Mutex::new(None);

fn memory_path() -> std::path::PathBuf {
    crate::config::data_dir().join("memory.json")
}

impl MemoryStore {
    /// 加载记忆（使用缓存，持锁）
    pub(crate) fn load() -> Self {
        let mut guard = STORE.lock().unwrap();
        guard.get_or_insert_with(|| crate::util::load_json(&memory_path())).clone()
    }

    /// 保存记忆（更新缓存并写盘，持锁）
    pub(crate) fn save(&self) {
        let mut guard = STORE.lock().unwrap();
        *guard = Some(self.clone());
        crate::util::save_json(&memory_path(), self);
    }

    /// 获取或创建用户记忆
    pub(crate) fn get_user_mut(&mut self, user_id: u64) -> &mut UserMemory {
        self.users
            .entry(user_id.to_string())
            .or_default()
    }
}

/// 初始化时调用：返回有记忆的用户数量
pub fn load_user_count() -> usize {
    let store = MemoryStore::load();
    store.users.len()
}
