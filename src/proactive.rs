use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

use crate::config;
use crate::emotion;
use crate::memory;
use crate::self_memory;
use crate::sender;
use crate::working_memory;

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

// ── 伪随机 (不依赖 rand crate) ─────────────────────────────────

/// 简单的伪随机: 用时间+用户ID做种子，返回 0.0~1.0
fn pseudo_random(seed_extra: u64) -> f64 {
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

    // 日期提醒 — 最高优先级
    if let Some(reminder_msg) = check_date_reminders(user_id, &state) {
        tracing::debug!(user_id, group_id, msg = %reminder_msg, "proactive: sending date reminder");
        sender::send_msg(group_id, user_id, &reminder_msg);
        record_sent(user_id);
        return;
    }

    let time_since_last = now.saturating_sub(state.last_sent);
    let time_since_reply = now.saturating_sub(state.last_user_reply);

    // 最低保底: 被无视太多次就别再烦了
    if state.ignore_count >= max_ignore {
        let cooldown = interval * (state.ignore_count as u64);
        if time_since_last < cooldown {
            tracing::debug!(user_id, group_id, ignore_count = state.ignore_count, cooldown, "proactive: cooling down");
            return;
        }
    }

    let emo = emotion::get_state(user_id);

    // ── 触发路径 1: 情绪冲动 ────────────────────────────────────
    // 情绪强烈时，不等固定间隔，随机概率触发
    if time_since_last > 600 && time_since_reply > 600 {
        let impulse_prob = mood_impulse_probability(&emo);
        let roll = pseudo_random(user_id.wrapping_add(now));
        if impulse_prob > 0.0 && roll < impulse_prob {
            let msg = generate_mood_message(user_id, &emo, group_id);
            tracing::debug!(user_id, group_id, msg = %msg, emotion = ?emo.current, intensity = emo.intensity, roll, "proactive: mood impulse");
            sender::send_msg(group_id, user_id, &msg);
            record_sent(user_id);
            return;
        }
    }

    // ── 触发路径 2: 随机化间隔的时间问候 ────────────────────────
    // 把固定间隔乘以一个 0.5~1.5 的随机因子
    let jitter = 0.5 + pseudo_random(user_id.wrapping_add(now / 60)) * 1.0;
    let effective_interval = (interval as f64 * jitter) as u64;

    // 低情绪时延长间隔
    let mood_adjusted = if emo.current == emotion::EmotionType::Sad || emo.current == emotion::EmotionType::Tired {
        (effective_interval as f64 * low_mood_mult) as u64
    } else {
        effective_interval
    };

    if time_since_last >= mood_adjusted && time_since_reply >= mood_adjusted {
        let msg = generate_greeting(user_id, group_id);
        tracing::debug!(user_id, group_id, msg = %msg, time_since_last, time_since_reply, jitter = format!("{:.2}", jitter), "proactive: sending greeting");
        sender::send_msg(group_id, user_id, &msg);
        record_sent(user_id);
    } else {
        tracing::debug!(
            user_id, group_id,
            time_since_last,
            time_since_reply,
            ignore_count = state.ignore_count,
            effective_interval,
            jitter = format!("{:.2}", jitter),
            emotion = ?emo.current,
            "proactive: not yet time"
        );
    }
}

/// 情绪冲动概率: 情绪越强烈，越可能突然想说话
fn mood_impulse_probability(emo: &emotion::EmotionState) -> f64 {
    match emo.current {
        // 开心/兴奋: 有话想说
        emotion::EmotionType::Happy | emotion::EmotionType::Excited => {
            (emo.intensity as f64 * 0.15).min(0.12)
        }
        // 难过/担心: 想找人倾诉
        emotion::EmotionType::Sad | emotion::EmotionType::Worried => {
            (emo.intensity as f64 * 0.12).min(0.10)
        }
        // 惊讶: 忍不住想说
        emotion::EmotionType::Surprised => {
            (emo.intensity as f64 * 0.18).min(0.15)
        }
        // 生气: 想吐槽
        emotion::EmotionType::Angry => {
            (emo.intensity as f64 * 0.08).min(0.06)
        }
        // 害羞: 不太会主动
        emotion::EmotionType::Shy => 0.0,
        // 思考/疲惫/中性: 低概率
        _ => 0.01,
    }
}

/// 情绪驱动的消息: 根据当前情绪状态生成自然的一句话
fn generate_mood_message(user_id: u64, emo: &emotion::EmotionState, _group_id: u64) -> String {
    let rand = pseudo_random(user_id.wrapping_add(now_secs()));

    // 尝试从自我记忆里找灵感
    let self_thoughts = self_memory::get_context(5);
    let has_recent_thought = !self_thoughts.is_empty();

    match emo.current {
        emotion::EmotionType::Happy | emotion::EmotionType::Excited => {
            let options = [
                "突然心情好好",
                "嘿嘿",
                "今天感觉不错",
                "有点开心",
                "哈~",
            ];
            let base = options[(rand * options.len() as f64) as usize % options.len()];
            // 有自我记忆时更有内容
            if has_recent_thought && rand > 0.5 {
                let thought = pick_random_thought(&self_thoughts, rand);
                if !thought.is_empty() {
                    return format!("{}\n{}", base, thought);
                }
            }
            base.to_string()
        }
        emotion::EmotionType::Sad | emotion::EmotionType::Worried => {
            let options = [
                "有点emo...",
                "唉",
                "不知道为什么有点低落",
                "在想一些事情",
            ];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
        emotion::EmotionType::Surprised => {
            let options = [
                "啊 想起来一件事",
                "对了",
                "差点忘了说",
                "噢对",
            ];
            let base = options[(rand * options.len() as f64) as usize % options.len()];
            if has_recent_thought {
                let thought = pick_random_thought(&self_thoughts, rand);
                if !thought.is_empty() {
                    return format!("{}{}", base, thought);
                }
            }
            base.to_string()
        }
        emotion::EmotionType::Angry => {
            let options = [
                "有点烦",
                "啧",
                "气",
            ];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
        _ => {
            // fallback
            let options = ["嗯...", "在想事情", "..."];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
    }
}

/// 从自我记忆文本中随机挑一条想法
fn pick_random_thought(context: &str, rand: f64) -> String {
    let lines: Vec<&str> = context.lines()
        .filter(|l| l.starts_with("- ") && l.len() > 4)
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let idx = (rand * lines.len() as f64) as usize % lines.len();
    lines[idx].trim_start_matches("- ").to_string()
}

fn is_quiet_hour(start: u32, end: u32) -> bool {
    let hour = current_hour();
    if start > end {
        hour >= start || hour < end
    } else {
        hour >= start && hour < end
    }
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

/// 时间问候 — 比以前更随机、更有变化
fn generate_greeting(user_id: u64, group_id: u64) -> String {
    let hour = current_hour();
    let emo = emotion::get_state(user_id);
    let rand = pseudo_random(user_id.wrapping_add(now_secs()));

    // 基础时间问候 (多选一)
    let time_options: &[&str] = if hour < 6 {
        &["这么晚还没睡吗？注意休息哦", "还不睡呀", "夜猫子"]
    } else if hour < 9 {
        &["早上好~新的一天开始了", "早", "早安~"]
    } else if hour < 12 {
        &["上午好~今天过得怎么样？", "在干嘛呀", "上午好"]
    } else if hour < 14 {
        &["中午好，吃午饭了吗？", "午饭吃什么", "中午好~"]
    } else if hour < 18 {
        &["下午好~在忙什么呢？", "下午好", "在干嘛呢"]
    } else if hour < 21 {
        &["晚上好~今天过得怎么样？", "晚上好", "今天过得怎样"]
    } else {
        &["晚上好~快到休息时间了呢", "还不休息吗", "晚安预备~"]
    };
    let time_greeting = time_options[(rand * time_options.len() as f64) as usize % time_options.len()];

    // 尝试从记忆和自我记忆里加点个性化内容
    let mem = memory::get_context(user_id);
    let self_thoughts = self_memory::get_context(3);
    let wm = working_memory::get_context(group_id, 3600);

    let mut extra = String::new();

    // 个人记忆触发
    if mem.contains("咖啡") && rand > 0.5 {
        extra = "要不要来杯咖啡？".to_string();
    } else if (mem.contains("学习") || mem.contains("工作")) && rand > 0.4 {
        extra = "最近学习/工作还顺利吗？".to_string();
    } else if mem.contains("游戏") && rand > 0.5 {
        extra = "最近有在玩游戏吗？".to_string();
    }

    // 自我记忆触发 (最近反思了什么，可以自然地带出来)
    if extra.is_empty() && !self_thoughts.is_empty() && rand > 0.6 {
        let thought = pick_random_thought(&self_thoughts, rand);
        if !thought.is_empty() {
            extra = thought;
        }
    }

    // 群聊工作记忆触发 (最近群里的事)
    if extra.is_empty() && !wm.is_empty() && rand > 0.7 {
        let lines: Vec<&str> = wm.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).collect();
        if !lines.is_empty() {
            let idx = (rand * 100.0) as usize % lines.len();
            let recent = lines[idx];
            // 不要太刻意，只在有自然关联时提及
            if recent.contains("？") || recent.contains("?") {
                extra = "对了 刚才那个问题解决了吗".to_string();
            }
        }
    }

    // 情绪尾巴
    let emo_suffix = match emo.current {
        emotion::EmotionType::Happy => " 我今天心情不错~",
        emotion::EmotionType::Thinking => " 我在想些事情...",
        emotion::EmotionType::Tired => " 有点困...",
        _ => "",
    };

    if extra.is_empty() {
        format!("{}{}", time_greeting, emo_suffix)
    } else {
        format!("{}{}\n{}", time_greeting, emo_suffix, extra)
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
