use tiny_http::{Method, Response};
use tracing::warn;

use crate::config;

use super::backup;
use super::{err, ok, parse_json};

// ── 表情包管理 ────────────────────────────────────────────────

pub(crate) fn handle_sticker() -> Response<std::io::Cursor<Vec<u8>>> {
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
pub(crate) fn handle_sticker_toggle(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut store = crate::sticker::store::load_store();
    let banned = store
        .stickers
        .iter_mut()
        .find(|e| e.hash == hash)
        .map(|entry| {
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
pub(crate) fn handle_sticker_delete(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
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
pub(crate) fn handle_sticker_image(hash: &str) -> Response<std::io::Cursor<Vec<u8>>> {
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
        let ext = fp
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/png",
        };
        return super::attach_headers(
            Response::from_data(data),
            &[
                ("Content-Type", mime),
                ("Cache-Control", "public, max-age=86400"),
            ],
        );
    }

    err(404, "image not found")
}

/// 更新表情包标签
pub(crate) fn handle_sticker_tags(hash: &str, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    let val: serde_json::Value = match super::parse_json(body) {
        Ok(v) => v,
        Err(e) => return super::err(400, &e),
    };
    let new_tags: Vec<String> = match val.get("tags").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
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
pub(crate) fn handle_sticker_description(
    hash: &str,
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
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

pub(crate) fn handle_dashboard() -> Response<std::io::Cursor<Vec<u8>>> {
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

// ── Handler: 用户记忆（读写 memory/users/{uid}.json 存储）──────

/// 组装全部用户记忆（前端数据形状：{users: {uid: {entries: [...]}}}）
fn memory_store_json() -> serde_json::Value {
    let mut users = serde_json::Map::new();
    for uid in crate::memory::store::all_user_ids() {
        let mem = crate::memory::store::load_user_memory(uid);
        users.insert(
            uid.to_string(),
            serde_json::to_value(&mem).unwrap_or_default(),
        );
    }
    serde_json::json!({ "users": users })
}

fn parse_importance(
    value: Option<&serde_json::Value>,
) -> Result<crate::memory::store::Importance, ()> {
    match value {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).map_err(|_| ()),
        _ => Ok(crate::memory::store::Importance::Normal),
    }
}

pub(crate) fn handle_memory(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    use crate::memory::store::{self, MemoryEntry};

    // GET /api/memory/export -> 导出全部
    if *method == Method::Get && segs.first() == Some(&"export") {
        let data = serde_json::to_string(&memory_store_json()).unwrap_or_default();
        return super::attach_headers(
            Response::from_string(data),
            &[
                ("Content-Type", "application/json; charset=utf-8"),
                (
                    "Content-Disposition",
                    "attachment; filename=\"memory_export.json\"",
                ),
            ],
        );
    }

    // POST /api/memory/{user_id}/batch -> 批量删除
    if *method == Method::Post && segs.len() == 2 && segs[1] == "batch" {
        let Some(uid) = segs[0].parse().ok() else {
            return err(400, "invalid user_id");
        };
        let body_val: serde_json::Value = match parse_json(body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let Some(indices) = body_val
            .get("indices")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect::<Vec<_>>()
            })
        else {
            return err(400, "indices required");
        };
        let mut mem = store::load_user_memory(uid);
        let mut sorted = indices;
        sorted.sort_unstable();
        sorted.dedup();
        let mut deleted = 0;
        for idx in sorted.into_iter().rev() {
            if idx < mem.entries.len() {
                mem.entries.remove(idx);
                deleted += 1;
            }
        }
        store::save_user_memory(uid, &mem);
        return ok(serde_json::json!({"ok": true, "deleted": deleted}));
    }

    match method {
        Method::Get => {
            // /api/memory -> 完整 store
            // /api/memory/{user_id} -> 单用户
            match segs.first().and_then(|s| s.parse::<u64>().ok()) {
                Some(uid) => {
                    let mem = store::load_user_memory(uid);
                    ok(serde_json::json!({
                        "user_id": uid.to_string(),
                        "entries": serde_json::to_value(&mem).unwrap_or_default()
                    }))
                }
                None => ok(memory_store_json()),
            }
        }
        Method::Post => {
            let Some(uid) = segs.first().and_then(|s| s.parse::<u64>().ok()) else {
                return err(400, "invalid user_id");
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let Some(content) = body_val
                .get("content")
                .and_then(|v| v.as_str())
                .filter(|c| !c.is_empty())
            else {
                return err(400, "content required");
            };
            let importance = match parse_importance(body_val.get("importance")) {
                Ok(i) => i,
                Err(_) => return err(400, "invalid importance"),
            };
            let now = crate::util::now_secs();
            let mut mem = store::load_user_memory(uid);
            mem.entries.push(MemoryEntry {
                content: content.to_string(),
                importance,
                created: now,
                last_accessed: now,
                access_count: 1,
                emotional_impact: None,
            });
            store::save_user_memory(uid, &mem);
            ok(serde_json::json!({"ok": true}))
        }
        Method::Put => {
            let Some(uid) = segs.first().and_then(|s| s.parse::<u64>().ok()) else {
                return err(400, "invalid user_id");
            };
            let Some(idx) = segs.get(1).and_then(|s| s.parse::<usize>().ok()) else {
                return err(400, "index required");
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            let mut mem = store::load_user_memory(uid);
            let Some(entry) = mem.entries.get_mut(idx) else {
                return err(404, "index out of range");
            };
            if let Some(content) = body_val.get("content").and_then(|v| v.as_str()) {
                entry.content = content.to_string();
            }
            match parse_importance(body_val.get("importance")) {
                Ok(importance) => entry.importance = importance,
                Err(_) => return err(400, "invalid importance"),
            }
            entry.last_accessed = crate::util::now_secs();
            store::save_user_memory(uid, &mem);
            ok(serde_json::json!({"ok": true}))
        }
        Method::Delete => {
            let Some(uid) = segs.first().and_then(|s| s.parse::<u64>().ok()) else {
                return err(400, "invalid user_id");
            };
            let Some(idx) = segs.get(1).and_then(|s| s.parse::<usize>().ok()) else {
                return err(400, "index required");
            };
            let mut mem = store::load_user_memory(uid);
            if idx >= mem.entries.len() {
                return err(404, "index out of range");
            }
            mem.entries.remove(idx);
            store::save_user_memory(uid, &mem);
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 工作记忆 ──────────────────────────────────────────

pub(crate) fn handle_working_memory(
    method: &Method,
    segs: &[&str],
    _body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            // 工作记忆已迁入状态库：读旧文件只会拿到过期内容
            let db = crate::db::db();
            if let Some(gid) = segs.first() {
                let group_id: u64 = match gid.parse() {
                    Ok(id) => id,
                    Err(_) => return err(400, "invalid group_id"),
                };
                let entries = match db.working_memory_of_group(group_id) {
                    Ok(rows) => rows
                        .into_iter()
                        .map(|row| {
                            serde_json::json!({
                                "id": row.id,
                                "user_id": row.user_id,
                                "content": row.content,
                                "timestamp": row.created_at,
                                "bot_replied": row.bot_replied,
                            })
                        })
                        .collect::<Vec<_>>(),
                    Err(error) => return err(500, &format!("读取工作记忆失败: {error}")),
                };
                ok(serde_json::json!({"group_id": gid, "entries": entries}))
            } else {
                // 保持前端既有形状：{ "groups": { "<gid>": { "entries": [...] } } }
                let groups = match db.working_memory_groups() {
                    Ok(groups) => groups,
                    Err(error) => return err(500, &format!("读取工作记忆失败: {error}")),
                };
                let mut view = serde_json::Map::new();
                for (group_id, rows) in groups {
                    let entries: Vec<serde_json::Value> = rows
                        .into_iter()
                        .map(|row| {
                            serde_json::json!({
                                "id": row.id,
                                "user_id": row.user_id,
                                "content": row.content,
                                "timestamp": row.created_at,
                                "bot_replied": row.bot_replied,
                            })
                        })
                        .collect();
                    view.insert(
                        group_id.to_string(),
                        serde_json::json!({ "entries": entries }),
                    );
                }
                ok(serde_json::json!({ "groups": view }))
            }
        }
        Method::Delete => {
            let gid: u64 = match segs.first().and_then(|s| s.parse().ok()) {
                Some(g) => g,
                None => return err(400, "group_id required"),
            };
            let idx: usize = match segs.get(1).and_then(|s| s.parse().ok()) {
                Some(i) => i,
                None => return err(400, "index required"),
            };
            backup::before_modify("working_memory");
            // 走类型化 store：它负责锁与原子写。
            // 后台不再自己拼 JSON——`working_memory.json` 是所有群共用的一份文件，
            // 第二条写路径会与消息线程的读改写互相覆盖。
            match crate::working_memory::delete_entry_at(gid, idx) {
                crate::working_memory::DeleteEntryOutcome::Removed => {
                    ok(serde_json::json!({"ok": true}))
                }
                crate::working_memory::DeleteEntryOutcome::GroupNotFound => {
                    err(404, "group not found")
                }
                crate::working_memory::DeleteEntryOutcome::IndexOutOfRange => {
                    err(404, "index out of range")
                }
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 情绪 ──────────────────────────────────────────────

pub(crate) fn handle_emotion(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            // 情绪状态已迁入状态库（按用户一行）：不能再读 emotion.json，
            // 那个文件已经退休，读它只会拿到过期内容
            let mut view = serde_json::Map::new();
            match crate::db::db().all_emotion_states() {
                Ok(states) => {
                    for (uid, json) in states {
                        match serde_json::from_str::<serde_json::Value>(&json) {
                            Ok(state) => {
                                view.insert(uid.to_string(), state);
                            }
                            Err(error) => {
                                warn!(%error, uid, "admin: 情绪状态解析失败，已跳过");
                            }
                        }
                    }
                }
                Err(error) => {
                    return err(500, &format!("读取情绪状态失败: {error}"));
                }
            }
            let store = serde_json::Value::Object(view);
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
            let uid: u64 = match segs.first().and_then(|s| s.parse().ok()) {
                Some(u) => u,
                None => return err(400, "user_id required"),
            };
            let body_val: serde_json::Value = match parse_json(body) {
                Ok(v) => v,
                Err(e) => return err(400, &e),
            };
            backup::before_modify("emotion");
            // 走类型化 store：锁、原子写、默认值都由它负责。
            // 后台不再自己拼一份 JSON——那是第二条写路径，会与消息线程
            // 的读改写互相覆盖。
            let state: crate::emotion::EmotionState = match serde_json::from_value(body_val) {
                Ok(state) => state,
                Err(e) => return err(400, &format!("情绪状态结构校验失败（未写入）: {e}")),
            };
            crate::emotion::update_state(uid, state);
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 黑名单 ──────────────────────────────────────────

pub(crate) fn handle_blocklist(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    // 运行时黑名单由进程级门禁状态持有（`crate::get_blacklist`），
    // 落盘是它的快照。这里不再自己读写 blocklist.json——第二条写路径
    // 会让主循环看到的内存集合与文件不一致。
    match method {
        Method::Get => {
            let runtime = crate::get_blacklist();
            // 配置里的 blacklist 是启动时并入运行时的静态名单，仍然分别展示，
            // 便于定位"这条为什么被拉黑"。
            let from_config = config::get().blacklist.clone();
            let mut all = runtime.clone();
            for uid in &from_config {
                if !all.contains(uid) {
                    all.push(*uid);
                }
            }
            ok(serde_json::json!({
                "blocked": all,
                "from_runtime": runtime,
                "from_config": from_config,
            }))
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
            crate::set_blacklisted(crate::db::Actor::Admin, uid, true);
            ok(serde_json::json!({"ok": true}))
        }
        Method::Delete => {
            let uid: u64 = match segs.first().and_then(|s| s.parse().ok()) {
                Some(u) => u,
                None => return err(400, "user_id required"),
            };
            backup::before_modify("blocklist");
            crate::set_blacklisted(crate::db::Actor::Admin, uid, false);
            ok(serde_json::json!({"ok": true}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── Handler: 归档 ──────────────────────────────────────────────

pub(crate) fn handle_archive() -> Response<std::io::Cursor<Vec<u8>>> {
    let path = config::data_dir().join("archive.json");
    let data = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let store: serde_json::Value = serde_json::from_str(&data)
        .unwrap_or(serde_json::json!({"working_memory": [], "long_term": []}));
    ok(store)
}

// ── Handler: 备份 ──────────────────────────────────────────────

pub(crate) fn handle_backups(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
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
            let action = body_val
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data_type = body_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("memory");
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
                    let backup_path = config::data_dir()
                        .join("backups")
                        .join(data_type)
                        .join(filename);
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

// ── 配额追踪 ────────────────────────────────────────────────────

pub(crate) fn handle_quota(method: &Method, segs: &[&str]) -> Response<std::io::Cursor<Vec<u8>>> {
    if *method != Method::Get {
        return err(405, "method not allowed");
    }
    match segs.first() {
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
            ok(serde_json::json!({
                "enabled": cfg.enabled,
                "segment_minutes": cfg.segment_minutes,
                "segments": cfg.segments,
            }))
        }
    }
}

// ── 防注入状态管理 ────────────────────────────────────────────────

pub(crate) fn handle_anti_injection(
    method: &Method,
    segs: &[&str],
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
                && let Ok(user_id) = user_id_str.parse::<u64>()
            {
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
            // POST /api/anti-injection/{user_id}/{action}
            if segs.len() < 2 {
                return err(400, "path required: /api/anti-injection/{user_id}/{action}");
            }
            let user_id = match segs[0].parse::<u64>() {
                Ok(u) => u,
                Err(_) => return err(400, "invalid user_id"),
            };
            let action = segs[1];

            match action {
                "unban" => {
                    crate::anti_injection::unban_user(user_id);
                    ok(
                        serde_json::json!({"success": true, "message": format!("用户{}已解封", user_id)}),
                    )
                }
                "enable-vision" => {
                    crate::anti_injection::enable_vision(user_id);
                    ok(
                        serde_json::json!({"success": true, "message": format!("用户{}识图已启用", user_id)}),
                    )
                }
                "reset-reputation" => {
                    crate::anti_injection::reset_reputation(user_id);
                    ok(
                        serde_json::json!({"success": true, "message": format!("用户{}信誉已重置", user_id)}),
                    )
                }
                "silent-ban" => {
                    crate::anti_injection::silent_ban_user(user_id);
                    ok(
                        serde_json::json!({"success": true, "message": format!("用户{}已静默封禁", user_id)}),
                    )
                }
                "ban" => {
                    crate::anti_injection::ban_user(user_id);
                    ok(
                        serde_json::json!({"success": true, "message": format!("用户{}已完全封禁", user_id)}),
                    )
                }
                _ => err(404, "unknown action"),
            }
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 配置管理 ──────────────────────────────────────────────────

pub(crate) fn handle_config(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    // GET /api/config/status — 配置解析状态
    if *method == Method::Get && segs.first() == Some(&"status") {
        let err = config::error_message();
        return ok(serde_json::json!({"ok": err.is_empty(), "error": err}));
    }
    // POST /api/config/reload — 热重载配置
    if *method == Method::Post && segs.first() == Some(&"reload") {
        match config::reload() {
            Ok(()) => return ok(serde_json::json!({"ok": true, "message": "配置已重新载入"})),
            Err(e) => return err(500, &e),
        }
    }
    // GET /api/config/raw — 返回原始 YAML 文本
    if *method == Method::Get && segs.first() == Some(&"raw") {
        let config_path = config::data_dir().join("config.yaml");
        return match std::fs::read_to_string(&config_path) {
            Ok(content) => ok(serde_json::json!({"content": content})),
            Err(_) => err(404, "config.yaml not found"),
        };
    }
    // PUT /api/config/raw — 保存原始 YAML 文本
    if *method == Method::Put && segs.first() == Some(&"raw") {
        let body_val: serde_json::Value = match parse_json(body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let content = match body_val.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return err(400, "content required"),
        };
        // 原文保存也必须先验证它能被解析成 Config：
        // 只验证"YAML 语法"会放过结构错误，而那正是"配置写得进、插件起不来"。
        if let Err(e) = serde_yaml::from_str::<config::Config>(content) {
            return err(400, &format!("配置结构校验失败（未写入）: {e}"));
        }
        let config_path = config::data_dir().join("config.yaml");
        if let Err(e) = crate::util::atomic_write(&config_path, content) {
            return err(500, &format!("写入失败: {}", e));
        }
        return ok(
            serde_json::json!({"ok": true, "message": "配置已保存，点击「重新载入配置」生效"}),
        );
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
                    && s.len() > 8
                {
                    *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len() - 4..]));
                }
                if let Some(v) = obj.get_mut("vision").and_then(|v| v.as_object_mut())
                    && let Some(key) = v.get_mut("api_key")
                    && let Some(s) = key.as_str()
                    && s.len() > 8
                {
                    *key = serde_json::json!(format!("{}...{}", &s[..4], &s[s.len() - 4..]));
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
            let existing_cfg: serde_json::Value =
                serde_yaml::from_str(&existing).unwrap_or(serde_json::json!({}));

            // 深合并：新配置中未发送的嵌套字段保留原值
            let mut merged = deep_merge(&existing_cfg, &new_cfg);

            // 脱敏字段还原：包含 "..." 的 api_key 保留原值
            if let (Some(new_obj), Some(old_obj)) =
                (merged.as_object_mut(), existing_cfg.as_object())
            {
                // api_key
                if let Some(key) = new_obj.get("api_key").and_then(|v| v.as_str())
                    && key.contains("...")
                    && let Some(old_key) = old_obj.get("api_key")
                {
                    new_obj.insert("api_key".to_string(), old_key.clone());
                }
                // vision.api_key
                if let (Some(new_vis), Some(old_vis)) = (
                    new_obj.get_mut("vision").and_then(|v| v.as_object_mut()),
                    old_obj.get("vision").and_then(|v| v.as_object()),
                ) && let Some(key) = new_vis.get("api_key").and_then(|v| v.as_str())
                    && key.contains("...")
                    && let Some(old_key) = old_vis.get("api_key")
                {
                    new_vis.insert("api_key".to_string(), old_key.clone());
                }
            }

            // 类型化保存：反序列化成 Config 再原子落盘。
            // 校验发生在写入之前，因此"改坏配置导致插件起不来"不可达。
            let parsed: config::Config = match serde_json::from_value(merged) {
                Ok(cfg) => cfg,
                Err(e) => return err(400, &format!("配置校验失败（未写入）: {e}")),
            };
            if let Err(e) = config::save(&parsed) {
                return err(500, &format!("write config: {e}"));
            }
            ok(serde_json::json!({"ok": true, "message": "配置已保存，点击「重新载入配置」生效"}))
        }
        _ => err(405, "method not allowed"),
    }
}

// ── 日程计划 ──────────────────────────────────────────────────

pub(crate) fn handle_analytics() -> Response<std::io::Cursor<Vec<u8>>> {
    ok(crate::tracking::summary())
}

/// Turn 契约影子观测：最近 24 小时的分布
///
/// 这是"要不要把「文本即发言」换成严格 `Turn` tagged union"的决策依据——
/// 看 `failure_rate`：它就是在当前流量下，严格契约会失败的比例。
pub(crate) fn handle_turn_shadow() -> Response<std::io::Cursor<Vec<u8>>> {
    ok(crate::ai::shadow::report(24 * 3600).to_json())
}

/// 后台操作审计：最近 100 条"谁在什么时候改了什么"
///
/// 审计表此前**只写不读**——写入方（[`crate::db::Db::record_audit`]）在，
/// 读方没有出口，只有它自己的单元测试读过。没有读方的审计回答不了它本来
/// 要回答的问题，所以补上这个出口而不是删掉读方。
pub(crate) fn handle_audit() -> Response<std::io::Cursor<Vec<u8>>> {
    const PAGE_SIZE: usize = 100;

    match crate::db::db().recent_audit(PAGE_SIZE) {
        Ok(entries) => {
            let items: Vec<serde_json::Value> = entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "actor": entry.actor,
                        "command": entry.command,
                        "detail": entry.detail,
                        "created_at": entry.created_at,
                    })
                })
                .collect();
            ok(serde_json::json!({ "entries": items }))
        }
        Err(error) => err(500, &format!("读取审计失败: {error}")),
    }
}

// ── 日程计划 ──────────────────────────────────────────────────

pub(crate) fn handle_schedule(method: &Method, body: &[u8]) -> Response<std::io::Cursor<Vec<u8>>> {
    // POST: 更新计划状态
    if method == &Method::Post {
        let body_val: serde_json::Value = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => return err(400, &format!("invalid json: {}", e)),
        };

        let action = body_val
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let kind = body_val.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let index = body_val.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        match (action, kind) {
            ("toggle", "day" | "week" | "month") => {
                let timeframe = match kind {
                    "day" => crate::schedule::Timeframe::Day,
                    "week" => crate::schedule::Timeframe::Week,
                    _ => crate::schedule::Timeframe::Month,
                };
                let plan = crate::schedule::plan_of(timeframe);
                let Some(item) = plan.items.get(index) else {
                    return err(404, "plan item not found");
                };
                // 管理页与她自己走同一条落笔路径，只是判定由人来做
                match crate::schedule::set_status(&item.id, Some(!item.completed), "", "") {
                    crate::schedule::SetStatusOutcome::Applied {
                        id,
                        content,
                        completed,
                    } => {
                        return ok(serde_json::json!({
                            "ok": true,
                            "id": id,
                            "content": content,
                            "completed": completed,
                        }));
                    }
                    crate::schedule::SetStatusOutcome::UnknownId => {
                        return err(404, "plan item not found");
                    }
                    crate::schedule::SetStatusOutcome::PersistenceFailed(error) => {
                        return err(500, &error);
                    }
                }
            }
            _ => return err(400, "invalid action or kind"),
        }
    }

    // GET: 返回计划数据
    //
    // 这里**绝不能**调用会写状态的推动逻辑：早先版本在 GET 里调了
    // `check_plan_push()`（它会把当天内容标记成"已推送"），于是每打开
    // 一次日程页面就替她消耗掉一次推动，bot 再也收不到。
    let mut timeframes = serde_json::Map::new();
    for timeframe in crate::schedule::Timeframe::ALL {
        let plan = crate::schedule::plan_of(timeframe);
        let total = plan.items.len();
        let done = plan.items.iter().filter(|i| i.completed).count();
        let key = match timeframe {
            crate::schedule::Timeframe::Day => "day",
            crate::schedule::Timeframe::Week => "week",
            crate::schedule::Timeframe::Month => "month",
        };
        timeframes.insert(
            key.to_string(),
            serde_json::json!({
                "label": timeframe.label(),
                "period": plan.period,
                "items": plan.items,
                "total": total,
                "done": done,
                "reflection": plan.reflection,
            }),
        );
    }

    ok(serde_json::json!({
        "timeframes": timeframes,
        "history": crate::schedule::push_history(),
    }))
}

// ── 对话管理 ──────────────────────────────────────────────────

pub(crate) fn handle_conversations(
    method: &Method,
    segs: &[&str],
) -> Response<std::io::Cursor<Vec<u8>>> {
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
                return err(
                    400,
                    "path: /api/conversations/{group|private}/{id}/{enable|disable}",
                );
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
                "group" => crate::toggle_group_chat(crate::db::Actor::Admin, id, enable),
                "private" => crate::toggle_private_chat(crate::db::Actor::Admin, id, enable),
                _ => return err(400, "kind must be group or private"),
            };

            let action = if enable { "开启" } else { "关闭" };
            let target = if kind == "group" {
                format!("群{}", id)
            } else {
                format!("用户{}", id)
            };
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

pub(crate) fn handle_humanity() -> Response<std::io::Cursor<Vec<u8>>> {
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
    } else {
        None
    };

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
    } else {
        None
    };

    let attention = if cfg.humanity.attention_enabled {
        let a = crate::conversation::attention::load_attention();
        Some(serde_json::json!({
            "attention_level": a.attention_level,
            "flow_state": a.flow_state,
            "focused_topic": a.focused_topic,
            "flow_recovering": a.flow_recovery_until > crate::util::now_secs(),
        }))
    } else {
        None
    };

    let biases = if cfg.humanity.cognitive_biases_enabled {
        let b = crate::memory::cognitive_biases::load_biases();
        Some(serde_json::json!({
            "confirmation_bias": b.confirmation_bias,
            "mood_congruence": b.mood_congruence,
            "anchoring_strength": b.anchoring_strength,
            "availability_heuristic": b.availability_heuristic,
        }))
    } else {
        None
    };

    // 关系已迁入状态库：读旧文件会永远拿到迁移那一刻的快照
    let rel_count = crate::db::db()
        .per_user_count(crate::db::PerUserState::Relationship)
        .unwrap_or(0);

    ok(serde_json::json!({
        "social_battery": battery,
        "circadian": circadian,
        "attention": attention,
        "cognitive_biases": biases,
        "relationship_count": rel_count,
        "config_enabled": {
            "social_battery": cfg.humanity.social_battery_enabled,
            "circadian": cfg.humanity.circadian_enabled,
            "attention": cfg.humanity.attention_enabled,
            "cognitive_biases": cfg.humanity.cognitive_biases_enabled,
            "response_timing": cfg.humanity.response_timing_enabled,
            "unpredictability": cfg.humanity.unpredictability_enabled,
        },
    }))
}

pub(crate) fn handle_relationships(
    method: &Method,
    segs: &[&str],
) -> Response<std::io::Cursor<Vec<u8>>> {
    if *method != Method::Get {
        return err(405, "method not allowed");
    }
    if let Some(uid) = segs.first() {
        // 单用户详细关系数据（包含新维度）
        let uid_num: u64 = match uid.parse() {
            Ok(v) => v,
            Err(_) => return err(400, "invalid user_id"),
        };
        let summary = crate::person_info::relationship::get_relationship_summary(uid_num);
        return ok(summary);
    }
    // 关系已迁入状态库：保持前端既有形状 { "relationships": { "<uid>": {...} } }
    let stored = match crate::db::db().all_per_user_states(crate::db::PerUserState::Relationship) {
        Ok(stored) => stored,
        Err(error) => return err(500, &format!("读取关系失败: {error}")),
    };
    let mut view = serde_json::Map::new();
    for (user_id, json) in stored {
        match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(state) => {
                view.insert(user_id.to_string(), state);
            }
            Err(error) => warn!(%error, user_id, "admin: 关系解析失败，已跳过"),
        }
    }
    ok(serde_json::json!({ "relationships": view }))
}

// ── Handler: 内存操作日志 ──────────────────────────────────────────

pub(crate) fn handle_memory_ops_log(
    method: &Method,
    segs: &[&str],
) -> Response<std::io::Cursor<Vec<u8>>> {
    match method {
        Method::Get => {
            let limit = segs
                .first()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(500);
            let logs = crate::memory::ops_log::get_logs(Some(limit));
            ok(serde_json::json!({ "entries": logs, "total": logs.len() }))
        }
        Method::Post => {
            if segs.first() == Some(&"clear") {
                crate::memory::ops_log::clear();
                return ok(serde_json::json!({"ok": true, "message": "日志已清空"}));
            }
            err(400, "use /api/memory-ops-log/clear to clear")
        }
        _ => err(405, "method not allowed"),
    }
}

/// 深合并两个 JSON 对象
/// - 对于两个都是 Object 的 key：递归合并
/// - 对于其他情况：新值覆盖旧值
///
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

// ── Handler: 心灵（意识流/日记/档案/心事/审计） ────────────────

fn stream_event_json(event: &crate::mind::StreamEvent) -> serde_json::Value {
    let mut value = serde_json::json!({
        "kind": event.kind,
        "content": event.content,
        "time": event.time,
        "about": event.about,
    });
    if let Some(recall) = &event.recall {
        value["recall"] = serde_json::json!({
            "id": recall.id,
            "source": recall.source,
            "completed": recall.completed,
        });
    }
    value
}

fn mind_now() -> serde_json::Value {
    let signals: Vec<serde_json::Value> = crate::mind::body_signals()
        .iter()
        .map(|s| serde_json::json!({"name": s.name, "level": s.level}))
        .collect();
    let loops: Vec<serde_json::Value> = crate::mind::wake::all()
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id, "kind": p.kind, "due_at": p.due_at,
                "reason": p.reason, "about_user": p.about_user,
                "target_group": p.target_group, "target_user": p.target_user,
            })
        })
        .collect();
    let recent_stream: Vec<serde_json::Value> = crate::mind::recent(2 * 3600, 20)
        .iter()
        .map(stream_event_json)
        .collect();
    let today = crate::util::ts_to_date_str(crate::util::now_secs());
    let diary_today = crate::mind::diary::recent(100)
        .iter()
        .filter(|e| e.date == today)
        .count();
    serde_json::json!({
        "night": crate::mind::is_night(),
        "body": signals,
        "loops": loops,
        "recent_stream": recent_stream,
        "diary_today": diary_today,
        "time": crate::util::now_formatted_cst(),
    })
}

pub(crate) fn handle_mind(
    method: &Method,
    segs: &[&str],
    body: &[u8],
) -> Response<std::io::Cursor<Vec<u8>>> {
    let section = segs.first().copied().unwrap_or("");
    let rest = &segs[1.min(segs.len())..];
    match (method, section) {
        (Method::Get, "now") => ok(mind_now()),
        (Method::Get, "stream") => match rest.first() {
            Some(date) => {
                let events: Vec<serde_json::Value> = crate::mind::stream::events_on_date(date)
                    .iter()
                    .map(stream_event_json)
                    .collect();
                ok(serde_json::json!({"date": date, "events": events}))
            }
            None => ok(serde_json::json!({"dates": crate::mind::stream::known_dates()})),
        },
        (Method::Get, "diary") => {
            let entries: Vec<serde_json::Value> = crate::mind::diary::recent(200)
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.id, "date": e.date, "content": e.content,
                        "feeling": e.feeling, "about": e.about,
                    })
                })
                .collect();
            ok(serde_json::json!({"entries": entries}))
        }
        (Method::Get, "persons") => {
            let persons: Vec<serde_json::Value> = crate::mind::persons::all()
                .into_iter()
                .map(|(uid, file)| serde_json::json!({"user_id": uid, "file": file}))
                .collect();
            ok(serde_json::json!({"persons": persons}))
        }
        (Method::Put, "persons") => {
            let Some(uid) = rest.first().and_then(|s| s.parse::<u64>().ok()) else {
                return err(400, "invalid user_id");
            };
            let parsed: Result<crate::mind::PersonFile, _> = serde_json::from_slice(body);
            match parsed {
                Ok(mut file) => {
                    file.updated_at = crate::util::now_secs();
                    crate::mind::persons::save(uid, &file);
                    ok(serde_json::json!({"saved": uid}))
                }
                Err(e) => err(400, &format!("bad person file: {e}")),
            }
        }
        (Method::Get, "loops") => {
            let loops: Vec<serde_json::Value> = crate::mind::wake::all()
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id, "kind": p.kind, "due_at": p.due_at,
                        "reason": p.reason, "about_user": p.about_user,
                        "target_group": p.target_group, "target_user": p.target_user,
                    })
                })
                .collect();
            ok(serde_json::json!({"loops": loops}))
        }
        (Method::Delete, "loops") => {
            let Some(id) = rest.first().and_then(|s| s.parse::<u64>().ok()) else {
                return err(400, "invalid loop id");
            };
            ok(serde_json::json!({"closed": crate::mind::wake::close(id)}))
        }
        (Method::Get, "security") => {
            ok(serde_json::json!({"events": crate::mind::security::tail(300)}))
        }
        (Method::Get, "social") => match rest.first() {
            Some(gid) => match gid.parse::<u64>() {
                Ok(group_id) => ok(serde_json::json!({
                    "group_id": group_id,
                    "state": crate::mind::social::state_for_admin(group_id),
                })),
                Err(_) => err(400, "invalid group_id"),
            },
            None => ok(serde_json::json!({
                "groups": crate::mind::social::known_groups(),
            })),
        },
        (Method::Get, "kernel") => match crate::mind::self_model::kernel() {
            Some(k) => ok(serde_json::json!({"kernel": k})),
            None => err(404, "kernel.json 不存在——先在 data/self/kernel.json 创建"),
        },
        (Method::Put, "kernel") => {
            let parsed: Result<crate::mind::self_model::Kernel, _> = serde_json::from_slice(body);
            match parsed {
                Ok(kernel) => match crate::mind::self_model::save_kernel(&kernel) {
                    Ok(()) => ok(serde_json::json!({"saved": true})),
                    Err(e) => err(500, &e),
                },
                Err(e) => err(400, &format!("bad kernel: {e}")),
            }
        }
        _ => err(404, "not found"),
    }
}
