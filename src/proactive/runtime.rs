use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

use crate::config;

// ── 运行时状态 (持久化到 proactive.json) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveState {
    pub last_sent: u64,
    pub ignore_count: u32,
    pub last_user_reply: u64,
    pub last_working_memory_ts: u64, // 上次发送时工作记忆中最新的用户消息时间戳
    pub pending_reminders: Vec<DateReminder>,
}

impl Default for ProactiveState {
    fn default() -> Self {
        let now = crate::util::now_secs();
        Self {
            last_sent: 0,
            ignore_count: 0,
            last_user_reply: now,
            last_working_memory_ts: 0,
            pending_reminders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateReminder {
    pub date: String,
    pub description: String,
    pub year_added: u32,
}

// ── 运行时配置覆盖 (持久化到 proactive_config.json) ────────────
// 用户通过命令修改的设置存这里，未修改的项回退到 config.yaml

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeConfig {
    pub enabled: Option<bool>,
    pub quiet_start: Option<u32>,
    pub quiet_end: Option<u32>,
    pub interval: Option<u64>,
}

fn state_path() -> std::path::PathBuf {
    config::data_dir().join("proactive.json")
}

fn runtime_path() -> std::path::PathBuf {
    config::data_dir().join("proactive_config.json")
}

// ── 状态持久化 ──────────────────────────────────────────────────

pub fn load_state(user_id: u64) -> ProactiveState {
    let path = state_path();
    let all: HashMap<String, ProactiveState> = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    all.get(&user_id.to_string()).cloned().unwrap_or_default()
}

pub fn save_state(user_id: u64, state: &ProactiveState) {
    let path = state_path();
    let mut all: HashMap<String, ProactiveState> = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    all.insert(user_id.to_string(), state.clone());
    if let Ok(json) = serde_json::to_string_pretty(&all) {
        fs::write(path, json).ok();
    }
}

// ── 运行时配置持久化 ────────────────────────────────────────────

fn load_runtime() -> RuntimeConfig {
    let path = runtime_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => RuntimeConfig::default(),
    }
}

fn save_runtime(rt: &RuntimeConfig) {
    let path = runtime_path();
    if let Ok(json) = serde_json::to_string_pretty(rt) {
        fs::write(path, json).ok();
    }
}

/// 合并 config.yaml 默认值 + 运行时覆盖
pub fn effective_config() -> (bool, u32, u32, u64, u32, f64) {
    let base = &config::get().proactive;
    let rt = load_runtime();
    (
        rt.enabled.unwrap_or(base.enabled),
        rt.quiet_start.unwrap_or(base.quiet_start),
        rt.quiet_end.unwrap_or(base.quiet_end),
        rt.interval.unwrap_or(base.interval),
        base.max_ignore,
        base.low_mood_multiplier,
    )
}

// ── 伪随机 (不依赖 rand crate) ─────────────────────────────────

/// 简单的伪随机: 用时间+用户ID做种子，返回 0.0~1.0
pub fn pseudo_random(seed_extra: u64) -> f64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = now.as_nanos() as u64;
    // xorshift64
    let mut x = ticks.wrapping_add(seed_extra).wrapping_add(0x9E3779B97F4A7C15);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x as f64) / (u64::MAX as f64)
}

// ── 公开接口 ────────────────────────────────────────────────────

pub fn user_count() -> usize {
    let path = state_path();
    let all: HashMap<String, ProactiveState> = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    all.len()
}

pub fn record_user_reply(user_id: u64) {
    let mut state = load_state(user_id);
    state.last_user_reply = crate::util::now_secs();
    state.ignore_count = 0;
    save_state(user_id, &state);
}

pub fn record_sent(user_id: u64, group_id: u64) {
    let mut state = load_state(user_id);
    state.last_sent = crate::util::now_secs();
    state.ignore_count += 1;
    // 记录当前工作记忆中最新的用户消息时间戳，下次无新消息时不触发
    if group_id > 0 {
        state.last_working_memory_ts = crate::working_memory::get_latest_user_message_ts(group_id);
    }
    save_state(user_id, &state);
}

pub fn add_date_reminder(user_id: u64, date: &str, description: &str) {
    let mut state = load_state(user_id);
    state.pending_reminders.push(DateReminder {
        date: date.to_string(),
        description: description.to_string(),
        year_added: crate::util::current_year(),
    });
    save_state(user_id, &state);
}

pub fn is_quiet_hour(start: u32, end: u32) -> bool {
    let hour = crate::util::current_hour_cst();
    if start > end {
        hour >= start || hour < end
    } else {
        hour >= start && hour < end
    }
}

pub fn check_date_reminders(user_id: u64, state: &ProactiveState) -> Option<String> {
    let today = crate::util::current_date_mm_dd();
    let year = crate::util::current_year();

    for reminder in &state.pending_reminders {
        if reminder.date == today && reminder.year_added == year {
            return Some(format!("今天是个特别的日子呢~\n{}", reminder.description));
        }
    }

    let mem_ctx = crate::memory::get_context(user_id);
    if mem_ctx.contains("生日") && today.ends_with("-01") {
        return Some("新的一月开始了，有什么计划吗？".to_string());
    }

    None
}

// ── 用户命令接口 ────────────────────────────────────────────────

pub fn set_enabled(enabled: bool) {
    let mut rt = load_runtime();
    rt.enabled = Some(enabled);
    save_runtime(&rt);
}

pub fn set_quiet_hours(start: u32, end: u32) {
    let mut rt = load_runtime();
    rt.quiet_start = Some(start);
    rt.quiet_end = Some(end);
    save_runtime(&rt);
}

pub fn set_interval(seconds: u64) {
    let mut rt = load_runtime();
    rt.interval = Some(seconds);
    save_runtime(&rt);
}
