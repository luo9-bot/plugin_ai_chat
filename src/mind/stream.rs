//! 意识流：她的人生时间线
//!
//! 四类事件，且写入权限在类型上就分了家：
//! - `Sensation`（感官）：代码写入，机械事实，永不含解释与抒情
//! - `Inner`（内心）：**只有回神路径可写**——她亲笔的念头、感受、决定
//! - `Acted`（行动）：她说出口的话、发的东西、一次被记录的沉默
//! - `Digested`（沉淀）：睡前整理写下的"之前的我"
//!
//! append-only jsonl，按天分文件；超过保留期的天文件物理删除——
//! 没被固化的经历会淡忘，这天然模拟人类记忆。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::config;
use crate::util;

/// 天文件的保留天数：72 小时后消亡
pub(crate) const KEEP_DAYS: u64 = 3;

// ── 事件模型 ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StreamKind {
    Sensation,
    Inner,
    Acted,
    Digested,
}

/// 身体信号：数值存在，解释不存在（由她诠释）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BodySignal {
    pub name: String,
    /// 0.0 ~ 1.0
    pub level: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecallMeta {
    pub id: String,
    pub source: String,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct StreamEvent {
    pub kind: StreamKind,
    /// 第一人称内容。Sensation 必须是转述；Inner 必须是她自己的话
    pub content: String,
    /// unix 秒
    pub time: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodySignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recall: Option<RecallMeta>,
}

impl StreamEvent {
    pub(crate) fn new(kind: StreamKind, content: impl Into<String>) -> Self {
        StreamEvent {
            kind,
            content: content.into(),
            time: util::now_secs(),
            body: None,
            about: None,
            recall: None,
        }
    }

    pub(crate) fn with_about(mut self, user_id: u64) -> Self {
        self.about = Some(user_id);
        self
    }

    pub(crate) fn with_recall(mut self, id: String, source: &str) -> Self {
        self.recall = Some(RecallMeta {
            id,
            source: source.to_string(),
            completed: true,
        });
        self
    }

    pub fn with_recall(mut self, id: impl Into<String>, source: impl Into<String>) -> Self {
        self.recall = Some(RecallMeta {
            id: id.into(),
            source: source.into(),
        });
        self
    }
}

// ── 写入 ────────────────────────────────────────────────────────

/// 写入一条她的内心活动。
///
/// # 纪律
/// 只允许回神路径（`mind::wake`）调用——她的内心只能由她此刻写出。
/// 任何模板、任何定时器、任何"体验发射器"都不得调用本函数。
pub(crate) fn push_inner(content: impl Into<String>) {
    push(StreamEvent::new(StreamKind::Inner, content));
}

/// 写入一次她的行动（说出口的话、发表情包、一次被记录的沉默）
pub(crate) fn push_acted(content: impl Into<String>) {
    push(StreamEvent::new(StreamKind::Acted, content));
}

/// 写入一条睡前整理的沉淀摘要
pub(crate) fn push_digested(summary: impl Into<String>) {
    push(StreamEvent::new(StreamKind::Digested, summary));
}

/// 追加一条事件到对应日期的 jsonl（append-only）
pub(crate) fn push(event: StreamEvent) {
    let path = day_file(event.time);
    let line = match serde_json::to_string(&event) {
        Ok(json) => json,
        Err(e) => {
            warn!(error = %e, "stream: 事件序列化失败");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(error = %e, "stream: 创建目录失败");
        return;
    }
    // 原子性：单行追加 + 换行；进程中断最多丢最后一行
    let mut line_with_nl = line;
    line_with_nl.push('\n');
    if let Err(e) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line_with_nl.as_bytes()))
    {
        warn!(error = %e, "stream: 事件写入失败");
    } else {
        debug!(kind = ?event.kind, "stream: 事件已写入");
    }
}

// ── 读取 ────────────────────────────────────────────────────────

/// 读取某个日期的全部事件（文件缺失返回空）
fn read_day(secs: u64) -> Vec<StreamEvent> {
    events_on_date(&util::ts_to_date_str(secs))
}

/// 读取指定日期（"YYYY-MM-DD"）的全部事件（admin API 用）
pub(crate) fn events_on_date(date: &str) -> Vec<StreamEvent> {
    let path = stream_dir().join(format!("{date}.jsonl"));
    let Ok(content) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<StreamEvent>(line) {
            Ok(event) => Some(event),
            Err(e) => {
                warn!(error = %e, "stream: 跳过损坏的事件行");
                None
            }
        })
        .collect()
}

/// 列出意识流目录下已有的日期（旧在前，admin API 用）
pub(crate) fn known_dates() -> Vec<String> {
    let Ok(entries) = fs::read_dir(stream_dir()) else {
        return Vec::new();
    };
    let mut dates: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            name.strip_suffix(".jsonl").map(str::to_string)
        })
        .collect();
    dates.sort();
    dates
}

/// 最近一段时间窗口内的事件（自动跨午夜读取昨天与今天，旧在上）
pub(crate) fn recent(window_secs: u64, max: usize) -> Vec<StreamEvent> {
    let now = util::now_secs();
    let window_start = now.saturating_sub(window_secs);

    let mut events: Vec<StreamEvent> = Vec::new();
    // 覆盖窗口可能触及的日期：今天与（必要时）昨天
    for day_start in [now, now.saturating_sub(86400)] {
        for event in read_day(day_start) {
            if event.time >= window_start {
                events.push(event);
            }
        }
    }
    events.sort_by_key(|e| e.time);
    if events.len() > max {
        let drop = events.len() - max;
        events.drain(..drop);
    }
    events
}

/// 最近事件的文本形态（"[HH:MM] 内容"，旧在上），供回神感官包直接使用
pub(crate) fn recent_text(window_secs: u64, max: usize) -> String {
    recent(window_secs, max)
        .iter()
        .map(|e| format!("[{}] {}", util::hh_mm(e.time), e.content))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── 清理 ────────────────────────────────────────────────────────

/// 删除超过保留期的天文件
pub(crate) fn cleanup(keep_days: u64) {
    let dir = stream_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let cutoff_secs = util::now_secs().saturating_sub(keep_days * 86400);
    let cutoff_date = util::ts_to_date_str(cutoff_secs);

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        // 文件名即日期（YYYY-MM-DD.jsonl），字符串比较即时间比较
        if name < cutoff_date.as_str() {
            if let Err(e) = fs::remove_file(entry.path()) {
                warn!(error = %e, name, "stream: 过期文件删除失败");
            } else {
                debug!(name, "stream: 过期天文件已清理");
            }
        }
    }
}

// ── 存储路径 ────────────────────────────────────────────────────

fn stream_dir() -> PathBuf {
    config::data_dir().join("mind").join("stream")
}

/// 天文件名（纯函数，便于测试）：`YYYY-MM-DD.jsonl`（东八区日期）
fn day_file_name(secs: u64) -> String {
    format!("{}.jsonl", util::ts_to_date_str(secs))
}

fn day_file(secs: u64) -> PathBuf {
    stream_dir().join(day_file_name(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_at(kind: StreamKind, content: &str, time: u64) -> StreamEvent {
        StreamEvent {
            kind,
            content: content.to_string(),
            time,
            body: None,
            about: None,
            recall: None,
        }
    }

    #[test]
    fn kind_serializes_snake_case() {
        let json = serde_json::to_string(&StreamKind::Inner).unwrap();
        assert_eq!(json, r#""inner""#);
        let kind: StreamKind = serde_json::from_str(r#""sensation""#).unwrap();
        assert_eq!(kind, StreamKind::Sensation);
    }

    #[test]
    fn event_round_trips_with_optional_fields() {
        let mut event =
            event_at(StreamKind::Sensation, "土豆说“在吗”", 1_700_000_000).with_about(42);
        event.body = Some(BodySignal {
            name: "困倦".into(),
            level: 0.7,
        });
        let json = serde_json::to_string(&event).unwrap();
        let back: StreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn optional_fields_omitted_when_none() {
        let event = event_at(StreamKind::Acted, "嗯", 1);
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("body"));
        assert!(!json.contains("about"));
    }

    #[test]
    fn recent_sorts_and_truncates() {
        let mut events = vec![
            event_at(StreamKind::Sensation, "晚", 3_000),
            event_at(StreamKind::Inner, "早", 1_000),
            event_at(StreamKind::Acted, "中", 2_000),
        ];
        events.sort_by_key(|e| e.time);
        assert_eq!(
            events
                .iter()
                .map(|e| e.content.as_str())
                .collect::<Vec<_>>(),
            vec!["早", "中", "晚"]
        );
        events.drain(..events.len() - 2);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn day_file_uses_local_date() {
        // 2024-01-01 00:00:00 UTC = 08:00 CST → 2024-01-01
        assert!(day_file_name(1_704_067_200).starts_with("2024-01-01"));
        // 2024-01-01 20:00:00 UTC = 次日 04:00 CST → 2024-01-02
        assert!(day_file_name(1_704_144_000).starts_with("2024-01-02"));
    }
}
