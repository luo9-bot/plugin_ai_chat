//! SQLite 记忆存储
//!
//! 基于 storage 共享连接，向 memories 表读写记忆数据。
//! 不自己管理连接，避免双连接冲突。

use rusqlite::params;
use tracing::{debug, info, warn};

use super::store::{Importance, MemoryEntry};

/// 初始化：由 storage::init 统一创建表，本模块只确认表存在
pub fn init(_data_dir: &std::path::Path) {
    // 表由 storage::sqlite 统一创建，此处无需重复操作
    debug!("memory: sqlite backend ready (shared connection)");
}

/// 检查 SQLite 是否可用
pub fn is_available() -> bool {
    crate::storage::sqlite::is_available()
}

/// 从 SQLite 加载用户记忆
pub fn load_user_memories(user_id: u64) -> Vec<MemoryEntry> {
    crate::storage::sqlite::with_db(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT content, importance, created_at, last_accessed, access_count FROM memories WHERE user_id = ?1"
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, user_id, "load_user_memories: prepare failed");
                return Vec::new();
            }
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
            Err(e) => {
                warn!(error = %e, user_id, "load_user_memories: query failed");
                Vec::new()
            }
        }
    })
}

/// 写入记忆到 SQLite
pub fn save_memory(user_id: u64, entry: &MemoryEntry) {
    crate::storage::sqlite::with_db(|conn| {
        let importance_str = match entry.importance {
            Importance::Permanent => "permanent",
            Importance::Important => "important",
            Importance::Normal => "normal",
        };

        let content_preview: String = entry.content.chars().take(40).collect();
        match conn.execute(
            "INSERT INTO memories (user_id, content, importance, created_at, last_accessed, access_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![user_id, entry.content, importance_str, entry.created, entry.last_accessed, entry.access_count],
        ) {
            Ok(rows) => debug!(user_id, content = %content_preview, rows, "save_memory: inserted"),
            Err(e) => warn!(user_id, error = %e, content = %content_preview, "save_memory: insert failed"),
        }
    })
}

/// 保存记忆的 embedding 向量
pub fn save_embedding(user_id: u64, content: &str, embedding: &[f32]) {
    crate::storage::sqlite::with_db(|conn| {
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        let content_preview: String = content.chars().take(40).collect();
        match conn.execute(
            "UPDATE memories SET embedding = ?1 WHERE user_id = ?2 AND content = ?3",
            params![bytes, user_id, content],
        ) {
            Ok(rows) => debug!(user_id, content = %content_preview, dim = embedding.len(), rows, "save_embedding: updated"),
            Err(e) => warn!(user_id, error = %e, content = %content_preview, "save_embedding: update failed"),
        }
    })
}

/// 获取记忆的 embedding 向量
pub fn get_embedding(user_id: u64, content: &str) -> Option<Vec<f32>> {
    crate::storage::sqlite::with_db(|conn| {
        let mut stmt = conn.prepare("SELECT embedding FROM memories WHERE user_id = ?1 AND content = ?2").ok()?;
        let bytes: Vec<u8> = stmt.query_row(params![user_id, content], |row| row.get(0)).ok()?;

        if bytes.is_empty() || bytes.len() % 4 != 0 {
            return None;
        }

        Some(
            bytes.chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect(),
        )
    })
}

/// 获取用户所有记忆的 embeddings
pub fn get_user_embeddings(user_id: u64) -> Vec<(String, Vec<f32>)> {
    crate::storage::sqlite::with_db(|conn| {
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
    })
}

/// 更新记忆的访问信息
pub fn touch_memory(user_id: u64, content: &str) {
    crate::storage::sqlite::with_db(|conn| {
        let now = crate::util::now_secs();
        let content_preview: String = content.chars().take(40).collect();
        match conn.execute(
            "UPDATE memories SET last_accessed = ?1, access_count = access_count + 1 WHERE user_id = ?2 AND content = ?3",
            params![now, user_id, content],
        ) {
            Ok(rows) => debug!(user_id, content = %content_preview, rows, "touch_memory: updated"),
            Err(e) => warn!(user_id, error = %e, content = %content_preview, "touch_memory: update failed"),
        }
    })
}

/// 删除记忆
pub fn delete_memory(user_id: u64, content: &str) {
    crate::storage::sqlite::with_db(|conn| {
        match conn.execute(
            "DELETE FROM memories WHERE user_id = ?1 AND content = ?2",
            params![user_id, content],
        ) {
            Ok(rows) => debug!(user_id, rows, "delete_memory: deleted"),
            Err(e) => warn!(user_id, error = %e, "delete_memory: delete failed"),
        }
    })
}

/// 删除用户所有记忆
pub fn delete_all_memories(user_id: u64) {
    crate::storage::sqlite::with_db(|conn| {
        match conn.execute("DELETE FROM memories WHERE user_id = ?1", params![user_id]) {
            Ok(rows) => debug!(user_id, rows, "delete_all_memories: deleted"),
            Err(e) => warn!(user_id, error = %e, "delete_all_memories: delete failed"),
        }
    })
}

/// 更新记忆的重要性
pub fn update_importance(user_id: u64, content: &str, importance: &Importance) {
    crate::storage::sqlite::with_db(|conn| {
        let importance_str = match importance {
            Importance::Permanent => "permanent",
            Importance::Important => "important",
            Importance::Normal => "normal",
        };

        match conn.execute(
            "UPDATE memories SET importance = ?1 WHERE user_id = ?2 AND content = ?3",
            params![importance_str, user_id, content],
        ) {
            Ok(rows) => debug!(user_id, rows, "update_importance: updated"),
            Err(e) => warn!(user_id, error = %e, "update_importance: update failed"),
        }
    })
}

/// 获取用户记忆数量
pub fn get_user_memory_count(user_id: u64) -> usize {
    crate::storage::sqlite::with_db(|conn| {
        let mut stmt = match conn.prepare("SELECT COUNT(*) FROM memories WHERE user_id = ?1") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        stmt.query_row(params![user_id], |row| row.get(0)).unwrap_or(0)
    })
}

/// 获取所有用户数量
pub fn get_total_user_count() -> usize {
    crate::storage::sqlite::with_db(|conn| {
        let mut stmt = match conn.prepare("SELECT COUNT(DISTINCT user_id) FROM memories") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        stmt.query_row([], |row| row.get(0)).unwrap_or(0)
    })
}

/// 搜索记忆（BM25 关键词检索）
pub fn search_memories(user_id: u64, query: &str, limit: usize) -> Vec<MemoryEntry> {
    crate::storage::sqlite::with_db(|conn| {
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
    })
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

    crate::storage::sqlite::with_db(|conn| {
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
    })
}
