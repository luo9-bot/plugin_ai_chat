//! 洛玖表情包数据结构和持久化

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 表情包条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerEntry {
    /// SHA256 哈希（唯一标识）
    pub hash: String,
    /// 文件路径（相对 data 目录）
    pub path: String,
    /// 情绪标签（逗号分隔）
    pub description: String,
    /// 情绪标签列表
    pub emotions: Vec<String>,
    /// VLM 生成的自然语言描述（用于 AI 上下文，持久化避免重复 VLM 调用）
    #[serde(default)]
    pub vlm_description: Option<String>,
    /// 使用次数
    pub query_count: u32,
    /// 是否已注册（可用）
    pub is_registered: bool,
    /// 是否被封禁
    pub is_banned: bool,
    /// 是否为内置表情（ne_sticker），永不被动替换
    pub is_builtin: bool,
    /// 注册时间
    pub registered_at: u64,
    /// 最后使用时间
    pub last_used_at: u64,
}

/// 表情包存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StickerStore {
    pub stickers: Vec<StickerEntry>,
}

static STORE: Mutex<Option<StickerStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("stickers.json")
}

/// 原子性地添加条目到存储（加载→修改→保存，持锁防止竞态）
pub(crate) fn add_entry_and_save(entry: StickerEntry) {
    let mut guard = STORE.lock().unwrap();
    let store = guard.get_or_insert_with(|| crate::util::load_json(&store_path()));
    store.stickers.push(entry);
    crate::util::save_json(&store_path(), store);
}

pub(crate) fn sticker_dir() -> std::path::PathBuf {
    crate::config::data_dir().join("sticker")
}

/// 内置表情包（NeSticker）目录
pub(crate) fn builtin_sticker_dir() -> std::path::PathBuf {
    crate::config::data_dir().join("ne_sticker")
}

pub(crate) fn load_store() -> StickerStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

/// 按哈希查找表情包条目
pub fn find_entry_by_hash(hash: &str) -> Option<StickerEntry> {
    let store = load_store();
    store.stickers.into_iter().find(|e| e.hash == hash)
}

/// 更新表情包的 VLM 自然语言描述
pub fn update_vlm_description(hash: &str, description: &str) {
    let mut guard = STORE.lock().unwrap();
    let store = guard.get_or_insert_with(|| crate::util::load_json(&store_path()));
    if let Some(entry) = store.stickers.iter_mut().find(|e| e.hash == hash) {
        entry.vlm_description = Some(description.to_string());
        crate::util::save_json(&store_path(), store);
    }
}

pub(crate) fn save_store(store: &StickerStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}
