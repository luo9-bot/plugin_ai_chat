use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use tracing::debug;

const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsLogEntry {
    pub timestamp: u64,
    pub operation: String,
    pub user_id: u64,
    pub group_id: u64,
    pub content_preview: String,
    pub importance: String,
    pub detail: String,
}

static OPS_LOG: Mutex<Option<VecDeque<OpsLogEntry>>> = Mutex::new(None);

fn log_path() -> PathBuf {
    crate::config::data_dir().join("memory_ops_log.json")
}

fn load_from_disk() -> VecDeque<OpsLogEntry> {
    let path = log_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => VecDeque::new(),
    }
}

fn save_to_disk(log: &VecDeque<OpsLogEntry>) {
    let path = log_path();
    if let Ok(json) = serde_json::to_string_pretty(log) {
        std::fs::write(path, json).ok();
    }
}

/// 初始化：从磁盘加载日志
pub fn init() {
    let loaded = load_from_disk();
    let mut guard = OPS_LOG.lock().unwrap();
    *guard = Some(loaded);
}

/// 记录一条内存操作日志
pub fn record(
    operation: &str,
    user_id: u64,
    group_id: u64,
    content: &str,
    importance: &str,
    detail: &str,
) {
    let preview: String = content.chars().take(80).collect();
    let entry = OpsLogEntry {
        timestamp: crate::util::now_secs(),
        operation: operation.to_string(),
        user_id,
        group_id,
        content_preview: preview,
        importance: importance.to_string(),
        detail: detail.to_string(),
    };

    let mut guard = OPS_LOG.lock().unwrap();
    let log = guard.get_or_insert_with(VecDeque::new);
    log.push_back(entry);
    // 超出上限时移除最旧的
    while log.len() > MAX_ENTRIES {
        log.pop_front();
    }
    // 异步持久化（不阻塞调用方）
    let snapshot: VecDeque<OpsLogEntry> = log.clone();
    drop(guard);
    std::thread::spawn(move || {
        save_to_disk(&snapshot);
    });
    debug!(operation, user_id, group_id, "ops_log: recorded");
}

/// 获取最近 N 条日志
pub fn get_logs(limit: Option<usize>) -> Vec<OpsLogEntry> {
    let guard = OPS_LOG.lock().unwrap();
    let log = guard.as_ref().map(|l| l.as_slices().0).unwrap_or_default();
    let n = limit.unwrap_or(500).min(log.len());
    // 返回最近 n 条（从尾部取）
    log[log.len() - n..].to_vec()
}

/// 清空日志
pub fn clear() {
    let mut guard = OPS_LOG.lock().unwrap();
    if let Some(log) = guard.as_mut() {
        log.clear();
    }
    drop(guard);
    save_to_disk(&VecDeque::new());
}
