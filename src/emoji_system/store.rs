//! 洛玖表情包数据结构和持久化

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 表情包条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiEntry {
    /// SHA256 哈希（唯一标识）
    pub hash: String,
    /// 文件路径（相对 data 目录）
    pub path: String,
    /// 情绪标签（逗号分隔）
    pub description: String,
    /// 情绪标签列表
    pub emotions: Vec<String>,
    /// 使用次数
    pub query_count: u32,
    /// 是否已注册（可用）
    pub is_registered: bool,
    /// 是否被封禁
    pub is_banned: bool,
    /// 注册时间
    pub registered_at: u64,
    /// 最后使用时间
    pub last_used_at: u64,
}

/// 表情包存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmojiStore {
    pub emojis: Vec<EmojiEntry>,
}

static STORE: Mutex<Option<EmojiStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("emojis.json")
}

pub(crate) fn emoji_dir() -> std::path::PathBuf {
    crate::config::data_dir().join("emoji")
}

pub(crate) fn load_store() -> EmojiStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

pub(crate) fn save_store(store: &EmojiStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}
