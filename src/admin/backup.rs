use std::path::Path;
use tracing::debug;

use crate::config;

/// 已经迁入状态库的数据类型
///
/// 这些类型不再有对应的 JSON 文件，备份要走 SQLite 的在线快照
/// （见 `db::Db::snapshot_into`）。留在这里是因为调用点仍然按
/// "改之前先备份"调用 `before_modify`。
const IN_STATE_DB: [&str; 3] = ["emotion", "blocklist", "working_memory"];

fn source_file(data_type: &str) -> Option<std::path::PathBuf> {
    let dir = config::data_dir();
    match data_type {
        "archive" => Some(dir.join("archive.json")),
        _ => None,
    }
}

/// 备份路径：`backups/{类型}/{类型}_{时间戳}.{扩展名}`
fn backup_target(data_type: &str, extension: &str) -> Option<std::path::PathBuf> {
    let backup_dir = config::data_dir().join("backups").join(data_type);
    std::fs::create_dir_all(&backup_dir).ok()?;
    let ts = super::format_timestamp(crate::util::now_secs());
    Some(backup_dir.join(format!("{data_type}_{ts}.{extension}")))
}

/// 改状态库里的数据之前先取一份一致快照
fn backup_state_db(data_type: &str) -> bool {
    let Some(target) = backup_target(data_type, "db") else {
        return false;
    };
    match crate::db::db().snapshot_into(&target) {
        Ok(()) => {
            debug!(path = ?target, data_type, "backup: 状态库快照");
            true
        }
        Err(error) => {
            tracing::warn!(%error, data_type, "backup: 状态库快照失败");
            false
        }
    }
}

pub fn before_modify(data_type: &str) {
    if IN_STATE_DB.contains(&data_type) {
        backup_state_db(data_type);
        return;
    }
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
    let types = ["working_memory", "emotion", "blocklist", "archive"];
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
