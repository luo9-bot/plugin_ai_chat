use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::shared::CtxKey;

/// 消息批次 (合并短时间内连续消息)
pub struct MessageBatch {
    pub messages: String,
    pub last_update: Instant,
    /// 每条消息对应的写入时间戳 (unix秒)，用于精确匹配工作记忆条目
    pub record_timestamps: Vec<u64>,
    /// 每条消息的到达时刻（unix 秒）——批次合并后用于还原真实发言顺序
    ///
    /// `Instant` 精确到亚秒但不可与其它时间来源比较，`record_timestamps`
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
    pub record_timestamps: Vec<u64>,
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

// ── 主线程本地状态 ────────────────────────────────────────────

/// 主线程本地状态 (thread_local)，仅包含主线程使用的字段
pub struct State {
    /// 私聊活跃用户集合 (user_id)
    pub active: HashSet<u64>,
    /// 群聊活跃群组集合 (group_id)
    pub active_groups: HashSet<u64>,
    /// 黑名单用户集合 (user_id)
    pub blacklist: HashSet<u64>,
    /// 消息批次缓冲 (按 (group_id, user_id) 隔离)
    pub batches: HashMap<CtxKey, MessageBatch>,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            active: HashSet::new(),
            active_groups: HashSet::new(),
            blacklist: crate::blocklist::load(),
            batches: HashMap::new(),
        }
    }

    pub fn is_blacklisted(&self, user_id: u64) -> bool {
        self.blacklist.contains(&user_id)
    }

    pub fn add_blacklist(&mut self, user_id: u64) {
        self.blacklist.insert(user_id);
        crate::blocklist::save(&self.blacklist);
    }

    pub fn remove_blacklist(&mut self, user_id: u64) {
        self.blacklist.remove(&user_id);
        crate::blocklist::save(&self.blacklist);
    }

    pub fn append_batch(&mut self, group_id: u64, user_id: u64, message: &str, record_ts: u64) {
        let key: CtxKey = (group_id, user_id);
        let now = Instant::now();
        let arrived = crate::util::now_secs();
        let arrived_ms = crate::util::now_millis();
        if let Some(batch) = self.batches.get_mut(&key) {
            batch.messages.push('\n');
            batch.messages.push_str(message);
            batch.last_update = now;
            batch.record_timestamps.push(record_ts);
            batch.arrived_at.push(arrived);
            batch.arrived_at_ms.push(arrived_ms);
        } else {
            self.batches.insert(
                key,
                MessageBatch {
                    messages: message.to_string(),
                    last_update: now,
                    record_timestamps: vec![record_ts],
                    arrived_at: vec![arrived],
                    arrived_at_ms: vec![arrived_ms],
                },
            );
        }
    }

    pub fn take_batch_for_processing(&mut self, group_id: u64, user_id: u64) -> Option<TakenBatch> {
        let key: CtxKey = (group_id, user_id);
        self.batches.remove(&key).map(|batch| TakenBatch {
            messages: batch.messages,
            record_timestamps: batch.record_timestamps,
            arrived_at: batch.arrived_at,
            arrived_at_ms: batch.arrived_at_ms,
        })
    }

    /// 遗忘用户的所有对话 (本地部分: batches)
    pub fn forget_user_local(&mut self, user_id: u64) {
        self.batches.retain(|&(_, uid), _| uid != user_id);
    }
}
