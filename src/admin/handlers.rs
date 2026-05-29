use tiny_http::{Header, Method, Response};

use crate::config;

use super::{err, ok, parse_json};
use super::backup;

// ── Handler: 自我记忆 ──────────────────────────────────────────

pub fn handle_self_thoughts(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
                "created": crate::util::now_secs()
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

// ── 表情包管理 ────────────────────────────────────────────────

pub fn handle_sticker() -> Response<std::io::Cursor<Vec<u8>>> {
    let (total, registered) = crate::sticker::get_stats();
    let store = crate::sticker::store::load_store();
    ok(serde_json::json!({
        "total": total,
        "registered": registered,
        "stickers": store.stickers.iter().map(|e| serde_json::json!({
            "hash": e.hash,
            "description": e.description,
            "vlm_description": e.vlm_description,
            "emotions": e.emotions,
            "query_count": e.query_count,
            "is_registered": e.is_registered,
            "is_banned": e.is_banned,
            "is_builtin": e.is_builtin,
            "path": e.path,
            "registered_at": e.registered_at,
            "last_used_at": e.last_used_at,
        })).collect::<Vec<_>>(),
    }))
}

/// 切换表情包封禁状态
pub fn handle_sticker_toggle(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut store = crate::sticker::store::load_store();
    let banned = store.stickers.iter_mut().find(|e| e.hash == hash).map(|entry| {
        entry.is_banned = !entry.is_banned;
        entry.is_banned
    });
    if let Some(is_banned) = banned {
        crate::sticker::store::save_store(&store);
        return ok(serde_json::json!({"ok": true, "is_banned": is_banned}));
    }
    err(404, "sticker not found")
}

/// 删除表情包
pub fn handle_sticker_delete(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut store = crate::sticker::store::load_store();
    let data_dir = crate::config::data_dir();
    if let Some(idx) = store.stickers.iter().position(|e| e.hash == hash) {
        let entry = &store.stickers[idx];
        let full_path = data_dir.join(&entry.path);
        if full_path.exists() {
            std::fs::remove_file(&full_path).ok();
        }
        store.stickers.remove(idx);
        crate::sticker::store::save_store(&store);
        return ok(serde_json::json!({"ok": true}));
    }
    err(404, "sticker not found")
}

/// 服务表情包图片文件
///
/// 1. 优先从注册表中查找哈希对应的路径
/// 2. 注册表未命中时，直接扫描 sticker/ 和 ne_sticker/ 目录查找文件
pub fn handle_sticker_image(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let data_dir = crate::config::data_dir();
    let mut full_path = None;

    // 1. 从注册表查找
    let store = crate::sticker::store::load_store();
    if let Some(entry) = store.stickers.iter().find(|e| e.hash == hash) {
        let candidate = data_dir.join(&entry.path);
        if candidate.exists() {
            full_path = Some(candidate);
        }
    }

    // 2. 注册表未命中，直接扫描目录
    if full_path.is_none() {
        for dir in &["sticker", "ne_sticker"] {
            let dir_path = data_dir.join(dir);
            if !dir_path.exists() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                        && (stem == hash || stem.starts_with(hash))
                    {
                        full_path = Some(path);
                        break;
                    }
                }
            }
            if full_path.is_some() {
                break;
            }
        }
    }

    if let Some(ref fp) = full_path {
        let data = match std::fs::read(fp) {
            Ok(d) => d,
            Err(_) => return err(500, "failed to read file"),
        };
        let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/png",
        };
        return Response::from_data(data)
            .with_header(Header::from_bytes("Content-Type", mime).unwrap())
            .with_header(Header::from_bytes("Cache-Control", "public, max-age=86400").unwrap());
    }

    err(404, "image not found")
}

/// 更新表情包标签
pub fn handle_sticker_tags(hash: &str, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let val: serde_json::Value = match super::parse_json(body) {
        Ok(v) => v,
        Err(e) => return super::err(400, &e),
    };
    let new_tags: Vec<String> = match val.get("tags").and_then(|v| v.as_array()) {
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect(),
        None => return super::err(400, "tags array required"),
    };

    let mut store = crate::sticker::store::load_store();
    if let Some(entry) = store.stickers.iter_mut().find(|e| e.hash == hash) {
        entry.emotions = new_tags.clone();
        entry.description = new_tags.join(",");
        crate::sticker::store::save_store(&store);
        return super::ok(serde_json::json!({"ok": true, "tags": new_tags}));
    }
    super::err(404, "sticker not found")
}

/// 更新表情包 VLM 自然语言描述
pub fn handle_sticker_description(hash: &str, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let val: serde_json::Value = match super::parse_json(body) {
        Ok(v) => v,
        Err(e) => return super::err(400, &e),
    };
    let new_desc = match val.get("description").and_then(|v| v.as_str()) {
        Some(s) => s.trim().to_string(),
        None => return super::err(400, "description string required"),
    };

    let mut store = crate::sticker::store::load_store();
    if let Some(entry) = store.stickers.iter_mut().find(|e| e.hash == hash) {
        entry.vlm_description = Some(new_desc.clone());
        crate::sticker::store::save_store(&store);
        return super::ok(serde_json::json!({"ok": true, "vlm_description": new_desc}));
    }
    super::err(404, "sticker not found")
}

// ── 仪表盘统计 ────────────────────────────────────────────────

pub fn handle_dashboard() -> Response<std::io::Cursor<Vec<u8>>> {
    let user_ids = crate::memory::store::all_user_ids();
    let user_count = user_ids.len();
    let mut mem_count: usize = 0;
    for uid in &user_ids {
        mem_count += crate::memory::store::load_user_memory(*uid).entries.len();
    }

    let (_, sticker_registered) = crate::sticker::get_stats();
    let emotion_count = crate::emotion::user_count();

    ok(serde_json::json!({
        "memory_users": user_count,
        "memory_entries": mem_count,
        "sticker_count": sticker_registered,
        "emotion_users": emotion_count,
        "active_groups": crate::get_active_groups().len(),
        "active_users": crate::get_active_users().len(),
    }))
}

// ── Handler: 用户记忆 ──────────────────────────────────────────

pub fn handle_memory(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
                "created": crate::util::now_secs(),
                "last_accessed": crate::util::now_secs(),
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
                entries[idx]["last_accessed"] = serde_json::json!(crate::util::now_secs());
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

pub fn handle_working_memory(method: &Method, segs: &[&str], _body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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



// ── Handler: 情绪 ──────────────────────────────────────────────

pub fn handle_emotion(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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

pub fn handle_mental_state(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
                        "created": crate::util::now_secs(),
                        "last_reinforced": crate::util::now_secs(),
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
                        "created": crate::util::now_secs(),
                        "last_reinforced": crate::util::now_secs()
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

pub fn handle_blocklist(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("blocklist.json");
    match method {
        Method::Get => {
            // 合并 blocklist.json 和 config.yaml 中的 blacklist
            let file_list: Vec<u64> =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| "[]".into()))
                    .unwrap_or_default();
            let mut all: Vec<u64> = file_list.clone();
            for uid in &config::get().blacklist {
                if !all.contains(uid) { all.push(*uid); }
            }
            ok(serde_json::json!({"blocked": all, "from_file": file_list, "from_config": &config::get().blacklist}))
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
            // 同步运行时状态
            crate::with_state(|s| { s.add_blacklist(uid); });
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
            // 同步运行时状态
            crate::with_state(|s| { s.remove_blacklist(uid); });
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 主动对话 ──────────────────────────────────────────

pub fn handle_proactive(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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

pub fn handle_archive() -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("archive.json");
    let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let store: serde_json::Value =
        serde_json::from_str(&data).unwrap_or(serde_json::json!({"working_memory": [], "long_term": []}));
    ok(store)
}

// ── Handler: 备份 ──────────────────────────────────────────────

pub fn handle_backups(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            if let Some(data_type) = segs.first() {
                ok(backup::list(data_type))
            } else {
                ok(backup::list_all_types())
            }
        }
        Method::Post => {
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let action = body_val.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let data_type = body_val.get("type").and_then(|v| v.as_str()).unwrap_or("self_memory");
            match action {
                "create" => {
                    backup::before_modify(data_type);
                    ok(serde_json::json!({"ok": true}))
                }
                "restore" => {
                    let filename = match body_val.get("filename").and_then(|v| v.as_str()) {
                        Some(f) => f,
                        None => return err(400, "filename required"),
                    };
                    match backup::restore(data_type, filename) {
                        Ok(()) => ok(serde_json::json!({"ok": true})),
                        Err(e) => err(400, &e),
                    }
                }
                "delete" => {
                    let filename = match body_val.get("filename").and_then(|v| v.as_str()) {
                        Some(f) => f,
                        None => return err(400, "filename required"),
                    };
                    let backup_path = config::data_dir().join("backups").join(data_type).join(filename);
                    if backup_path.exists() {
                        std::fs::remove_file(&backup_path).ok();
                    }
                    ok(serde_json::json!({"ok": true}))
                }
                _ => err(400, "unknown action: use create, restore, or delete"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 同步 ──────────────────────────────────────────────

pub fn handle_sync(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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

// ── 配额追踪 ────────────────────────────────────────────────────

pub fn handle_quota(method: &Method, segs: &[&str]) -> Response<std::io::Cursor<Vec<u8>>> {
    if *method != Method::Get {
        return err(405, "method not allowed");
    }
    match segs.first() {
        Some(&"interest") => {
            let interest = crate::quota::get_all_interest();
            let map: serde_json::Map<String, serde_json::Value> = interest.iter()
                .map(|(uid, i)| (uid.to_string(), serde_json::json!({
                    "score": i.score,
                    "marked_count": i.marked_count,
                    "last_reviewed": i.last_reviewed,
                    "last_message": i.last_message,
                })))
                .collect();
            ok(serde_json::json!({"users": map}))
        }
        Some(&"segments") => {
            if let Some(gid_str) = segs.get(1) {
                let group_id: u64 = match gid_str.parse() {
                    Ok(v) => v,
                    Err(_) => return err(400, "invalid group_id"),
                };
                let logs = crate::quota::get_segment_logs(group_id, 20);
                ok(serde_json::json!({"group_id": group_id, "segments": logs}))
            } else {
                let groups = crate::quota::get_groups_with_logs();
                ok(serde_json::json!({"groups": groups}))
            }
        }
        _ => {
            // API quota 配置
            let cfg = &config::get().quota;
            let interest = crate::quota::get_all_interest();
            let users: serde_json::Map<String, serde_json::Value> = interest.iter()
                .map(|(uid, i)| (uid.to_string(), serde_json::json!({
                    "score": i.score,
                    "marked_count": i.marked_count,
                    "last_reviewed": i.last_reviewed,
                })))
                .collect();
            ok(serde_json::json!({
                 "enabled": cfg.enabled,
                 "segment_minutes": cfg.segment_minutes,
                 "segments": cfg.segments,
                 "users": users,
             }))
        }
    }
}

// ── 防注入状态管理 ────────────────────────────────────────────────

pub fn handle_anti_injection(
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
            if let Some(&user_id_str) = segs.first()
                && let Ok(user_id) = user_id_str.parse::<u64>() {
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

pub fn handle_config(method: &Method, segs: &[&str], body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    // POST /api/config/reload — 热重载配置
    if *method == Method::Post && segs.first() == Some(&"reload") {
        match config::reload() {
            Ok(()) => return ok(serde_json::json!({"ok": true, "message": "配置已重新载入"})),
            Err(e) => return err(500, &e),
        }
    }
    // 原有的 config GET/PUT 逻辑
    handle_config_main(method, body)
}

fn handle_config_main(method: &Method, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
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
                if let Some(key) = obj.get_mut("api_key")
                    && let Some(s) = key.as_str()
                        && s.len() > 8 {
                            *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len()-4..]));
                        }
                if let Some(v) = obj.get_mut("vision").and_then(|v| v.as_object_mut())
                    && let Some(key) = v.get_mut("api_key")
                        && let Some(s) = key.as_str()
                            && s.len() > 8 {
                                *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len()-4..]));
                            }
            }
            ok(cfg)
        }
        Method::Put => {
            let new_cfg: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => return err(400, &format!("invalid json: {}", e)),
            };
            // 读取现有配置以保留未发送的字段
            let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
            let existing_cfg: serde_json::Value = serde_yaml::from_str(&existing).unwrap_or(serde_json::json!({}));

            // 深合并：新配置中未发送的嵌套字段保留原值
            let mut merged = deep_merge(&existing_cfg, &new_cfg);

            // 脱敏字段还原：包含 "..." 的 api_key 保留原值
            if let (Some(new_obj), Some(old_obj)) = (merged.as_object_mut(), existing_cfg.as_object()) {
                // api_key
                if let Some(key) = new_obj.get("api_key").and_then(|v| v.as_str())
                    && key.contains("...")
                        && let Some(old_key) = old_obj.get("api_key") {
                            new_obj.insert("api_key".to_string(), old_key.clone());
                        }
                // vision.api_key
                if let (Some(new_vis), Some(old_vis)) = (
                    new_obj.get_mut("vision").and_then(|v| v.as_object_mut()),
                    old_obj.get("vision").and_then(|v| v.as_object())
                )
                    && let Some(key) = new_vis.get("api_key").and_then(|v| v.as_str())
                        && key.contains("...")
                            && let Some(old_key) = old_vis.get("api_key") {
                                new_vis.insert("api_key".to_string(), old_key.clone());
                            }
            }

            // 使用模板保存：保留注释和格式，只替换值
            let yaml = match config::save_config_with_comments(&merged) {
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

// ── 日程计划 ──────────────────────────────────────────────────

pub fn handle_analytics() -> Response<std::io::Cursor<Vec<u8>>> {
    ok(crate::tracking::UsageStore::summary())
}

// ── 日程计划 ──────────────────────────────────────────────────

pub fn handle_schedule(method: &Method, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    // POST: 更新计划状态
    if method == &Method::Post {
        let body_val: serde_json::Value = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => return err(400, &format!("invalid json: {}", e)),
        };

        let action = body_val.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let kind = body_val.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let index = body_val.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        match (action, kind) {
            ("toggle", "weekly") => {
                let mut plan = crate::schedule::load_weekly_plan();
                if index >= plan.goals.len() {
                    return err(404, "goal not found");
                }
                let goal = &mut plan.goals[index];
                goal.completed = !goal.completed;
                let completed = goal.completed;
                let content = goal.content.clone();
                if completed {
                    goal.completed_at = crate::util::now_secs();
                    crate::schedule::record_push_log("周计划完成", &content);
                } else {
                    goal.completed_at = 0;
                }
                crate::schedule::save_weekly_plan(&plan);
                return ok(serde_json::json!({"ok": true, "completed": completed}));
            }
            ("toggle", "monthly") => {
                let mut plan = crate::schedule::load_monthly_plan();
                if index >= plan.goals.len() {
                    return err(404, "goal not found");
                }
                let goal = &mut plan.goals[index];
                goal.completed = !goal.completed;
                let completed = goal.completed;
                let content = goal.content.clone();
                if completed {
                    goal.completed_at = crate::util::now_secs();
                    crate::schedule::record_push_log("月计划完成", &content);
                } else {
                    goal.completed_at = 0;
                }
                crate::schedule::save_monthly_plan(&plan);
                return ok(serde_json::json!({"ok": true, "completed": completed}));
            }
            _ => return err(400, "invalid action or kind"),
        }
    }

    // GET: 返回计划数据
    let weekly = crate::schedule::load_weekly_plan();
    let monthly = crate::schedule::load_monthly_plan();
    let pushes = crate::schedule::check_plan_push();

    // 推动状态
    let push_state_path = crate::config::data_dir().join("plan_push_state.json");
    let push_state: serde_json::Value = std::fs::read_to_string(&push_state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({"pushed_today": [], "date": ""}));

    // 推动历史日志（从 push_history.json 读取）
    let history_path = crate::config::data_dir().join("push_history.json");
    let history: Vec<serde_json::Value> = std::fs::read_to_string(&history_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // 统计
    let total_weekly = weekly.goals.len();
    let done_weekly = weekly.goals.iter().filter(|g| g.completed).count();
    let total_monthly = monthly.goals.len();
    let done_monthly = monthly.goals.iter().filter(|g| g.completed).count();

    ok(serde_json::json!({
        "weekly": {
            "week_start": weekly.week_start,
            "goals": weekly.goals,
            "week_reflection": weekly.week_reflection,
            "total": total_weekly,
            "done": done_weekly,
        },
        "monthly": {
            "month": monthly.month,
            "goals": monthly.goals,
            "total": total_monthly,
            "done": done_monthly,
        },
        "pushes": pushes,
        "push_state": push_state,
        "push_history": history,
    }))
}

// ── 对话管理 ──────────────────────────────────────────────────

pub fn handle_conversations(method: &Method, segs: &[&str]) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            // GET /api/conversations -- 列出所有活跃群聊和私聊
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

// ── Handler: 人性化状态 ──────────────────────────────────────────

pub fn handle_humanity() -> Response<std::io::Cursor<Vec<u8>>> {
    let cfg = config::get();

    let battery = if cfg.humanity.social_battery_enabled {
        let b = crate::social_battery::load();
        Some(serde_json::json!({
            "level": b.level,
            "capacity": b.capacity,
            "percentage": crate::social_battery::level_percentage(&b),
            "is_burned_out": b.is_burned_out,
            "is_passive_mode": b.is_passive_mode,
            "active_minutes": b.active_minutes,
        }))
    } else { None };

    let circadian = if cfg.humanity.circadian_enabled {
        let c = crate::circadian::calculate();
        Some(serde_json::json!({
            "energy_level": c.energy_level,
            "cognitive_clarity": c.cognitive_clarity,
            "patience_level": c.patience_level,
            "sociability": c.sociability,
            "humor_sensitivity": c.humor_sensitivity,
            "current_hour": c.current_hour,
            "is_quiet_hours": crate::circadian::is_quiet_hours(),
        }))
    } else { None };

    let attention = if cfg.humanity.attention_enabled {
        let a = crate::conversation::attention::load_attention();
        Some(serde_json::json!({
            "attention_level": a.attention_level,
            "flow_state": a.flow_state,
            "focused_topic": a.focused_topic,
            "flow_recovering": a.flow_recovery_until > crate::util::now_secs(),
        }))
    } else { None };

    let biases = if cfg.humanity.cognitive_biases_enabled {
        let b = crate::memory::cognitive_biases::load_biases();
        Some(serde_json::json!({
            "confirmation_bias": b.confirmation_bias,
            "recency_bias": b.recency_bias,
            "mood_congruence": b.mood_congruence,
            "anchoring_strength": b.anchoring_strength,
            "availability_heuristic": b.availability_heuristic,
        }))
    } else { None };

    let motivation = crate::proactive::motivation::get_dominant_motivation()
        .map(|(name, strength)| serde_json::json!({"type": name, "strength": strength}));

    let rel_path = config::data_dir().join("relationships.json");
    let rel_count = std::fs::read_to_string(&rel_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("relationships").and_then(|r| r.as_object()).map(|o| o.len()))
        .unwrap_or(0);

    let inner_thoughts = if cfg.humanity.inner_thought_enabled {
        let thoughts = crate::self_memory::inner_thought::get_active_thoughts(10);
        Some(thoughts.iter().map(|t| serde_json::json!({
            "content": t.content,
            "timestamp": t.timestamp,
            "emotional_impact": t.emotional_impact,
            "action_potential": t.action_potential,
            "faded": t.faded,
            "recall_count": t.recall_count,
        })).collect::<Vec<_>>())
    } else { None };

    ok(serde_json::json!({
        "social_battery": battery,
        "circadian": circadian,
        "attention": attention,
        "cognitive_biases": biases,
        "motivation": motivation,
        "relationship_count": rel_count,
        "inner_thoughts": inner_thoughts,
        "config_enabled": {
            "social_battery": cfg.humanity.social_battery_enabled,
            "circadian": cfg.humanity.circadian_enabled,
            "attention": cfg.humanity.attention_enabled,
            "cognitive_biases": cfg.humanity.cognitive_biases_enabled,
            "satisficing": cfg.humanity.satisficing_enabled,
            "response_timing": cfg.humanity.response_timing_enabled,
            "unpredictability": cfg.humanity.unpredictability_enabled,
            "inner_thought": cfg.humanity.inner_thought_enabled,
        },
    }))
}

pub fn handle_relationships(method: &Method, segs: &[&str]) -> Response<std::io::Cursor<Vec<u8>>> {
    if *method != Method::Get {
        return err(405, "method not allowed");
    }
    let rel_path = config::data_dir().join("relationships.json");
    let data = std::fs::read_to_string(&rel_path).unwrap_or_else(|_| "{}".into());
    let store: serde_json::Value = serde_json::from_str(&data).unwrap_or(serde_json::json!({"relationships": {}}));
    if let Some(uid) = segs.first() {
        let rels = store.get("relationships").and_then(|v| v.as_object());
        match rels.and_then(|r| r.get(*uid)) {
            Some(rel) => ok(rel.clone()),
            None => err(404, "relationship not found"),
        }
    } else {
        ok(store)
    }
}

pub fn handle_info() -> Response<std::io::Cursor<Vec<u8>>> {
    ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build_time": option_env!("BUILD_TIME").unwrap_or("dev"),
    }))
}

/// 深合并两个 JSON 对象
/// - 对于两个都是 Object 的 key：递归合并
/// - 对于其他情况：新值覆盖旧值
/// 这样前端发送部分嵌套字段时，不会丢失未发送的字段
fn deep_merge(base: &serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            let mut result = base_map.clone();
            for (key, patch_val) in patch_map {
                if let Some(base_val) = result.get(key) {
                    // 两边都是 Object → 递归深合并
                    if base_val.is_object() && patch_val.is_object() {
                        result.insert(key.clone(), deep_merge(base_val, patch_val));
                    } else {
                        // 其他类型（包括 Array）：新值覆盖
                        result.insert(key.clone(), patch_val.clone());
                    }
                } else {
                    // patch 中有而 base 中没有的 key：直接插入
                    result.insert(key.clone(), patch_val.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        // 非 Object 类型：直接返回 patch
        _ => patch.clone(),
    }
}
