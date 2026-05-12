use tracing::debug;

use crate::config;
use crate::util::now_secs;
use super::store::{
    STORE, save_store, current_segment_start, current_max_replies, get_segment_count,
    SegmentCount, SegmentMessage, SegmentLogEntry,
};

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
    let today = crate::util::today_str();
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

/// 获取当前段已使用次数 (用于日志/调试)
pub fn get_used(group_id: u64) -> u32 {
    let seg_start = current_segment_start();
    let store = STORE.lock().unwrap();
    store.as_ref().map_or(0, |s| get_segment_count(s, group_id, seg_start))
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
        if let Some(logs) = store.segment_log.get_mut(&group_id)
            && let Some(entry) = logs.iter_mut().find(|e| e.segment_start == seg_start)
                && let Some(msg) = entry.messages.iter_mut().rev()
                    .find(|m| m.user_id == user_id && !m.replied)
                {
                    msg.replied = true;
                    msg.reason = reason.to_string();
                }
        save_store(store);
    }
}

pub fn mark_segment_reason(group_id: u64, user_id: u64, reason: &str) {
    let seg_start = current_segment_start();
    let mut store_guard = STORE.lock().unwrap();
    if let Some(store) = store_guard.as_mut() {
        if let Some(logs) = store.segment_log.get_mut(&group_id)
            && let Some(entry) = logs.iter_mut().find(|e| e.segment_start == seg_start)
                && let Some(msg) = entry.messages.iter_mut().rev()
                    .find(|m| m.user_id == user_id && m.reason.is_empty())
                {
                    msg.reason = reason.to_string();
                }
        save_store(store);
    }
}

// ── 段回顾 ─────────────────────────────────────────────────

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
