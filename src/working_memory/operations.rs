use super::store::{Entry, WorkingMemoryStore};

/// 记录一条消息 (无论是否回复)，返回写入时间戳
///
/// 存储前剥离 Unicode emoji，防止污染记忆
pub fn record(group_id: u64, user_id: u64, content: &str, bot_replied: bool) -> u64 {
    let mut store = WorkingMemoryStore::load();
    let ts = crate::util::now_secs();
    let group = store.groups.entry(group_id.to_string()).or_default();
    // 剥离 Unicode emoji 后存储
    let cleaned = crate::emoji::strip_emoji(content);
    group.entries.push(Entry {
        user_id,
        content: cleaned,
        timestamp: ts,
        bot_replied,
    });
    // 每群最多保留 200 条
    if group.entries.len() > 200 {
        let drain_count = group.entries.len() - 200;
        group.entries.drain(0..drain_count);
    }
    store.save();
    ts
}

/// 标记某用户最近的消息为已回复
pub fn mark_replied(group_id: u64, user_id: u64) {
    let mut store = WorkingMemoryStore::load();
    if let Some(group) = store.groups.get_mut(&group_id.to_string())
        && let Some(entry) = group.entries.iter_mut().rev().find(|e| e.user_id == user_id && !e.bot_replied)
    {
        entry.bot_replied = true;
    }
    store.save();
}

/// 记录机器人自己的回复到工作记忆（让 AI 知道之前说过什么）
pub fn record_bot_reply(group_id: u64, content: &str) {
    let self_qq = crate::config::get().self_qq;
    if self_qq == 0 { return; }
    let mut store = WorkingMemoryStore::load();
    let ts = crate::util::now_secs();
    let group = store.groups.entry(group_id.to_string()).or_default();
    let cleaned = crate::emoji::strip_emoji(content);
    group.entries.push(Entry {
        user_id: self_qq,
        content: cleaned,
        timestamp: ts,
        bot_replied: true,
    });
    if group.entries.len() > 200 {
        let drain_count = group.entries.len() - 200;
        group.entries.drain(0..drain_count);
    }
    store.save();
}

/// 获取某群最近用户消息的最新时间戳（排除 bot 自身）
/// 用于主动消息系统判断是否有新消息
pub fn get_latest_user_message_ts(group_id: u64) -> u64 {
    let store = WorkingMemoryStore::load();
    let self_qq = crate::config::get().self_qq;
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            group.entries.iter()
                .rev()
                .filter(|e| self_qq == 0 || e.user_id != self_qq)
                .map(|e| e.timestamp)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// 获取某群最近的消息
pub fn get_recent(group_id: u64, max_age_secs: u64, max_count: usize) -> Vec<Entry> {
    let store = WorkingMemoryStore::load();
    let now = crate::util::now_secs();
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            group.entries.iter()
                .rev()
                .filter(|e| now.saturating_sub(e.timestamp) < max_age_secs)
                .take(max_count)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        })
        .unwrap_or_default()
}

/// 获取指定时间戳之后的群聊消息
pub fn get_since(group_id: u64, since_timestamp: u64, max_count: usize) -> Vec<Entry> {
    let store = WorkingMemoryStore::load();
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            group.entries.iter()
                .filter(|e| e.timestamp > since_timestamp)
                .take(max_count)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// 获取格式化的群聊工作记忆上下文 (用于 AI 决策)
pub fn get_context(group_id: u64, max_age_secs: u64) -> String {
    get_context_with_window(group_id, max_age_secs, 15)
}

/// 带缓存稳定性放大的上下文获取
///
/// 保留 base_count * 2 条消息，但只将后半部分标记为"新消息"。
/// 旧消息逐步退出而不是突然消失，减少上下文抖动。
/// 每条消息附带相对时间戳，帮助 AI 理解时间关系。
pub fn get_context_with_window(group_id: u64, max_age_secs: u64, base_count: usize) -> String {
    get_context_filtered(group_id, max_age_secs, base_count, false)
}

/// 获取排除了 bot 自身消息的工作记忆上下文（用于主动消息生成等场景）
///
/// 防止 bot 把自己的消息当成其他用户的消息来理解，产生幻觉。
pub fn get_context_no_self(group_id: u64, max_age_secs: u64) -> String {
    get_context_filtered(group_id, max_age_secs, 15, true)
}

fn get_context_filtered(group_id: u64, max_age_secs: u64, base_count: usize, exclude_self: bool) -> String {
    let expanded_count = base_count * 2;
    let entries = get_recent(group_id, max_age_secs, expanded_count);
    if entries.is_empty() {
        return String::new();
    }

    let now = crate::util::now_secs();
    let self_qq = crate::config::get().self_qq;

    // 排除 bot 自身消息（主动消息场景下防止自我引用幻觉）
    let filtered: Vec<&Entry> = if exclude_self && self_qq > 0 {
        entries.iter().filter(|e| e.user_id != self_qq).collect()
    } else {
        entries.iter().collect()
    };

    if filtered.is_empty() {
        return String::new();
    }

    let new_start = filtered.len().saturating_sub(base_count);

    let mut lines: Vec<String> = Vec::new();
    let mut iter = filtered.iter().enumerate().peekable();
    while let Some((idx, first)) = iter.next() {
        let is_new = idx >= new_start;
        let is_self = self_qq > 0 && first.user_id == self_qq;
        let who = if is_self { "bot".to_string() } else {
            crate::person_info::get_display_name(first.user_id, group_id)
                .unwrap_or_else(|| "群友".to_string())
        };
        let tag = if first.bot_replied { "[已回复]" } else { "" };
        let mut block = first.content.clone();
        while let Some((_, next)) = iter.peek() {
            if next.user_id == first.user_id && next.bot_replied == first.bot_replied {
                block.push('\n');
                block.push_str(&next.content);
                iter.next();
            } else {
                break;
            }
        }
        let time_ago = format_time_ago(now.saturating_sub(first.timestamp));
        let new_tag = if is_new { "" } else { "[旧] " };
        lines.push(format!("[{}{} {} ({})", who, tag, new_tag, time_ago));
        // 实际内容追加在下一行，避免行过长
        lines.push(format!("  {}", block));
    }
    format!("# 群聊工作记忆 (最近消息)\n{}", lines.join("\n"))
}

/// 格式化相对时间
fn format_time_ago(seconds_ago: u64) -> String {
    if seconds_ago < 60 {
        "刚刚".to_string()
    } else if seconds_ago < 3600 {
        format!("{}分钟前", seconds_ago / 60)
    } else if seconds_ago < 86400 {
        format!("{}小时前", seconds_ago / 3600)
    } else {
        format!("{}天前", seconds_ago / 86400)
    }
}

/// 清理过期的工作记忆 (归档后删除)
pub fn cleanup(max_age_secs: u64) {
    let mut store = WorkingMemoryStore::load();
    let now = crate::util::now_secs();
    let mut to_archive = Vec::new();

    for (group_id_str, group) in store.groups.iter_mut() {
        let group_id: u64 = group_id_str.parse().unwrap_or(0);
        let mut remaining = Vec::new();
        for entry in group.entries.drain(..) {
            if now.saturating_sub(entry.timestamp) >= max_age_secs {
                to_archive.push((group_id, entry));
            } else {
                remaining.push(entry);
            }
        }
        group.entries = remaining;
    }

    // 移除空群
    store.groups.retain(|_, g| !g.entries.is_empty());

    if !to_archive.is_empty() {
        crate::archive::archive_working_memory(to_archive);
        store.save();
    }
}

/// 用精确时间戳更新工作记忆中的 [图片] 为实际图片描述
/// record_timestamps: handle_group_msg 阶段写入工作记忆时的时间戳
pub fn update_image_content(group_id: u64, user_id: u64, image_descriptions: &[String], record_timestamps: &[u64]) {
    if group_id == 0 || image_descriptions.is_empty() || record_timestamps.is_empty() { return; }
    let mut store = WorkingMemoryStore::load();
    let group = match store.groups.get_mut(&group_id.to_string()) {
        Some(g) => g,
        None => return,
    };

    // 用时间戳精确匹配每条工作记忆条目，替换其中的 [图片]
    let mut desc_idx = 0usize;
    for &ts in record_timestamps {
        if desc_idx >= image_descriptions.len() { break; }
        if let Some(entry) = group.entries.iter_mut().find(|e| e.user_id == user_id && e.timestamp == ts) {
            while entry.content.contains("[图片]") && desc_idx < image_descriptions.len() {
                entry.content = entry.content.replacen("[图片]", &format!("[图片: {}]", image_descriptions[desc_idx]), 1);
                desc_idx += 1;
            }
        }
    }
    store.save();
}

/// 返回有工作记忆的群数量 (用于启动日志)
pub fn group_count() -> usize {
    let store = WorkingMemoryStore::load();
    store.groups.len()
}

/// 获取群内活跃参与者列表（不含 bot 自身）
pub fn get_participants(group_id: u64) -> Vec<u64> {
    let store = WorkingMemoryStore::load();
    let now = crate::util::now_secs();
    let self_qq = crate::config::get().self_qq;
    store.groups
        .get(&group_id.to_string())
        .map(|group| {
            let mut users: Vec<u64> = group.entries.iter()
                .filter(|e| now.saturating_sub(e.timestamp) < 7200) // 最近2小时
                .filter(|e| self_qq == 0 || e.user_id != self_qq) // 排除 bot 自身
                .map(|e| e.user_id)
                .collect();
            users.sort_unstable();
            users.dedup();
            users
        })
        .unwrap_or_default()
}
