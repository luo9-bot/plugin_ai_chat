use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// 用户对话状态
pub struct UserContext {
    /// 对话历史 (role, content)
    pub history: Vec<(String, String)>,
    /// 当前情绪标签 (由 AI 回复中解析)
    pub emotion: String,
}

/// 上下文键: (group_id, user_id)
/// 私聊: (0, user_id)，群聊: (group_id, user_id)
pub type CtxKey = (u64, u64);

// ── 跨线程共享状态 ────────────────────────────────────────────

/// 跨线程共享状态，由 RwLock 保护。
/// 包含 spawned 线程需要读写的字段。
pub struct SharedState {
    /// 机器人在各群最近发出的消息 (group_id → [(message, time)])
    recent_bot_messages: HashMap<u64, Vec<(String, Instant)>>,
    /// 机器人上次回复时间 ((group_id, user_id) → Instant)
    /// group_id=0 表示私聊，(group_id, 0) 表示群级对话状态
    last_reply_times: HashMap<(u64, u64), Instant>,
    /// 用户对话上下文 (按 (group_id, user_id) 隔离)
    pub contexts: HashMap<CtxKey, UserContext>,
    /// 各群组最近一次消息处理时间 (unix秒)
    pub last_conversation_times: HashMap<u64, u64>,
    /// 已触发过对话后反思的群组，新消息到达时清除
    pub reflected_groups: HashSet<u64>,
    /// 活跃群聊集合 (由主线程同步，供管理线程读取)
    pub active_groups: HashSet<u64>,
    /// 活跃私聊用户集合 (由主线程同步，供管理线程读取)
    pub active_users: HashSet<u64>,
    /// 各群组最近审查到的工作记忆时间戳 (unix秒)
    pub last_reviewed_timestamps: HashMap<u64, u64>,
    /// 各群组上次反思时的对话内容 (标准化后)
    pub last_reflected_content: HashMap<u64, String>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            recent_bot_messages: HashMap::new(),
            last_reply_times: HashMap::new(),
            contexts: HashMap::new(),
            last_conversation_times: HashMap::new(),
            reflected_groups: HashSet::new(),
            active_groups: HashSet::new(),
            active_users: HashSet::new(),
            last_reviewed_timestamps: HashMap::new(),
            last_reflected_content: HashMap::new(),
        }
    }

    /// 同步活跃对话状态（由主线程调用）
    pub fn sync_active(&mut self, groups: &HashSet<u64>, users: &HashSet<u64>) {
        self.active_groups = groups.clone();
        self.active_users = users.clone();
    }

    /// 记录机器人回复了某用户 (group_id=0 表示私聊)
    /// 群聊时同时记录群级状态 (group_id, 0)
    pub fn record_reply(&mut self, group_id: u64, user_id: u64) {
        if group_id > 0 {
            self.last_reply_times.insert((group_id, 0), Instant::now());
        }
        self.last_reply_times.insert((group_id, user_id), Instant::now());
    }

    /// 检查是否在对话跟进时间内
    pub fn is_in_follow_up(&self, group_id: u64, user_id: u64, timeout_secs: u64) -> bool {
        if group_id > 0 {
            self.last_reply_times
                .get(&(group_id, 0))
                .map(|t| t.elapsed().as_secs() < timeout_secs)
                .unwrap_or(false)
        } else {
            self.last_reply_times
                .get(&(0, user_id))
                .map(|t| t.elapsed().as_secs() < timeout_secs)
                .unwrap_or(false)
        }
    }

    /// 记录机器人在群里发出的消息
    pub fn record_bot_message(&mut self, group_id: u64, message: &str) {
        if group_id == 0 { return; }
        let entry = self.recent_bot_messages.entry(group_id).or_default();
        entry.push((message.to_string(), Instant::now()));
        if entry.len() > 10 {
            entry.remove(0);
        }
    }

    /// 获取机器人在某群最近的消息 (返回 owned String，因为无法跨锁返回引用)
    pub fn get_recent_bot_messages(&self, group_id: u64, max_age_secs: u64, max_count: usize) -> Vec<String> {
        self.recent_bot_messages
            .get(&group_id)
            .map(|msgs| {
                msgs.iter()
                    .rev()
                    .filter(|(_, t)| t.elapsed().as_secs() < max_age_secs)
                    .take(max_count)
                    .map(|(msg, _)| msg.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取或创建用户上下文
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
        while ctx.history.len() > max_pairs * 2 {
            ctx.history.remove(0);
        }
    }

    /// 克隆用户对话历史 (用于跨锁返回)
    pub fn get_history_clone(&self, group_id: u64, user_id: u64) -> Vec<(String, String)> {
        let key: CtxKey = (group_id, user_id);
        self.contexts.get(&key)
            .map(|ctx| ctx.history.clone())
            .unwrap_or_default()
    }

    /// 记录群组最近对话时间，并清除反思标记
    pub fn record_conversation(&mut self, group_id: u64, timestamp: u64) {
        self.last_conversation_times.insert(group_id, timestamp);
        self.reflected_groups.remove(&group_id);
    }

    /// 返回空闲超过指定秒数且未反思过的群组列表
    pub fn get_idle_groups(&self, now: u64, idle_secs: u64) -> Vec<u64> {
        self.last_conversation_times.iter()
            .filter(|(gid, last_time)| {
                **gid > 0
                    && now.saturating_sub(**last_time) >= idle_secs
                    && !self.reflected_groups.contains(*gid)
            })
            .map(|(gid, _)| *gid)
            .collect()
    }

    /// 遗忘用户在指定上下文中的对话 (共享部分)
    pub fn forget_context_shared(&mut self, group_id: u64, user_id: u64) {
        let key: CtxKey = (group_id, user_id);
        self.contexts.remove(&key);
        self.last_reply_times.remove(&(group_id, user_id));
    }

    /// 获取上次回复某用户的时间 (用于冷却检查)
    pub fn last_reply_to_user(&self, group_id: u64, user_id: u64) -> Option<Instant> {
        self.last_reply_times.get(&(group_id, user_id)).copied()
    }

    /// 遗忘用户的所有对话 (共享部分)
    pub fn forget_user_shared(&mut self, user_id: u64) {
        self.contexts.retain(|&(_, uid), _| uid != user_id);
        self.last_reply_times.retain(|&(_, uid), _| uid != user_id);
    }
}

/// 返回长时间对话中需要定期审查的群组 (自由函数，需要跨 State/SharedState 数据)
pub fn get_groups_needing_review(
    conversation_times: &HashMap<u64, u64>,
    review_times: &HashMap<u64, u64>,
    now: u64,
    review_interval: u64,
    max_idle: u64,
) -> Vec<u64> {
    conversation_times.iter()
        .filter(|(gid, last_time)| {
            **gid > 0
                && now.saturating_sub(**last_time) < max_idle
                && review_times.get(*gid)
                    .is_none_or(|&last_review| now.saturating_sub(last_review) >= review_interval)
        })
        .map(|(gid, _)| *gid)
        .collect()
}
