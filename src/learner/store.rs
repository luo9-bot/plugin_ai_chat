//! 表达学习数据结构和持久化

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionHabit { pub situation: String, pub style: String, pub count: u32, pub source_group: u64 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JargonType { Pinyin, English, Chinese }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JargonEntry { pub content: String, pub jargon_type: JargonType, pub meaning: String, pub source_group: u64 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearnerStore { pub expressions: Vec<ExpressionHabit>, pub jargon: Vec<JargonEntry>, pub last_learned: HashMap<u64, u64> }

pub(crate) static STORE: Mutex<Option<LearnerStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf { crate::config::data_dir().join("learner.json") }

pub(crate) fn load_store() -> LearnerStore {
    let mut g = STORE.lock().unwrap();
    if g.is_none() { *g = Some(crate::util::load_json(&store_path())); }
    g.clone().unwrap_or_default()
}

pub(crate) fn save_store(store: &LearnerStore) {
    let mut g = STORE.lock().unwrap();
    *g = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}

pub const LEARN_INTERVAL_SECS: u64 = 30;
pub const MIN_MESSAGES: usize = 5;
pub const SIMILARITY_THRESHOLD: f64 = 0.75;
