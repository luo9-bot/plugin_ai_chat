use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::path::PathBuf;
use std::fs;

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

// ── 兼容旧数据格式 ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
struct OldMemoryEntry {
    pub content: String,
    pub importance: Importance,
    pub group_id: Option<u64>,
    pub created: u64,
    pub last_accessed: u64,
    pub access_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
struct OldMemoryStore {
    pub users: HashMap<String, OldUserMemory>,
    pub group_memories: Option<HashMap<String, Vec<OldMemoryEntry>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
struct OldUserMemory {
    pub entries: Vec<OldMemoryEntry>,
}

// ── 内存缓存 ────────────────────────────────────────────────────

static USER_CACHE: Mutex<Option<HashMap<u64, MemoryFile>>> = Mutex::new(None);
static GROUP_CACHE: Mutex<Option<HashMap<u64, MemoryFile>>> = Mutex::new(None);
static GROUP_USER_CACHE: Mutex<Option<HashMap<(u64, u64), MemoryFile>>> = Mutex::new(None);

// ── 文件路径 ────────────────────────────────────────────────────

fn memory_dir() -> PathBuf {
    crate::config::data_dir().join("memory")
}

fn user_path(user_id: u64) -> PathBuf {
    memory_dir().join("users").join(format!("{}.json", user_id))
}

fn group_dir(group_id: u64) -> PathBuf {
    memory_dir().join("groups").join(group_id.to_string())
}

fn group_memory_path(group_id: u64) -> PathBuf {
    group_dir(group_id).join("group.json")
}

fn group_user_path(group_id: u64, user_id: u64) -> PathBuf {
    group_dir(group_id).join(format!("{}.json", user_id))
}

// ── 文件 I/O ────────────────────────────────────────────────────

fn ensure_dir(path: &PathBuf) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
}

fn load_json<T: serde::de::DeserializeOwned + Default>(path: &PathBuf) -> T {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn save_json<T: serde::Serialize>(path: &PathBuf, data: &T) {
    ensure_dir(path);
    if let Ok(json) = serde_json::to_string_pretty(data) {
        fs::write(path, json).ok();
    }
}

/// 单用户记忆文件
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryFile {
    pub entries: Vec<MemoryEntry>,
}

// ── CRUD ────────────────────────────────────────────────────────

pub fn load_user_memory(user_id: u64) -> MemoryFile {
    let mut cache = USER_CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    map.entry(user_id).or_insert_with(|| load_json(&user_path(user_id))).clone()
}

pub fn save_user_memory(user_id: u64, mem: &MemoryFile) {
    let path = user_path(user_id);
    ensure_dir(&path);
    save_json(&path, mem);
    let mut cache = USER_CACHE.lock().unwrap();
    if let Some(ref mut map) = *cache {
        map.insert(user_id, mem.clone());
    }
}

pub fn load_group_memory(group_id: u64) -> MemoryFile {
    let mut cache = GROUP_CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    map.entry(group_id).or_insert_with(|| load_json(&group_memory_path(group_id))).clone()
}

pub fn save_group_memory(group_id: u64, mem: &MemoryFile) {
    let path = group_memory_path(group_id);
    ensure_dir(&path);
    save_json(&path, mem);
    let mut cache = GROUP_CACHE.lock().unwrap();
    if let Some(ref mut map) = *cache {
        map.insert(group_id, mem.clone());
    }
}

pub fn load_group_user_memory(group_id: u64, user_id: u64) -> MemoryFile {
    let key = (group_id, user_id);
    let mut cache = GROUP_USER_CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    map.entry(key).or_insert_with(|| load_json(&group_user_path(group_id, user_id))).clone()
}

pub fn save_group_user_memory(group_id: u64, user_id: u64, mem: &MemoryFile) {
    let path = group_user_path(group_id, user_id);
    ensure_dir(&path);
    save_json(&path, mem);
    let mut cache = GROUP_USER_CACHE.lock().unwrap();
    if let Some(ref mut map) = *cache {
        map.insert((group_id, user_id), mem.clone());
    }
}

/// 初始化：迁移旧数据 + 创建文件夹
pub fn init() {
    let new_dir = memory_dir();
    ensure_dir(&new_dir.join("users"));
    ensure_dir(&new_dir.join("groups"));

    // 迁移旧数据（如果存在）
    let old_path = crate::config::data_dir().join("memory.json");
    if old_path.exists() && fs::read_dir(&new_dir.join("users")).map(|mut d| d.next().is_none()).unwrap_or(true) {
        tracing::info!("memory: migrating from old memory.json to new directory structure");
        if let Ok(content) = fs::read_to_string(&old_path) {
            if let Ok(old_store) = serde_json::from_str::<OldMemoryStore>(&content) {
                migrate_old_store(&old_store);
            }
        }
        // 备份旧文件
        let backup = crate::config::data_dir().join("memory.json.bak");
        fs::rename(&old_path, &backup).ok();
        tracing::info!("memory: migration complete, old file backed up to memory.json.bak");
    }
}

fn migrate_old_store(old: &OldMemoryStore) {
    for (uid_str, user_mem) in &old.users {
        let uid: u64 = match uid_str.parse() { Ok(id) => id, Err(_) => continue };
        let mut global = MemoryFile::default();
        let mut groups: HashMap<u64, MemoryFile> = HashMap::new();

        for old_entry in &user_mem.entries {
            let entry = MemoryEntry {
                content: old_entry.content.clone(),
                importance: old_entry.importance.clone(),
                created: old_entry.created,
                last_accessed: old_entry.last_accessed,
                access_count: old_entry.access_count,
            };
            match old_entry.group_id {
                Some(0) | None => global.entries.push(entry),
                Some(gid) => groups.entry(gid).or_default().entries.push(entry),
            }
        }

        if !global.entries.is_empty() {
            save_user_memory(uid, &global);
        }
        for (gid, group_mem) in groups {
            save_group_user_memory(gid, uid, &group_mem);
        }
    }

    // 迁移群级别记忆
    if let Some(ref group_mems) = old.group_memories {
        for (gid_str, entries) in group_mems {
            let gid: u64 = match gid_str.parse() { Ok(id) => id, Err(_) => continue };
            let mut mem = MemoryFile::default();
            for old_entry in entries {
                mem.entries.push(MemoryEntry {
                    content: old_entry.content.clone(),
                    importance: old_entry.importance.clone(),
                    created: old_entry.created,
                    last_accessed: old_entry.last_accessed,
                    access_count: old_entry.access_count,
                });
            }
            if !mem.entries.is_empty() {
                save_group_memory(gid, &mem);
            }
        }
    }
}

/// 有记忆的用户数量
pub fn load_user_count() -> usize {
    all_user_ids().len()
}

/// 获取所有有记忆的用户列表
pub fn all_user_ids() -> Vec<u64> {
    let dir = memory_dir().join("users");
    let mut ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".json")
            {
                if let Ok(uid) = name.trim_end_matches(".json").parse::<u64>() {
                    ids.push(uid);
                }
            }
        }
    }
    ids
}
