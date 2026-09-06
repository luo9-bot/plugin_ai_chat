//! 训练数据留档：SQLite 按群归档
//!
//! 目录：`data/mind/archive/{群号}/`
//! - `messages.db`：群里每个人的消息（除 bot 外），训练"人类怎么说话"的语料
//! - `replies.db`：对哪条消息她做了什么回复——(触发, 回复) 配对，
//!   是"以她的人格回应"的监督信号
//!
//! 这是离线训练管线（tools/style_trainer）的数据基础：每天的数据带
//! `day` 冗余列，手动导出时按 `WHERE day = 'YYYY-MM-DD'` 即可。
//! 防注入被拦/替换的消息不入库——训练语料要真实。

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tracing::warn;

use crate::config;
use crate::util;

const SCHEMA_MESSAGES: &str = "
CREATE TABLE IF NOT EXISTS messages (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        INTEGER NOT NULL,
    day       TEXT    NOT NULL,
    user_id   INTEGER NOT NULL,
    user_name TEXT    NOT NULL DEFAULT '',
    content   TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_ts   ON messages(ts);
CREATE INDEX IF NOT EXISTS idx_messages_user ON messages(user_id, ts);
";

const SCHEMA_REPLIES: &str = "
CREATE TABLE IF NOT EXISTS replies (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    ts              INTEGER NOT NULL,
    day             TEXT    NOT NULL,
    user_id         INTEGER NOT NULL,
    trigger_content TEXT    NOT NULL DEFAULT '',
    reply_content   TEXT    NOT NULL,
    was_reply       INTEGER NOT NULL DEFAULT 0,
    reward          REAL
);
CREATE INDEX IF NOT EXISTS idx_replies_ts ON replies(ts);
";

fn archive_dir(group_id: u64) -> PathBuf {
    config::data_dir()
        .join("mind")
        .join("archive")
        .join(group_id.to_string())
}

/// 每群每库的常驻连接（SQLite 写串行，Mutex 保证线程安全）
fn conn_cache() -> &'static Mutex<HashMap<String, Connection>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Connection>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn open_conn(group_id: u64, kind: &str, schema: &str) -> Option<Connection> {
    let path = archive_dir(group_id).join(format!("{kind}.db"));
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        warn!(error = %e, group_id, "archive: 创建目录失败");
        return None;
    }
    let conn = Connection::open(&path).ok()?;
    conn.execute_batch(schema).ok()?;
    // 旧库迁移：reward 列（列已存在时静默忽略）
    let _ = conn.execute_batch("ALTER TABLE replies ADD COLUMN reward REAL");
    // WAL：写不阻塞读，断电恢复
    conn.pragma_update(None, "journal_mode", "WAL").ok()?;
    conn.pragma_update(None, "synchronous", "NORMAL").ok()?;
    Some(conn)
}

fn with_conn<F, T>(group_id: u64, kind: &str, schema: &str, f: F) -> Option<T>
where
    F: FnOnce(&mut Connection) -> Option<T>,
{
    let mut cache = conn_cache().lock().ok()?;
    let key = format!("{group_id}:{kind}");
    if !cache.contains_key(&key) {
        let conn = open_conn(group_id, kind, schema)?;
        cache.insert(key.clone(), conn);
    }
    let conn = cache.get_mut(&key)?;
    f(conn)
}

/// 归档一条群友消息（防注入放行的才进库）
pub fn record_message(group_id: u64, user_id: u64, user_name: &str, content: &str) {
    let ts = util::now_secs();
    let day = util::ts_to_date_str(ts);
    let stored = with_conn(group_id, "messages", SCHEMA_MESSAGES, |conn| {
        let ok = conn
            .execute(
                "INSERT INTO messages (ts, day, user_id, user_name, content) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![ts as i64, day, user_id as i64, user_name, content],
            )
            .is_ok();
        Some(ok)
    });
    if stored != Some(true) {
        warn!(group_id, user_id, "archive: 消息留档失败");
    }
}

/// 归档一条她的回复（与触发消息配对——训练监督信号的形状）
///
/// 返回新行 id，供回复效果追踪把 reward 精确写回这一行。
pub fn record_reply(
    group_id: u64,
    user_id: u64,
    trigger_content: &str,
    reply_content: &str,
    was_reply: bool,
) -> Option<i64> {
    let ts = util::now_secs();
    let day = util::ts_to_date_str(ts);
    let stored = with_conn(group_id, "replies", SCHEMA_REPLIES, |conn| {
        conn.execute(
            "INSERT INTO replies (ts, day, user_id, trigger_content, reply_content, was_reply)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                ts as i64,
                day,
                user_id as i64,
                trigger_content,
                reply_content,
                was_reply as i64
            ],
        )
        .ok()
        .map(|_| conn.last_insert_rowid())
    });
    if stored.is_none() {
        warn!(group_id, user_id, "archive: 回复留档失败");
    }
    stored
}

/// 把回复效果（ASI 0~100 → reward 0~1）写回对应归档行——
/// 强化训练的样本权重来源："获得互动的回复"概率上升
pub fn set_reward(group_id: u64, reply_id: i64, reward: f32) {
    let reward = reward.clamp(0.0, 1.0);
    let stored = with_conn(group_id, "replies", SCHEMA_REPLIES, |conn| {
        Some(
            conn.execute(
                "UPDATE replies SET reward = ?1 WHERE id = ?2",
                rusqlite::params![reward, reply_id],
            )
            .is_ok(),
        )
    });
    if stored != Some(true) {
        warn!(group_id, reply_id, "archive: reward 写回失败");
    }
}

/// 归档统计（admin 用）：各群已归档的消息/回复条数
pub fn stats() -> Vec<(u64, u64, u64)> {
    let dir = config::data_dir().join("mind").join("archive");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let gid: u64 = entry.file_name().to_str()?.parse().ok()?;
            let count = |db: &str, table: &str| -> u64 {
                let path = dir.join(gid.to_string()).join(format!("{db}.db"));
                let Ok(conn) = Connection::open(&path) else {
                    return 0;
                };
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0) as u64
            };
            Some((
                gid,
                count("messages", "messages"),
                count("replies", "replies"),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_well_formed() {
        // 内存库验证建表语句本身合法
        let conn = Connection::open_in_memory().unwrap();
        assert!(conn.execute_batch(SCHEMA_MESSAGES).is_ok());
        assert!(conn.execute_batch(SCHEMA_REPLIES).is_ok());
        assert!(conn
            .execute(
                "INSERT INTO messages (ts, day, user_id, user_name, content) VALUES (1, '2026-09-06', 2, 'n', 'c')",
                []
            )
            .is_ok());
    }
}
