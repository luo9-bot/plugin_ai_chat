//! 日记：她的情景记忆
//!
//! 睡前整理时她亲笔写的日记，永久保存（索引 json + 按月 md 人类可读）。
//! 联想回忆（P2）以日记为主要语料。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::warn;

use crate::config;
use crate::util;

/// 日记容量上限：索引里的条目数（她的日记不删，但给 admin 一次量级警告线）
const MAX_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiaryEntry {
    /// 形如 "2026-06-28#3"
    pub id: String,
    /// 东八区日期 "YYYY-MM-DD"
    pub date: String,
    /// 第一人称日记内容
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feeling: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<u64>,
}

fn dir() -> PathBuf {
    config::data_dir().join("mind").join("diary")
}

fn index_path() -> PathBuf {
    dir().join("index.json")
}

fn month_md(date: &str) -> PathBuf {
    let month = &date[..7];
    dir().join(format!("{month}.md"))
}

fn load_index() -> Vec<DiaryEntry> {
    let Ok(content) = fs::read_to_string(index_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "diary: 索引解析失败，按空处理");
        Vec::new()
    })
}

fn save_index(entries: &[DiaryEntry]) {
    match serde_json::to_string_pretty(entries) {
        Ok(json) => {
            if let Err(e) = crate::util::atomic_write(index_path(), json) {
                warn!(error = %e, "diary: 索引落盘失败");
            }
        }
        Err(e) => warn!(error = %e, "diary: 索引序列化失败"),
    }
}

/// 写入一批日记（睡前整理调用）
pub fn add(entries: Vec<DiaryEntry>) {
    if entries.is_empty() {
        return;
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index = load_index();
    let mut md_appends: Vec<(String, String)> = Vec::new();

    for mut entry in entries {
        if entry.content.trim().is_empty() {
            continue;
        }
        let date = if entry.date.is_empty() {
            util::ts_to_date_str(util::now_secs())
        } else {
            entry.date.clone()
        };
        // 同日序号：接在现有同日条目之后
        let seq = index.iter().filter(|e| e.date == date).count() + 1;
        entry.id = format!("{date}#{seq}");
        entry.date = date.clone();

        let md_line = match &entry.feeling {
            Some(f) => format!("- [{f}] {}\n", entry.content),
            None => format!("- {}\n", entry.content),
        };
        md_appends.push((month_md(&date).to_string_lossy().to_string(), md_line));
        index.push(entry);
    }

    while index.len() > MAX_ENTRIES {
        index.remove(0);
    }
    save_index(&index);

    // 按月 md 追加（人类可读视图）
    for (path, line) in md_appends {
        use std::io::Write as _;
        if let Err(e) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(line.as_bytes()))
        {
            warn!(error = %e, path = %path, "diary: md 追加失败");
        }
    }
}

/// 最近的日记（旧在前）
pub fn recent(n: usize) -> Vec<DiaryEntry> {
    let index = load_index();
    let start = index.len().saturating_sub(n);
    index[start..].to_vec()
}

/// 涉及某人的日记条数（清洗用）
pub fn purge_about(uid: u64) -> usize {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut index = load_index();
    let before = index.len();
    index.retain(|e| e.about != Some(uid));
    let removed = before - index.len();
    if removed > 0 {
        save_index(&index);
    }
    removed
}

static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_serializes_compactly() {
        let entry = DiaryEntry {
            id: "2026-06-28#1".into(),
            date: "2026-06-28".into(),
            content: "今天和豆聊了火锅".into(),
            feeling: Some("开心".into()),
            about: Some(42),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""feeling":"开心""#));
        let back: DiaryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "2026-06-28#1");
    }
}
