//! 人物档案数据结构和持久化

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct PersonProfile {
    pub user_id: u64, pub person_name: String, pub name_reason: String,
    pub know_times: u32, pub know_since: u64, pub last_know: u64,
    pub memory_points: Vec<String>, pub group_nicknames: HashMap<u64, String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonStore { pub profiles: HashMap<u64, PersonProfile> }

pub(crate) static STORE: Mutex<Option<PersonStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf { crate::config::data_dir().join("person_info.json") }

pub(crate) fn load_store() -> PersonStore {
    let mut g = STORE.lock().unwrap();
    if g.is_none() { *g = Some(crate::util::load_json(&store_path())); }
    g.clone().unwrap_or_default()
}

pub(crate) fn save_store(store: &PersonStore) {
    let mut g = STORE.lock().unwrap();
    *g = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}
