//! 唯一的权威状态库
//!
//! 这一层存在的理由：状态曾经散在六种持久化惯例里（`thread_local`、
//! 进程内 `RwLock`、整文件 JSON、每群 JSON、每群 SQLite、append-only jsonl），
//! 各自有各自的生命周期，靠纪律保持同步。结果是**激活状态根本没落盘**——
//! 从后台开启/关闭一个对话在重启后消失；而拉黑写在 `blocklist.json` 里，
//! 与内存里的集合是两份真相。
//!
//! 这里给出一个 SQLite（WAL）作为**唯一写者**：进程内一个连接、一把锁，
//! 写操作串行；读可以另外开只读连接（WAL 下写不阻塞读）。
//!
//! `scope` 用枚举而不是 `'private' | 'group'` 字面量：调用点不可能拼错，
//! 存储格式只在 [`Scope::as_str`] 一处出现。

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::{Connection, OptionalExtension};

use crate::util::MutexExt;

/// 状态行的作用域
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// 私聊：`id` 是 user_id
    Private,
    /// 群聊：`id` 是 group_id
    Group,
}

impl Scope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Scope::Private => "private",
            Scope::Group => "group",
        }
    }
}

/// 谁在改状态——审计要能回答"谁改的"
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    /// 启动流程（把配置里的名单并入运行时）
    Boot,
    /// 聊天里的管理命令
    Command,
    /// Web 后台
    Admin,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Boot => "boot",
            Actor::Command => "command",
            Actor::Admin => "admin",
        }
    }
}

/// 存储层错误
#[derive(Debug)]
pub(crate) enum DbError {
    /// 打不开库（路径、权限、磁盘）
    Open(String),
    /// SQL 执行失败
    Sql(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Open(source) => write!(f, "状态库打不开: {source}"),
            DbError::Sql(source) => write!(f, "状态库执行失败: {source}"),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        DbError::Sql(error.to_string())
    }
}

/// 一条后台操作审计
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditEntry {
    pub actor: String,
    pub command: String,
    pub detail: String,
    pub created_at: i64,
}

/// 状态库 schema
///
/// 放在独立 `.sql` 文件里而不是 Rust 字符串字面量里：SQL 注释中的引号会
/// 提前闭合字面量（这个错我犯过三次），而独立文件还有语法高亮，
/// 也能被独立校验。
const SCHEMA: &str = include_str!("schema.sql");

/// 测试临时目录名去重（并行测试不能撞同一个目录）
#[cfg(test)]
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 一条工作记忆
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkingMemoryRow {
    /// 稳定身份：取代了以前"用写入时间戳认条目"的做法
    pub id: i64,
    pub user_id: u64,
    pub content: String,
    pub created_at: i64,
    pub bot_replied: bool,
}

/// 按群内下标删除的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteAtOutcome {
    Removed,
    /// 该群没有这一条（下标越界）
    OutOfRange,
}

/// 每群保留的最大工作记忆条数
///
/// 唯一来源：`working_memory` 模块与迁移都从这里取，避免两处各写一个 200。
pub(crate) const WORKING_MEMORY_KEEP: usize = 200;

/// 插入一条工作记忆并把该群裁剪到上限；返回新 id
///
/// 抽出来是为了让**迁移**也走同一条插入路径：迁移必须保留原始时间戳，
/// 不能走 `working_memory_push` 那种"用当前时间"的入口。
fn insert_working_memory(
    conn: &Connection,
    group_id: u64,
    user_id: u64,
    content: &str,
    created_at: i64,
    bot_replied: bool,
    keep: usize,
) -> Result<i64, DbError> {
    conn.execute(
        "INSERT INTO working_memory (group_id, user_id, content, created_at, bot_replied)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![group_id, user_id, content, created_at, bot_replied as i64],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "DELETE FROM working_memory
         WHERE group_id = ?1 AND id NOT IN (
             SELECT id FROM working_memory WHERE group_id = ?1 ORDER BY id DESC LIMIT ?2
         )",
        rusqlite::params![group_id, keep as i64],
    )?;
    Ok(id)
}

/// 进程级单例状态：每个子系统有自己的行
///
/// 用枚举而不是裸表名字符串：调用点不可能拼错，而表名只在
/// [`SingletonState::table`] 一处出现（因此拼进 SQL 的是固定字面量，
/// 不存在注入面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingletonState {
    /// 认知偏差（`memory::cognitive_biases`）
    CognitiveBiases,
    /// 注意力模型（`conversation::attention`）
    Attention,
}

impl SingletonState {
    fn table(self) -> &'static str {
        match self {
            SingletonState::CognitiveBiases => "cognitive_biases",
            SingletonState::Attention => "attention_state",
        }
    }
}

/// 按用户一行的子系统状态
///
/// 与 [`SingletonState`] 的区别是基数：这里是每人一行，因此读写都是
/// 主键操作，而不是"读整份 / 写整份"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerUserState {
    /// 人物档案（`person_info`）
    Person,
    /// 关系（`person_info::relationship`）
    Relationship,
}

impl PerUserState {
    fn table(self) -> &'static str {
        match self {
            PerUserState::Person => "person",
            PerUserState::Relationship => "relationship",
        }
    }
}

/// 一条配额段内消息
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuotaMessage {
    pub segment_start: u64,
    pub user_id: u64,
    pub message: String,
    pub ts: i64,
}

/// 影子观测的一个形态
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnShadowStat {
    pub shape: String,
    pub total: u64,
    /// 其中在严格 Turn 契约下会被判为不合法的次数
    pub invalid: u64,
}

/// 一次模型调用的用量
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiUsage {
    pub ts: i64,
    pub model: String,
    pub prompt_name: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cache_hit: u32,
    pub cache_miss: u32,
}

/// 终身累计（不受明细裁剪影响）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ApiUsageTotals {
    pub calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_hit: u64,
    pub cache_miss: u64,
}

/// 按 prompt 分组的终身累计
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiUsageByPrompt {
    pub prompt_name: String,
    pub calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit: u64,
    pub cache_miss: u64,
}

/// 明细保留条数上限
pub(crate) const API_USAGE_KEEP: i64 = 10_000;

/// 读一行的列顺序必须与各查询的 SELECT 一致
fn read_working_memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkingMemoryRow> {
    Ok(WorkingMemoryRow {
        id: row.get(0)?,
        user_id: row.get(1)?,
        content: row.get(2)?,
        created_at: row.get(3)?,
        bot_replied: row.get::<_, i64>(4)? != 0,
    })
}

/// 状态库句柄：进程内唯一的写连接
pub(crate) struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// 打开（或创建）状态库
    pub(crate) fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return Err(DbError::Open(e.to_string()));
        }
        let conn = Connection::open(path).map_err(|e| DbError::Open(e.to_string()))?;
        Self::prepare(conn)
    }

    /// 内存库（测试用）：schema 与生产完全一致，避免测试与线上分叉
    pub(crate) fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory().map_err(|e| DbError::Open(e.to_string()))?;
        Self::prepare(conn)
    }

    fn prepare(conn: Connection) -> Result<Self, DbError> {
        conn.execute_batch(SCHEMA)?;
        // WAL：写不阻塞读，断电可恢复
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| DbError::Open(e.to_string()))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| DbError::Open(e.to_string()))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| DbError::Open(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 借出写连接。
    ///
    /// 锁被毒化只意味着某次写入中断，连接与已提交数据仍然自洽，
    /// 因此取回内部值继续用，而不是让整个插件因一次中断永久失去状态库。
    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let guard = self.conn.lock_recover();
        f(&guard)
    }

    // ── 激活状态 ────────────────────────────────────────────────

    /// 开启/关闭一个会话；返回状态是否真的变化
    pub(crate) fn set_activation(
        &self,
        scope: Scope,
        id: u64,
        enabled: bool,
    ) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            if enabled {
                let changed = conn.execute(
                    "INSERT OR IGNORE INTO activation (scope, id, enabled_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![scope.as_str(), id, crate::util::now_secs()],
                )?;
                Ok(changed > 0)
            } else {
                let changed = conn.execute(
                    "DELETE FROM activation WHERE scope = ?1 AND id = ?2",
                    rusqlite::params![scope.as_str(), id],
                )?;
                Ok(changed > 0)
            }
        })
    }

    pub(crate) fn activations(&self, scope: Scope) -> Result<Vec<u64>, DbError> {
        self.with_conn(|conn| {
            let mut statement =
                conn.prepare("SELECT id FROM activation WHERE scope = ?1 ORDER BY enabled_at, id")?;
            let rows = statement.query_map(rusqlite::params![scope.as_str()], |row| row.get(0))?;
            let mut ids = Vec::new();
            for id in rows {
                ids.push(id?);
            }
            Ok(ids)
        })
    }

    // ── 黑名单 ──────────────────────────────────────────────────

    /// 拉黑；重复拉黑不覆盖原有原因与时间
    pub(crate) fn add_blocked(&self, user_id: u64, reason: &str) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "INSERT OR IGNORE INTO blocklist (user_id, reason, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![user_id, reason, crate::util::now_secs()],
            )?;
            Ok(changed > 0)
        })
    }

    pub(crate) fn remove_blocked(&self, user_id: u64) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "DELETE FROM blocklist WHERE user_id = ?1",
                rusqlite::params![user_id],
            )?;
            Ok(changed > 0)
        })
    }

    pub(crate) fn blocked_users(&self) -> Result<Vec<u64>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare("SELECT user_id FROM blocklist ORDER BY user_id")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            let mut ids = Vec::new();
            for id in rows {
                ids.push(id?);
            }
            Ok(ids)
        })
    }

    pub(crate) fn blocked_count(&self) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM blocklist", [], |row| row.get(0))?;
            Ok(count as usize)
        })
    }

    // ── 情绪状态 ────────────────────────────────────────────────

    /// 读一个用户的情绪状态原始 JSON；没有记录时返回 `None`
    ///
    /// 主键查询：这一步取代了"为了一个用户解析整份 `emotion.json`"。
    pub(crate) fn emotion_state(&self, user_id: u64) -> Result<Option<String>, DbError> {
        self.with_conn(|conn| {
            let found = conn
                .query_row(
                    "SELECT state FROM emotion WHERE user_id = ?1",
                    rusqlite::params![user_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            Ok(found)
        })
    }

    /// 写入一个用户的情绪状态
    pub(crate) fn set_emotion_state(&self, user_id: u64, state: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO emotion (user_id, state, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
                rusqlite::params![user_id, state, crate::util::now_secs()],
            )?;
            Ok(())
        })
    }

    /// 一次写入多个用户的情绪状态（衰减批处理用）
    ///
    /// 放在一个事务里：一批 N 个用户要么全落盘，要么一个都不落，
    /// 不会留下"衰减了一半"的状态。
    pub(crate) fn set_emotion_states(&self, states: &[(u64, String)]) -> Result<(), DbError> {
        if states.is_empty() {
            return Ok(());
        }
        self.with_conn(|conn| {
            let now = crate::util::now_secs();
            let transaction = conn.unchecked_transaction()?;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO emotion (user_id, state, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(user_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
                )?;
                for (user_id, state) in states {
                    statement.execute(rusqlite::params![user_id, state, now])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    pub(crate) fn emotion_user_count(&self) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM emotion", [], |row| row.get(0))?;
            Ok(count as usize)
        })
    }

    /// 所有用户的情绪状态（供后台列表视图）
    pub(crate) fn all_emotion_states(&self) -> Result<Vec<(u64, String)>, DbError> {
        self.with_conn(|conn| {
            let mut statement =
                conn.prepare("SELECT user_id, state FROM emotion ORDER BY user_id")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            let mut states = Vec::new();
            for entry in rows {
                states.push(entry?);
            }
            Ok(states)
        })
    }

    /// 把当前库一致地快照到指定路径
    ///
    /// 用 SQLite 自己的 `VACUUM INTO`，**不要**直接复制 `.db` 文件：
    /// WAL 里可能还有尚未合并的已提交数据，复制出来的快照会缺东西。
    /// 目标路径必须不存在（这是 `VACUUM INTO` 的约束）。
    pub(crate) fn snapshot_into(&self, path: &Path) -> Result<(), DbError> {
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| DbError::Open(format!("清理旧快照 {}: {e}", path.display())))?;
        }
        let target = path
            .to_str()
            .ok_or_else(|| DbError::Open("快照路径不是合法 UTF-8".to_string()))?;
        self.with_conn(|conn| {
            conn.execute("VACUUM INTO ?1", rusqlite::params![target])?;
            Ok(())
        })
    }

    // ── 工作记忆 ────────────────────────────────────────────────

    /// 追加一条工作记忆，并把该群裁剪到最多 `keep` 条；返回新条目的 id
    ///
    /// 插入与裁剪在同一个事务里：并发写入不会让某个群短暂超过上限。
    pub(crate) fn working_memory_push(
        &self,
        group_id: u64,
        user_id: u64,
        content: &str,
        bot_replied: bool,
        keep: usize,
    ) -> Result<i64, DbError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let id = insert_working_memory(
                &transaction,
                group_id,
                user_id,
                content,
                crate::util::now_secs() as i64,
                bot_replied,
                keep,
            )?;
            transaction.commit()?;
            Ok(id)
        })
    }

    /// 把某用户最近一条未回复的条目标记为已回复；返回是否有条目被标记
    pub(crate) fn working_memory_mark_replied(
        &self,
        group_id: u64,
        user_id: u64,
    ) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE working_memory SET bot_replied = 1
                 WHERE id = (
                     SELECT id FROM working_memory
                     WHERE group_id = ?1 AND user_id = ?2 AND bot_replied = 0
                     ORDER BY id DESC LIMIT 1
                 )",
                rusqlite::params![group_id, user_id],
            )?;
            Ok(changed > 0)
        })
    }

    /// 取 `after` 时间戳之后的条目（按 id 升序，最多 `limit` 条）
    pub(crate) fn working_memory_since(
        &self,
        group_id: u64,
        after: i64,
        limit: usize,
    ) -> Result<Vec<WorkingMemoryRow>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT id, user_id, content, created_at, bot_replied FROM working_memory
                 WHERE group_id = ?1 AND created_at > ?2 ORDER BY id LIMIT ?3",
            )?;
            let rows = statement.query_map(
                rusqlite::params![group_id, after, limit as i64],
                read_working_memory_row,
            )?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 取某个群的全部条目（按 id 升序）
    pub(crate) fn working_memory_of_group(
        &self,
        group_id: u64,
    ) -> Result<Vec<WorkingMemoryRow>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT id, user_id, content, created_at, bot_replied FROM working_memory
                 WHERE group_id = ?1 ORDER BY id",
            )?;
            let rows = statement.query_map(rusqlite::params![group_id], read_working_memory_row)?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 该群是否还有工作记忆
    pub(crate) fn working_memory_group_exists(&self, group_id: u64) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM working_memory WHERE group_id = ?1",
                rusqlite::params![group_id],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }

    /// 取陈旧条目（`created_at <= cutoff`），连同其群号，供归档
    pub(crate) fn working_memory_expired(
        &self,
        cutoff: i64,
    ) -> Result<Vec<(u64, WorkingMemoryRow)>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT group_id, id, user_id, content, created_at, bot_replied FROM working_memory
                 WHERE created_at <= ?1 ORDER BY group_id, id",
            )?;
            let rows = statement.query_map(rusqlite::params![cutoff], |row| {
                let group_id: u64 = row.get(0)?;
                Ok((group_id, read_working_memory_row(row)?))
            })?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 按 id 删除条目；返回删除的行数
    pub(crate) fn working_memory_delete_ids(&self, ids: &[i64]) -> Result<usize, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let mut removed = 0usize;
            {
                let mut statement =
                    transaction.prepare("DELETE FROM working_memory WHERE id = ?1")?;
                for id in ids {
                    removed += statement.execute(rusqlite::params![id])?;
                }
            }
            transaction.commit()?;
            Ok(removed)
        })
    }

    /// 按群内下标删除（下标按 id 升序，与后台列表的顺序一致）
    pub(crate) fn working_memory_delete_at(
        &self,
        group_id: u64,
        index: usize,
    ) -> Result<DeleteAtOutcome, DbError> {
        self.with_conn(|conn| {
            let target: Option<i64> = conn
                .query_row(
                    "SELECT id FROM working_memory WHERE group_id = ?1 ORDER BY id LIMIT 1 OFFSET ?2",
                    rusqlite::params![group_id, index as i64],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(id) = target else {
                return Ok(DeleteAtOutcome::OutOfRange);
            };
            conn.execute("DELETE FROM working_memory WHERE id = ?1", rusqlite::params![id])?;
            Ok(DeleteAtOutcome::Removed)
        })
    }

    /// 覆盖某条目的正文
    pub(crate) fn working_memory_set_content(&self, id: i64, content: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE working_memory SET content = ?2 WHERE id = ?1",
                rusqlite::params![id, content],
            )?;
            Ok(())
        })
    }

    /// 按群分组返回全部工作记忆（供后台列表视图）
    pub(crate) fn working_memory_groups(
        &self,
    ) -> Result<Vec<(u64, Vec<WorkingMemoryRow>)>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT group_id, id, user_id, content, created_at, bot_replied FROM working_memory
                 ORDER BY group_id, id",
            )?;
            let rows = statement.query_map([], |row| {
                let group_id: u64 = row.get(0)?;
                Ok((group_id, read_working_memory_row(row)?))
            })?;

            let mut groups: Vec<(u64, Vec<WorkingMemoryRow>)> = Vec::new();
            for entry in rows {
                let (group_id, row) = entry?;
                match groups.last_mut() {
                    Some((current, rows)) if *current == group_id => rows.push(row),
                    _ => groups.push((group_id, vec![row])),
                }
            }
            Ok(groups)
        })
    }

    /// 有工作记忆的群数量（用于启动日志）
    pub(crate) fn working_memory_group_count(&self) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT group_id) FROM working_memory",
                [],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
    }

    // ── API 用量 ────────────────────────────────────────────────

    /// 追加一次调用用量，并更新终身累计
    ///
    /// 两条语句都是 O(1)：一条 INSERT，一条 UPSERT。旧实现是"读整份
    /// 10,000 条记录 + 改 + 写回整份文件"，而它在**每次模型调用后**执行。
    pub(crate) fn record_api_usage(&self, usage: &ApiUsage, keep: i64) -> Result<(), DbError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            transaction.execute(
                "INSERT INTO api_usage
                 (ts, model, prompt_name, prompt_tokens, completion_tokens, total_tokens, cache_hit, cache_miss)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    usage.ts,
                    usage.model,
                    usage.prompt_name,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.total_tokens,
                    usage.cache_hit,
                    usage.cache_miss,
                ],
            )?;
            transaction.execute(
                "INSERT INTO api_usage_total
                 (prompt_name, calls, prompt_tokens, completion_tokens, total_tokens, cache_hit, cache_miss)
                 VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(prompt_name) DO UPDATE SET
                     calls = calls + 1,
                     prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                     completion_tokens = completion_tokens + excluded.completion_tokens,
                     total_tokens = total_tokens + excluded.total_tokens,
                     cache_hit = cache_hit + excluded.cache_hit,
                     cache_miss = cache_miss + excluded.cache_miss",
                rusqlite::params![
                    usage.prompt_name,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.total_tokens,
                    usage.cache_hit,
                    usage.cache_miss,
                ],
            )?;
            // 明细裁剪：按主键范围删除，不扫描表
            transaction.execute(
                "DELETE FROM api_usage
                 WHERE id <= (SELECT MAX(id) FROM api_usage) - ?1",
                rusqlite::params![keep],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    /// 终身累计（各 prompt 求和）
    pub(crate) fn api_usage_totals(&self) -> Result<ApiUsageTotals, DbError> {
        self.with_conn(|conn| {
            let totals = conn.query_row(
                "SELECT COALESCE(SUM(calls), 0), COALESCE(SUM(prompt_tokens), 0),
                        COALESCE(SUM(completion_tokens), 0), COALESCE(SUM(cache_hit), 0),
                        COALESCE(SUM(cache_miss), 0)
                 FROM api_usage_total",
                [],
                |row| {
                    Ok(ApiUsageTotals {
                        calls: row.get::<_, i64>(0)? as u64,
                        prompt_tokens: row.get::<_, i64>(1)? as u64,
                        completion_tokens: row.get::<_, i64>(2)? as u64,
                        cache_hit: row.get::<_, i64>(3)? as u64,
                        cache_miss: row.get::<_, i64>(4)? as u64,
                    })
                },
            )?;
            Ok(totals)
        })
    }

    /// 按 prompt 分组的终身累计（按 total_tokens 降序）
    pub(crate) fn api_usage_by_prompt(&self) -> Result<Vec<ApiUsageByPrompt>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT prompt_name, calls, prompt_tokens, completion_tokens, total_tokens,
                        cache_hit, cache_miss
                 FROM api_usage_total ORDER BY total_tokens DESC, prompt_name",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(ApiUsageByPrompt {
                    prompt_name: row.get(0)?,
                    calls: row.get::<_, i64>(1)? as u64,
                    prompt_tokens: row.get::<_, i64>(2)? as u64,
                    completion_tokens: row.get::<_, i64>(3)? as u64,
                    total_tokens: row.get::<_, i64>(4)? as u64,
                    cache_hit: row.get::<_, i64>(5)? as u64,
                    cache_miss: row.get::<_, i64>(6)? as u64,
                })
            })?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 最近的用量明细（最新在前）
    pub(crate) fn api_usage_recent(&self, limit: usize) -> Result<Vec<ApiUsage>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT ts, model, prompt_name, prompt_tokens, completion_tokens, total_tokens,
                        cache_hit, cache_miss
                 FROM api_usage ORDER BY id DESC LIMIT ?1",
            )?;
            let rows = statement.query_map(rusqlite::params![limit as i64], |row| {
                Ok(ApiUsage {
                    ts: row.get(0)?,
                    model: row.get(1)?,
                    prompt_name: row.get(2)?,
                    prompt_tokens: row.get(3)?,
                    completion_tokens: row.get(4)?,
                    total_tokens: row.get(5)?,
                    cache_hit: row.get(6)?,
                    cache_miss: row.get(7)?,
                })
            })?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 迁移用：直接写入一条终身累计（不做 +1 累加）
    fn set_api_usage_total(&self, entry: &ApiUsageByPrompt) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO api_usage_total
                 (prompt_name, calls, prompt_tokens, completion_tokens, total_tokens, cache_hit, cache_miss)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(prompt_name) DO UPDATE SET
                     calls = excluded.calls,
                     prompt_tokens = excluded.prompt_tokens,
                     completion_tokens = excluded.completion_tokens,
                     total_tokens = excluded.total_tokens,
                     cache_hit = excluded.cache_hit,
                     cache_miss = excluded.cache_miss",
                rusqlite::params![
                    entry.prompt_name,
                    entry.calls as i64,
                    entry.prompt_tokens as i64,
                    entry.completion_tokens as i64,
                    entry.total_tokens as i64,
                    entry.cache_hit as i64,
                    entry.cache_miss as i64,
                ],
            )?;
            Ok(())
        })
    }

    // ── 进程级单例状态 ──────────────────────────────────────────

    /// 读某个子系统的单例状态；没有记录时返回 `None`
    pub(crate) fn singleton_state(&self, which: SingletonState) -> Result<Option<String>, DbError> {
        // 表名来自 `SingletonState::table()` 的固定字面量，不含外部输入
        let sql = format!("SELECT state FROM {} WHERE id = 1", which.table());
        self.with_conn(|conn| {
            let found = conn
                .query_row(&sql, [], |row| row.get::<_, String>(0))
                .optional()?;
            Ok(found)
        })
    }

    /// 写某个子系统的单例状态
    ///
    /// 只 UPSERT 自己那一行：另一个子系统的行不会被本次写入触碰，
    /// 这正是原先把两部分塞进同一个 JSON 时丢失更新的根因。
    pub(crate) fn set_singleton_state(
        &self,
        which: SingletonState,
        state: &str,
    ) -> Result<(), DbError> {
        let sql = format!(
            "INSERT INTO {} (id, state, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
            which.table()
        );
        self.with_conn(|conn| {
            conn.execute(&sql, rusqlite::params![state, crate::util::now_secs()])?;
            Ok(())
        })
    }

    // ── 配额 ────────────────────────────────────────────────────

    /// 跨天重置：日期不同则清空计数与段日志，返回是否发生了重置
    pub(crate) fn quota_roll_day(&self, day: &str) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let stored: Option<String> = conn
                .query_row("SELECT day FROM quota_meta WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .optional()?;
            if stored.as_deref() == Some(day) {
                return Ok(false);
            }
            let transaction = conn.unchecked_transaction()?;
            transaction.execute("DELETE FROM quota_segment", [])?;
            transaction.execute("DELETE FROM quota_segment_message", [])?;
            transaction.execute(
                "INSERT INTO quota_meta (id, day) VALUES (1, ?1)
                 ON CONFLICT(id) DO UPDATE SET day = excluded.day",
                rusqlite::params![day],
            )?;
            transaction.commit()?;
            Ok(true)
        })
    }

    pub(crate) fn quota_segment_count(
        &self,
        group_id: u64,
        segment_start: u64,
    ) -> Result<u32, DbError> {
        self.with_conn(|conn| {
            let count: Option<i64> = conn
                .query_row(
                    "SELECT count FROM quota_segment WHERE group_id = ?1 AND segment_start = ?2",
                    rusqlite::params![group_id, segment_start],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(count.unwrap_or(0).max(0) as u32)
        })
    }

    /// 检查并扣减一个配额名额；返回是否允许（`false` = 本段已用尽）
    ///
    /// "读计数 → 比较 → 加一"必须在**一个事务**里：否则两个线程同时看到
    /// "还剩一个名额"就会都放行。
    pub(crate) fn quota_consume(
        &self,
        group_id: u64,
        segment_start: u64,
        max: u32,
    ) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            let current: Option<i64> = transaction
                .query_row(
                    "SELECT count FROM quota_segment WHERE group_id = ?1 AND segment_start = ?2",
                    rusqlite::params![group_id, segment_start],
                    |row| row.get(0),
                )
                .optional()?;
            if current.unwrap_or(0) >= max as i64 {
                // 未提交即返回：事务被丢弃（回滚），而这里本来也没有写入
                return Ok(false);
            }
            transaction.execute(
                "INSERT INTO quota_segment (group_id, segment_start, count) VALUES (?1, ?2, 1)
                 ON CONFLICT(group_id, segment_start) DO UPDATE SET count = count + 1",
                rusqlite::params![group_id, segment_start],
            )?;
            transaction.commit()?;
            Ok(true)
        })
    }

    /// 记录一条段内消息
    pub(crate) fn quota_log_message(
        &self,
        group_id: u64,
        segment_start: u64,
        user_id: u64,
        message: &str,
        ts: i64,
    ) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO quota_segment_message (group_id, segment_start, user_id, message, ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![group_id, segment_start, user_id, message, ts],
            )?;
            Ok(())
        })
    }

    /// 删除 `cutoff` 之前的段内消息；返回删除条数
    pub(crate) fn quota_prune_messages(&self, cutoff: i64) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            let removed = conn.execute(
                "DELETE FROM quota_segment_message WHERE ts < ?1",
                rusqlite::params![cutoff],
            )?;
            Ok(removed)
        })
    }

    /// 某个群最近的若干段消息（段起始时间倒序）
    pub(crate) fn quota_messages(
        &self,
        group_id: u64,
        limit_segments: usize,
    ) -> Result<Vec<QuotaMessage>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT segment_start, user_id, message, ts FROM quota_segment_message
                 WHERE group_id = ?1
                   AND segment_start IN (
                       SELECT DISTINCT segment_start FROM quota_segment_message
                       WHERE group_id = ?1 ORDER BY segment_start DESC LIMIT ?2
                   )
                 ORDER BY segment_start DESC, id",
            )?;
            let rows =
                statement.query_map(rusqlite::params![group_id, limit_segments as i64], |row| {
                    Ok(QuotaMessage {
                        segment_start: row.get(0)?,
                        user_id: row.get(1)?,
                        message: row.get(2)?,
                        ts: row.get(3)?,
                    })
                })?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    /// 有段日志的群
    pub(crate) fn quota_groups_with_messages(&self) -> Result<Vec<u64>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn
                .prepare("SELECT DISTINCT group_id FROM quota_segment_message ORDER BY group_id")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            let mut groups = Vec::new();
            for group in rows {
                groups.push(group?);
            }
            Ok(groups)
        })
    }

    /// 迁移用：直接写入一个段的计数（不做 +1）
    pub(crate) fn quota_set_segment_count(
        &self,
        group_id: u64,
        segment_start: u64,
        count: u32,
    ) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO quota_segment (group_id, segment_start, count) VALUES (?1, ?2, ?3)
                 ON CONFLICT(group_id, segment_start) DO UPDATE SET count = excluded.count",
                rusqlite::params![group_id, segment_start, count],
            )?;
            Ok(())
        })
    }

    // ── 按用户一行的状态 ────────────────────────────────────────

    /// 读某个用户的记录；没有则 `None`
    pub(crate) fn per_user_state(
        &self,
        which: PerUserState,
        user_id: u64,
    ) -> Result<Option<String>, DbError> {
        // 表名来自 `PerUserState::table()` 的固定字面量，不含外部输入
        let sql = format!("SELECT state FROM {} WHERE user_id = ?1", which.table());
        self.with_conn(|conn| {
            let found = conn
                .query_row(&sql, rusqlite::params![user_id], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?;
            Ok(found)
        })
    }

    /// 写某个用户的记录（单行 UPSERT）
    pub(crate) fn set_per_user_state(
        &self,
        which: PerUserState,
        user_id: u64,
        state: &str,
    ) -> Result<(), DbError> {
        let sql = format!(
            "INSERT INTO {} (user_id, state, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
            which.table()
        );
        self.with_conn(|conn| {
            conn.execute(
                &sql,
                rusqlite::params![user_id, state, crate::util::now_secs()],
            )?;
            Ok(())
        })
    }

    /// 全部记录（供后台列表视图）
    pub(crate) fn all_per_user_states(
        &self,
        which: PerUserState,
    ) -> Result<Vec<(u64, String)>, DbError> {
        let sql = format!(
            "SELECT user_id, state FROM {} ORDER BY user_id",
            which.table()
        );
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&sql)?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }

    pub(crate) fn per_user_count(&self, which: PerUserState) -> Result<usize, DbError> {
        let sql = format!("SELECT COUNT(*) FROM {}", which.table());
        self.with_conn(|conn| {
            let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
            Ok(count as usize)
        })
    }

    // ── Turn 契约影子观测 ───────────────────────────────────────

    /// 记录一次模型输出在**严格 Turn 契约**下会怎样
    ///
    /// 只观测、不干预：写入失败也不影响表达路径。
    pub(crate) fn record_turn_shadow(
        &self,
        shape: &str,
        valid_under_turn: bool,
        detail: &str,
    ) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO turn_shadow (ts, shape, valid_under_turn, detail)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    crate::util::now_secs(),
                    shape,
                    valid_under_turn as i64,
                    detail
                ],
            )?;
            Ok(())
        })
    }

    /// 影子观测汇总：每个形态的次数与其中在严格契约下非法的次数
    pub(crate) fn turn_shadow_summary(
        &self,
        since_ts: i64,
    ) -> Result<Vec<TurnShadowStat>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT shape,
                        COUNT(*),
                        COALESCE(SUM(CASE WHEN valid_under_turn = 0 THEN 1 ELSE 0 END), 0)
                 FROM turn_shadow WHERE ts >= ?1
                 GROUP BY shape ORDER BY COUNT(*) DESC",
            )?;
            let rows = statement.query_map(rusqlite::params![since_ts], |row| {
                Ok(TurnShadowStat {
                    shape: row.get(0)?,
                    total: row.get::<_, i64>(1)? as u64,
                    invalid: row.get::<_, i64>(2)? as u64,
                })
            })?;
            let mut stats = Vec::new();
            for entry in rows {
                stats.push(entry?);
            }
            Ok(stats)
        })
    }

    /// 裁剪过期的影子观测
    pub(crate) fn prune_turn_shadow(&self, before_ts: i64) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            let removed = conn.execute(
                "DELETE FROM turn_shadow WHERE ts < ?1",
                rusqlite::params![before_ts],
            )?;
            Ok(removed)
        })
    }

    // ── 审计 ────────────────────────────────────────────────────
    /// 记录一次后台/命令改动。
    ///
    /// 审计是"后台没有独立写路径"这句话的凭据：任何一次状态改动都能回答
    /// "谁在什么时候改了什么"。
    pub(crate) fn record_audit(
        &self,
        actor: Actor,
        command: &str,
        detail: &str,
    ) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO admin_audit (actor, command, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![actor.as_str(), command, detail, crate::util::now_secs()],
            )?;
            Ok(())
        })
    }

    pub(crate) fn recent_audit(&self, limit: usize) -> Result<Vec<AuditEntry>, DbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT actor, command, detail, created_at FROM admin_audit
                 ORDER BY id DESC LIMIT ?1",
            )?;
            let rows = statement.query_map(rusqlite::params![limit as i64], |row| {
                Ok(AuditEntry {
                    actor: row.get(0)?,
                    command: row.get(1)?,
                    detail: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            let mut entries = Vec::new();
            for entry in rows {
                entries.push(entry?);
            }
            Ok(entries)
        })
    }
}

/// 一次性迁移：把旧的 `blocklist.json` 并入状态库
///
/// 这不是为了"兼容旧格式"而背负担，而是**保住真实数据**：跳过它会让
/// 先前被拉黑的人静默恢复发言，而用户不会收到任何提示。
/// 导入成功后把文件改名为 `.migrated`，因此不会重复执行。
pub(crate) fn migrate_legacy_blocklist() -> Result<usize, DbError> {
    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };
    let legacy = dir.join("blocklist.json");
    if !legacy.exists() {
        return Ok(0);
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    let ids: Vec<u64> = serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    let mut imported = 0usize;
    for user_id in ids {
        if db.add_blocked(user_id, "migrated from blocklist.json")? {
            imported += 1;
        }
    }

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 黑名单已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    if imported > 0 {
        tracing::info!(imported, "db: 已把旧 blocklist.json 导入状态库");
    }
    Ok(imported)
}

/// 一次性迁移：把旧的 `emotion.json` 并入状态库
///
/// 与黑名单迁移同理：不导入会让所有用户的情绪状态在升级后**一起归零**，
/// 而情绪是她"此刻的心情"，不是可以随手丢的缓存。
/// 导入后把文件改名为 `.migrated`，因此不会重复执行。
pub(crate) fn migrate_legacy_emotion() -> Result<usize, DbError> {
    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };
    let legacy = dir.join("emotion.json");
    if !legacy.exists() {
        return Ok(0);
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    // 旧格式是"用户 id 字符串 → EmotionState"的一张扁平表
    let states: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    let mut imported = 0usize;
    for (key, value) in states {
        let Ok(user_id) = key.parse::<u64>() else {
            continue;
        };
        // 库里已有记录时不覆盖：迁移只负责补空缺
        if db.emotion_state(user_id)?.is_some() {
            continue;
        }
        db.set_emotion_state(user_id, &value.to_string())?;
        imported += 1;
    }

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 情绪状态已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    if imported > 0 {
        tracing::info!(imported, "db: 已把旧 emotion.json 导入状态库");
    }
    Ok(imported)
}

/// 一次性迁移：把旧的 `working_memory.json` 并入状态库
///
/// 与黑名单/情绪迁移同理：不导入会让"刚才聊了什么"归零——工作记忆正是
/// 她此刻的短期上下文，也是 `catch_up` 深读的来源。
/// 迁移**保留原始时间戳**：否则旧条目会看起来像刚写的，
/// 而过期清理与深读游标都依赖时间。
pub(crate) fn migrate_legacy_working_memory() -> Result<usize, DbError> {
    #[derive(serde::Deserialize)]
    struct LegacyEntry {
        user_id: u64,
        content: String,
        timestamp: u64,
        #[serde(default)]
        bot_replied: bool,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyGroup {
        #[serde(default)]
        entries: Vec<LegacyEntry>,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyStore {
        #[serde(default)]
        groups: std::collections::HashMap<String, LegacyGroup>,
    }

    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };
    let legacy = dir.join("working_memory.json");
    if !legacy.exists() {
        return Ok(0);
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    let parsed: LegacyStore = serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    let mut imported = 0usize;
    db.with_conn(|conn| {
        let transaction = conn.unchecked_transaction()?;
        for (group_key, group) in parsed.groups {
            let Ok(group_id) = group_key.parse::<u64>() else {
                continue;
            };
            for entry in group.entries {
                insert_working_memory(
                    &transaction,
                    group_id,
                    entry.user_id,
                    &entry.content,
                    entry.timestamp as i64,
                    entry.bot_replied,
                    WORKING_MEMORY_KEEP,
                )?;
                imported += 1;
            }
        }
        transaction.commit()?;
        Ok(())
    })?;

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 工作记忆已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    if imported > 0 {
        tracing::info!(imported, "db: 已把旧 working_memory.json 导入状态库");
    }
    Ok(imported)
}

/// 一次性迁移：把旧的 `api_usage.json` 并入状态库
///
/// 分两部分导入，缺一不可：
/// - `aggregated.by_prompt` → `api_usage_total`（**终身累计**。旧文件里
///   明细早已被裁到 10,000 条，只导明细会让历史总量缩水）；
/// - `records` → `api_usage`（保留原始时间戳的明细）。
pub(crate) fn migrate_legacy_api_usage() -> Result<usize, DbError> {
    #[derive(serde::Deserialize)]
    struct LegacyRecord {
        #[serde(default)]
        timestamp: u64,
        #[serde(default)]
        model: String,
        #[serde(default)]
        prompt_name: String,
        #[serde(default)]
        prompt_tokens: u32,
        #[serde(default)]
        completion_tokens: u32,
        #[serde(default)]
        total_tokens: u32,
        #[serde(default)]
        prompt_cache_hit: u32,
        #[serde(default)]
        prompt_cache_miss: u32,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyPromptStat {
        calls: u64,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_tokens: u64,
        cache_hit: u64,
        cache_miss: u64,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyAggregated {
        #[serde(default)]
        by_prompt: std::collections::HashMap<String, LegacyPromptStat>,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyStore {
        #[serde(default)]
        records: Vec<LegacyRecord>,
        #[serde(default)]
        aggregated: LegacyAggregated,
    }

    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };
    let legacy = dir.join("api_usage.json");
    if !legacy.exists() {
        return Ok(0);
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    let parsed: LegacyStore = serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    // 累计先写：即使明细导入中途失败，总量也已经保住
    for (prompt_name, stat) in parsed.aggregated.by_prompt {
        db.set_api_usage_total(&ApiUsageByPrompt {
            prompt_name,
            calls: stat.calls,
            prompt_tokens: stat.prompt_tokens,
            completion_tokens: stat.completion_tokens,
            total_tokens: stat.total_tokens,
            cache_hit: stat.cache_hit,
            cache_miss: stat.cache_miss,
        })?;
    }

    let mut imported = 0usize;
    for record in parsed.records {
        db.record_api_usage(
            &ApiUsage {
                ts: record.timestamp as i64,
                model: record.model,
                prompt_name: record.prompt_name,
                prompt_tokens: record.prompt_tokens,
                completion_tokens: record.completion_tokens,
                total_tokens: record.total_tokens,
                cache_hit: record.prompt_cache_hit,
                cache_miss: record.prompt_cache_miss,
            },
            API_USAGE_KEEP,
        )?;
        imported += 1;
    }

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 用量记录已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    if imported > 0 {
        tracing::info!(imported, "db: 已把旧 api_usage.json 导入状态库");
    }
    Ok(imported)
}

/// 一次性迁移：把旧的 `cognitive_state.json` 拆进各自的行
///
/// 旧文件把两个子系统塞在一起（`biases` + `attention_json`）。导入时
/// **两部分都要导**：只导一边等于静默丢掉另一边。
pub(crate) fn migrate_legacy_cognitive_state() -> Result<(), DbError> {
    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(());
    };
    let legacy = dir.join("cognitive_state.json");
    if !legacy.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    if let Some(biases) = parsed.get("biases").filter(|value| !value.is_null())
        && db
            .singleton_state(SingletonState::CognitiveBiases)?
            .is_none()
    {
        // 已有记录时不覆盖：迁移只补空缺
        db.set_singleton_state(SingletonState::CognitiveBiases, &biases.to_string())?;
    }
    if let Some(attention) = parsed
        .get("attention_json")
        .filter(|value| !value.is_null())
        && db.singleton_state(SingletonState::Attention)?.is_none()
    {
        db.set_singleton_state(SingletonState::Attention, &attention.to_string())?;
    }

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 认知状态已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    Ok(())
}

/// 一次性迁移：把旧的 `quota.json` 并入状态库
///
/// 段计数本身会在下一个 5 分钟段自愈，但段日志是后台可查看的 48 小时
/// 历史；不导会让那个页面直接空掉。计数与日志都保留原始时间。
pub(crate) fn migrate_legacy_quota() -> Result<usize, DbError> {
    #[derive(serde::Deserialize)]
    struct LegacyCount {
        segment_start: u64,
        count: u32,
    }
    #[derive(serde::Deserialize)]
    struct LegacyMessage {
        user_id: u64,
        message: String,
        timestamp: u64,
    }
    #[derive(serde::Deserialize)]
    struct LegacyLogEntry {
        segment_start: u64,
        #[serde(default)]
        messages: Vec<LegacyMessage>,
    }
    #[derive(serde::Deserialize, Default)]
    struct LegacyStore {
        #[serde(default)]
        date: String,
        #[serde(default)]
        counts: std::collections::HashMap<String, Vec<LegacyCount>>,
        #[serde(default)]
        segment_log: std::collections::HashMap<String, Vec<LegacyLogEntry>>,
    }

    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };
    let legacy = dir.join("quota.json");
    if !legacy.exists() {
        return Ok(0);
    }

    let raw = std::fs::read_to_string(&legacy)
        .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
    let parsed: LegacyStore = serde_json::from_str(&raw).unwrap_or_default();

    let db = db();
    let mut imported = 0usize;
    for (group_key, counts) in parsed.counts {
        let Ok(group_id) = group_key.parse::<u64>() else {
            continue;
        };
        for count in counts {
            db.quota_set_segment_count(group_id, count.segment_start, count.count)?;
            imported += 1;
        }
    }
    for (group_key, logs) in parsed.segment_log {
        let Ok(group_id) = group_key.parse::<u64>() else {
            continue;
        };
        for entry in logs {
            for message in entry.messages {
                db.quota_log_message(
                    group_id,
                    entry.segment_start,
                    message.user_id,
                    &message.message,
                    message.timestamp as i64,
                )?;
                imported += 1;
            }
        }
    }
    // 记录旧文件里的日期，避免同一天被当成"跨天"而清空刚导入的数据
    if !parsed.date.is_empty() {
        let _ = db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO quota_meta (id, day) VALUES (1, ?1)
                 ON CONFLICT(id) DO UPDATE SET day = excluded.day",
                rusqlite::params![parsed.date],
            )?;
            Ok(())
        });
    }

    let archived = legacy.with_extension("json.migrated");
    if let Err(error) = std::fs::rename(&legacy, &archived) {
        tracing::warn!(
            %error,
            "db: 配额记录已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
        );
    }
    if imported > 0 {
        tracing::info!(imported, "db: 已把旧 quota.json 导入状态库");
    }
    Ok(imported)
}

/// 一次性迁移：把旧的 `person_info.json` / `relationships.json` 并入状态库
///
/// 两者形状相同（`{"<集合名>": {"<uid>": {...}}}`），因此共用一条迁移路径。
/// 不导会让「她认识谁、和谁多熟」整体归零——那是她认人与人设的基础。
pub(crate) fn migrate_legacy_per_user_files() -> Result<usize, DbError> {
    let Some(dir) = crate::config::try_data_dir() else {
        return Ok(0);
    };

    let mut imported = 0usize;
    for (file, collection, which) in [
        ("person_info.json", "profiles", PerUserState::Person),
        (
            "relationships.json",
            "relationships",
            PerUserState::Relationship,
        ),
    ] {
        let legacy = dir.join(file);
        if !legacy.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&legacy)
            .map_err(|e| DbError::Open(format!("读取 {}: {e}", legacy.display())))?;
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        let Some(entries) = parsed.get(collection).and_then(|value| value.as_object()) else {
            continue;
        };

        let db = db();
        for (key, value) in entries {
            let Ok(user_id) = key.parse::<u64>() else {
                continue;
            };
            // 已有记录时不覆盖：迁移只补空缺
            if db.per_user_state(which, user_id)?.is_some() {
                continue;
            }
            db.set_per_user_state(which, user_id, &value.to_string())?;
            imported += 1;
        }

        let archived = legacy.with_extension("json.migrated");
        if let Err(error) = std::fs::rename(&legacy, &archived) {
            tracing::warn!(
                %error,
                file,
                "db: 已导入，但旧文件改名失败（下次启动会再导入一次，结果幂等）"
            );
        }
    }

    if imported > 0 {
        tracing::info!(imported, "db: 已把旧人物/关系档案导入状态库");
    }
    Ok(imported)
}

/// 进程级状态库
static DB: OnceLock<Db> = OnceLock::new();

/// 数据目录里的状态库路径
fn default_path() -> std::path::PathBuf {
    crate::config::data_dir().join("state.db")
}

/// 初始化（幂等）。启动流程调用一次，其它地方用 [`db`]。
pub(crate) fn init() -> Result<(), DbError> {
    let db = Db::open(&default_path())?;
    // 已经初始化过就沿用既有的（重复调用不是错误）
    let _ = DB.set(db);
    Ok(())
}

/// 取状态库；未显式初始化时惰性打开
///
/// 惰性打开是为了让后台线程（admin）不必依赖启动顺序：拿到的永远是同一个库。
/// 配置尚未就绪时（单元测试、极早的调用）回落到内存库，而不是 panic——
/// 把启动顺序问题变成崩溃只会让调用方更难查。
///
/// **测试构建一律用内存库。** 否则一旦某个测试调用了 `config::init()`，
/// 全局库就变成真实文件：状态跨测试、跨**运行**累积，于是"断言某个计数
/// 等于 2"这类用例会在第十几次运行时失败（实测发生过）。另外测试也不该
/// 往仓库的 `data/` 目录里写状态库。文件路径由 `opens_a_file_backed_db`
/// 单独覆盖。
#[allow(clippy::expect_used)]
pub(crate) fn db() -> &'static Db {
    DB.get_or_init(|| {
        #[cfg(test)]
        {
            Db::open_in_memory().expect("内存状态库必须能打开")
        }
        #[cfg(not(test))]
        match crate::config::try_data_dir() {
            Some(dir) => Db::open(&dir.join("state.db")).unwrap_or_else(|error| {
                tracing::error!(%error, "db: 打开状态库失败，回落到内存库（本次运行的状态不会持久化）");
                Db::open_in_memory().expect("内存状态库必须能打开")
            }),
            None => Db::open_in_memory().expect("内存状态库必须能打开"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// schema 必须包含所有表。
    ///
    /// 加这条是因为我确实在一次编辑里误删过 `api_usage_total`（改动落在
    /// schema 字符串上，编译与其它测试都照常通过）。表名列表是显式的：
    /// 新增表时刻意要来这里加一行，而不是让"少了一张表"悄悄发生。
    #[test]
    fn schema_contains_every_expected_table() {
        let db = Db::open_in_memory().expect("内存库");
        for table in [
            "activation",
            "blocklist",
            "admin_audit",
            "emotion",
            "working_memory",
            "api_usage",
            "api_usage_total",
            "cognitive_biases",
            "attention_state",
            "quota_segment",
            "quota_segment_message",
            "quota_meta",
            "person",
            "relationship",
        ] {
            let found: i64 = db
                .with_conn(|conn| {
                    let count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        rusqlite::params![table],
                        |row| row.get(0),
                    )?;
                    Ok(count)
                })
                .expect("查询 sqlite_master");
            assert_eq!(found, 1, "schema 缺少表 {table}");
        }
    }

    /// 文件库路径必须可用，且**跨重开保持数据**。
    ///
    /// 全局 `db()` 在测试构建里一律走内存库（见其文档），所以文件路径
    /// 需要单独覆盖，否则 WAL 建库、写入、重开这一整条生产路径没有测试。
    #[test]
    fn opens_a_file_backed_db_and_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!(
            "ai_chat_db_test_{}_{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let path = dir.join("state.db");

        {
            let db = Db::open(&path).expect("打开文件库");
            db.set_activation(Scope::Group, 4242, true).expect("写入");
            db.set_emotion_state(7, r#"{"intensity":0.5}"#)
                .expect("写入");
        }

        // 重开：数据必须还在（WAL 已合并）
        let reopened = Db::open(&path).expect("重开文件库");
        assert!(
            reopened
                .activations(Scope::Group)
                .expect("查询")
                .contains(&4242)
        );
        assert_eq!(
            reopened.emotion_state(7).expect("查询").as_deref(),
            Some(r#"{"intensity":0.5}"#)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn activation_persists_and_reports_real_changes() {
        let db = Db::open_in_memory().expect("内存库");
        let scope = Scope::Group;
        let group = 12345;

        assert!(!db.activations(scope).expect("查询").contains(&group));
        assert!(db.set_activation(scope, group, true).expect("开启"));
        assert!(db.activations(scope).expect("查询").contains(&group));
        // 幂等：重复开启不再报告变化
        assert!(!db.set_activation(scope, group, true).expect("重复开启"));
        assert_eq!(db.activations(scope).expect("列表"), vec![group]);

        assert!(db.set_activation(scope, group, false).expect("关闭"));
        assert!(!db.activations(scope).expect("查询").contains(&group));
        assert!(!db.set_activation(scope, group, false).expect("重复关闭"));
        assert!(db.activations(scope).expect("列表").is_empty());
    }

    /// 作用域互不干扰：同一个 id 在私聊与群聊里是两件事
    #[test]
    fn scopes_do_not_leak_into_each_other() {
        let db = Db::open_in_memory().expect("内存库");
        let id = 777;

        db.set_activation(Scope::Private, id, true)
            .expect("私聊开启");
        assert!(db.activations(Scope::Private).expect("查询").contains(&id));
        assert!(
            !db.activations(Scope::Group).expect("查询").contains(&id),
            "私聊的开启不该让群聊也算开启"
        );
        assert!(db.activations(Scope::Group).expect("列群聊").is_empty());
    }

    #[test]
    fn blocklist_add_is_idempotent_and_removal_is_explicit() {
        let db = Db::open_in_memory().expect("内存库");

        assert!(db.add_blocked(42, "注入").expect("拉黑"));
        assert!(!db.add_blocked(42, "另一个原因").expect("重复拉黑"));
        assert_eq!(db.blocked_users().expect("列表"), vec![42]);
        assert_eq!(db.blocked_count().expect("计数"), 1);

        assert!(db.remove_blocked(42).expect("解禁"));
        assert!(!db.remove_blocked(42).expect("重复解禁"));
        assert_eq!(db.blocked_count().expect("计数"), 0);
    }

    /// 两个子系统的单例状态互不覆盖。
    ///
    /// 这是设计书 §6.2c 那个丢失更新的回归测试：它们曾经共用一个
    /// `cognitive_state.json`，各自「读整份 → 改自己那一半 → 写回整份」，
    /// 交错写入时后写的会把前一次改动整段抹掉。
    #[test]
    fn singleton_states_do_not_clobber_each_other() {
        let db = Db::open_in_memory().expect("内存库");

        db.set_singleton_state(SingletonState::CognitiveBiases, r#"{"recency":0.4}"#)
            .expect("写认知偏差");
        // 注意力随后写入：它不该带走认知偏差
        db.set_singleton_state(SingletonState::Attention, r#"{"focused_topic":"猫"}"#)
            .expect("写注意力");

        assert_eq!(
            db.singleton_state(SingletonState::CognitiveBiases)
                .expect("读认知偏差")
                .as_deref(),
            Some(r#"{"recency":0.4}"#),
            "写注意力不该抹掉认知偏差"
        );
        assert_eq!(
            db.singleton_state(SingletonState::Attention)
                .expect("读注意力")
                .as_deref(),
            Some(r#"{"focused_topic":"猫"}"#)
        );

        // 反向亦然：再写认知偏差不该动到注意力
        db.set_singleton_state(SingletonState::CognitiveBiases, r#"{"recency":0.9}"#)
            .expect("再写认知偏差");
        assert_eq!(
            db.singleton_state(SingletonState::Attention)
                .expect("读注意力")
                .as_deref(),
            Some(r#"{"focused_topic":"猫"}"#),
            "写认知偏差不该抹掉注意力"
        );
        assert_eq!(
            db.singleton_state(SingletonState::CognitiveBiases)
                .expect("读认知偏差")
                .as_deref(),
            Some(r#"{"recency":0.9}"#),
            "UPSERT 应当是覆盖而不是插入第二行"
        );
    }

    /// 没写过的子系统是 None（而不是空串或默认 JSON）
    #[test]
    fn missing_singleton_state_is_none() {
        let db = Db::open_in_memory().expect("内存库");
        assert!(
            db.singleton_state(SingletonState::Attention)
                .expect("读")
                .is_none()
        );
    }

    #[test]
    fn audit_records_what_changed_and_is_readable_newest_first() {
        let db = Db::open_in_memory().expect("内存库");

        db.record_audit(Actor::Admin, "set_activation", "group=1 enabled=true")
            .expect("审计");
        db.record_audit(Actor::Command, "blocklist_add", "user=9")
            .expect("审计");

        let entries = db.recent_audit(10).expect("读取审计");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "blocklist_add", "最新的排在最前");
        assert_eq!(entries[0].actor, "command");
        assert_eq!(entries[1].command, "set_activation");
        assert!(entries[1].created_at > 0, "审计必须带时间");
    }

    /// 情绪状态按用户一行：读一个用户不必碰其他用户的记录
    #[test]
    fn emotion_state_is_one_row_per_user() {
        let db = Db::open_in_memory().expect("内存库");

        assert!(db.emotion_state(1).expect("查询").is_none());
        assert_eq!(db.emotion_user_count().expect("计数"), 0);

        db.set_emotion_state(1, r#"{"intensity":0.5}"#)
            .expect("写入");
        assert_eq!(
            db.emotion_state(1).expect("查询").as_deref(),
            Some(r#"{"intensity":0.5}"#)
        );

        // UPSERT：同一用户再写是覆盖，不是插入第二行
        db.set_emotion_state(1, r#"{"intensity":0.9}"#)
            .expect("覆盖");
        assert_eq!(db.emotion_user_count().expect("计数"), 1);
        assert_eq!(
            db.emotion_state(1).expect("查询").as_deref(),
            Some(r#"{"intensity":0.9}"#)
        );

        // 另一个用户互不影响
        db.set_emotion_state(2, r#"{"intensity":0.1}"#)
            .expect("写入");
        assert_eq!(db.emotion_user_count().expect("计数"), 2);
        assert_eq!(
            db.emotion_state(1).expect("查询").as_deref(),
            Some(r#"{"intensity":0.9}"#),
            "写用户 2 不该动到用户 1 的那一行"
        );
    }

    /// 衰减批处理一次写入多个用户；空批次是无操作
    #[test]
    fn emotion_batch_write_covers_all_users() {
        let db = Db::open_in_memory().expect("内存库");
        db.set_emotion_state(1, "before").expect("写入");

        db.set_emotion_states(&[(1, "after".to_string()), (2, "new".to_string())])
            .expect("批量写入");

        assert_eq!(db.emotion_state(1).expect("查询").as_deref(), Some("after"));
        assert_eq!(db.emotion_state(2).expect("查询").as_deref(), Some("new"));
        assert_eq!(db.emotion_user_count().expect("计数"), 2);

        db.set_emotion_states(&[]).expect("空批次必须是无操作");
        assert_eq!(db.emotion_user_count().expect("计数"), 2);
    }
}
