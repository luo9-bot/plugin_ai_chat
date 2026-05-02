use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// 用户对话状态
pub struct UserContext {
    /// 对话历史 (role, content)
    pub history: Vec<(String, String)>,
    /// 当前情绪标签 (由 AI 回复中解析)
    pub emotion: String,
}

/// 消息批次 (合并短时间内连续消息)
pub struct MessageBatch {
    pub messages: String,
    pub last_update: Instant,
}

/// 上下文键: (group_id, user_id)
/// 私聊: (0, user_id)，群聊: (group_id, user_id)
pub type CtxKey = (u64, u64);

/// 全局状态
pub struct State {
    /// 私聊活跃用户集合 (user_id)
    pub active: HashSet<u64>,
    /// 群聊活跃群组集合 (group_id)，管理员开启后整个群可聊
    pub active_groups: HashSet<u64>,
    /// 黑名单用户集合 (user_id)，完全忽略消息和记忆
    pub blacklist: HashSet<u64>,
    /// 用户对话上下文 (按 (group_id, user_id) 隔离)
    pub contexts: HashMap<CtxKey, UserContext>,
    /// 消息批次缓冲 (按 (group_id, user_id) 隔离)
    pub batches: HashMap<CtxKey, MessageBatch>,
    /// 机器人上次回复时间 (group_id, user_id) → Instant，用于对话跟进判断
    /// group_id=0 表示私聊
    pub last_reply_times: HashMap<(u64, u64), Instant>,
    /// 机器人在各群最近发出的消息 (group_id → [(message, time)])，用于 AI 回复决策
    recent_bot_messages: HashMap<u64, Vec<(String, Instant)>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            active: HashSet::new(),
            active_groups: HashSet::new(),
            blacklist: crate::blocklist::load(),
            contexts: HashMap::new(),
            batches: HashMap::new(),
            last_reply_times: HashMap::new(),
            recent_bot_messages: HashMap::new(),
        }
    }

    /// 检查用户是否在黑名单中
    pub fn is_blacklisted(&self, user_id: u64) -> bool {
        self.blacklist.contains(&user_id)
    }

    /// 加入黑名单
    pub fn add_blacklist(&mut self, user_id: u64) {
        self.blacklist.insert(user_id);
        crate::blocklist::save(&self.blacklist);
    }

    /// 移除黑名单
    pub fn remove_blacklist(&mut self, user_id: u64) {
        self.blacklist.remove(&user_id);
        crate::blocklist::save(&self.blacklist);
    }

    /// 记录机器人回复了某用户 (group_id=0 表示私聊)
    /// 群聊时同时记录群级状态 (group_id, 0)，表示机器人正在该群参与对话
    pub fn record_reply(&mut self, group_id: u64, user_id: u64) {
        if group_id > 0 {
            // 群聊: 记录群级对话状态 (任何人可以接话)
            self.last_reply_times.insert((group_id, 0), Instant::now());
        }
        self.last_reply_times.insert((group_id, user_id), Instant::now());
    }

    /// 检查是否在对话跟进时间内
    /// 群聊: 检查 (group_id, 0) — 机器人刚在群里回过话，任何人都可以接话
    /// 私聊: 检查 (0, user_id)
    pub fn is_in_follow_up(&self, group_id: u64, user_id: u64, timeout_secs: u64) -> bool {
        if group_id > 0 {
            // 群聊: 检查群级对话状态
            self.last_reply_times
                .get(&(group_id, 0))
                .map(|t| t.elapsed().as_secs() < timeout_secs)
                .unwrap_or(false)
        } else {
            // 私聊: 检查用户级
            self.last_reply_times
                .get(&(0, user_id))
                .map(|t| t.elapsed().as_secs() < timeout_secs)
                .unwrap_or(false)
        }
    }

    /// 追加消息到用户批次 (按 (group_id, user_id) 隔离)
    pub fn append_batch(&mut self, group_id: u64, user_id: u64, message: &str) {
        let key: CtxKey = (group_id, user_id);
        let now = Instant::now();
        if let Some(batch) = self.batches.get_mut(&key) {
            batch.messages.push('\n');
            batch.messages.push_str(message);
            batch.last_update = now;
        } else {
            self.batches.insert(key, MessageBatch {
                messages: message.to_string(),
                last_update: now,
            });
        }
    }

    /// 检查并取出已超时的批次
    pub fn take_expired_batch(&mut self, group_id: u64, user_id: u64, timeout_ms: u64) -> Option<MessageBatch> {
        let key: CtxKey = (group_id, user_id);
        let should_take = self.batches.get(&key).map_or(false, |batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&key)
        } else {
            None
        }
    }

    /// 取出批次消息用于处理
    /// 在 AI 处理期间如果有新消息到来，会追加到同一个槽位
    pub fn take_batch_for_processing(&mut self, group_id: u64, user_id: u64) -> Option<String> {
        let key: CtxKey = (group_id, user_id);
        self.batches.remove(&key).map(|batch| batch.messages)
    }

    /// 检查并取出处理期间新到达的消息
    pub fn take_new_messages(&mut self, group_id: u64, user_id: u64, timeout_ms: u64) -> Option<String> {
        let key: CtxKey = (group_id, user_id);
        let should_take = self.batches.get(&key).map_or(false, |batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&key).map(|batch| batch.messages)
        } else {
            None
        }
    }

    /// 获取或创建用户上下文 (按 (group_id, user_id) 隔离)
    pub fn get_or_create_context(&mut self, group_id: u64, user_id: u64) -> &mut UserContext {
        let key: CtxKey = (group_id, user_id);
        self.contexts.entry(key).or_insert(UserContext {
            history: Vec::new(),
            emotion: String::new(),
        })
    }

    /// 向用户历史追加一条消息，并保持窗口大小
    pub fn push_history(&mut self, group_id: u64, user_id: u64, role: &str, content: &str, max_pairs: usize) {
        let ctx = self.get_or_create_context(group_id, user_id);
        ctx.history.push((role.to_string(), content.to_string()));
        // 滑动窗口: 保留最近 max_pairs 对 (user + assistant)
        while ctx.history.len() > max_pairs * 2 {
            ctx.history.remove(0);
        }
    }

    /// 记录机器人在群里发出的消息 (用于 AI 回复决策上下文)
    pub fn record_bot_message(&mut self, group_id: u64, message: &str) {
        if group_id == 0 { return; }
        let entry = self.recent_bot_messages.entry(group_id).or_default();
        entry.push((message.to_string(), Instant::now()));
        // 只保留最近 10 条
        if entry.len() > 10 {
            entry.remove(0);
        }
    }

    /// 获取机器人在某群最近的消息 (最多返回最近 N 条，时间在 max_age 秒内)
    pub fn get_recent_bot_messages(&self, group_id: u64, max_age_secs: u64, max_count: usize) -> Vec<&str> {
        self.recent_bot_messages
            .get(&group_id)
            .map(|msgs| {
                msgs.iter()
                    .rev()
                    .filter(|(_, t)| t.elapsed().as_secs() < max_age_secs)
                    .take(max_count)
                    .map(|(msg, _)| msg.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    /// 遗忘用户在指定上下文中的对话
    pub fn forget_context(&mut self, group_id: u64, user_id: u64) {
        let key: CtxKey = (group_id, user_id);
        self.contexts.remove(&key);
        self.batches.remove(&key);
        self.last_reply_times.remove(&(group_id, user_id));
    }

    /// 遗忘用户的所有对话 (所有群聊 + 私聊上下文)
    pub fn forget_user(&mut self, user_id: u64) {
        self.contexts.retain(|&(_, uid), _| uid != user_id);
        self.batches.retain(|&(_, uid), _| uid != user_id);
        self.last_reply_times.retain(|&(_, uid), _| uid != user_id);
    }
}
