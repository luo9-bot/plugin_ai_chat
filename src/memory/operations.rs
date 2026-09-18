use std::sync::Mutex;
use tracing::{debug, info};

use super::store::{Importance, MemoryEntry, MemoryFile};
use crate::util::MutexExt;

/// Embedding 批量队列
static EMBED_QUEUE: Mutex<Option<Vec<String>>> = Mutex::new(None);
const EMBED_BATCH_SIZE: usize = 10;

fn flush_embed_batch(texts: Vec<String>) {
    let embeddings = crate::memory::embedding::embed_batch(&texts);
    for (text, emb_opt) in texts.into_iter().zip(embeddings) {
        if let Some(emb) = emb_opt {
            crate::memory::vector_store::add_vector(&text, emb);
        }
    }
}

fn flush_embed_queue() {
    let batch = {
        let mut guard = EMBED_QUEUE.lock_recover();
        guard.as_mut().and_then(|q| {
            if q.is_empty() {
                None
            } else {
                Some(std::mem::take(q))
            }
        })
    };
    if let Some(texts) = batch {
        flush_embed_batch(texts);
    }
}

fn queue_embedding(content: &str) {
    if !crate::config::get().embedding.enabled() {
        return;
    }
    let mut guard = EMBED_QUEUE.lock_recover();
    let queue = guard.get_or_insert_with(Vec::new);
    queue.push(content.to_string());
    if queue.len() >= EMBED_BATCH_SIZE {
        let batch = std::mem::take(queue);
        drop(guard);
        std::thread::spawn(move || {
            flush_embed_batch(batch);
        });
    }
}

pub(crate) fn touch_entry(entry: &mut MemoryEntry, importance: Importance) {
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
pub(crate) fn add(user_id: u64, group_id: u64, content: &str, importance: Importance) {
    add_with_impact(user_id, group_id, content, importance, None);
}

/// 添加记忆并记录情绪冲击（-10~+10）：冲击极强的记忆可能以闪回的方式突现
pub(crate) fn add_with_impact(
    user_id: u64,
    group_id: u64,
    content: &str,
    importance: Importance,
    emotional_impact: Option<f32>,
) {
    let self_qq = crate::config::get().self_qq;
    if self_qq > 0 && user_id == self_qq {
        debug!(user_id, content = %content.chars().take(40).collect::<String>(), "memory: skipped (self_qq)");
        return;
    }

    let imp_str = match importance {
        Importance::Permanent => "permanent",
        Importance::Important => "important",
        Importance::Normal => "normal",
    };
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
            emotional_impact,
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
            emotional_impact,
        });
        crate::memory::store::save_group_user_memory(group_id, user_id, &mem);
        info!(user_id, group_id, content = %content_preview, "memory: saved (group-user)");
    }

    queue_embedding(content);
    crate::memory::graph::update_graph_from_memory(user_id, content);

    let loc = if group_id == 0 {
        "global"
    } else {
        &format!("group_{}", group_id)
    };
    super::ops_log::record(
        "add",
        user_id,
        group_id,
        content,
        imp_str,
        &format!("saved ({})", loc),
    );
}

pub(crate) fn flush_pending_embeddings() {
    flush_embed_queue();
}

// ── 删除/修正 ────────────────────────────────────────────────────

fn filter_and_archive(
    user_entries: &mut Vec<MemoryEntry>,
    predicate: impl Fn(&MemoryEntry) -> bool,
    user_id: u64,
) -> Vec<MemoryEntry> {
    let mut archived = Vec::new();
    let mut remaining = Vec::new();
    for entry in user_entries.drain(..) {
        if predicate(&entry) {
            archived.push(entry);
        } else {
            remaining.push(entry);
        }
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

pub(crate) fn forget(user_id: u64, pattern: &str) -> Vec<String> {
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
                let archived =
                    filter_and_archive(&mut gmem.entries, |e| e.content.contains(pattern), user_id);
                if !archived.is_empty() {
                    crate::memory::store::save_group_user_memory(gid, user_id, &gmem);
                    total += archived.len();
                }
            }
        }
    }

    if total > 0 {
        super::ops_log::record(
            "forget",
            user_id,
            0,
            pattern,
            "normal",
            &format!("forgot {} entries", total),
        );
        vec![format!("已遗忘 {} 条记忆", total)]
    } else {
        vec!["没有找到匹配的记忆".to_string()]
    }
}

pub(crate) fn forget_all(user_id: u64) {
    super::ops_log::record(
        "forget_all",
        user_id,
        0,
        "*",
        "normal",
        "forget all memories",
    );
    let mem = crate::memory::store::load_user_memory(user_id);
    if !mem.entries.is_empty() {
        for entry in &mem.entries {
            crate::memory::vector_store::remove_vector(&entry.content);
        }
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
                    for entry in &gmem.entries {
                        crate::memory::vector_store::remove_vector(&entry.content);
                    }
                    crate::archive::archive_long_term_memory(user_id, gmem.entries);
                }
                crate::memory::store::save_group_user_memory(gid, user_id, &MemoryFile::default());
            }
        }
    }
}

// ── 遗忘命令 ────────────────────────────────────────────────────

pub(crate) fn check_forget_command(user_id: u64, message: &str) -> Option<String> {
    let forget_patterns = ["忘掉", "忘记", "不要记", "别记"];
    for pattern in &forget_patterns {
        if message.contains(pattern) {
            let content = message
                .replace(pattern, "")
                .replace("我刚才说的", "")
                .replace("刚才说的", "")
                .replace("刚才说", "")
                .trim()
                .to_string();
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
