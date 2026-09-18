use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::shared::CtxKey;

/// 消息批次 (合并短时间内连续消息)
pub struct MessageBatch {
    pub messages: String,
    pub last_update: Instant,
    /// 每条消息对应的写入时间戳 (unix秒)，用于精确匹配工作记忆条目
    pub entry_ids: Vec<u64>,
    /// 每条消息的到达时刻（unix 秒）——批次合并后用于还原真实发言顺序
    ///
    /// `Instant` 精确到亚秒但不可与其它时间来源比较，`entry_ids`
    /// 只有秒级精度且语义是"工作记忆写入时间"。跨用户排序需要一份
    /// 可比较、足够细的到达时刻，否则同一秒内两个人的话谁先谁后
    /// 只能靠 HashMap 迭代顺序决定。
    pub arrived_at: Vec<u64>,
    /// 每条消息的到达毫秒（用于把同一秒内的发言排出确定顺序）
    pub arrived_at_ms: Vec<u64>,
}

/// 从批次缓冲里取出的一批消息（交给群聊/私聊处理管线）
#[derive(Debug, Clone)]
pub struct TakenBatch {
    /// 合并后的消息文本
    pub messages: String,
    /// 每条消息对应的工作记忆写入时间戳（秒）
    pub entry_ids: Vec<u64>,
    /// 每条消息的到达时刻（秒）
    pub arrived_at: Vec<u64>,
    /// 每条消息的到达毫秒（排序用）
    pub arrived_at_ms: Vec<u64>,
}

impl TakenBatch {
    /// 这批消息最早到达的时刻（秒，找不到时取当前时间）
    pub fn first_arrival(&self) -> u64 {
        self.arrived_at
            .first()
            .copied()
            .unwrap_or_else(crate::util::now_secs)
    }

    /// 排序用的到达时刻（毫秒）
    pub fn sort_key_ms(&self) -> u64 {
        self.arrived_at_ms
            .first()
            .copied()
            .unwrap_or_else(crate::util::now_millis)
    }
}

// ── 批次缓冲：1ms 主循环线程独占 ──────────────────────────────

/// 待处理消息的批次缓冲
///
/// 只在主循环线程上读写：批次里存着 `Instant`，且"到期即取走"的语义
/// 依赖单一线程顺序。后台线程（admin HTTP、消息队列）不得触碰它——
/// 关闭一个对话不需要清空缓冲，主循环读门禁状态时自然会丢弃它。
#[derive(Default)]
pub struct BatchBuffer {
    batches: HashMap<CtxKey, MessageBatch>,
}

impl BatchBuffer {
    pub fn append(&mut self, group_id: u64, user_id: u64, message: &str, entry_id: u64) {
        let key: CtxKey = (group_id, user_id);
        let now = Instant::now();
        let arrived = crate::util::now_secs();
        let arrived_ms = crate::util::now_millis();
        if let Some(batch) = self.batches.get_mut(&key) {
            batch.messages.push('\n');
            batch.messages.push_str(message);
            batch.last_update = now;
            batch.entry_ids.push(entry_id);
            batch.arrived_at.push(arrived);
            batch.arrived_at_ms.push(arrived_ms);
        } else {
            self.batches.insert(
                key,
                MessageBatch {
                    messages: message.to_string(),
                    last_update: now,
                    entry_ids: vec![entry_id],
                    arrived_at: vec![arrived],
                    arrived_at_ms: vec![arrived_ms],
                },
            );
        }
    }

    /// 取出所有已到期、且未被 `is_busy` 认领的批次
    ///
    /// 一次性取出（而非逐个回调）是为了让调用方在**不持有缓冲借用**的情况下
    /// 继续做分发与投递。
    pub fn take_expired(
        &mut self,
        timeout_ms: u64,
        is_busy: impl Fn(CtxKey) -> bool,
    ) -> Vec<(CtxKey, TakenBatch)> {
        let expired: Vec<CtxKey> = self
            .batches
            .iter()
            .filter(|(_, batch)| batch.last_update.elapsed().as_millis() >= timeout_ms as u128)
            .map(|(&key, _)| key)
            .filter(|key| !is_busy(*key))
            .collect();

        expired
            .into_iter()
            .filter_map(|key| self.take(key).map(|taken| (key, taken)))
            .collect()
    }

    fn take(&mut self, key: CtxKey) -> Option<TakenBatch> {
        self.batches.remove(&key).map(|batch| TakenBatch {
            messages: batch.messages,
            entry_ids: batch.entry_ids,
            arrived_at: batch.arrived_at,
            arrived_at_ms: batch.arrived_at_ms,
        })
    }

    /// 丢弃某个用户的全部未处理批次
    pub fn forget_user(&mut self, user_id: u64) {
        self.batches.retain(|&(_, uid), _| uid != user_id);
    }

    /// 丢弃某个会话的未处理批次（关闭对话时用）
    pub fn forget_chat(&mut self, key: CtxKey) {
        self.batches.remove(&key);
    }
}

// ── 门禁状态：进程级，任何线程都可读写 ────────────────────────

/// "谁在对话、谁被拉黑"
///
/// 与 [`BatchBuffer`] 分开是因为两者的**所有者不同**：后台线程（admin HTTP、
/// 管理命令）必须能改门禁状态并且立刻生效，而批次缓冲只属于主循环线程。
/// 早先两者共用一个 `thread_local`，于是后台改的永远是**自己线程的副本**——
/// 对话开关从未生效过，拉黑/解禁也要重启才生效。
pub struct GateState {
    active: HashSet<u64>,
    active_groups: HashSet<u64>,
    blacklist: HashSet<u64>,
}

impl GateState {
    /// 从状态库载入门禁快照
    ///
    /// 激活与黑名单都是**持久**状态：先前它们只活在内存里（黑名单另有一个
    /// `blocklist.json` 作为第二份真相），于是后台开的对话重启即失。
    /// 读库失败时回落到空集合：门禁拿不到名单时应当拒绝放行，
    /// 而不是把所有人当成已授权。
    pub fn load() -> Self {
        let db = crate::db::db();
        let active = db
            .activations(crate::db::Scope::Private)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "gate: 读取活跃私聊失败，按空集合处理");
                Vec::new()
            });
        let active_groups = db
            .activations(crate::db::Scope::Group)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "gate: 读取活跃群聊失败，按空集合处理");
                Vec::new()
            });
        let blacklist = db.blocked_users().unwrap_or_else(|error| {
            tracing::warn!(%error, "gate: 读取黑名单失败，按空集合处理");
            Vec::new()
        });

        Self {
            active: active.into_iter().collect(),
            active_groups: active_groups.into_iter().collect(),
            blacklist: blacklist.into_iter().collect(),
        }
    }

    pub fn is_private_active(&self, user_id: u64) -> bool {
        self.active.contains(&user_id)
    }

    /// 返回状态是否真的发生了变化
    pub fn set_private_active(&mut self, user_id: u64, enabled: bool) -> bool {
        if enabled {
            self.active.insert(user_id)
        } else {
            self.active.remove(&user_id)
        }
    }

    pub fn is_group_active(&self, group_id: u64) -> bool {
        self.active_groups.contains(&group_id)
    }

    /// 返回状态是否真的发生了变化
    pub fn set_group_active(&mut self, group_id: u64, enabled: bool) -> bool {
        if enabled {
            self.active_groups.insert(group_id)
        } else {
            self.active_groups.remove(&group_id)
        }
    }

    pub fn is_blacklisted(&self, user_id: u64) -> bool {
        self.blacklist.contains(&user_id)
    }

    /// 返回状态是否真的发生了变化
    pub fn set_blacklisted(&mut self, user_id: u64, blocked: bool) -> bool {
        if blocked {
            self.blacklist.insert(user_id)
        } else {
            self.blacklist.remove(&user_id)
        }
    }

    pub fn active_users(&self) -> impl Iterator<Item = u64> + '_ {
        self.active.iter().copied()
    }

    pub fn active_groups(&self) -> impl Iterator<Item = u64> + '_ {
        self.active_groups.iter().copied()
    }

    pub fn blacklisted(&self) -> impl Iterator<Item = u64> + '_ {
        self.blacklist.iter().copied()
    }
}
