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
    pub last_working_memory_ts: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub enabled: Option<bool>,
    pub quiet_start: Option<u32>,
    pub quiet_end: Option<u32>,
    pub interval: Option<u64>,
    /// 群级最近发送时间戳 (group_id → last_sent)
    pub group_last_sent: Option<HashMap<u64, u64>>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            quiet_start: None,
            quiet_end: None,
            interval: None,
            group_last_sent: Some(HashMap::new()),
        }
    }
}

// ── 群级发送冷却（内存态，持久化到 proactive_config.json） ──

/// 更新群级发送时间戳
pub fn update_group_last_sent(group_id: u64) {
    let mut rt = load_runtime();
    let map = rt.group_last_sent.get_or_insert_with(HashMap::new);
    map.insert(group_id, crate::util::now_secs());
    save_runtime(&rt);
}

/// 获取群级上次发送时间
pub fn get_group_last_sent(group_id: u64) -> u64 {
    let rt = load_runtime();
    rt.group_last_sent
        .as_ref()
        .and_then(|m| m.get(&group_id).copied())
        .unwrap_or(0)
}

// ── 文件 I/O ────────────────────────────────────────────────────

fn state_path() -> std::path::PathBuf {
    crate::config::data_dir().join("proactive.json")
}

fn runtime_path() -> std::path::PathBuf {
    crate::config::data_dir().join("proactive_config.json")
}

pub fn load_state(user_id: u64) -> ProactiveState {
    let path = state_path();
    let all: HashMap<String, ProactiveState> = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    all.get(&user_id.to_string()).cloned().unwrap_or_default()
}

fn save_state(user_id: u64, state: &ProactiveState) {
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

// ── 伪随机 ──────────────────────────────────────────────────────

pub fn pseudo_random(seed_extra: u64) -> f64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = now.as_nanos() as u64;
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
    if group_id > 0 {
        state.last_working_memory_ts = crate::working_memory::get_latest_user_message_ts(group_id);
    }
    save_state(user_id, &state);

    // 群级同步：更新群组最近发送时间
    if group_id > 0 {
        update_group_last_sent(group_id);
    }
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

    let mem_ctx = crate::memory::get_context(user_id, 0);
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

pub fn set_interval(interval: u64) {
    let mut rt = load_runtime();
    rt.interval = Some(interval);
    save_runtime(&rt);
}
