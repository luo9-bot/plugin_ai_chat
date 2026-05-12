//! 洛玖表情包数据结构和持久化

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// URL → 描述文本缓存（避免重复 VLM 调用）
static URL_DESCRIPTION_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// 缓存图片 URL 的描述文本
pub fn cache_url_description(url: &str, description: &str) {
    let mut guard = URL_DESCRIPTION_CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    cache.insert(url.to_string(), description.to_string());
}

/// 获取缓存的图片描述
pub fn get_cached_description(url: &str) -> Option<String> {
    let guard = URL_DESCRIPTION_CACHE.lock().unwrap();
    let cache: &HashMap<String, String> = guard.as_ref()?;
    cache.get(url).cloned()
}

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

pub(crate) fn save_store(store: &StickerStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}
