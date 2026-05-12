//! 统一 SQLite 数据库管理
//!
//! 所有模块共享同一个 SQLite 连接，通过表名隔离数据。

use rusqlite::{params, Connection};
use std::sync::Mutex;
use tracing::{info, warn};

/// 全局数据库连接
static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// 初始化数据库
pub fn init(data_dir: &std::path::Path) {
    let db_path = data_dir.join("memory.db");
    match Connection::open(&db_path) {
        Ok(conn) => {
            // 性能优化
            conn.execute_batch("
                PRAGMA journal_mode=WAL;
                PRAGMA synchronous=NORMAL;
                PRAGMA temp_store=MEMORY;
                PRAGMA cache_size=65536;
            ").ok();

            // 创建所有表
            create_tables(&conn);

            let mut guard = DB.lock().unwrap();
            *guard = Some(conn);
            info!("storage: SQLite initialized at {:?}", db_path);
        }
        Err(e) => {
            warn!(error = %e, "storage: SQLite init failed");
        }
    }
}

fn create_tables(conn: &Connection) {
    conn.execute_batch("
        -- 长期记忆
        CREATE TABLE IF NOT EXISTS memories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            content TEXT NOT NULL,
            importance TEXT NOT NULL DEFAULT 'normal',
            created_at INTEGER NOT NULL,
            last_accessed INTEGER NOT NULL,
            access_count INTEGER DEFAULT 0,
            embedding BLOB
        );
        CREATE INDEX IF NOT EXISTS idx_mem_user ON memories(user_id);

        -- 工作记忆
        CREATE TABLE IF NOT EXISTS working_memories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            content TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            bot_replied INTEGER DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_wm_group ON working_memories(group_id);

        -- 情绪状态
        CREATE TABLE IF NOT EXISTS emotions (
            user_id INTEGER PRIMARY KEY,
            current TEXT NOT NULL DEFAULT 'Neutral',
            intensity REAL NOT NULL DEFAULT 0.0,
            last_update INTEGER NOT NULL,
            crisis_level TEXT NOT NULL DEFAULT 'None',
            last_crisis_detected INTEGER DEFAULT 0,
            crisis_clean_count INTEGER DEFAULT 0,
            last_crisis_intervention INTEGER DEFAULT 0,
            interaction_rate REAL DEFAULT 0.0,
            history_json TEXT DEFAULT '[]'
        );

        -- 自我记忆
        CREATE TABLE IF NOT EXISTS self_memories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            category TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            source_group INTEGER DEFAULT 0
        );

        -- 人物档案
        CREATE TABLE IF NOT EXISTS person_profiles (
            user_id INTEGER PRIMARY KEY,
            person_name TEXT DEFAULT '',
            name_reason TEXT DEFAULT '',
            know_times INTEGER DEFAULT 0,
            know_since INTEGER DEFAULT 0,
            last_know INTEGER DEFAULT 0,
            memory_points_json TEXT DEFAULT '[]',
            group_nicknames_json TEXT DEFAULT '{}'
        );

        -- 回复效果
        CREATE TABLE IF NOT EXISTS reply_effects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_id INTEGER NOT NULL,
            target_user INTEGER NOT NULL,
            reply_text TEXT NOT NULL,
            sent_at INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'Pending',
            asi_score REAL,
            followups_json TEXT DEFAULT '[]'
        );

        -- 表达习惯
        CREATE TABLE IF NOT EXISTS expressions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            situation TEXT NOT NULL,
            style TEXT NOT NULL,
            count INTEGER DEFAULT 1,
            source_group INTEGER DEFAULT 0
        );

        -- 黑话
        CREATE TABLE IF NOT EXISTS jargons (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL UNIQUE,
            jargon_type TEXT NOT NULL,
            meaning TEXT DEFAULT '',
            source_group INTEGER DEFAULT 0
        );

        -- 黑名单
        CREATE TABLE IF NOT EXISTS blocklist (
            user_id INTEGER PRIMARY KEY
        );
    ").ok();
}

/// 获取数据库连接引用
pub fn with_db<F, R>(f: F) -> R
where
    F: FnOnce(&Connection) -> R,
{
    let guard = DB.lock().unwrap();
    let conn = guard.as_ref().expect("Database not initialized");
    f(conn)
}

/// 检查数据库是否可用
pub fn is_available() -> bool {
    let guard = DB.lock().unwrap();
    guard.is_some()
}

// ── 通用 JSON 迁移工具 ────────────────────────────────────────

/// 从 JSON 文件迁移数据到 SQLite
///
/// 封装为独立函数，方便后续废弃。
/// 调用一次后，数据已迁移，后续不再需要。
pub fn migrate_json_to_sqlite(data_dir: &std::path::Path) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => {
            warn!("migrate: database not available");
            return;
        }
    };

    let mut total = 0;

    // 迁移 memory.json
    total += migrate_memory(conn, data_dir);
    // 迁移 working_memory.json
    total += migrate_working_memory(conn, data_dir);
    // 迁移 emotion.json
    total += migrate_emotions(conn, data_dir);
    // 迁移 self_memory.json
    total += migrate_self_memories(conn, data_dir);
    // 迁移 person_info.json
    total += migrate_person_profiles(conn, data_dir);
    // 迁移 blocklist.json
    total += migrate_blocklist(conn, data_dir);

    info!(count = total, "migrate: JSON → SQLite completed");
}

fn migrate_memory(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("memory.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    #[derive(serde::Deserialize)]
    struct MemStore { users: std::collections::HashMap<String, UserMem> }
    #[derive(serde::Deserialize)]
    struct UserMem { entries: Vec<MemEntry> }
    #[derive(serde::Deserialize)]
    struct MemEntry {
        content: String,
        importance: String,
        created: u64,
        last_accessed: u64,
        access_count: u32,
    }

    let store: MemStore = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut count = 0;
    for (uid_str, user) in &store.users {
        let uid: u64 = match uid_str.parse() { Ok(u) => u, Err(_) => continue };
        for entry in &user.entries {
            // 检查是否已存在
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id=?1 AND content=?2",
                params![uid, entry.content],
                |r| r.get::<_, i64>(0),
            ).unwrap_or(0) > 0;
            if !exists {
                conn.execute(
                    "INSERT INTO memories (user_id,content,importance,created_at,last_accessed,access_count) VALUES (?1,?2,?3,?4,?5,?6)",
                    params![uid, entry.content, entry.importance, entry.created, entry.last_accessed, entry.access_count],
                ).ok();
                count += 1;
            }
        }
    }
    count
}

fn migrate_working_memory(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("working_memory.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    #[derive(serde::Deserialize)]
    struct WmStore { groups: std::collections::HashMap<String, GroupMem> }
    #[derive(serde::Deserialize)]
    struct GroupMem { entries: Vec<WmEntry> }
    #[derive(serde::Deserialize)]
    struct WmEntry { user_id: u64, content: String, timestamp: u64, bot_replied: bool }

    let store: WmStore = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut count = 0;
    for (gid_str, group) in &store.groups {
        let gid: u64 = match gid_str.parse() { Ok(g) => g, Err(_) => continue };
        for entry in &group.entries {
            conn.execute(
                "INSERT INTO working_memories (group_id,user_id,content,timestamp,bot_replied) VALUES (?1,?2,?3,?4,?5)",
                params![gid, entry.user_id, entry.content, entry.timestamp, entry.bot_replied],
            ).ok();
            count += 1;
        }
    }
    count
}

fn migrate_emotions(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("emotion.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    #[derive(serde::Deserialize)]
    struct EmoEntry {
        current: String,
        intensity: f64,
        last_update: u64,
        crisis_level: String,
        last_crisis_detected: u64,
        crisis_clean_count: u32,
        last_crisis_intervention: u64,
        interaction_rate: f64,
    }

    let store: std::collections::HashMap<String, EmoEntry> = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut count = 0;
    for (uid_str, entry) in &store {
        let uid: u64 = match uid_str.parse() { Ok(u) => u, Err(_) => continue };
        conn.execute(
            "INSERT OR REPLACE INTO emotions (user_id,current,intensity,last_update,crisis_level,last_crisis_detected,crisis_clean_count,last_crisis_intervention,interaction_rate) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![uid, entry.current, entry.intensity, entry.last_update, entry.crisis_level, entry.last_crisis_detected, entry.crisis_clean_count, entry.last_crisis_intervention, entry.interaction_rate],
        ).ok();
        count += 1;
    }
    count
}

fn migrate_self_memories(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("self_memory.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    #[derive(serde::Deserialize)]
    struct SmEntry { content: String, category: String, created_at: u64, source_group: u64 }

    let store: Vec<SmEntry> = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut count = 0;
    for entry in &store {
        conn.execute(
            "INSERT INTO self_memories (content,category,created_at,source_group) VALUES (?1,?2,?3,?4)",
            params![entry.content, entry.category, entry.created_at, entry.source_group],
        ).ok();
        count += 1;
    }
    count
}

fn migrate_person_profiles(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("person_info.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    #[derive(serde::Deserialize)]
    struct PpStore { profiles: std::collections::HashMap<String, PpEntry> }
    #[derive(serde::Deserialize)]
    struct PpEntry {
        person_name: String,
        name_reason: String,
        know_times: u32,
        know_since: u64,
        last_know: u64,
        memory_points: Vec<String>,
        group_nicknames: std::collections::HashMap<u64, String>,
    }

    let store: PpStore = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut count = 0;
    for (uid_str, entry) in &store.profiles {
        let uid: u64 = match uid_str.parse() { Ok(u) => u, Err(_) => continue };
        let mp_json = serde_json::to_string(&entry.memory_points).unwrap_or_default();
        let gn_json = serde_json::to_string(&entry.group_nicknames).unwrap_or_default();
        conn.execute(
            "INSERT OR REPLACE INTO person_profiles (user_id,person_name,name_reason,know_times,know_since,last_know,memory_points_json,group_nicknames_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![uid, entry.person_name, entry.name_reason, entry.know_times, entry.know_since, entry.last_know, mp_json, gn_json],
        ).ok();
        count += 1;
    }
    count
}

fn migrate_blocklist(conn: &Connection, data_dir: &std::path::Path) -> usize {
    let path = data_dir.join("blocklist.json");
    if !path.exists() { return 0; }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let ids: Vec<u64> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let mut count = 0;
    for id in &ids {
        conn.execute("INSERT OR IGNORE INTO blocklist (user_id) VALUES (?1)", params![id]).ok();
        count += 1;
    }
    count
}
