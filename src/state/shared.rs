use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// 用户对话状态
pub struct UserContext {
    /// 对话历史 (role, content)
    pub history: Vec<(String, String)>,
    /// 当前情绪标签 (由 AI 回复中解析)
    pub emotion: String,
    /// AI 生成的历史摘要（当历史过长时使用），记录重要内容
    pub conversation_summary: String,
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
    /// 群级对话历史 (group_id → history)，用于群聊中跨用户共享上下文
    pub group_history: HashMap<u64, Vec<(String, String)>>,
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
            group_history: HashMap::new(),
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
            conversation_summary: String::new(),
        })
    }

    /// 向用户历史追加一条消息。
    /// 超出窗口大小时，用 AI 对最早的消息做摘要压缩（而非简单丢弃），
    /// 保留重要信息的同时控制 token 消耗。
    pub fn push_history(&mut self, group_id: u64, user_id: u64, role: &str, content: &str, max_pairs: usize) {
        let ctx = self.get_or_create_context(group_id, user_id);
        ctx.history.push((role.to_string(), content.to_string()));
        if ctx.history.len() > max_pairs * 2 {
            // 窗口过大时，取最早的一半消息请求 AI 压缩为摘要
            let keep_recent = max_pairs as usize;
            let compress_end = ctx.history.len().saturating_sub(keep_recent);
            if compress_end > 2 {
                let to_compress: Vec<String> = ctx.history[..compress_end].iter()
                    .map(|(r, c)| format!("[{}] {}", r, c))
                    .collect();
                let prompt_text = to_compress.join("\n");
                // 异步 AI 摘要：如果已有历史摘要，合并后再压缩
                let existing_summary = ctx.conversation_summary.clone();
                let final_prompt = if existing_summary.is_empty() {
                    prompt_text
                } else {
                    format!("之前的摘要：{}\n\n新的对话：\n{}", existing_summary, prompt_text)
                };
                // 调用 AI 生成摘要（异步触发，不阻塞）
                let compressed = crate::ai::analyze(
                    crate::prompt::PromptManager::get().raw("history_attention"),
                    &final_prompt,
                );
                if let Ok(summary) = compressed {
                    if let Some(json_str) = crate::ai::extract_json(&summary) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                            if let Some(s) = val.get("summary").and_then(|v| v.as_str()) {
                                ctx.conversation_summary = s.to_string();
                            }
                        }
                    }
                }
                // 删除已压缩的部分，保留最近的消息
                ctx.history.drain(..compress_end);
            } else {
                // 只有少量溢出，简单丢弃最旧条目
                ctx.history.remove(0);
            }
        }
    }

    /// 克隆用户对话历史 (用于跨锁返回)
    pub fn get_history_clone(&self, group_id: u64, user_id: u64) -> Vec<(String, String)> {
        let key: CtxKey = (group_id, user_id);
        self.contexts.get(&key)
            .map(|ctx| ctx.history.clone())
            .unwrap_or_default()
    }

    /// 向群级历史追加一条消息（群聊中跨用户可见）
    pub fn push_group_history(&mut self, group_id: u64, role: &str, content: &str, max_pairs: usize) {
        if group_id == 0 { return; }
        let history = self.group_history.entry(group_id).or_default();
        history.push((role.to_string(), content.to_string()));
        while history.len() > max_pairs * 2 {
            history.remove(0);
        }
    }

    /// 克隆群级历史
    pub fn get_group_history_clone(&self, group_id: u64) -> Vec<(String, String)> {
        if group_id == 0 { return vec![]; }
        self.group_history.get(&group_id)
            .cloned()
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
