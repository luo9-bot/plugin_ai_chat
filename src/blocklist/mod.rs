use std::collections::HashSet;
use std::fs;

fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("blocklist.json")
}

pub fn load() -> HashSet<u64> {
    let path = store_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashSet::new(),
    }
}

pub fn save(list: &HashSet<u64>) {
    let path = store_path();
    if let Ok(json) = serde_json::to_string_pretty(list) {
        fs::write(path, json).ok();
    }
}

pub fn load_count() -> usize {
    load().len()
}
