use std::collections::VecDeque;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// 细粒度信誉系统
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    /// 内容信誉：因违规内容降低
    pub content: f32,
    /// 频率信誉：因刷屏降低（rate limit 不直接降低此值）
    pub spam: f32,
    /// 信任信誉：综合信任度
    pub trust: f32,
}

impl Default for Reputation {
    fn default() -> Self {
        Self { content: 1.0, spam: 1.0, trust: 1.0 }
    }
}

impl Reputation {
    /// 综合信誉分
    pub fn combined(&self) -> f32 {
        (self.content * 0.5 + self.spam * 0.2 + self.trust * 0.3).clamp(0.0, 1.0)
    }

    /// 惩罚系数（基于内容信誉，更敏感）
    pub fn penalty_multiplier(&self) -> f32 {
        let c = self.content;
        if c >= 0.9 { 1.0 }
        else if c >= 0.7 { 1.5 }
        else if c >= 0.5 { 2.5 }
        else if c >= 0.3 { 4.0 }
        else { 6.0 }
    }
}

/// 上下文消息（带时间戳）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub content: String,
    pub timestamp: u64,
}

/// 用户行为记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBehavior {
    pub message_times: Vec<u64>,
    pub recent_messages: VecDeque<ContextMessage>,
    pub reputation: Reputation,
    pub violation_count: u32,
    pub high_severity_count: u32,
    pub last_violation: Option<u64>,
    pub banned: bool,
    pub silent_banned: bool,
    pub vision_disabled: bool,
    pub severity_score: f32,
}

impl Default for UserBehavior {
    fn default() -> Self {
        Self {
            message_times: Vec::new(),
            recent_messages: VecDeque::with_capacity(10),
            reputation: Reputation::default(),
            violation_count: 0,
            high_severity_count: 0,
            last_violation: None,
            banned: false,
            silent_banned: false,
            vision_disabled: false,
            severity_score: 0.0,
        }
    }
}

impl UserBehavior {
    fn cleanup_old_timestamps(&mut self) {
        let one_hour_ago = crate::util::now_secs().saturating_sub(3600);
        self.message_times.retain(|t| *t > one_hour_ago);
    }

    pub fn record_message(&mut self, normalized: &str) {
        self.message_times.push(crate::util::now_secs());
        self.cleanup_old_timestamps();
        self.recent_messages.push_back(ContextMessage {
            content: normalized.to_string(),
            timestamp: crate::util::now_secs(),
        });
        if self.recent_messages.len() > 8 {
            self.recent_messages.pop_front();
        }
    }

    pub fn messages_last_minute(&self) -> u32 {
        let one_minute_ago = crate::util::now_secs().saturating_sub(60);
        self.message_times.iter().filter(|t| **t > one_minute_ago).count() as u32
    }

    pub fn messages_last_hour(&self) -> u32 {
        self.message_times.len() as u32
    }

    /// 记录违规（内容类）
    pub fn record_violation(&mut self, severity: f32) {
        self.violation_count += 1;
        self.last_violation = Some(crate::util::now_secs());
        self.severity_score += severity;
        if severity >= 3.0 {
            self.high_severity_count += 1;
        }
        // 内容信誉下降：高严重度违规立即产生显著惩罚
        let base_penalty = 0.15 * severity;
        let repeat_factor = 1.0 + self.violation_count as f32 * 0.2;
        let penalty = base_penalty * repeat_factor;
        self.reputation.content = (self.reputation.content - penalty).max(0.0);
        self.reputation.trust = (self.reputation.trust - penalty * 0.6).max(0.0);
    }

    /// 记录频率违规（不降低 content reputation）
    pub fn record_rate_limit(&mut self) {
        self.reputation.spam = (self.reputation.spam - 0.05).max(0.0);
    }

    /// 信誉恢复（允许完全恢复，但速度较慢）
    pub fn recover_reputation(&mut self) {
        if let Some(last) = self.last_violation {
            let elapsed = crate::util::now_secs().saturating_sub(last) as f32;
            let recovery = (elapsed / 7200.0) * 0.01; // 每2小时恢复1%
            self.reputation.content = (self.reputation.content + recovery).min(1.0);
            self.reputation.trust = (self.reputation.trust + recovery * 0.5).min(1.0);
        }
        self.reputation.spam = (self.reputation.spam + 0.01).min(1.0);
        self.severity_score = (self.severity_score - 0.1).max(0.0);
    }

    /// 是否应该静默封禁
    pub fn should_silent_ban(&self) -> bool {
        self.reputation.content < 0.3
            && self.high_severity_count >= 2
    }
}

// ── 磁盘持久化 ──────────────────────────────────────────────────

fn behaviors_path() -> std::path::PathBuf {
    crate::config::data_dir().join("user_behaviors.json")
}

fn correlators_path() -> std::path::PathBuf {
    crate::config::data_dir().join("context_correlators.json")
}

/// 上下文关联器（可序列化版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableCorrelator {
    pub messages: VecDeque<ContextMessage>,
    pub max_messages: usize,
    pub max_age_secs: u64,
}

impl SerializableCorrelator {
    pub fn new(max_messages: usize, max_age_secs: u64) -> Self {
        Self {
            messages: VecDeque::with_capacity(max_messages),
            max_messages,
            max_age_secs,
        }
    }

    pub fn record(&mut self, content: &str) {
        self.messages.push_back(ContextMessage {
            content: content.to_string(),
            timestamp: crate::util::now_secs(),
        });
        if self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }
        self.cleanup();
    }

    fn cleanup(&mut self) {
        let cutoff = crate::util::now_secs().saturating_sub(self.max_age_secs);
        while self.messages.front().is_some_and(|m| m.timestamp < cutoff) {
            self.messages.pop_front();
        }
    }

    pub fn get_all_views(&self) -> Vec<String> {
        let mut all = Vec::new();
        // preserved
        for m in &self.messages {
            all.push(m.content.clone());
        }
        // stripped
        if !self.messages.is_empty() {
            let merged: String = self.messages.iter().map(|m| m.content.as_str()).collect();
            all.push(merged);
        }
        // rolling
        let msgs: Vec<&str> = self.messages.iter().map(|m| m.content.as_str()).collect();
        let len = msgs.len();
        if len >= 2 {
            for i in 0..len - 1 {
                all.push(format!("{}{}", msgs[i], msgs[i + 1]));
            }
        }
        if len >= 3 {
            for i in 0..len - 2 {
                all.push(format!("{}{}{}", msgs[i], msgs[i + 1], msgs[i + 2]));
            }
        }
        if len >= 2 {
            let merged: String = msgs.iter().copied().collect();
            all.push(merged);
        }
        // token continuity
        let mut merge_buf = String::new();
        let mut in_run = false;
        for msg in &msgs {
            let char_count = msg.chars().count();
            if char_count <= 2 {
                merge_buf.push_str(msg);
                in_run = true;
            } else {
                if in_run && merge_buf.chars().count() >= 2 {
                    all.push(merge_buf.clone());
                }
                merge_buf.clear();
                in_run = false;
            }
        }
        if in_run && merge_buf.chars().count() >= 2 {
            all.push(merge_buf);
        }
        all
    }
}

fn load_behaviors() -> HashMap<u64, UserBehavior> {
    let path = behaviors_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_behaviors(map: &HashMap<u64, UserBehavior>) {
    let path = behaviors_path();
    if let Ok(json) = serde_json::to_string_pretty(map) {
        std::fs::write(path, json).ok();
    }
}

fn load_correlators() -> HashMap<u64, SerializableCorrelator> {
    let path = correlators_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_correlators(map: &HashMap<u64, SerializableCorrelator>) {
    let path = correlators_path();
    if let Ok(json) = serde_json::to_string_pretty(map) {
        std::fs::write(path, json).ok();
    }
}

/// 获取用户行为记录（可变引用）— 按需加载，修改后写回磁盘
pub fn with_behavior_mut<F, R>(user_id: u64, f: F) -> R
where
    F: FnOnce(&mut UserBehavior) -> R,
{
    let mut map = load_behaviors();
    let behavior = map.entry(user_id).or_default();
    let result = f(behavior);
    save_behaviors(&map);
    result
}

/// 获取用户行为记录（只读）— 按需加载
pub fn with_behavior<F, R>(user_id: u64, f: F) -> R
where
    F: FnOnce(&UserBehavior) -> R,
{
    let map = load_behaviors();
    match map.get(&user_id) {
        Some(behavior) => f(behavior),
        None => f(&UserBehavior::default()),
    }
}

/// 恢复信誉（自动调用）
pub fn recover_reputation(user_id: u64) {
    with_behavior_mut(user_id, |b| b.recover_reputation());
}

/// 记录消息（同时更新上下文关联器）
pub fn record_message(user_id: u64, normalized: &str) {
    with_behavior_mut(user_id, |b| b.record_message(normalized));
    // 更新上下文关联器
    let mut map = load_correlators();
    let correlator = map.entry(user_id)
        .or_insert_with(|| SerializableCorrelator::new(10, 300));
    correlator.record(normalized);
    save_correlators(&map);
}

/// 获取上下文关联器的多视图段（用于检测跨消息攻击）
pub fn get_context_segments(user_id: u64) -> Vec<String> {
    let map = load_correlators();
    match map.get(&user_id) {
        Some(c) => c.get_all_views(),
        None => Vec::new(),
    }
}

/// 记录违规
pub fn record_violation(user_id: u64, severity: f32) {
    with_behavior_mut(user_id, |b| b.record_violation(severity));
}

/// 记录频率违规
pub fn record_rate_limit(user_id: u64) {
    with_behavior_mut(user_id, |b| b.record_rate_limit());
}

/// 检查是否被封禁
pub fn is_banned(user_id: u64) -> bool {
    with_behavior(user_id, |b| b.banned)
}

/// 检查是否被静默封禁
pub fn is_silent_banned(user_id: u64) -> bool {
    with_behavior(user_id, |b| b.silent_banned)
}

/// 检查识图是否禁用
pub fn is_vision_disabled(user_id: u64) -> bool {
    with_behavior(user_id, |b| b.vision_disabled)
}

/// 获取信誉
pub fn get_reputation(user_id: u64) -> f32 {
    with_behavior(user_id, |b| b.reputation.combined())
}

/// 获取违规次数
pub fn get_violation_count(user_id: u64) -> u32 {
    with_behavior(user_id, |b| b.violation_count)
}

/// 获取惩罚系数
pub fn get_penalty_multiplier(user_id: u64) -> f32 {
    with_behavior(user_id, |b| b.reputation.penalty_multiplier())
}

/// 检查是否应该静默封禁并执行
pub fn check_and_apply_silent_ban(user_id: u64) -> bool {
    with_behavior_mut(user_id, |b| {
        if b.should_silent_ban() {
            b.silent_banned = true;
            b.vision_disabled = true;
            warn!(user_id, violations = b.violation_count, reputation = b.reputation.combined(), "用户触发非察觉性封禁");
            true
        } else {
            false
        }
    })
}

/// 检查是否应该自动封禁并执行
pub fn check_and_apply_auto_ban(user_id: u64, threshold: u32) -> bool {
    with_behavior_mut(user_id, |b| {
        if b.violation_count >= threshold {
            b.banned = true;
            b.vision_disabled = true;
            warn!(user_id, violations = b.violation_count, "用户被完全封禁");
            true
        } else {
            false
        }
    })
}

/// 记录 AI 审查失败
pub fn record_ai_review_failure(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.record_violation(2.0);
        if b.violation_count >= 3 {
            b.silent_banned = true;
            b.vision_disabled = true;
            warn!(user_id, "AI审查失败过多，触发非察觉性封禁");
        }
    });
}

/// 手动封禁用户
pub fn ban_user(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.banned = true;
        b.reputation.content = 0.0;
        b.reputation.trust = 0.0;
        b.vision_disabled = true;
        info!(user_id, "用户已被手动封禁");
    });
}

/// 手动静默封禁用户
pub fn silent_ban_user(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.silent_banned = true;
        b.vision_disabled = true;
        info!(user_id, "用户已被静默封禁");
    });
}

/// 解封用户
pub fn unban_user(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.banned = false;
        b.silent_banned = false;
        b.reputation.content = 0.5;
        b.reputation.trust = 0.5;
        b.violation_count = 0;
        b.high_severity_count = 0;
        b.severity_score = 0.0;
        info!(user_id, "用户已被完全解封");
    });
}

/// 启用识图
pub fn enable_vision(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.vision_disabled = false;
        info!(user_id, "用户识图已重新启用");
    });
}

/// 重置信誉
pub fn reset_reputation(user_id: u64) {
    with_behavior_mut(user_id, |b| {
        b.reputation = Reputation::default();
        b.violation_count = 0;
        b.high_severity_count = 0;
        b.severity_score = 0.0;
        info!(user_id, "用户信誉已重置");
    });
}

/// 全用户风险状态摘要（供管理 API 使用）
pub fn get_all_user_statuses() -> Vec<serde_json::Value> {
    let map = load_behaviors();
    map.iter().map(|(uid, b)| {
        serde_json::json!({
            "user_id": uid,
            "content_reputation": b.reputation.content,
            "spam_reputation": b.reputation.spam,
            "trust_reputation": b.reputation.trust,
            "combined_reputation": b.reputation.combined(),
            "violation_count": b.violation_count,
            "high_severity_count": b.high_severity_count,
            "severity_score": b.severity_score,
            "banned": b.banned,
            "silent_banned": b.silent_banned,
            "vision_disabled": b.vision_disabled,
            "penalty_multiplier": b.reputation.penalty_multiplier(),
            "context_messages": b.recent_messages.len(),
        })
    }).collect()
}

/// 获取用户状态描述
pub fn get_user_status(user_id: u64) -> String {
    with_behavior(user_id, |b| {
        format!(
            "用户 {}:\n  内容信誉: {:.2}\n  频率信誉: {:.2}\n  信任信誉: {:.2}\n  综合信誉: {:.2}\n  违规次数: {}\n  高严重度: {}\n  封禁: {}\n  静默封禁: {}\n  识图禁用: {}\n  惩罚系数: {:.1}x\n  上下文窗口: {}条",
            user_id,
            b.reputation.content,
            b.reputation.spam,
            b.reputation.trust,
            b.reputation.combined(),
            b.violation_count,
            b.high_severity_count,
            if b.banned { "是" } else { "否" },
            if b.silent_banned { "是" } else { "否" },
            if b.vision_disabled { "是" } else { "否" },
            b.reputation.penalty_multiplier(),
            b.recent_messages.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reputation_default() {
        let rep = Reputation::default();
        assert_eq!(rep.combined(), 1.0);
        assert_eq!(rep.penalty_multiplier(), 1.0);
    }

    #[test]
    fn test_reputation_decay() {
        let mut rep = Reputation::default();
        rep.content = 0.3;
        rep.trust = 0.5;
        let combined = rep.combined();
        // 0.3*0.5 + 1.0*0.2 + 0.5*0.3 = 0.15 + 0.2 + 0.15 = 0.5
        assert!((combined - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_penalty_multiplier_levels() {
        let mut rep = Reputation::default();
        rep.content = 1.0;
        assert_eq!(rep.penalty_multiplier(), 1.0);

        rep.content = 0.8;
        assert_eq!(rep.penalty_multiplier(), 1.5);

        rep.content = 0.5;
        assert_eq!(rep.penalty_multiplier(), 2.5);

        rep.content = 0.3;
        assert_eq!(rep.penalty_multiplier(), 4.0);

        rep.content = 0.0;
        assert_eq!(rep.penalty_multiplier(), 6.0);
    }
}
