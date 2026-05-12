use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::shared::CtxKey;

/// 消息批次 (合并短时间内连续消息)
pub struct MessageBatch {
    pub messages: String,
    pub last_update: Instant,
    /// 每条消息对应的写入时间戳 (unix秒)，用于精确匹配工作记忆条目
    pub record_timestamps: Vec<u64>,
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
    /// 各群组最近一次审查时间 (unix秒)
    pub last_review_times: HashMap<u64, u64>,
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
            last_review_times: HashMap::new(),
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
        if let Some(batch) = self.batches.get_mut(&key) {
            batch.messages.push('\n');
            batch.messages.push_str(message);
            batch.last_update = now;
            batch.record_timestamps.push(record_ts);
        } else {
            self.batches.insert(key, MessageBatch {
                messages: message.to_string(),
                last_update: now,
                record_timestamps: vec![record_ts],
            });
        }
    }

    pub fn take_expired_batch(&mut self, group_id: u64, user_id: u64, timeout_ms: u64) -> Option<MessageBatch> {
        let key: CtxKey = (group_id, user_id);
        let should_take = self.batches.get(&key).is_some_and(|batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&key)
        } else {
            None
        }
    }

    pub fn take_batch_for_processing(&mut self, group_id: u64, user_id: u64) -> Option<(String, Vec<u64>)> {
        let key: CtxKey = (group_id, user_id);
        self.batches.remove(&key).map(|batch| (batch.messages, batch.record_timestamps))
    }

    pub fn take_new_messages(&mut self, group_id: u64, user_id: u64, timeout_ms: u64) -> Option<String> {
        let key: CtxKey = (group_id, user_id);
        let should_take = self.batches.get(&key).is_some_and(|batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&key).map(|batch| batch.messages)
        } else {
            None
        }
    }

    /// 遗忘用户在指定上下文中的对话 (本地部分: batches)
    pub fn forget_context_local(&mut self, group_id: u64, user_id: u64) {
        let key: CtxKey = (group_id, user_id);
        self.batches.remove(&key);
    }

    /// 遗忘用户的所有对话 (本地部分: batches)
    pub fn forget_user_local(&mut self, user_id: u64) {
        self.batches.retain(|&(_, uid), _| uid != user_id);
    }
}
