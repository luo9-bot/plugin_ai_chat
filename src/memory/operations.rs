use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

use super::store::{Importance, MemoryEntry, MemoryFile};

/// Embedding 批量队列
static EMBED_QUEUE: Mutex<Option<Vec<String>>> = Mutex::new(None);
const EMBED_BATCH_SIZE: usize = 10;

fn flush_embed_queue() {
    let batch = {
        let mut guard = EMBED_QUEUE.lock().unwrap();
        guard.as_mut().and_then(|q| {
            if q.is_empty() { None } else { Some(std::mem::take(q)) }
        })
    };
    if let Some(texts) = batch {
        let embeddings = crate::memory::embedding::embed_batch(&texts);
        for (text, emb_opt) in texts.into_iter().zip(embeddings.into_iter()) {
            if let Some(emb) = emb_opt {
                crate::memory::vector_store::add_vector(&text, emb);
            }
        }
    }
}

fn queue_embedding(content: &str) {
    if !crate::config::get().embedding.enabled() { return; }
    let mut guard = EMBED_QUEUE.lock().unwrap();
    let queue = guard.get_or_insert_with(Vec::new);
    queue.push(content.to_string());
    if queue.len() >= EMBED_BATCH_SIZE {
        drop(guard);
        flush_embed_queue();
    }
}

fn touch_entry(entry: &mut MemoryEntry, importance: Importance) {
    let now = crate::util::now_secs();
    entry.last_accessed = now;
    entry.access_count += 1;
    if importance == Importance::Permanent {
        entry.importance = Importance::Permanent;
    }
}

// ── 三层存储 API ────────────────────────────────────────────────

/// 添加记忆
///
/// group_id=0  → 用户的全局记忆（私聊/跨群共享，存 users/{uid}.json）
/// group_id>0  → 用户在群里的特定记忆（存 groups/{gid}/{uid}.json）
pub fn add(user_id: u64, group_id: u64, content: &str, importance: Importance) {
    let self_qq = crate::config::get().self_qq;
    if self_qq > 0 && user_id == self_qq {
        debug!(user_id, content = %content.chars().take(40).collect::<String>(), "memory: skipped (self_qq)");
        return;
    }

    let now = crate::util::now_secs();
    let content_preview: String = content.chars().take(40).collect();

    if group_id == 0 {
        // 用户全局记忆
        let mut mem = crate::memory::store::load_user_memory(user_id);
        if let Some(existing) = mem.entries.iter_mut().find(|e| e.content == content) {
            touch_entry(existing, importance);
            crate::memory::store::save_user_memory(user_id, &mem);
            debug!(user_id, content = %content_preview, "memory: updated existing (global)");
            info!(user_id, content = %content_preview, "memory: saved to JSON (update, global)");
            return;
        }
        mem.entries.push(MemoryEntry {
            content: content.to_string(),
            importance,
            created: now,
            last_accessed: now,
            access_count: 1,
        });
        crate::memory::store::save_user_memory(user_id, &mem);
        info!(user_id, content = %content_preview, "memory: saved (global)");
    } else {
        // 用户在群里的特定记忆
        let mut mem = crate::memory::store::load_group_user_memory(group_id, user_id);
        if let Some(existing) = mem.entries.iter_mut().find(|e| e.content == content) {
            touch_entry(existing, importance);
            crate::memory::store::save_group_user_memory(group_id, user_id, &mem);
            debug!(user_id, group_id, content = %content_preview, "memory: updated existing (group-user)");
            return;
        }
        mem.entries.push(MemoryEntry {
            content: content.to_string(),
            importance,
            created: now,
            last_accessed: now,
            access_count: 1,
        });
        crate::memory::store::save_group_user_memory(group_id, user_id, &mem);
        info!(user_id, group_id, content = %content_preview, "memory: saved (group-user)");
    }

    queue_embedding(content);
    crate::memory::graph::update_graph_from_memory(user_id, content);
}

/// 添加群级别记忆（存 groups/{gid}/group.json）
pub fn add_group_memory(group_id: u64, content: &str, importance: Importance) {
    let mut mem = crate::memory::store::load_group_memory(group_id);
    if mem.entries.iter().any(|e| e.content == content) {
        return;
    }
    let now = crate::util::now_secs();
    mem.entries.push(MemoryEntry {
        content: content.to_string(),
        importance,
        created: now,
        last_accessed: now,
        access_count: 1,
    });
    crate::memory::store::save_group_memory(group_id, &mem);
    debug!(group_id, content = %content.chars().take(40).collect::<String>(), "memory: saved (group level)");
}

pub fn flush_pending_embeddings() { flush_embed_queue(); }

// ── 删除/修正 ────────────────────────────────────────────────────

fn filter_and_archive(
    user_entries: &mut Vec<MemoryEntry>,
    predicate: impl Fn(&MemoryEntry) -> bool,
    user_id: u64,
) -> Vec<MemoryEntry> {
    let mut archived = Vec::new();
    let mut remaining = Vec::new();
    for entry in user_entries.drain(..) {
        if predicate(&entry) { archived.push(entry); }
        else { remaining.push(entry); }
    }
    *user_entries = remaining;
    if !archived.is_empty() {
        for entry in &archived {
            crate::memory::vector_store::remove_vector(&entry.content);
        }
        crate::archive::archive_long_term_memory(user_id, archived.clone());
    }
    archived
}

pub fn forget(user_id: u64, pattern: &str) -> Vec<String> {
    let mut total = 0;

    // 全局记忆
    let mut mem = crate::memory::store::load_user_memory(user_id);
    let archived = filter_and_archive(&mut mem.entries, |e| e.content.contains(pattern), user_id);
    if !archived.is_empty() {
        crate::memory::store::save_user_memory(user_id, &mem);
        total += archived.len();
    }

    // 所有群的群内记忆
    let groups_dir = crate::config::data_dir().join("memory").join("groups");
    if let Ok(entries) = std::fs::read_dir(&groups_dir) {
        for entry in entries.flatten() {
            if let Ok(gid) = entry.file_name().to_string_lossy().parse::<u64>() {
                let mut gmem = crate::memory::store::load_group_user_memory(gid, user_id);
                let archived = filter_and_archive(&mut gmem.entries, |e| e.content.contains(pattern), user_id);
                if !archived.is_empty() {
                    crate::memory::store::save_group_user_memory(gid, user_id, &gmem);
                    total += archived.len();
                }
            }
        }
    }

    if total > 0 { vec![format!("已遗忘 {} 条记忆", total)] }
    else { vec!["没有找到匹配的记忆".to_string()] }
}

pub fn correct(user_id: u64, old: &str, new: &str) -> usize {
    let mut count = 0;
    let now = crate::util::now_secs();

    // 全局记忆
    let mut mem = crate::memory::store::load_user_memory(user_id);
    for entry in &mut mem.entries {
        if entry.content.contains(old) {
            crate::memory::vector_store::remove_vector(&entry.content);
            entry.content = new.to_string();
            entry.last_accessed = now;
            count += 1;
        }
    }
    if count > 0 { crate::memory::store::save_user_memory(user_id, &mem); }

    // 群内记忆
    let groups_dir = crate::config::data_dir().join("memory").join("groups");
    if let Ok(entries) = std::fs::read_dir(&groups_dir) {
        for dir_entry in entries.flatten() {
            if let Ok(gid) = dir_entry.file_name().to_string_lossy().parse::<u64>() {
                let mut gmem = crate::memory::store::load_group_user_memory(gid, user_id);
                for entry in &mut gmem.entries {
                    if entry.content.contains(old) {
                        crate::memory::vector_store::remove_vector(&entry.content);
                        entry.content = new.to_string();
                        entry.last_accessed = now;
                        count += 1;
                    }
                }
                if count > 0 { crate::memory::store::save_group_user_memory(gid, user_id, &gmem); }
            }
        }
    }

    if count > 0 { debug!(user_id, old, new, count, "memory: corrected entries"); }
    count
}

pub fn forget_all(user_id: u64) {
    let mem = crate::memory::store::load_user_memory(user_id);
    if !mem.entries.is_empty() {
        for entry in &mem.entries { crate::memory::vector_store::remove_vector(&entry.content); }
        crate::archive::archive_long_term_memory(user_id, mem.entries);
    }
    crate::memory::store::save_user_memory(user_id, &MemoryFile::default());

    // 清除所有群的群内记忆
    let groups_dir = crate::config::data_dir().join("memory").join("groups");
    if let Ok(entries) = std::fs::read_dir(&groups_dir) {
        for dir_entry in entries.flatten() {
            if let Ok(gid) = dir_entry.file_name().to_string_lossy().parse::<u64>() {
                let gmem = crate::memory::store::load_group_user_memory(gid, user_id);
                if !gmem.entries.is_empty() {
                    for entry in &gmem.entries { crate::memory::vector_store::remove_vector(&entry.content); }
                    crate::archive::archive_long_term_memory(user_id, gmem.entries);
                }
                crate::memory::store::save_group_user_memory(gid, user_id, &MemoryFile::default());
            }
        }
    }
}

// ── 注入冷却（磁盘持久化） ──────────────────────────────────────

const INJECTION_COOLDOWN_SECS: u64 = 1800;

fn inject_key(user_id: u64, group_id: u64, content: &str) -> String {
    format!("{}:{}:{}", user_id, group_id, content)
}

fn recently_injected_path() -> std::path::PathBuf {
    crate::config::data_dir().join("recently_injected.json")
}

fn load_injected_map() -> HashMap<String, u64> {
    let path = recently_injected_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_injected_map(map: &HashMap<String, u64>) {
    let path = recently_injected_path();
    if let Ok(json) = serde_json::to_string_pretty(map) {
        std::fs::write(path, json).ok();
    }
}

fn is_recently_injected(user_id: u64, group_id: u64, content: &str) -> bool {
    let key = inject_key(user_id, group_id, content);
    let map = load_injected_map();
    if let Some(ts) = map.get(&key) {
        return crate::util::now_secs().saturating_sub(*ts) < INJECTION_COOLDOWN_SECS;
    }
    false
}

fn mark_injected(user_id: u64, group_id: u64, content: &str) {
    let key = inject_key(user_id, group_id, content);
    let now = crate::util::now_secs();
    let mut map = load_injected_map();
    map.insert(key, now);
    // 清理过期条目
    map.retain(|_, ts| now.saturating_sub(*ts) < INJECTION_COOLDOWN_SECS);
    save_injected_map(&map);
}

// ── 上下文读取 ───────────────────────────────────────────────────

fn collect_entries(
    entries: &[MemoryEntry],
    user_id: u64,
    group_id: u64,
    cfg: &crate::config::MemoryConfig,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let now = crate::util::now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut permanent = Vec::new();
    let mut important = Vec::new();
    let mut normal = Vec::new();

    for entry in entries {
        if entry.importance == Importance::Normal
            && now.saturating_sub(entry.last_accessed) > normal_expire {
            continue;
        }
        if entry.importance != Importance::Permanent
            && is_recently_injected(user_id, group_id, &entry.content) {
            continue;
        }
        let tag = match entry.importance {
            Importance::Permanent => "[永久]",
            Importance::Important => "[重要]",
            Importance::Normal => {
                if now.saturating_sub(entry.last_accessed) > important_fade { "[淡忘]" }
                else { "" }
            }
        };
        let formatted = format!("- {}{}", tag, entry.content);
        match entry.importance {
            Importance::Permanent => permanent.push(formatted),
            Importance::Important => important.push(formatted),
            Importance::Normal => normal.push(formatted),
        }
        if entry.importance != Importance::Permanent {
            mark_injected(user_id, group_id, &entry.content);
        }
    }
    (permanent, important, normal)
}

/// 获取用户在指定上下文中的记忆
///
/// current_group_id=0  → 只显示全局记忆（私聊）
/// current_group_id>0  → 显示全局 + 该群特定记忆（群聊）
pub fn get_context(user_id: u64, current_group_id: u64) -> String {
    let cfg = &crate::config::get().memory;

    // 全局记忆
    let global = crate::memory::store::load_user_memory(user_id);
    let (perm, imp, norm) = collect_entries(&global.entries, user_id, current_group_id, cfg);
    let mut lines: Vec<String> = Vec::new();
    lines.extend(perm);
    lines.extend(imp);
    lines.extend(norm.into_iter().take(5));

    // 群特定记忆（仅群聊上下文时显示）
    if current_group_id > 0 {
        let group_user = crate::memory::store::load_group_user_memory(current_group_id, user_id);
        let (g_perm, g_imp, g_norm) = collect_entries(&group_user.entries, user_id, current_group_id, cfg);
        if !g_perm.is_empty() || !g_imp.is_empty() || !g_norm.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("--- 在群{}中的记忆 ---", current_group_id));
            for e in g_perm { lines.push(e); }
            for e in g_imp { lines.push(e); }
            for e in g_norm.into_iter().take(5) { lines.push(e); }
        }
    }

    if lines.is_empty() { return String::new(); }
    let display_name = crate::person_info::get_display_name(user_id, current_group_id)
        .unwrap_or_else(|| "群友".to_string());
    format!("# 关于{}的记忆\n{}", display_name, lines.join("\n"))
}

/// 获取群内其他成员的记忆（交叉引用）
pub fn get_group_context(group_id: u64, exclude_user: u64) -> String {
    let participants = crate::working_memory::get_participants(group_id);
    if participants.is_empty() { return String::new(); }

    let cfg = &crate::config::get().memory;
    let mut user_blocks = Vec::new();

    for &uid in &participants {
        if uid == exclude_user || uid == 0 { continue; }

        let global = crate::memory::store::load_user_memory(uid);
        let group_user = crate::memory::store::load_group_user_memory(group_id, uid);
        let (_, _, mut lines) = collect_entries(&global.entries, uid, group_id, cfg);
        let (_, _, mut g_lines) = collect_entries(&group_user.entries, uid, group_id, cfg);
        lines.append(&mut g_lines);

        if !lines.is_empty() {
            let name = crate::person_info::get_display_name(uid, group_id)
                .unwrap_or_else(|| "群友".to_string());
            user_blocks.push(format!("{}:\n{}", name, lines.join("\n")));
        }
    }

    if user_blocks.is_empty() { return String::new(); }
    format!("# 群内其他成员的记忆\n{}", user_blocks.join("\n"))
}

/// 获取群级别记忆（groups/{gid}/group.json）
pub fn get_group_level_context(group_id: u64) -> String {
    let mem = crate::memory::store::load_group_memory(group_id);
    if mem.entries.is_empty() { return String::new(); }
    let lines: Vec<String> = mem.entries.iter().map(|e| format!("- {}", e.content)).collect();
    format!("# 群记忆\n{}", lines.join("\n"))
}

// ── 命令识别 ────────────────────────────────────────────────────

pub fn check_forget_command(user_id: u64, message: &str) -> Option<String> {
    let forget_patterns = ["忘掉", "忘记", "不要记", "别记"];
    for pattern in &forget_patterns {
        if message.contains(pattern) {
            let content = message
                .replace(pattern, "")
                .replace("我刚才说的", "").replace("刚才说的", "").replace("刚才说", "")
                .trim().to_string();
            if content.is_empty() {
                forget_all(user_id);
                return Some("已清除相关记忆".to_string());
            } else {
                let result = forget(user_id, &content);
                return Some(result.join("\n"));
            }
        }
    }
    None
}
