use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::debug;

use crate::config;
use crate::util::{now_secs, current_hour_cst};

// ── 段日志 & 用户兴趣 ────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentMessage {
    pub user_id: u64,
    pub message: String,
    pub replied: bool,
    pub reason: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UserInterest {
    pub score: f32,
    pub marked_count: u32,
    pub last_reviewed: u64,
    pub last_message: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentLogEntry {
    pub segment_start: u64,
    pub messages: Vec<SegmentMessage>,
    pub reviewed: bool,
}

// ── 持久化结构 ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default)]
struct SegmentCount {
    /// 配额段起始时间 (unix seconds)
    segment_start: u64,
    /// 已使用次数
    count: u32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct QuotaStore {
    /// 日期 YYYY-MM-DD，跨天自动重置
    date: String,
    /// group_id -> 各段计数
    counts: HashMap<u64, Vec<SegmentCount>>,
    #[serde(default)]
    segment_log: HashMap<u64, Vec<SegmentLogEntry>>,
    #[serde(default)]
    user_interest: HashMap<u64, UserInterest>,
    #[serde(default)]
    last_reviewed_segment: HashMap<u64, u64>,
}

// ── 运行时状态 ──────────────────────────────────────────────

static STORE: Mutex<Option<QuotaStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("quota.json")
}

fn today_str() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    let (y, m, d) = days_to_date(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap(y);
    let md = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for &dim in &md {
        if remaining < dim { return (y, month, remaining + 1); }
        remaining -= dim;
        month += 1;
    }
    (y, 12, 31)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// 初始化：加载持久化数据，跨天则重置
pub fn init() {
    let path = store_path();
    let mut store: QuotaStore = match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => QuotaStore::default(),
    };
    let today = today_str();
    if store.date != today {
        store.date = today;
        store.counts.clear();
        store.segment_log.clear();
        store.last_reviewed_segment.clear();
        // user_interest 跨天保留
        save_store(&store);
    }
    // 裁剪 48 小时前的段日志
    let cutoff = now_secs().saturating_sub(48 * 3600);
    for logs in store.segment_log.values_mut() {
        logs.retain(|e| e.segment_start >= cutoff);
    }
    let count: usize = store.counts.values().map(|v| v.len()).sum();
    debug!(groups = store.counts.len(), segments = count, "quota: loaded");
    *STORE.lock().unwrap() = Some(store);
}

fn save_store(store: &QuotaStore) {
    let path = store_path();
    if let Ok(json) = serde_json::to_string_pretty(store) {
        std::fs::write(path, json).ok();
    }
}

// ── 配额段计算 ──────────────────────────────────────────────

/// 获取当前配额段的起始时间 (unix seconds)
fn current_segment_start() -> u64 {
    let cfg = &config::get().quota;
    let segment_secs = cfg.segment_minutes as u64 * 60;
    let now = now_secs();
    // 对齐到段边界
    (now / segment_secs) * segment_secs
}

/// 获取当前小时对应的配额上限
fn current_max_replies() -> u32 {
    let cfg = &config::get().quota;
    let hour = current_hour_cst();
    for seg in &cfg.segments {
        if hour >= seg.start_hour && hour < seg.end_hour {
            return seg.max_replies;
        }
    }
    0
}

// ── 核心 API ────────────────────────────────────────────────

/// 检查当前配额段是否还有余量（不扣减）
pub fn has_quota(group_id: u64) -> bool {
    let cfg = &config::get().quota;
    if !cfg.enabled {
        return true;
    }
    let max = current_max_replies();
    if max == 0 {
        return false;
    }
    let seg_start = current_segment_start();
    let store = STORE.lock().unwrap();
    let store = match store.as_ref() {
        Some(s) => s,
        None => return true,
    };
    let current = get_segment_count(store, group_id, seg_start);
    current < max
}

/// 检查配额并扣减。返回 true 表示允许回复。
pub fn check_and_consume(group_id: u64) -> bool {
    let cfg = &config::get().quota;
    if !cfg.enabled {
        return true;
    }
    let max = current_max_replies();
    if max == 0 {
        return false;
    }
    let seg_start = current_segment_start();

    let mut store_guard = STORE.lock().unwrap();
    let store = match store_guard.as_mut() {
        Some(s) => s,
        None => return true,
    };

    // 跨天检查
    let today = today_str();
    if store.date != today {
        store.date = today;
        store.counts.clear();
    }

    let counts = store.counts.entry(group_id).or_default();
    // 找到当前段
    match counts.iter_mut().find(|s| s.segment_start == seg_start) {
        Some(seg) => {
            if seg.count < max {
                seg.count += 1;
                save_store(store);
                true
            } else {
                false
            }
        }
        None => {
            counts.push(SegmentCount { segment_start: seg_start, count: 1 });
            save_store(store);
            true
        }
    }
}

/// 计算消息优先级分 (0.0~1.0)
///
/// 设计：
/// - darling + bot 刚回复(2min内) → 0.45，无需 @即可延续对话
/// - 2 分钟后窗口关闭 → 0.25，需要 @bot 才能突破
/// - 普通用户 @bot + bot 活跃 → 0.35，仍无法突破（阈值 0.45）
pub fn calculate_priority(
    user_id: u64,
    group_id: u64,
    message: &str,
    at_pattern: &str,
    darling_qq: u64,
) -> f32 {
    let is_darling = darling_qq > 0 && user_id == darling_qq;
    let is_at_bot = !at_pattern.is_empty() && message.contains(at_pattern);

    // bot 最近在群里发过消息（2 分钟内）= 对话窗口
    let bot_recently_active = crate::read_shared_state(|s| {
        s.get_recent_bot_messages(group_id, 120, 1)
            .first()
            .is_some()
    });

    let mut score = 0.0f32;

    if bot_recently_active {
        if is_darling {
            // darling 在活跃对话中：无需 @ 也能延续
            score += 0.45;
            if is_at_bot { score += 0.10; }
        } else if is_at_bot {
            // 普通用户 @bot：有基础分但不足以突破
            score += 0.20;
        }
    } else {
        // 对话窗口已关闭：只有 @bot 才有分
        if is_at_bot {
            score += 0.40;
            if is_darling { score += 0.05; }
        }
    }

    // 兴趣加成 (最多 +0.30)
    let interest = get_interest_score(user_id);
    score += interest * 0.30;

    score.min(1.00)
}

/// 综合判断：优先级 + 配额，成功时自动消费配额。返回 true 表示允许回复。
///
/// - 配额充足 → 消费配额，返回 true
/// - 配额耗尽 + 优先级 >= 0.45 → 突破配额（不消费），返回 true
/// - 配额耗尽 + 优先级 < 0.45 → 返回 false
pub fn try_reply(group_id: u64, user_id: u64, message: &str, at_pattern: &str, darling_qq: u64) -> bool {
    let cfg = &config::get().quota;
    if !cfg.enabled {
        return true;
    }

    // 先检查配额
    if has_quota(group_id) {
        return check_and_consume(group_id);
    }

    // 配额耗尽，检查优先级
    let priority = calculate_priority(user_id, group_id, message, at_pattern, darling_qq);
    let threshold = 0.45f32;
    if priority >= threshold {
        debug!(user_id, group_id, priority, "quota: 配额耗尽但优先级突破");
        return true;
    }

    debug!(user_id, group_id, priority, "quota: 配额耗尽，跳过回复");
    false
}

/// 获取当前段已使用次数 (用于日志/调试)
pub fn get_used(group_id: u64) -> u32 {
    let seg_start = current_segment_start();
    let store = STORE.lock().unwrap();
    store.as_ref().map_or(0, |s| get_segment_count(s, group_id, seg_start))
}

fn get_segment_count(store: &QuotaStore, group_id: u64, seg_start: u64) -> u32 {
    store.counts
        .get(&group_id)
        .and_then(|counts| counts.iter().find(|s| s.segment_start == seg_start))
        .map(|s| s.count)
        .unwrap_or(0)
}

// ── 段日志记录 ────────────────────────────────────────────────

pub fn log_segment_message(group_id: u64, user_id: u64, message: &str) {
    let seg_start = current_segment_start();
    let mut store_guard = STORE.lock().unwrap();
    let store = match store_guard.as_mut() {
        Some(s) => s,
        None => return,
    };
    let logs = store.segment_log.entry(group_id).or_default();
    match logs.iter_mut().find(|e| e.segment_start == seg_start) {
        Some(entry) => {
            entry.messages.push(SegmentMessage {
                user_id,
                message: message.to_string(),
                replied: false,
                reason: String::new(),
                timestamp: now_secs(),
            });
        }
        None => {
            logs.push(SegmentLogEntry {
                segment_start: seg_start,
                messages: vec![SegmentMessage {
                    user_id,
                    message: message.to_string(),
                    replied: false,
                    reason: String::new(),
                    timestamp: now_secs(),
                }],
                reviewed: false,
            });
        }
    }
    save_store(store);
}

pub fn mark_segment_replied(group_id: u64, user_id: u64, reason: &str) {
    let seg_start = current_segment_start();
    let mut store_guard = STORE.lock().unwrap();
    if let Some(store) = store_guard.as_mut() {
        if let Some(logs) = store.segment_log.get_mut(&group_id) {
            if let Some(entry) = logs.iter_mut().find(|e| e.segment_start == seg_start) {
                if let Some(msg) = entry.messages.iter_mut().rev()
                    .find(|m| m.user_id == user_id && !m.replied)
                {
                    msg.replied = true;
                    msg.reason = reason.to_string();
                }
            }
        }
        save_store(store);
    }
}

pub fn mark_segment_reason(group_id: u64, user_id: u64, reason: &str) {
    let seg_start = current_segment_start();
    let mut store_guard = STORE.lock().unwrap();
    if let Some(store) = store_guard.as_mut() {
        if let Some(logs) = store.segment_log.get_mut(&group_id) {
            if let Some(entry) = logs.iter_mut().find(|e| e.segment_start == seg_start) {
                if let Some(msg) = entry.messages.iter_mut().rev()
                    .find(|m| m.user_id == user_id && m.reason.is_empty())
                {
                    msg.reason = reason.to_string();
                }
            }
        }
        save_store(store);
    }
}

// ── 段回顾 & 兴趣系统 ─────────────────────────────────────────

pub fn check_and_review_segment(group_id: u64) {
    let current_seg = current_segment_start();
    let last_reviewed = {
        let store_guard = STORE.lock().unwrap();
        store_guard.as_ref()
            .and_then(|s| s.last_reviewed_segment.get(&group_id).copied())
            .unwrap_or(0)
    };

    if current_seg != last_reviewed && last_reviewed > 0 {
        review_previous_segment(group_id);
    }

    let mut store_guard = STORE.lock().unwrap();
    if let Some(store) = store_guard.as_mut() {
        store.last_reviewed_segment.insert(group_id, current_seg);
        save_store(store);
    }
}

fn review_previous_segment(group_id: u64) {
    let mut store_guard = STORE.lock().unwrap();
    let store = match store_guard.as_mut() {
        Some(s) => s,
        None => return,
    };

    let current_seg = current_segment_start();
    let logs = store.segment_log.entry(group_id).or_default();

    let prev_entry = logs.iter_mut()
        .filter(|e| e.segment_start < current_seg && !e.reviewed)
        .max_by_key(|e| e.segment_start);

    let entry = match prev_entry {
        Some(e) => e,
        None => return,
    };

    entry.reviewed = true;

    let interesting_user = entry.messages.iter()
        .find(|m| !m.replied && !m.reason.is_empty())
        .map(|m| (m.user_id, m.timestamp));

    if let Some((uid, msg_ts)) = interesting_user {
        let interest = store.user_interest.entry(uid).or_default();
        interest.score = (interest.score + 0.15_f32).min(1.0_f32);
        interest.marked_count += 1;
        interest.last_reviewed = now_secs();
        interest.last_message = msg_ts;
        debug!(user_id = uid, score = interest.score, "interest: 用户被标记为感兴趣");
    }
    save_store(store);
}

pub fn decay_all_interest() {
    let mut store_guard = STORE.lock().unwrap();
    if let Some(store) = store_guard.as_mut() {
        for interest in store.user_interest.values_mut() {
            interest.score *= 0.5;
            if interest.score < 0.01 {
                interest.score = 0.0;
            }
        }
        save_store(store);
    }
}

pub fn get_interest_score(user_id: u64) -> f32 {
    let store_guard = STORE.lock().unwrap();
    store_guard.as_ref()
        .and_then(|s| s.user_interest.get(&user_id))
        .map(|i| i.score)
        .unwrap_or(0.0)
}

// ── Admin API ──────────────────────────────────────────────────

pub fn get_all_interest() -> HashMap<u64, UserInterest> {
    let store_guard = STORE.lock().unwrap();
    store_guard.as_ref()
        .map(|s| s.user_interest.clone())
        .unwrap_or_default()
}

pub fn get_segment_logs(group_id: u64, limit: usize) -> Vec<SegmentLogEntry> {
    let store_guard = STORE.lock().unwrap();
    store_guard.as_ref()
        .and_then(|s| s.segment_log.get(&group_id))
        .map(|logs| {
            let mut sorted = logs.clone();
            sorted.sort_by(|a, b| b.segment_start.cmp(&a.segment_start));
            sorted.into_iter().take(limit).collect()
        })
        .unwrap_or_default()
}

pub fn get_groups_with_logs() -> Vec<u64> {
    let store_guard = STORE.lock().unwrap();
    store_guard.as_ref()
        .map(|s| s.segment_log.keys().copied().collect())
        .unwrap_or_default()
}
