use std::collections::HashMap;
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
    pub group_id: u64,
    pub last_update: Instant,
}

/// 全局状态
pub struct State {
    /// 私聊活跃用户集合 (user_id)
    pub active: std::collections::HashSet<u64>,
    /// 群聊活跃群组集合 (group_id)，管理员开启后整个群可聊
    pub active_groups: std::collections::HashSet<u64>,
    /// 用户对话上下文
    pub contexts: HashMap<u64, UserContext>,
    /// 消息批次缓冲
    pub batches: HashMap<u64, MessageBatch>,
    /// 机器人上次回复时间 (group_id, user_id) → Instant，用于对话跟进判断
    /// group_id=0 表示私聊
    pub last_reply_times: HashMap<(u64, u64), Instant>,
    /// 机器人在各群最近发出的消息 (group_id → [(message, time)])，用于 AI 回复决策
    recent_bot_messages: HashMap<u64, Vec<(String, Instant)>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            active: std::collections::HashSet::new(),
            active_groups: std::collections::HashSet::new(),
            contexts: HashMap::new(),
            batches: HashMap::new(),
            last_reply_times: HashMap::new(),
            recent_bot_messages: HashMap::new(),
        }
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

    /// 追加消息到用户批次
    pub fn append_batch(&mut self, user_id: u64, group_id: u64, message: &str) {
        let now = Instant::now();
        if let Some(batch) = self.batches.get_mut(&user_id) {
            batch.messages.push('\n');
            batch.messages.push_str(message);
            batch.group_id = group_id;
            batch.last_update = now;
        } else {
            self.batches.insert(user_id, MessageBatch {
                messages: message.to_string(),
                group_id,
                last_update: now,
            });
        }
    }

    /// 检查并取出已超时的批次
    pub fn take_expired_batch(&mut self, user_id: u64, timeout_ms: u64) -> Option<MessageBatch> {
        let should_take = self.batches.get(&user_id).map_or(false, |batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&user_id)
        } else {
            None
        }
    }

    /// 取出批次消息用于处理，但保留槽位
    /// 在 AI 处理期间如果有新消息到来，会追加到同一个槽位
    pub fn take_batch_for_processing(&mut self, user_id: u64) -> Option<String> {
        self.batches.remove(&user_id).map(|batch| batch.messages)
    }

    /// 检查并取出处理期间新到达的消息
    pub fn take_new_messages(&mut self, user_id: u64, timeout_ms: u64) -> Option<String> {
        let should_take = self.batches.get(&user_id).map_or(false, |batch| {
            batch.last_update.elapsed().as_millis() >= timeout_ms as u128
        });
        if should_take {
            self.batches.remove(&user_id).map(|batch| batch.messages)
        } else {
            None
        }
    }

    /// 获取或创建用户上下文
    pub fn get_or_create_context(&mut self, user_id: u64) -> &mut UserContext {
        self.contexts.entry(user_id).or_insert(UserContext {
            history: Vec::new(),
            emotion: String::new(),
        })
    }

    /// 向用户历史追加一条消息，并保持窗口大小
    pub fn push_history(&mut self, user_id: u64, role: &str, content: &str, max_pairs: usize) {
        let ctx = self.get_or_create_context(user_id);
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

    /// 遗忘用户对话
    pub fn forget_user(&mut self, user_id: u64) {
        self.contexts.remove(&user_id);
        self.batches.remove(&user_id);
        self.last_reply_times.retain(|&(_, uid), _| uid != user_id);
    }
}
