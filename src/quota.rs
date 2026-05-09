use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::debug;

use crate::config;
use crate::util::{now_secs, current_hour_cst};

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
        save_store(&store);
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
/// 极端保守：只有 darling 在 bot 刚发过消息的群里 @bot 才能突破 0.55
pub fn calculate_priority(
    user_id: u64,
    group_id: u64,
    message: &str,
    at_pattern: &str,
    darling_qq: u64,
) -> f32 {
    let mut score = 0.0f32;

    // 基础分
    if !at_pattern.is_empty() && message.contains(at_pattern) {
        score += 0.4;   // @bot
    } else {
        // 检查是否在回复 bot 的消息 (简化：消息中包含 bot 相关内容)
        // 由 decide_reply 的 AI 来判断更准确，这里只做粗筛
        score += 0.0;   // 其他消息基础分 0
    }

    // 修正：bot 最近在群里发过消息（对话进行中）
    let bot_recently_active = crate::read_shared_state(|s| {
        s.get_recent_bot_messages(group_id, 180, 1) // 3 分钟内
            .first()
            .is_some()
    });
    if bot_recently_active {
        score += 0.1;
    }

    // 修正：用户是 darling
    if darling_qq > 0 && user_id == darling_qq {
        score += 0.05;
    }

    score.min(0.65) // 硬上限
}

/// 综合判断：优先级 + 配额。返回 true 表示应该尝试回复。
///
/// - 配额充足 → true
/// - 配额耗尽 + 优先级 >= 0.55 → true（突破配额）
/// - 配额耗尽 + 优先级 < 0.55 → false
pub fn try_reply(group_id: u64, user_id: u64, message: &str, at_pattern: &str, darling_qq: u64) -> bool {
    let cfg = &config::get().quota;
    if !cfg.enabled {
        return true;
    }

    // 先检查配额
    if has_quota(group_id) {
        return true;
    }

    // 配额耗尽，检查优先级
    let priority = calculate_priority(user_id, group_id, message, at_pattern, darling_qq);
    let threshold = 0.55f32;
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
