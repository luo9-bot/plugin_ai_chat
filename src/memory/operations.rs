use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

use super::store::{Importance, MemoryEntry, MemoryStore};

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

/// 添加一条用户记忆
///
/// group_id 含义:
///   0 = 私聊记忆 (不在群聊上下文中展示)
///  >0 = 群聊记忆 (只在该群聊上下文中展示)
pub fn add(user_id: u64, group_id: u64, content: &str, importance: Importance) {
    let self_qq = crate::config::get().self_qq;
    if self_qq > 0 && user_id == self_qq {
        debug!(user_id, content = %content.chars().take(40).collect::<String>(), "memory: skipped (self_qq)");
        return;
    }

    let mut store = MemoryStore::load();
    let now = crate::util::now_secs();
    let user = store.get_user_mut(user_id);

    if let Some(entry) = user.entries.iter_mut().find(|e| e.content == content) {
        entry.last_accessed = now;
        entry.access_count += 1;
        if importance == Importance::Permanent {
            entry.importance = Importance::Permanent;
        }
        let content_preview: String = content.chars().take(40).collect();
        debug!(user_id, content = %content_preview, "memory: updated existing entry");
        store.save();
        info!(user_id, content = %content_preview, "memory: saved to JSON (update)");
        return;
    }

    let content_preview: String = content.chars().take(40).collect();
    debug!(user_id, group_id, content = %content_preview, ?importance, "memory: added new entry");
    user.entries.push(MemoryEntry {
        content: content.to_string(),
        importance,
        group_id,
        created: now,
        last_accessed: now,
        access_count: 1,
    });
    store.save();
    info!(user_id, content = %content_preview, "memory: saved to JSON (new)");

    queue_embedding(content);
    crate::memory::graph::update_graph_from_memory(user_id, content);
}

/// 添加群组级别的记忆（不归属具体用户，如群氛围、共同话题）
pub fn add_group_memory(group_id: u64, content: &str, importance: Importance) {
    let mut store = MemoryStore::load();
    let now = crate::util::now_secs();

    let entries = store.group_memories.entry(group_id.to_string()).or_default();
    if entries.iter().any(|e| e.content == content) {
        return;
    }

    entries.push(MemoryEntry {
        content: content.to_string(),
        importance,
        group_id,
        created: now,
        last_accessed: now,
        access_count: 1,
    });
    store.save();
    debug!(group_id, content = %content.chars().take(40).collect::<String>(), "memory: added group memory");
}

pub fn flush_pending_embeddings() { flush_embed_queue(); }

pub fn forget(user_id: u64, pattern: &str) -> Vec<String> {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let mut archived = Vec::new();
    let mut remaining = Vec::new();
    for entry in user.entries.drain(..) {
        if entry.content.contains(pattern) { archived.push(entry); }
        else { remaining.push(entry); }
    }
    user.entries = remaining;
    let removed = archived.len();
    if removed > 0 {
        for entry in &archived { crate::memory::vector_store::remove_vector(&entry.content); }
        crate::archive::archive_long_term_memory(user_id, archived);
    }
    store.save();
    if removed > 0 { vec![format!("已遗忘 {} 条记忆", removed)] }
    else { vec!["没有找到匹配的记忆".to_string()] }
}

pub fn correct(user_id: u64, old: &str, new: &str) -> usize {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let now = crate::util::now_secs();
    let mut count = 0;
    if new.is_empty() {
        let mut archived = Vec::new();
        let mut remaining = Vec::new();
        for entry in user.entries.drain(..) {
            if entry.content.contains(old) { archived.push(entry); count += 1; }
            else { remaining.push(entry); }
        }
        user.entries = remaining;
        if !archived.is_empty() {
            for entry in &archived { crate::memory::vector_store::remove_vector(&entry.content); }
            crate::archive::archive_long_term_memory(user_id, archived);
        }
    } else {
        for entry in &mut user.entries {
            if entry.content.contains(old) {
                crate::memory::vector_store::remove_vector(&entry.content);
                entry.content = new.to_string();
                entry.last_accessed = now;
                count += 1;
            }
        }
    }
    if count > 0 { store.save(); debug!(user_id, old, new, count, "memory: corrected entries"); }
    count
}

pub fn forget_all(user_id: u64) {
    let mut store = MemoryStore::load();
    if let Some(user) = store.users.remove(&user_id.to_string())
        && !user.entries.is_empty() {
            for entry in &user.entries { crate::memory::vector_store::remove_vector(&entry.content); }
            crate::archive::archive_long_term_memory(user_id, user.entries);
        }
    store.save();
}

/// 最近注入的记忆缓存
static RECENTLY_INJECTED: std::sync::Mutex<Option<HashMap<String, u64>>> = std::sync::Mutex::new(None);
const INJECTION_COOLDOWN_SECS: u64 = 1800;

fn is_recently_injected(user_id: u64, content: &str) -> bool {
    let key = format!("{}:{}", user_id, content);
    let guard = RECENTLY_INJECTED.lock().unwrap();
    if let Some(ref map) = *guard && let Some(ts) = map.get(&key) {
        return crate::util::now_secs().saturating_sub(*ts) < INJECTION_COOLDOWN_SECS;
    }
    false
}

fn mark_injected(user_id: u64, content: &str) {
    let key = format!("{}:{}", user_id, content);
    let mut guard = RECENTLY_INJECTED.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(key, crate::util::now_secs());
}

/// 判断记忆条目是否应在当前上下文中展示
pub(crate) fn should_show_entry(entry: &MemoryEntry, current_group_id: u64) -> bool {
    match entry.group_id as i64 {
        -1 => current_group_id == 0,        // 私聊标记：只私聊显示
        0 => true,                           // 传统遗留：到处显示（向后兼容）
        gid => gid as u64 == current_group_id, // 群特定：只在该群显示
    }
}

/// 获取用户在指定上下文中的记忆
///
/// current_group_id=0: 私聊上下文 — 只展示 legacy + 私聊记忆
/// current_group_id>0: 群聊上下文 — 只展示 legacy + 该群记忆
pub fn get_context(user_id: u64, current_group_id: u64) -> String {
    let store = MemoryStore::load();
    let user = match store.users.get(&user_id.to_string()) {
        Some(u) => u,
        None => return String::new(),
    };
    if user.entries.is_empty() { return String::new(); }

    let cfg = &crate::config::get().memory;
    let now = crate::util::now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut permanent_lines = Vec::new();
    let mut important_lines = Vec::new();
    let mut normal_lines = Vec::new();
    let source_label = if current_group_id > 0 { format!("(来自群{})", current_group_id) } else { "(私聊)".to_string() };

    for entry in &user.entries {
        // 范围过滤：只展示当前上下文相关的记忆
        if !should_show_entry(entry, current_group_id) {
            continue;
        }
        if entry.importance == Importance::Normal
            && now.saturating_sub(entry.last_accessed) > normal_expire {
            continue;
        }
        if entry.importance != Importance::Permanent
            && is_recently_injected(user_id, &entry.content) {
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
        match entry.importance {
            Importance::Permanent => permanent_lines.push(format!("- {}{} {}", tag, entry.content, source_label)),
            Importance::Important => important_lines.push(format!("- {}{} {}", tag, entry.content, source_label)),
            Importance::Normal => normal_lines.push(format!("- {}{} {}", tag, entry.content, source_label)),
        }
        if entry.importance != Importance::Permanent {
            mark_injected(user_id, &entry.content);
        }
    }

    let mut lines = permanent_lines;
    lines.extend(important_lines);
    lines.extend(normal_lines.into_iter().take(5));
    if lines.is_empty() { return String::new(); }
    format!("# 关于用户{}的记忆\n{}", user_id, lines.join("\n"))
}

/// 获取群内其他成员的记忆（交叉引用）
/// 只展示与当前群相关的记忆
pub fn get_group_context(group_id: u64, exclude_user: u64) -> String {
    let participants = crate::working_memory::get_participants(group_id);
    if participants.is_empty() { return String::new(); }

    let store = MemoryStore::load();
    let now = crate::util::now_secs();
    let normal_expire = crate::config::get().memory.normal_expire_days * 86400;
    let mut user_blocks = Vec::new();

    for user_id in &participants {
        if *user_id == exclude_user { continue; }
        if let Some(user_mem) = store.users.get(&user_id.to_string()) {
            let mut lines = Vec::new();
            for entry in &user_mem.entries {
                if !should_show_entry(entry, group_id) { continue; }
                if entry.importance == Importance::Normal
                    && now.saturating_sub(entry.last_accessed) > normal_expire {
                    continue;
                }
                let tag = match entry.importance {
                    Importance::Permanent => "[永久]",
                    Importance::Important => "[重要]",
                    Importance::Normal => "[普通]",
                };
                lines.push(format!("  - {}{}", tag, entry.content));
            }
            if !lines.is_empty() {
                user_blocks.push(format!("用户{}:\n{}", user_id, lines.join("\n")));
            }
        }
    }

    if user_blocks.is_empty() { return String::new(); }
    format!("# 群内其他成员的记忆\n{}", user_blocks.join("\n"))
}

/// 获取群组级别的记忆（群氛围、共同话题等）
pub fn get_group_level_context(group_id: u64) -> String {
    let store = MemoryStore::load();
    let entries = match store.group_memories.get(&group_id.to_string()) {
        Some(e) => e,
        None => return String::new(),
    };
    let lines: Vec<String> = entries.iter().map(|e| {
        format!("- {}", e.content)
    }).collect();
    format!("# 群记忆\n{}", lines.join("\n"))
}

pub fn check_forget_command(user_id: u64, message: &str) -> Option<String> {
    let forget_patterns = ["忘掉", "忘记", "不要记", "别记"];
    for pattern in &forget_patterns {
        if message.contains(pattern) {
            let content = message
                .replace(pattern, "")
                .replace("我刚才说的", "").replace("刚才说的", "").replace("刚才说", "")
                .trim().to_string();
            if content.is_empty() {
                let mut store = MemoryStore::load();
                let user = store.get_user_mut(user_id);
                let mut archived = Vec::new();
                let mut remaining = Vec::new();
                for entry in user.entries.drain(..) {
                    if entry.importance == Importance::Permanent { remaining.push(entry); }
                    else { archived.push(entry); }
                }
                user.entries = remaining;
                if !archived.is_empty() {
                    for entry in &archived { crate::memory::vector_store::remove_vector(&entry.content); }
                    crate::archive::archive_long_term_memory(user_id, archived);
                }
                store.save();
                return Some("已清除近期记忆".to_string());
            } else {
                let result = forget(user_id, &content);
                return Some(result.join("\n"));
            }
        }
    }
    None
}
