use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

use crate::config;
use crate::emotion;
use crate::memory;
use crate::sender;

// ── 运行时状态 (持久化到 proactive.json) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveState {
    pub last_sent: u64,
    pub ignore_count: u32,
    pub last_user_reply: u64,
    pub pending_reminders: Vec<DateReminder>,
}

impl Default for ProactiveState {
    fn default() -> Self {
        let now = now_secs();
        Self {
            last_sent: 0,
            ignore_count: 0,
            last_user_reply: now,
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn state_path() -> std::path::PathBuf {
    config::data_dir().join("proactive.json")
}

fn runtime_path() -> std::path::PathBuf {
    config::data_dir().join("proactive_config.json")
}

// ── 状态持久化 ──────────────────────────────────────────────────

fn load_state(user_id: u64) -> ProactiveState {
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
fn effective_config() -> (bool, u32, u32, u64, u32, f64) {
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
    state.last_user_reply = now_secs();
    state.ignore_count = 0;
    save_state(user_id, &state);
}

pub fn record_sent(user_id: u64) {
    let mut state = load_state(user_id);
    state.last_sent = now_secs();
    state.ignore_count += 1;
    save_state(user_id, &state);
}

pub fn add_date_reminder(user_id: u64, date: &str, description: &str) {
    let mut state = load_state(user_id);
    state.pending_reminders.push(DateReminder {
        date: date.to_string(),
        description: description.to_string(),
        year_added: current_year(),
    });
    save_state(user_id, &state);
}

// ── 主动消息检查 ────────────────────────────────────────────────

pub fn check_proactive_messages(user_id: u64, group_id: u64) {
    let (enabled, quiet_start, quiet_end, interval, max_ignore, low_mood_mult) = effective_config();
    if !enabled {
        return;
    }

    let state = load_state(user_id);
    let now = now_secs();

    if is_quiet_hour(quiet_start, quiet_end) {
        tracing::debug!(user_id, group_id, "proactive: quiet hour, skipping");
        return;
    }

    if let Some(reminder_msg) = check_date_reminders(user_id, &state) {
        tracing::debug!(user_id, group_id, msg = %reminder_msg, "proactive: sending date reminder");
        sender::send_msg(group_id, user_id, &reminder_msg);
        record_sent(user_id);
        return;
    }

    let time_since_last = now.saturating_sub(state.last_sent);
    let time_since_reply = now.saturating_sub(state.last_user_reply);

    if should_send_greeting(user_id, interval, max_ignore, low_mood_mult, &state) {
        let msg = generate_greeting(user_id);
        tracing::debug!(user_id, group_id, msg = %msg, time_since_last, time_since_reply, "proactive: sending greeting");
        sender::send_msg(group_id, user_id, &msg);
        record_sent(user_id);
    } else {
        tracing::debug!(
            user_id, group_id,
            time_since_last,
            time_since_reply,
            ignore_count = state.ignore_count,
            interval,
            "proactive: not yet time"
        );
    }
}

fn is_quiet_hour(start: u32, end: u32) -> bool {
    let hour = current_hour();
    if start > end {
        hour >= start || hour < end
    } else {
        hour >= start && hour < end
    }
}

fn should_send_greeting(
    user_id: u64,
    interval: u64,
    max_ignore: u32,
    low_mood_mult: f64,
    state: &ProactiveState,
) -> bool {
    let now = now_secs();
    let time_since_last_msg = now.saturating_sub(state.last_sent);
    let time_since_user_reply = now.saturating_sub(state.last_user_reply);

    if time_since_last_msg < interval {
        return false;
    }
    if time_since_user_reply < interval {
        return false;
    }
    if state.ignore_count >= max_ignore {
        let extra_wait = interval * (state.ignore_count as u64);
        if time_since_last_msg < extra_wait {
            return false;
        }
    }

    let emo = emotion::get_state(user_id);
    if emo.current == emotion::EmotionType::Sad || emo.current == emotion::EmotionType::Tired {
        let low_mood_interval = (interval as f64 * low_mood_mult) as u64;
        if time_since_last_msg < low_mood_interval {
            return false;
        }
    }

    true
}

fn check_date_reminders(user_id: u64, state: &ProactiveState) -> Option<String> {
    let today = current_date();
    let year = current_year();

    for reminder in &state.pending_reminders {
        if reminder.date == today && reminder.year_added == year {
            return Some(format!("今天是个特别的日子呢~\n{}", reminder.description));
        }
    }

    let mem_ctx = memory::get_context(user_id);
    if mem_ctx.contains("生日") && today.ends_with("-01") {
        return Some("新的一月开始了，有什么计划吗？".to_string());
    }

    None
}

fn generate_greeting(user_id: u64) -> String {
    let hour = current_hour();
    let emo = emotion::get_state(user_id);

    let time_greeting = if hour < 6 {
        "这么晚还没睡吗？注意休息哦"
    } else if hour < 9 {
        "早上好~新的一天开始了"
    } else if hour < 12 {
        "上午好~今天过得怎么样？"
    } else if hour < 14 {
        "中午好，吃午饭了吗？"
    } else if hour < 18 {
        "下午好~在忙什么呢？"
    } else if hour < 21 {
        "晚上好~今天过得怎么样？"
    } else {
        "晚上好~快到休息时间了呢"
    };

    let mem = memory::get_context(user_id);
    let personal = if mem.contains("咖啡") {
        "要不要来杯咖啡？"
    } else if mem.contains("学习") || mem.contains("工作") {
        "最近学习/工作还顺利吗？"
    } else if mem.contains("游戏") {
        "最近有在玩游戏吗？"
    } else {
        ""
    };

    let emo_suffix = match emo.current {
        emotion::EmotionType::Happy => " 我今天心情不错~",
        emotion::EmotionType::Thinking => " 我在想些事情...",
        _ => "",
    };

    if personal.is_empty() {
        format!("{}{}", time_greeting, emo_suffix)
    } else {
        format!("{}{}\n{}", time_greeting, emo_suffix, personal)
    }
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

// ── 时间工具 ────────────────────────────────────────────────────

fn current_hour() -> u32 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64 + 8 * 3600;
    ((secs % 86400) / 3600) as u32
}

fn current_date() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let (_, month, day) = crate::ai::days_to_ymd(days);
    format!("{:02}-{:02}", month, day)
}

fn current_year() -> u32 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let (year, _, _) = crate::ai::days_to_ymd(days);
    year as u32
}
