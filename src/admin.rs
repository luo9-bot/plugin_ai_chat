use tiny_http::{Header, Method, Request, Response, Server};
use tracing::{debug, info, warn};

use crate::config;

// ── 工具函数 ────────────────────────────────────────────────────

fn now_secs() -> u64 {
    crate::util::now_secs()
}

fn json_response(status: u16, body: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body_str = body.to_string();
    Response::from_string(body_str)
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
        )
        .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
        .with_header(
            Header::from_bytes("Access-Control-Allow-Headers", "Authorization, Content-Type")
                .unwrap(),
        )
        .with_header(
            Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
                .unwrap(),
        )
}

fn ok(body: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(200, body)
}

fn err(status: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(status, serde_json::json!({"error": msg}))
}

fn read_body(request: &mut Request) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body: {}", e))?;
    Ok(buf)
}

fn parse_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|e| format!("parse json: {}", e))
}

fn check_auth(request: &Request) -> bool {
    let token = &config::get().admin.token;
    if token.is_empty() {
        return true;
    }
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().to_string().to_lowercase() == "authorization")
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .map(|t| t == token)
        .unwrap_or(false)
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts % 60;
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    // 简单用 unix timestamp 做文件名，不依赖 chrono
    format!("{}{:02}{:02}{:02}", ts / 86400, hours, mins, secs)
}

// ── 备份模块 ────────────────────────────────────────────────────

mod backup {
    use super::*;
    use std::path::Path;

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
        let ts = format_timestamp(now_secs());
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
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
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
        info!(data_type, filename, "backup: restored");
        Ok(())
    }
}

// ── Handler: 自我记忆 ──────────────────────────────────────────

fn handle_self_thoughts(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("self_memory.json");

    // POST /api/self-thoughts/batch -> 批量删除
    if *method == Method::Post && segs.first() == Some(&"batch") {
        let body_val: serde_json::Value = match parse_json(body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let indices: Vec<usize> = match body_val.get("indices").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect(),
            None => return err(400, "indices required"),
        };
        backup::before_modify("self_memory");
        let mut store: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                .unwrap_or(serde_json::json!({}));
        let thoughts = store.get_mut("thoughts").and_then(|v| v.as_array_mut()).unwrap();
        let mut sorted = indices;
        sorted.sort_unstable();
        sorted.dedup();
        // 从后往前删，避免索引偏移
        let mut deleted = 0;
        for idx in sorted.into_iter().rev() {
            if idx < thoughts.len() {
                thoughts.remove(idx);
                deleted += 1;
            }
        }
        std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
        return ok(serde_json::json!({"ok": true, "deleted": deleted}));
    }

    // GET /api/self-thoughts/export -> 导出
    if *method == Method::Get && segs.first() == Some(&"export") {
        let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
        return Response::from_string(data)
            .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
            .with_header(Header::from_bytes("Content-Disposition", "attachment; filename=\"self_memory.json\"").unwrap());
    }

    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
            ok(store)
        }
        Method::Post => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let content = match body_val.get("content").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => c,
                _ => return err(400, "content required"),
            };
            let category = body_val
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("reflection");
            backup::before_modify("self_memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            let thoughts = store
                .get_mut("thoughts")
                .and_then(|v| v.as_array_mut())
                .unwrap();
            thoughts.push(serde_json::json!({
                "content": content,
                "category": category,
                "created": now_secs()
            }));
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Put => {
            let idx: usize = match segs.first().and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("self_memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            let thoughts = store
                .get_mut("thoughts")
                .and_then(|v| v.as_array_mut())
                .unwrap();
            if idx >= thoughts.len() {
                return err(404, "index out of range");
            }
            if let Some(content) = body_val.get("content").and_then(|v| v.as_str()) {
                thoughts[idx]["content"] = serde_json::json!(content);
            }
            if let Some(category) = body_val.get("category").and_then(|v| v.as_str()) {
                thoughts[idx]["category"] = serde_json::json!(category);
            }
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Delete => {
            let idx: usize = match segs.first().and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            backup::before_modify("self_memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            let thoughts = store
                .get_mut("thoughts")
                .and_then(|v| v.as_array_mut())
                .unwrap();
            if idx >= thoughts.len() {
                return err(404, "index out of range");
            }
            thoughts.remove(idx);
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 用户记忆 ──────────────────────────────────────────

fn handle_memory(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("memory.json");

    // GET /api/memory/export -> 导出全部
    if *method == Method::Get && segs.first() == Some(&"export") {
        let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
        return Response::from_string(data)
            .with_header(Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap())
            .with_header(Header::from_bytes("Content-Disposition", "attachment; filename=\"memory.json\"").unwrap());
    }

    // POST /api/memory/{user_id}/batch -> 批量删除
    if *method == Method::Post && segs.len() == 2 && segs[1] == "batch" {
        let uid = segs[0];
        let body_val: serde_json::Value = match parse_json(body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let indices: Vec<usize> = match body_val.get("indices").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect(),
            None => return err(400, "indices required"),
        };
        backup::before_modify("memory");
        let mut store: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                .unwrap_or(serde_json::json!({"users": {}}));
        let users = store.get_mut("users").and_then(|v| v.as_object_mut()).unwrap();
        if let Some(user) = users.get_mut(uid) {
            let entries = user.get_mut("entries").and_then(|v| v.as_array_mut()).unwrap();
            let mut sorted = indices;
            sorted.sort_unstable();
            sorted.dedup();
            let mut deleted = 0;
            for idx in sorted.into_iter().rev() {
                if idx < entries.len() {
                    entries.remove(idx);
                    deleted += 1;
                }
            }
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            return ok(serde_json::json!({"ok": true, "deleted": deleted}));
        }
        return err(404, "user not found");
    }

    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({"users": {}}));
            // /api/memory -> 完整 store
            // /api/memory/{user_id} -> 单用户
            if let Some(uid) = segs.first() {
                let users = store.get("users").and_then(|v| v.as_object()).unwrap();
                match users.get(*uid) {
                    Some(user) => ok(serde_json::json!({"user_id": uid, "entries": user["entries"]})),
                    None => ok(serde_json::json!({"user_id": uid, "entries": []})),
                }
            } else {
                ok(store)
            }
        }
        Method::Post => {
            let uid = match segs.first() {
                Some(u) => *u,
                None => return err(400, "user_id required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let content = match body_val.get("content").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => c,
                _ => return err(400, "content required"),
            };
            let importance = body_val
                .get("importance")
                .and_then(|v| v.as_str())
                .unwrap_or("normal");
            backup::before_modify("memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"users": {}}));
            let users = store
                .get_mut("users")
                .and_then(|v| v.as_object_mut())
                .unwrap();
            let user = users
                .entry(uid.to_string())
                .or_insert(serde_json::json!({"entries": []}));
            let entries = user
                .get_mut("entries")
                .and_then(|v| v.as_array_mut())
                .unwrap();
            entries.push(serde_json::json!({
                "content": content,
                "importance": serde_json::from_str::<serde_json::Value>(&format!("\"{}\"", importance)).unwrap_or(serde_json::json!("Normal")),
                "created": now_secs(),
                "last_accessed": now_secs(),
                "access_count": 1
            }));
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Put => {
            let uid = match segs.first() {
                Some(u) => *u,
                None => return err(400, "user_id required"),
            };
            let idx: usize = match segs.get(1).and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"users": {}}));
            let users = store
                .get_mut("users")
                .and_then(|v| v.as_object_mut())
                .unwrap();
            if let Some(user) = users.get_mut(uid) {
                let entries = user
                    .get_mut("entries")
                    .and_then(|v| v.as_array_mut())
                    .unwrap();
                if idx >= entries.len() {
                    return err(404, "index out of range");
                }
                if let Some(content) = body_val.get("content").and_then(|v| v.as_str()) {
                    entries[idx]["content"] = serde_json::json!(content);
                }
                if let Some(importance) = body_val.get("importance").and_then(|v| v.as_str()) {
                    entries[idx]["importance"] = serde_json::from_str::<serde_json::Value>(&format!("\"{}\"", importance)).unwrap_or(serde_json::json!("Normal"));
                }
                entries[idx]["last_accessed"] = serde_json::json!(now_secs());
                std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                ok(serde_json::json!({"ok": true}))
            } else {
                err(404, "user not found")
            }
        }
        Method::Delete => {
            let uid = match segs.first() {
                Some(u) => *u,
                None => return err(400, "user_id required"),
            };
            let idx: usize = match segs.get(1).and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            backup::before_modify("memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"users": {}}));
            let users = store
                .get_mut("users")
                .and_then(|v| v.as_object_mut())
                .unwrap();
            if let Some(user) = users.get_mut(uid) {
                let entries = user
                    .get_mut("entries")
                    .and_then(|v| v.as_array_mut())
                    .unwrap();
                if idx >= entries.len() {
                    return err(404, "index out of range");
                }
                entries.remove(idx);
                std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                ok(serde_json::json!({"ok": true}))
            } else {
                err(404, "user not found")
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 工作记忆 ──────────────────────────────────────────

fn handle_working_memory(method: &Method, segs: &[&str], _body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("working_memory.json");
    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({"groups": {}}));
            if let Some(gid) = segs.first() {
                let groups = store.get("groups").and_then(|v| v.as_object()).unwrap();
                match groups.get(*gid) {
                    Some(group) => ok(serde_json::json!({"group_id": gid, "entries": group["entries"]})),
                    None => ok(serde_json::json!({"group_id": gid, "entries": []})),
                }
            } else {
                ok(store)
            }
        }
        Method::Delete => {
            let gid = match segs.first() {
                Some(g) => *g,
                None => return err(400, "group_id required"),
            };
            let idx: usize = match segs.get(1).and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            backup::before_modify("working_memory");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"groups": {}}));
            let groups = store
                .get_mut("groups")
                .and_then(|v| v.as_object_mut())
                .unwrap();
            if let Some(group) = groups.get_mut(gid) {
                let entries = group
                    .get_mut("entries")
                    .and_then(|v| v.as_array_mut())
                    .unwrap();
                if idx >= entries.len() {
                    return err(404, "index out of range");
                }
                entries.remove(idx);
                std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                ok(serde_json::json!({"ok": true}))
            } else {
                err(404, "group not found")
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 人格 ──────────────────────────────────────────────

fn handle_personality(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("personality.json");
    match method {
        Method::Get => {
            if segs.first() == Some(&"snapshots") {
                let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
                let store: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
                let snapshots = store
                    .get("snapshots")
                    .and_then(|v| v.as_object())
                    .map(|o| o.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                return ok(serde_json::json!({"snapshots": snapshots}));
            }
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let mut store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
            // 文件不存在时填充默认值
            if store.get("current").is_none() {
                store["current"] = serde_json::json!({
                    "name": "default",
                    "template": "default",
                    "traits": {
                        "humor": 0.5, "warmth": 0.6, "curiosity": 0.5,
                        "formality": 0.3, "verbosity": 0.4, "empathy": 0.6
                    },
                    "custom_prompt": ""
                });
            }
            if store.get("snapshots").is_none() {
                store["snapshots"] = serde_json::json!({});
            }
            ok(store)
        }
        Method::Put => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("personality");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            if let Some(current) = body_val.get("current") {
                store["current"] = current.clone();
            }
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Post => {
            // POST /api/personality/snapshots -> 保存快照
            // POST /api/personality/snapshots/{name}/load -> 加载快照
            if segs.first() == Some(&"snapshots") {
                if segs.len() >= 3 && segs.get(2) == Some(&"load") {
                    let name = segs[1];
                    backup::before_modify("personality");
                    let mut store: serde_json::Value = serde_json::from_str(
                        &std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()),
                    )
                    .unwrap_or(serde_json::json!({}));
                    let snapshot = store
                        .get("snapshots")
                        .and_then(|s| s.get(name))
                        .cloned();
                    if let Some(snap) = snapshot {
                        store["current"] = snap;
                        std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                        return ok(serde_json::json!({"ok": true}));
                    }
                    return err(404, "snapshot not found");
                }
                // 保存当前为快照
                let body_val: serde_json::Value = match parse_json(body) {
                    Ok(v) => v,
                    Err(e) => return err(400, &e),
                };
                let name = match body_val.get("name").and_then(|v| v.as_str()) {
                    Some(n) if !n.is_empty() => n,
                    _ => return err(400, "name required"),
                };
                backup::before_modify("personality");
                let mut store: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()),
                )
                .unwrap_or(serde_json::json!({}));
                let current = store.get("current").cloned().unwrap_or(serde_json::json!({
                    "name": "default", "template": "default",
                    "traits": {"humor": 0.5, "warmth": 0.6, "curiosity": 0.5, "formality": 0.3, "verbosity": 0.4, "empathy": 0.6},
                    "custom_prompt": ""
                }));
                if store.get("snapshots").is_none() {
                    store["snapshots"] = serde_json::json!({});
                }
                let snapshots = store
                    .get_mut("snapshots")
                    .and_then(|v| v.as_object_mut())
                    .unwrap();
                snapshots.insert(name.to_string(), current);
                std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                return ok(serde_json::json!({"ok": true}));
            }
            err(400, "invalid path")
        }
        Method::Delete => {
            // DELETE /api/personality/snapshots/{name}
            if segs.first() == Some(&"snapshots") {
                let name = match segs.get(1) {
                    Some(n) => *n,
                    None => return err(400, "snapshot name required"),
                };
                backup::before_modify("personality");
                let mut store: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()),
                )
                .unwrap_or(serde_json::json!({}));
                let snapshots = store
                    .get_mut("snapshots")
                    .and_then(|v| v.as_object_mut())
                    .unwrap();
                if snapshots.remove(name).is_some() {
                    std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                    ok(serde_json::json!({"ok": true}))
                } else {
                    err(404, "snapshot not found")
                }
            } else {
                err(400, "invalid path")
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 情绪 ──────────────────────────────────────────────

fn handle_emotion(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("emotion.json");
    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
            if let Some(uid) = segs.first() {
                match store.get(*uid) {
                    Some(state) => ok(serde_json::json!({"user_id": uid, "state": state})),
                    None => ok(serde_json::json!({"user_id": uid, "state": null})),
                }
            } else {
                ok(store)
            }
        }
        Method::Put => {
            let uid = match segs.first() {
                Some(u) => *u,
                None => return err(400, "user_id required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("emotion");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            store[uid] = body_val;
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 心理状态 ──────────────────────────────────────────

fn handle_mental_state(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("mental_state.json");
    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({"concerns": [], "deliberations": []}));
            ok(store)
        }
        Method::Post => {
            let sub = match segs.first() {
                Some(s) => *s,
                None => return err(400, "path: concerns or deliberations"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("mental_state");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"concerns": [], "deliberations": []}));
            match sub {
                "concerns" => {
                    let content = match body_val.get("content").and_then(|v| v.as_str()) {
                        Some(c) if !c.is_empty() => c,
                        _ => return err(400, "content required"),
                    };
                    let category = body_val
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("social");
                    let strength = body_val
                        .get("strength")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5) as f32;
                    let arr = store
                        .get_mut("concerns")
                        .and_then(|v| v.as_array_mut())
                        .unwrap();
                    arr.push(serde_json::json!({
                        "content": content,
                        "category": category,
                        "strength": strength,
                        "created": now_secs(),
                        "last_reinforced": now_secs(),
                        "trigger_user": 0,
                        "trigger_group": 0
                    }));
                }
                "deliberations" => {
                    let content = match body_val.get("content").and_then(|v| v.as_str()) {
                        Some(c) if !c.is_empty() => c,
                        _ => return err(400, "content required"),
                    };
                    let source = body_val
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("manual");
                    let strength = body_val
                        .get("strength")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.5) as f32;
                    let arr = store
                        .get_mut("deliberations")
                        .and_then(|v| v.as_array_mut())
                        .unwrap();
                    arr.push(serde_json::json!({
                        "content": content,
                        "source": source,
                        "strength": strength,
                        "created": now_secs(),
                        "last_reinforced": now_secs()
                    }));
                }
                _ => return err(400, "use concerns or deliberations"),
            }
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Delete => {
            let sub = match segs.first() {
                Some(s) => *s,
                None => return err(400, "path: concerns or deliberations"),
            };
            let idx: usize = match segs.get(1).and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            backup::before_modify("mental_state");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({"concerns": [], "deliberations": []}));
            let key = match sub {
                "concerns" => "concerns",
                "deliberations" => "deliberations",
                _ => return err(400, "use concerns or deliberations"),
            };
            let arr = store
                .get_mut(key)
                .and_then(|v| v.as_array_mut())
                .unwrap();
            if idx >= arr.len() {
                return err(404, "index out of range");
            }
            arr.remove(idx);
            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 黑名单 ──────────────────────────────────────────

fn handle_blocklist(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("blocklist.json");
    match method {
        Method::Get => {
            let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".into());
            let list: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!([]));
            ok(serde_json::json!({"blocked": list}))
        }
        Method::Post => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let uid = match body_val.get("user_id").and_then(|v| v.as_u64()) {
                Some(u) => u,
                None => return err(400, "user_id required"),
            };
            backup::before_modify("blocklist");
            let mut list: Vec<u64> =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".into()))
                    .unwrap_or_default();
            if !list.contains(&uid) {
                list.push(uid);
            }
            std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        Method::Delete => {
            let uid: u64 = match segs.first().and_then(|s| s.parse().ok()) {
                Some(u) => u,
                None => return err(400, "user_id required"),
            };
            backup::before_modify("blocklist");
            let mut list: Vec<u64> =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".into()))
                    .unwrap_or_default();
            list.retain(|&x| x != uid);
            std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 主动对话 ──────────────────────────────────────────

fn handle_proactive(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let state_path = config::data_dir().join("proactive.json");
    let config_path = config::data_dir().join("proactive_config.json");
    match method {
        Method::Get => {
            if segs.first() == Some(&"config") {
                let data = std::fs::read_to_string(&config_path).unwrap_or_else(|_| "{}".into());
                let cfg: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
                return ok(cfg);
            }
            let data = std::fs::read_to_string(&state_path).unwrap_or_else(|_| "{}".into());
            let store: serde_json::Value =
                serde_json::from_str(&data).unwrap_or(serde_json::json!({}));
            if let Some(uid) = segs.first() {
                match store.get(*uid) {
                    Some(state) => ok(serde_json::json!({"user_id": uid, "state": state})),
                    None => ok(serde_json::json!({"user_id": uid, "state": null})),
                }
            } else {
                ok(store)
            }
        }
        Method::Put => {
            if segs.first() == Some(&"config") {
                let body_val: serde_json::Value = match parse_json(body) {
                    Ok(v) => v,
                    Err(e) => return err(400, &e),
                };
                backup::before_modify("proactive_config");
                std::fs::write(&config_path, serde_json::to_string_pretty(&body_val).unwrap()).ok();
                return ok(serde_json::json!({"ok": true}));
            }
            let uid = match segs.first() {
                Some(u) => *u,
                None => return err(400, "user_id required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("proactive");
            let mut store: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap_or_else(|_| "{}".into()))
                    .unwrap_or(serde_json::json!({}));
            store[uid] = body_val;
            std::fs::write(&state_path, serde_json::to_string_pretty(&store).unwrap()).ok();
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 归档 ──────────────────────────────────────────────

fn handle_archive() -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("archive.json");
    let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let store: serde_json::Value =
        serde_json::from_str(&data).unwrap_or(serde_json::json!({"working_memory": [], "long_term": []}));
    ok(store)
}

// ── Handler: 备份 ──────────────────────────────────────────────

fn handle_backups(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            if let Some(data_type) = segs.first() {
                ok(backup::list(data_type))
            } else {
                ok(backup::list_all_types())
            }
        }
        Method::Post => {
            // POST /api/backups/restore
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let data_type = match body_val.get("type").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => return err(400, "type required"),
            };
            let filename = match body_val.get("filename").and_then(|v| v.as_str()) {
                Some(f) => f,
                None => return err(400, "filename required"),
            };
            match backup::restore(data_type, filename) {
                Ok(()) => ok(serde_json::json!({"ok": true})),
                Err(e) => err(400, &e),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 同步 ──────────────────────────────────────────────

fn handle_sync(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let action = match segs.first() {
        Some(a) => *a,
        None => return err(400, "action required: push, pull, or status"),
    };
    match (method, action) {
        (Method::Get, "status") => {
            let cfg = &config::get().sync;
            ok(serde_json::json!({
                "enabled": cfg.enabled,
                "api_url": cfg.api_url,
                "db_name": cfg.db_name,
            }))
        }
        (Method::Post, "push") => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let data_type = body_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("self_memory");
            if data_type != "self_memory" {
                return err(400, "only self_memory sync is supported");
            }
            if !config::get().sync.enabled {
                return err(400, "sync not enabled in config");
            }
            match crate::self_memory::sync_all_to_remote() {
                Ok(count) => ok(serde_json::json!({"synced": count})),
                Err(e) => err(500, &e),
            }
        }
        (Method::Post, "pull") => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let data_type = body_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("self_memory");
            let mode = body_val
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("merge");
            if data_type != "self_memory" {
                return err(400, "only self_memory sync is supported");
            }
            if !config::get().sync.enabled {
                return err(400, "sync not enabled in config");
            }
            // 从远程拉取数据
            match crate::self_memory::remote_list_all() {
                Ok(remote_data) => {
                    let remote_thoughts = remote_data
                        .get("thoughts")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    if remote_thoughts.is_empty() {
                        return ok(serde_json::json!({"pulled": 0}));
                    }
                    let path = config::data_dir().join("self_memory.json");
                    backup::before_modify("self_memory");
                    match mode {
                        "replace" => {
                            let new_store = serde_json::json!({"thoughts": remote_thoughts});
                            std::fs::write(&path, serde_json::to_string_pretty(&new_store).unwrap()).ok();
                            ok(serde_json::json!({"pulled": remote_thoughts.len(), "mode": "replace"}))
                        }
                        _ => {
                            // merge: 按 content+created 去重
                            let mut store: serde_json::Value = serde_json::from_str(
                                &std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into()),
                            )
                            .unwrap_or(serde_json::json!({"thoughts": []}));
                            let local = store
                                .get("thoughts")
                                .and_then(|v| v.as_array())
                                .unwrap();
                            let existing: std::collections::HashSet<String> = local
                                .iter()
                                .map(|t| {
                                    format!(
                                        "{}:{}",
                                        t.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                        t.get("created").and_then(|v| v.as_u64()).unwrap_or(0)
                                    )
                                })
                                .collect();
                            let thoughts = store
                                .get_mut("thoughts")
                                .and_then(|v| v.as_array_mut())
                                .unwrap();
                            let mut added = 0;
                            for t in &remote_thoughts {
                                let key = format!(
                                    "{}:{}",
                                    t.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                                    t.get("created").and_then(|v| v.as_u64()).unwrap_or(0)
                                );
                                if !existing.contains(&key) {
                                    thoughts.push(t.clone());
                                    added += 1;
                                }
                            }
                            std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).ok();
                            ok(serde_json::json!({"pulled": added, "mode": "merge"}))
                        }
                    }
                }
                Err(e) => err(500, &format!("fetch remote failed: {}", e)),
            }
        }
        (Method::Get, "deleted") => {
            if !config::get().sync.enabled {
                return err(400, "sync not enabled");
            }
            match crate::self_memory::remote_list_deleted() {
                Ok(data) => ok(data),
                Err(e) => err(500, &e),
            }
        }
        (Method::Post, "restore") => {
            if !config::get().sync.enabled {
                return err(400, "sync not enabled");
            }
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let id = match body_val.get("id").and_then(|v| v.as_str()) {
                Some(i) => i,
                None => return err(400, "id required"),
            };
            match crate::self_memory::remote_restore(id) {
                Ok(()) => ok(serde_json::json!({"ok": true})),
                Err(e) => err(500, &e),
            }
        }
        (Method::Post, "purge") => {
            if !config::get().sync.enabled {
                return err(400, "sync not enabled");
            }
            match crate::self_memory::remote_purge() {
                Ok(count) => ok(serde_json::json!({"purged": count})),
                Err(e) => err(500, &e),
            }
        }
        (Method::Delete, "remote") => {
            if !config::get().sync.enabled {
                return err(400, "sync not enabled");
            }
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let id = match body_val.get("id").and_then(|v| v.as_str()) {
                Some(i) => i,
                None => return err(400, "id required"),
            };
            match crate::self_memory::remote_delete(id) {
                Ok(()) => ok(serde_json::json!({"ok": true})),
                Err(e) => err(500, &e),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 主路由 ─────────────────────────────────────────────────────

fn route(request: &mut Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let method = request.method().clone();
    let url = request.url().to_string();

    // CORS 预检
    if method == Method::Options {
        return ok(serde_json::json!({"ok": true}));
    }

    // 静态页面
    if method == Method::Get && (url == "/" || url == "/index.html") {
        return Response::from_string(crate::admin_ui::HTML)
            .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    }

    // 登录端点（不需要 auth）
    if method == Method::Post && url == "/api/login" {
        let body = match read_body(request) {
            Ok(b) => b,
            Err(e) => return err(400, &e),
        };
        let val: serde_json::Value = match parse_json(&body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let token = val.get("token").and_then(|v| v.as_str()).unwrap_or("");
        if token == config::get().admin.token {
            return ok(serde_json::json!({"ok": true}));
        }
        return err(403, "invalid token");
    }

    // 认证检查
    if !check_auth(request) {
        return err(401, "unauthorized");
    }

    // 读取 body（如果有）
    let body = if matches!(method, Method::Post | Method::Put) {
        match read_body(request) {
            Ok(b) => b,
            Err(e) => return err(400, &e),
        }
    } else {
        Vec::new()
    };

    // 解析路径
    let path = url.split('?').next().unwrap_or(&url);
    let segs = path_segments(path);

    if segs.is_empty() || segs[0] != "api" {
        return err(404, "not found");
    }

    let api_segs = &segs[1..]; // 去掉 "api"

    match api_segs.first() {
        Some(&"self-thoughts") => handle_self_thoughts(&method, &api_segs[1..], &body),
        Some(&"memory") => handle_memory(&method, &api_segs[1..], &body),
        Some(&"working-memory") => handle_working_memory(&method, &api_segs[1..], &body),
        Some(&"personality") => handle_personality(&method, &api_segs[1..], &body),
        Some(&"emotion") => handle_emotion(&method, &api_segs[1..], &body),
        Some(&"mental-state") => handle_mental_state(&method, &api_segs[1..], &body),
        Some(&"blocklist") => handle_blocklist(&method, &api_segs[1..], &body),
        Some(&"proactive") => handle_proactive(&method, &api_segs[1..], &body),
        Some(&"archive") => {
            if method == Method::Get {
                handle_archive()
            } else {
                err(405, "method not allowed")
            }
        }
        Some(&"backups") => handle_backups(&method, &api_segs[1..], &body),
        Some(&"sync") => handle_sync(&method, &api_segs[1..], &body),
        Some(&"anti-injection") => handle_anti_injection(&method, &api_segs[1..], &body),
        Some(&"conversations") => handle_conversations(&method, &api_segs[1..]),
        Some(&"config") => handle_config(&method, &body),
        _ => err(404, "not found"),
    }
}

// ── 防注入状态管理 ────────────────────────────────────────────────

fn handle_anti_injection(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            // GET /api/anti-injection/users - 获取所有用户风险状态
            if segs.first() == Some(&"users") {
                let users = crate::anti_injection::get_all_user_statuses();
                return ok(serde_json::json!({"users": users}));
            }
            // GET /api/anti-injection/:user_id - 获取特定用户状态
            if let Some(&user_id_str) = segs.first() {
                if let Ok(user_id) = user_id_str.parse::<u64>() {
                    let status = crate::anti_injection::get_user_status(user_id);
                    let reputation = crate::anti_injection::get_reputation(user_id);
                    let violation_count = crate::anti_injection::get_violation_count(user_id);
                    let vision_disabled = crate::anti_injection::is_vision_disabled(user_id);
                    let silent_banned = crate::anti_injection::is_silent_banned(user_id);
                    let penalty = crate::anti_injection::get_penalty_multiplier(user_id);

                    return ok(serde_json::json!({
                        "user_id": user_id,
                        "status": status,
                        "reputation": reputation,
                        "violation_count": violation_count,
                        "vision_disabled": vision_disabled,
                        "silent_banned": silent_banned,
                        "penalty_multiplier": penalty,
                    }));
                }
            }

            // 返回配置信息
            let cfg = &config::get().anti_injection;
            ok(serde_json::json!({
                "config": {
                    "input": {
                        "max_message_length": cfg.input.max_message_length,
                        "sensitive_action": cfg.input.sensitive_action,
                    },
                    "output": {
                        "action": cfg.output.action,
                    },
                    "behavior": {
                        "rate_limit": cfg.behavior.rate_limit,
                        "max_messages_per_minute": cfg.behavior.max_messages_per_minute,
                        "max_messages_per_hour": cfg.behavior.max_messages_per_hour,
                        "reputation_threshold": cfg.behavior.reputation_threshold,
                        "auto_ban": cfg.behavior.auto_ban,
                        "auto_ban_threshold": cfg.behavior.auto_ban_threshold,
                    }
                },
                "note": "关键词过滤、注入模式检测、编码绕过检测、色情/暴力/违法内容检测、输出检测始终强制开启"
            }))
        }
        Method::Post => {
            // POST /api/anti-injection/unban - 解封用户
            // POST /api/anti-injection/enable-vision - 启用识图
            // POST /api/anti-injection/reset-reputation - 重置信誉
            let action = segs.first().unwrap_or(&"");
            let params: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => return err(400, &format!("invalid json: {}", e)),
            };
            let user_id = match params.get("user_id").and_then(|v| v.as_u64()) {
                Some(id) => id,
                None => return err(400, "missing user_id"),
            };

            match *action {
                "unban" => {
                    crate::anti_injection::unban_user(user_id);
                    ok(serde_json::json!({"success": true, "message": format!("用户{}已解封", user_id)}))
                }
                "enable-vision" => {
                    crate::anti_injection::enable_vision(user_id);
                    ok(serde_json::json!({"success": true, "message": format!("用户{}识图已启用", user_id)}))
                }
                "reset-reputation" => {
                    crate::anti_injection::reset_reputation(user_id);
                    ok(serde_json::json!({"success": true, "message": format!("用户{}信誉已重置", user_id)}))
                }
                "silent-ban" => {
                    crate::anti_injection::silent_ban_user(user_id);
                    ok(serde_json::json!({"success": true, "message": format!("用户{}已静默封禁", user_id)}))
                }
                "ban" => {
                    crate::anti_injection::ban_user(user_id);
                    ok(serde_json::json!({"success": true, "message": format!("用户{}已完全封禁", user_id)}))
                }
                _ => err(404, "unknown action"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 配置管理 ──────────────────────────────────────────────────

/// 按 config.example.yaml 的逻辑顺序重排 JSON key
fn reorder_config_keys(val: &serde_json::Value) -> serde_json::Value {
    use serde_json::json;
    let obj = match val.as_object() {
        Some(o) => o,
        None => return val.clone(),
    };
    let get = |k: &str| obj.get(k).cloned().unwrap_or(json!(null));
    // 顶层顺序
    let ordered = json!({
        "api_key": get("api_key"),
        "base_url": get("base_url"),
        "model": get("model"),
        "self_qq": get("self_qq"),
        "admin_qq": get("admin_qq"),
        "prompts": get("prompts"),
        "vision": get("vision"),
        "sync": get("sync"),
        "admin": get("admin"),
        "ai": get("ai"),
        "conversation": get("conversation"),
        "memory": get("memory"),
        "emotion": get("emotion"),
        "proactive": get("proactive"),
        "self_reflection": get("self_reflection"),
        "mental_state": get("mental_state"),
        "style": get("style"),
        "log": get("log"),
        "anti_injection": get("anti_injection"),
        "whitelist": get("whitelist"),
        "blacklist": get("blacklist"),
        "auto_start_users": get("auto_start_users"),
        "messages": get("messages"),
    });
    ordered
}

fn handle_config(method: &Method, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let config_path = config::data_dir().join("config.yaml");
    match method {
        Method::Get => {
            let data = match std::fs::read_to_string(&config_path) {
                Ok(d) => d,
                Err(_) => return err(404, "config.yaml not found"),
            };
            let mut cfg: serde_json::Value = match serde_yaml::from_str(&data) {
                Ok(v) => v,
                Err(e) => return err(500, &format!("parse config: {}", e)),
            };
            // 脱敏：隐藏 api_key
            if let Some(obj) = cfg.as_object_mut() {
                if let Some(key) = obj.get_mut("api_key") {
                    if let Some(s) = key.as_str() {
                        if s.len() > 8 {
                            *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len()-4..]));
                        }
                    }
                }
                if let Some(v) = obj.get_mut("vision").and_then(|v| v.as_object_mut()) {
                    if let Some(key) = v.get_mut("api_key") {
                        if let Some(s) = key.as_str() {
                            if s.len() > 8 {
                                *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len()-4..]));
                            }
                        }
                    }
                }
            }
            ok(cfg)
        }
        Method::Put => {
            let new_cfg: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => return err(400, &format!("invalid json: {}", e)),
            };
            // 读取现有配置以保留被脱敏的字段
            let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
            let existing_cfg: serde_json::Value = serde_yaml::from_str(&existing).unwrap_or(serde_json::json!({}));

            // 合并：新配置中包含 "..." 的字段保留原值
            let mut merged = new_cfg.clone();
            if let (Some(new_obj), Some(old_obj)) = (merged.as_object_mut(), existing_cfg.as_object()) {
                // api_key
                if let Some(key) = new_obj.get("api_key").and_then(|v| v.as_str()) {
                    if key.contains("...") {
                        if let Some(old_key) = old_obj.get("api_key") {
                            new_obj.insert("api_key".to_string(), old_key.clone());
                        }
                    }
                }
                // vision.api_key
                if let (Some(new_vis), Some(old_vis)) = (
                    new_obj.get_mut("vision").and_then(|v| v.as_object_mut()),
                    old_obj.get("vision").and_then(|v| v.as_object())
                ) {
                    if let Some(key) = new_vis.get("api_key").and_then(|v| v.as_str()) {
                        if key.contains("...") {
                            if let Some(old_key) = old_vis.get("api_key") {
                                new_vis.insert("api_key".to_string(), old_key.clone());
                            }
                        }
                    }
                }
            }

            // 按 config.example.yaml 顺序重排 key 后转为 YAML
            let ordered = reorder_config_keys(&merged);
            let yaml = match serde_yaml::to_string(&ordered) {
                Ok(y) => y,
                Err(e) => return err(500, &format!("serialize: {}", e)),
            };
            if let Err(e) = std::fs::write(&config_path, &yaml) {
                return err(500, &format!("write config: {}", e));
            }
            ok(serde_json::json!({"ok": true, "message": "配置已保存，重启插件后生效"}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 对话管理 ──────────────────────────────────────────────────

fn handle_conversations(method: &Method, segs: &[&str]) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            // GET /api/conversations — 列出所有活跃群聊和私聊
            let groups = crate::get_active_groups();
            let users = crate::get_active_users();
            ok(serde_json::json!({
                "groups": groups,
                "private_users": users,
            }))
        }
        Method::Post => {
            // POST /api/conversations/group/{id}/enable
            // POST /api/conversations/group/{id}/disable
            // POST /api/conversations/private/{id}/enable
            // POST /api/conversations/private/{id}/disable
            if segs.len() < 3 {
                return err(400, "path: /api/conversations/{group|private}/{id}/{enable|disable}");
            }
            let kind = segs[0];
            let id: u64 = match segs[1].parse() {
                Ok(v) => v,
                Err(_) => return err(400, "invalid id"),
            };
            let enable = match segs[2] {
                "enable" => true,
                "disable" => false,
                _ => return err(400, "action must be enable or disable"),
            };

            let changed = match kind {
                "group" => crate::toggle_group_chat(id, enable),
                "private" => crate::toggle_private_chat(id, enable),
                _ => return err(400, "kind must be group or private"),
            };

            let action = if enable { "开启" } else { "关闭" };
            let target = if kind == "group" { format!("群{}", id) } else { format!("用户{}", id) };
            ok(serde_json::json!({
                "ok": true,
                "changed": changed,
                "message": if changed { format!("已{}{}", action, target) } else { format!("{}已处于{}状态", target, action) }
            }))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 启动服务器 ──────────────────────────────────────────────────

pub fn start_server() {
    let cfg = &config::get().admin;
    let base_port = cfg.port;
    let mut port = base_port;
    let server = loop {
        let addr = format!("0.0.0.0:{}", port);
        match Server::http(&addr) {
            Ok(s) => break s,
            Err(e) => {
                if port < base_port + 5 {
                    warn!(port, error = %e, "admin: port busy, trying next");
                    port += 1;
                } else {
                    warn!(error = %e, "admin: failed to bind any port, giving up");
                    return;
                }
            }
        }
    };
    info!(port, "admin: server started at http://0.0.0.0:{}", port);

    for mut request in server.incoming_requests() {
        let response = route(&mut request);
        request.respond(response).ok();
    }
}
