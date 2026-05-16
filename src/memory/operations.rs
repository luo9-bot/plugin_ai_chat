use std::collections::HashMap;
use tracing::{debug, info};

use super::store::{Importance, MemoryEntry, MemoryStore};

pub fn add(user_id: u64, content: &str, importance: Importance) {
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
    debug!(user_id, content = %content_preview, ?importance, "memory: added new entry");
    user.entries.push(MemoryEntry {
        content: content.to_string(),
        importance,
        created: now,
        last_accessed: now,
        access_count: 1,
    });
    store.save();
    info!(user_id, content = %content_preview, "memory: saved to JSON (new)");

    // 生成 embedding 并写入 vectors.bin（可选）
    if crate::config::get().embedding.enabled()
        && let Some(embedding) = crate::memory::embedding::embed_text(content) {
            crate::memory::vector_store::add_vector(content, embedding);
            debug!(user_id, "memory: embedding saved to vectors.bin");
        }

    // 更新知识图谱
    crate::memory::graph::update_graph_from_memory(user_id, content);
}

pub fn forget(user_id: u64, pattern: &str) -> Vec<String> {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let mut archived = Vec::new();
    let mut remaining = Vec::new();
    for entry in user.entries.drain(..) {
        if entry.content.contains(pattern) {
            archived.push(entry);
        } else {
            remaining.push(entry);
        }
    }
    user.entries = remaining;
    let removed = archived.len();
    if removed > 0 {
        // 从向量存储中删除被遗忘的记忆
        for entry in &archived {
            crate::memory::vector_store::remove_vector(&entry.content);
        }
        crate::archive::archive_long_term_memory(user_id, archived);
    }
    store.save();
    if removed > 0 {
        vec![format!("已遗忘 {} 条记忆", removed)]
    } else {
        vec!["没有找到匹配的记忆".to_string()]
    }
}

/// 修正用户记忆：根据 old 模糊匹配，替换为 new (new 为空则删除)
/// 返回修正的条数
pub fn correct(user_id: u64, old: &str, new: &str) -> usize {
    let mut store = MemoryStore::load();
    let user = store.get_user_mut(user_id);
    let now = crate::util::now_secs();
    let mut count = 0;

    if new.is_empty() {
        // 删除匹配的记忆
        let mut archived = Vec::new();
        let mut remaining = Vec::new();
        for entry in user.entries.drain(..) {
            if entry.content.contains(old) {
                archived.push(entry);
                count += 1;
            } else {
                remaining.push(entry);
            }
        }
        user.entries = remaining;
        if !archived.is_empty() {
            // 从向量存储中删除
            for entry in &archived {
                crate::memory::vector_store::remove_vector(&entry.content);
            }
            crate::archive::archive_long_term_memory(user_id, archived);
        }
    } else {
        // 更新匹配的记忆
        for entry in &mut user.entries {
            if entry.content.contains(old) {
                // 从向量存储中删除旧的
                crate::memory::vector_store::remove_vector(&entry.content);
                entry.content = new.to_string();
                entry.last_accessed = now;
                count += 1;
            }
        }
    }

    if count > 0 {
        store.save();
        debug!(user_id, old, new, count, "memory: corrected entries");
    }
    count
}

pub fn forget_all(user_id: u64) {
    let mut store = MemoryStore::load();
    if let Some(user) = store.users.remove(&user_id.to_string())
        && !user.entries.is_empty() {
            // 从向量存储中删除
            for entry in &user.entries {
                crate::memory::vector_store::remove_vector(&entry.content);
            }
            crate::archive::archive_long_term_memory(user_id, user.entries);
        }
    store.save();
}

/// 最近注入的记忆缓存 (user_id_content_hash -> timestamp)
///
/// 避免同一记忆在短时间内反复注入上下文
static RECENTLY_INJECTED: std::sync::Mutex<Option<HashMap<String, u64>>> = std::sync::Mutex::new(None);
const INJECTION_COOLDOWN_SECS: u64 = 1800; // 30 分钟内不重复注入

fn is_recently_injected(user_id: u64, content: &str) -> bool {
    let key = format!("{}:{}", user_id, content);
    let guard = RECENTLY_INJECTED.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(ts) = map.get(&key) {
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

pub fn get_context(user_id: u64) -> String {
    let store = MemoryStore::load();
    let user = match store.users.get(&user_id.to_string()) {
        Some(u) => u,
        None => return String::new(),
    };
    if user.entries.is_empty() {
        return String::new();
    }

    let cfg = &crate::config::get().memory;
    let now = crate::util::now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut permanent_lines = Vec::new();
    let mut important_lines = Vec::new();
    let mut normal_lines = Vec::new();

    for entry in &user.entries {
        if entry.importance == Importance::Normal
            && now.saturating_sub(entry.last_accessed) > normal_expire
        {
            continue;
        }
        // 对普通记忆应用冷却，重要/永久记忆每次都注入
        if entry.importance != Importance::Permanent
            && is_recently_injected(user_id, &entry.content)
        {
            continue;
        }
        let tag = match entry.importance {
            Importance::Permanent => "[永久]",
            Importance::Important => "[重要]",
            Importance::Normal => {
                if now.saturating_sub(entry.last_accessed) > important_fade {
                    "[淡忘]"
                } else {
                    ""
                }
            }
        };
        match entry.importance {
            Importance::Permanent => permanent_lines.push(format!("- {}{}", tag, entry.content)),
            Importance::Important => important_lines.push(format!("- {}{}", tag, entry.content)),
            Importance::Normal => normal_lines.push(format!("- {}{}", tag, entry.content)),
        }
        // 只标记非永久记忆为已注入（永久记忆每次都展示）
        if entry.importance != Importance::Permanent {
            mark_injected(user_id, &entry.content);
        }
    }

    // 按优先级拼接：永久 > 重要 > 普通，普通记忆最多 5 条
    let mut lines = permanent_lines;
    lines.extend(important_lines);
    lines.extend(normal_lines.into_iter().take(5));

    if lines.is_empty() {
        return String::new();
    }

    format!("# 关于用户的记忆\n{}", lines.join("\n"))
}

/// 获取群组级别的记忆上下文 (包含群内所有参与者的记忆)
/// 解决问题: 用户A提及用户B时，AI需要知道B是谁
pub fn get_group_context(group_id: u64, exclude_user: u64) -> String {
    // 从工作记忆获取群内参与者列表
    let participants = crate::working_memory::get_participants(group_id);
    if participants.is_empty() {
        return String::new();
    }

    let store = MemoryStore::load();
    let cfg = &crate::config::get().memory;
    let now = crate::util::now_secs();
    let normal_expire = cfg.normal_expire_days * 86400;
    let important_fade = cfg.important_fade_days * 86400;
    let mut user_blocks = Vec::new();

    for user_id in &participants {
        if *user_id == exclude_user {
            continue; // 排除当前用户（已有独立记忆上下文）
        }
        if let Some(user_mem) = store.users.get(&user_id.to_string()) {
            let mut lines = Vec::new();
            for entry in &user_mem.entries {
                if entry.importance == Importance::Normal
                    && now.saturating_sub(entry.last_accessed) > normal_expire
                {
                    continue;
                }
                let tag = match entry.importance {
                    Importance::Permanent => "[永久]",
                    Importance::Important => "[重要]",
                    Importance::Normal => {
                        if now.saturating_sub(entry.last_accessed) > important_fade {
                            "[淡忘]"
                        } else {
                            ""
                        }
                    }
                };
                lines.push(format!("  - {}{}", tag, entry.content));
            }
            if !lines.is_empty() {
                user_blocks.push(format!("用户{}:\n{}", user_id, lines.join("\n")));
            }
        }
    }

    if user_blocks.is_empty() {
        return String::new();
    }
    format!("# 群内其他成员的记忆\n{}", user_blocks.join("\n"))
}

pub fn check_forget_command(user_id: u64, message: &str) -> Option<String> {
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
                let mut store = MemoryStore::load();
                let user = store.get_user_mut(user_id);
                let mut archived = Vec::new();
                let mut remaining = Vec::new();
                for entry in user.entries.drain(..) {
                    if entry.importance == Importance::Permanent {
                        remaining.push(entry);
                    } else {
                        archived.push(entry);
                    }
                }
                user.entries = remaining;
                if !archived.is_empty() {
                    // 从向量存储中删除
                    for entry in &archived {
                        crate::memory::vector_store::remove_vector(&entry.content);
                    }
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
