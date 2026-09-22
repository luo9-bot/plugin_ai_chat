//! 安全审计日志：五道滤壳的全部命中、拉黑、拒收记录
//!
//! append-only jsonl（data/mind/security_log.jsonl），供 WebUI 审计（P5）。

use std::fs;
use std::io::Write as _;

use crate::config;
use tracing::warn;

/// 记录一次滤壳事件
///
/// `gate`：perception / translation / inner_dialog / consolidation / persona
/// `action`：zero_tolerance_blacklist / rejected / quarantined / warned …
pub(crate) fn log_event(user_id: u64, gate: &str, action: &str, detail: &str) {
    let entry = serde_json::json!({
        "time": crate::util::now_secs(),
        "user_id": user_id,
        "gate": gate,
        "action": action,
        "detail": detail,
    });
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };

    let dir = config::data_dir().join("mind");
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!(error = %e, "security: 创建目录失败");
        return;
    }
    let path = dir.join("security_log.jsonl");
    let mut line_with_nl = line;
    line_with_nl.push('\n');
    if let Err(e) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line_with_nl.as_bytes()))
    {
        warn!(error = %e, "security: 审计日志写入失败");
    }
}

/// 最近的审计事件（admin API 用，新在前）
pub(crate) fn tail(n: usize) -> Vec<serde_json::Value> {
    let path = config::data_dir().join("mind").join("security_log.jsonl");
    let Ok(content) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let events: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let start = events.len().saturating_sub(n);
    events[start..].iter().rev().cloned().collect()
}
