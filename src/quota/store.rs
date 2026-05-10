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
pub(super) struct SegmentCount {
    /// 配额段起始时间 (unix seconds)
    pub(super) segment_start: u64,
    /// 已使用次数
    pub(super) count: u32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(super) struct QuotaStore {
    /// 日期 YYYY-MM-DD，跨天自动重置
    pub(super) date: String,
    /// group_id -> 各段计数
    pub(super) counts: HashMap<u64, Vec<SegmentCount>>,
    #[serde(default)]
    pub(super) segment_log: HashMap<u64, Vec<SegmentLogEntry>>,
    #[serde(default)]
    pub(super) user_interest: HashMap<u64, UserInterest>,
    #[serde(default)]
    pub(super) last_reviewed_segment: HashMap<u64, u64>,
}

// ── 运行时状态 ──────────────────────────────────────────────

pub(super) static STORE: Mutex<Option<QuotaStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("quota.json")
}

/// 初始化：加载持久化数据，跨天则重置
pub fn init() {
    let path = store_path();
    let mut store: QuotaStore = match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => QuotaStore::default(),
    };
    let today = crate::util::today_str();
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

pub(super) fn save_store(store: &QuotaStore) {
    let path = store_path();
    if let Ok(json) = serde_json::to_string_pretty(store) {
        std::fs::write(path, json).ok();
    }
}

// ── 配额段计算 ──────────────────────────────────────────────

/// 获取当前配额段的起始时间 (unix seconds)
pub(super) fn current_segment_start() -> u64 {
    let cfg = &config::get().quota;
    let segment_secs = cfg.segment_minutes as u64 * 60;
    let now = now_secs();
    // 对齐到段边界
    (now / segment_secs) * segment_secs
}

/// 获取当前小时对应的配额上限
pub(super) fn current_max_replies() -> u32 {
    let cfg = &config::get().quota;
    let hour = current_hour_cst();
    for seg in &cfg.segments {
        if hour >= seg.start_hour && hour < seg.end_hour {
            return seg.max_replies;
        }
    }
    0
}

pub(super) fn get_segment_count(store: &QuotaStore, group_id: u64, seg_start: u64) -> u32 {
    store.counts
        .get(&group_id)
        .and_then(|counts| counts.iter().find(|s| s.segment_start == seg_start))
        .map(|s| s.count)
        .unwrap_or(0)
}
