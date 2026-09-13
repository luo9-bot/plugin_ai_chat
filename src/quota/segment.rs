use super::store::{
    STORE, SegmentCount, SegmentLogEntry, SegmentMessage, current_max_replies,
    current_segment_start, get_segment_count, save_store,
};
use crate::config;
use crate::util::now_secs;

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
            counts.push(SegmentCount {
                segment_start: seg_start,
                count: 1,
            });
            save_store(store);
            true
        }
    }
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
                timestamp: now_secs(),
            });
        }
        None => {
            logs.push(SegmentLogEntry {
                segment_start: seg_start,
                messages: vec![SegmentMessage {
                    user_id,
                    message: message.to_string(),
                    timestamp: now_secs(),
                }],
            });
        }
    }
    save_store(store);
}
