//! 回复效果数据结构和持久化

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EffectStatus { Pending, Finalized }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowupMessage { pub user_id: u64, pub content: String, pub timestamp: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyEffectRecord {
    pub reply_text: String,
    pub target_user: u64,
    pub group_id: u64,
    pub sent_at: u64,
    pub followups: Vec<FollowupMessage>,
    pub asi_score: Option<f64>,
    pub status: EffectStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EffectStore { pub records: Vec<ReplyEffectRecord> }

pub(crate) static STORE: Mutex<Option<EffectStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("reply_effects.json")
}

pub(crate) fn load_store() -> EffectStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() { *guard = Some(crate::util::load_json(&store_path())); }
    guard.clone().unwrap_or_default()
}

pub(crate) fn save_store(store: &EffectStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}

pub const OBSERVATION_WINDOW: u64 = 600;
pub const MAX_FOLLOWUPS: usize = 10;
pub const MAX_ACTIVE_RECORDS: usize = 20;
