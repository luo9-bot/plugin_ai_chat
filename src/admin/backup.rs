use std::path::Path;
use tracing::debug;

use crate::config;

fn source_file(data_type: &str) -> Option<std::path::PathBuf> {
    let dir = config::data_dir();
    match data_type {
        "self_memory" => Some(dir.join("self_memory.json")),
        "memory" => Some(dir.join("memory.json")),
        "working_memory" => Some(dir.join("working_memory.json")),
        "personality" => Some(dir.join("personality.json")),
        "emotion" => Some(dir.join("emotion.json")),
        "mental_state" => Some(dir.join("mental_state.json")),
        "blocklist" => Some(dir.join("blocklist.json")),
        "proactive" => Some(dir.join("proactive.json")),
        "proactive_config" => Some(dir.join("proactive_config.json")),
        "archive" => Some(dir.join("archive.json")),
        _ => None,
    }
}

pub fn before_modify(data_type: &str) {
    let src = match source_file(data_type) {
        Some(s) => s,
        None => return,
    };
    if !src.exists() {
        return;
    }
    let backup_dir = config::data_dir().join("backups").join(data_type);
    std::fs::create_dir_all(&backup_dir).ok();
    let ts = super::format_timestamp(crate::util::now_secs());
    let name = format!("{}_{}.json", data_type, ts);
    let dst = backup_dir.join(&name);
    if std::fs::copy(&src, &dst).is_ok() {
        debug!(data_type, backup = %name, "backup: created");
    }
    prune(&backup_dir, 20);
}

fn prune(dir: &Path, max_count: usize) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|b| std::cmp::Reverse(b.file_name()));
    for entry in entries.into_iter().skip(max_count) {
        std::fs::remove_file(entry.path()).ok();
    }
}

pub fn list(data_type: &str) -> serde_json::Value {
    let dir = config::data_dir().join("backups").join(data_type);
    let mut items = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            items.push(serde_json::json!({"filename": name, "size": size}));
        }
    }
    items.sort_by(|a, b| {
        b["filename"]
            .as_str()
            .unwrap_or("")
            .cmp(a["filename"].as_str().unwrap_or(""))
    });
    serde_json::json!({"backups": items})
}

pub fn list_all_types() -> serde_json::Value {
    let types = [
        "self_memory",
        "memory",
        "working_memory",
        "personality",
        "emotion",
        "mental_state",
        "blocklist",
        "proactive",
        "proactive_config",
        "archive",
    ];
    let mut counts = serde_json::Map::new();
    for t in &types {
        let dir = config::data_dir().join("backups").join(t);
        let count = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        counts.insert(t.to_string(), serde_json::json!(count));
    }
    serde_json::json!({"types": types, "counts": counts})
}

pub fn restore(data_type: &str, filename: &str) -> Result<(), String> {
    // 安全校验：仅允许安全字符
    if !filename
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err("invalid filename".into());
    }
    if !filename.ends_with(".json") {
        return Err("filename must end with .json".into());
    }
    let src = match source_file(data_type) {
        Some(s) => s,
        None => return Err("unknown data type".into()),
    };
    let backup_path = config::data_dir()
        .join("backups")
        .join(data_type)
        .join(filename);
    if !backup_path.exists() {
        return Err("backup file not found".into());
    }
    // 恢复前先备份当前状态
    before_modify(data_type);
    std::fs::copy(&backup_path, &src).map_err(|e| format!("restore failed: {}", e))?;
    tracing::info!(data_type, filename, "backup: restored");
    Ok(())
}
