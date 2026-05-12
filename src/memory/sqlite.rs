//! SQLite 记忆存储
//!
//! 替代 JSON 文件存储，提供更高效的查询和检索能力。

use rusqlite::{params, Connection};
use std::sync::Mutex;
use tracing::{info, warn};

use super::store::{Importance, MemoryEntry};

/// SQLite 连接池
static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// 初始化 SQLite 数据库
pub fn init(data_dir: &std::path::Path) {
    let db_path = data_dir.join("memory.db");
    match Connection::open(&db_path) {
        Ok(conn) => {
            // 创建表
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_id INTEGER NOT NULL,
                    content TEXT NOT NULL,
                    importance TEXT NOT NULL DEFAULT 'normal',
                    created_at INTEGER NOT NULL,
                    last_accessed INTEGER NOT NULL,
                    access_count INTEGER DEFAULT 0,
                    embedding BLOB
                );
                CREATE INDEX IF NOT EXISTS idx_memories_user ON memories(user_id);
                CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance);
                "
            ).ok();

            let mut guard = DB.lock().unwrap();
            *guard = Some(conn);
            info!("memory: SQLite initialized at {:?}", db_path);
        }
        Err(e) => {
            warn!(error = %e, "memory: SQLite init failed, falling back to JSON");
        }
    }
}

/// 检查 SQLite 是否可用
pub fn is_available() -> bool {
    let guard = DB.lock().unwrap();
    guard.is_some()
}

/// 从 SQLite 加载用户记忆
pub fn load_user_memories(user_id: u64) -> Vec<MemoryEntry> {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut stmt = match conn.prepare(
        "SELECT content, importance, created_at, last_accessed, access_count FROM memories WHERE user_id = ?1"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let entries = stmt.query_map(params![user_id], |row| {
        let importance_str: String = row.get(1)?;
        let importance = match importance_str.as_str() {
            "permanent" => Importance::Permanent,
            "important" => Importance::Important,
            _ => Importance::Normal,
        };
        Ok(MemoryEntry {
            content: row.get(0)?,
            importance,
            created: row.get(2)?,
            last_accessed: row.get(3)?,
            access_count: row.get(4)?,
        })
    });

    match entries {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// 写入记忆到 SQLite
pub fn save_memory(user_id: u64, entry: &MemoryEntry) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let importance_str = match entry.importance {
        Importance::Permanent => "permanent",
        Importance::Important => "important",
        Importance::Normal => "normal",
    };

    conn.execute(
        "INSERT INTO memories (user_id, content, importance, created_at, last_accessed, access_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user_id, entry.content, importance_str, entry.created, entry.last_accessed, entry.access_count],
    ).ok();
}

/// 保存记忆的 embedding 向量
pub fn save_embedding(user_id: u64, content: &str, embedding: &[f32]) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    // 将 f32 切片转换为字节
    let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

    conn.execute(
        "UPDATE memories SET embedding = ?1 WHERE user_id = ?2 AND content = ?3",
        params![bytes, user_id, content],
    ).ok();
}

/// 获取记忆的 embedding 向量
pub fn get_embedding(user_id: u64, content: &str) -> Option<Vec<f32>> {
    let guard = DB.lock().unwrap();
    let conn = guard.as_ref()?;

    let mut stmt = conn.prepare("SELECT embedding FROM memories WHERE user_id = ?1 AND content = ?2").ok()?;
    let bytes: Vec<u8> = stmt.query_row(params![user_id, content], |row| row.get(0)).ok()?;

    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }

    let embedding: Vec<f32> = bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Some(embedding)
}

/// 获取用户所有记忆的 embeddings
pub fn get_user_embeddings(user_id: u64) -> Vec<(String, Vec<f32>)> {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut stmt = match conn.prepare("SELECT content, embedding FROM memories WHERE user_id = ?1 AND embedding IS NOT NULL") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map(params![user_id], |row| {
        let content: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        Ok((content, bytes))
    });

    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok())
            .filter_map(|(content, bytes)| {
                if bytes.len() % 4 != 0 { return None; }
                let embedding: Vec<f32> = bytes.chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                Some((content, embedding))
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// 更新记忆的访问信息
pub fn touch_memory(user_id: u64, content: &str) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let now = crate::util::now_secs();
    conn.execute(
        "UPDATE memories SET last_accessed = ?1, access_count = access_count + 1 WHERE user_id = ?2 AND content = ?3",
        params![now, user_id, content],
    ).ok();
}

/// 删除记忆
pub fn delete_memory(user_id: u64, content: &str) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    conn.execute(
        "DELETE FROM memories WHERE user_id = ?1 AND content = ?2",
        params![user_id, content],
    ).ok();
}

/// 删除用户所有记忆
pub fn delete_all_memories(user_id: u64) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    conn.execute("DELETE FROM memories WHERE user_id = ?1", params![user_id]).ok();
}

/// 更新记忆的重要性
pub fn update_importance(user_id: u64, content: &str, importance: &Importance) {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let importance_str = match importance {
        Importance::Permanent => "permanent",
        Importance::Important => "important",
        Importance::Normal => "normal",
    };

    conn.execute(
        "UPDATE memories SET importance = ?1 WHERE user_id = ?2 AND content = ?3",
        params![importance_str, user_id, content],
    ).ok();
}

/// 获取用户记忆数量
pub fn get_user_memory_count(user_id: u64) -> usize {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return 0,
    };

    let mut stmt = match conn.prepare("SELECT COUNT(*) FROM memories WHERE user_id = ?1") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    stmt.query_row(params![user_id], |row| row.get(0)).unwrap_or(0)
}

/// 获取所有用户数量
pub fn get_total_user_count() -> usize {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return 0,
    };

    let mut stmt = match conn.prepare("SELECT COUNT(DISTINCT user_id) FROM memories") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    stmt.query_row([], |row| row.get(0)).unwrap_or(0)
}

/// 搜索记忆（BM25 关键词检索）
pub fn search_memories(user_id: u64, query: &str, limit: usize) -> Vec<MemoryEntry> {
    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return Vec::new(),
    };

    // 简单的 LIKE 搜索（后续可升级为 FTS5）
    let keywords: Vec<&str> = query.split_whitespace().collect();
    if keywords.is_empty() {
        return load_user_memories(user_id);
    }

    let mut conditions = Vec::new();
    for (i, _) in keywords.iter().enumerate() {
        conditions.push(format!("content LIKE ?{}", i + 2));
    }
    let where_clause = conditions.join(" AND ");

    let sql = format!(
        "SELECT content, importance, created_at, last_accessed, access_count FROM memories WHERE user_id = ?1 AND {} ORDER BY last_accessed DESC LIMIT {}",
        where_clause, limit
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(user_id)];
    for kw in &keywords {
        params.push(Box::new(format!("%{}%", kw)));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let entries = stmt.query_map(param_refs.as_slice(), |row| {
        let importance_str: String = row.get(1)?;
        let importance = match importance_str.as_str() {
            "permanent" => Importance::Permanent,
            "important" => Importance::Important,
            _ => Importance::Normal,
        };
        Ok(MemoryEntry {
            content: row.get(0)?,
            importance,
            created: row.get(2)?,
            last_accessed: row.get(3)?,
            access_count: row.get(4)?,
        })
    });

    match entries {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// 从 JSON 迁移数据到 SQLite
pub fn migrate_from_json(json_path: &std::path::Path) {
    if !json_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(json_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let store: super::store::MemoryStore = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return,
    };

    let guard = DB.lock().unwrap();
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return,
    };

    let mut migrated = 0;
    for (user_id_str, user_mem) in &store.users {
        let user_id: u64 = match user_id_str.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };

        for entry in &user_mem.entries {
            let importance_str = match entry.importance {
                Importance::Permanent => "permanent",
                Importance::Important => "important",
                Importance::Normal => "normal",
            };

            // 检查是否已存在
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND content = ?2",
                params![user_id, entry.content],
                |row| row.get::<_, i64>(0),
            ).unwrap_or(0) > 0;

            if !exists {
                conn.execute(
                    "INSERT INTO memories (user_id, content, importance, created_at, last_accessed, access_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![user_id, entry.content, importance_str, entry.created, entry.last_accessed, entry.access_count],
                ).ok();
                migrated += 1;
            }
        }
    }

    info!(count = migrated, "memory: migrated from JSON to SQLite");
}
