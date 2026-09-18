//! 人物档案系统

pub mod relationship;
mod store;

pub use store::*;

use tracing::{debug, info};

pub(crate) fn register_person(user_id: u64) {
    if user_id == 0 {
        return;
    }
    let now = crate::util::now_secs();
    let mut is_new = false;
    update_profile(user_id, |p| {
        if p.know_since == 0 {
            is_new = true;
            p.know_since = now;
        }
        p.know_times += 1;
        p.last_know = now;
    });
    if is_new {
        info!(user_id, "person_info: new person");
    }
}

pub(crate) fn add_memory_point(user_id: u64, point: &str) {
    let mut added = false;
    update_profile(user_id, |p| {
        if p.memory_points.iter().any(|existing| existing == point) {
            return;
        }
        p.memory_points.push(point.into());
        if p.memory_points.len() > 20 {
            p.memory_points.remove(0);
        }
        added = true;
    });
    if added {
        debug!(user_id, point, "person_info: memory point added");
    }
}

/// 获取用户显示名称（用于 AI 上下文，避免暴露原始 user_id）
///
/// 人物档案（含创作者播种的名字，见 mind::persons）优先——那是她认人的依据；
/// 其次自动学到的 person_name，再次群昵称。
pub(crate) fn get_display_name(user_id: u64, group_id: u64) -> Option<String> {
    if let Some(name) = crate::mind::persons::display_name_or_address(user_id) {
        return Some(name);
    }
    let profile = load_profile(user_id)?;
    if !profile.person_name.is_empty() {
        return Some(profile.person_name);
    }
    if group_id > 0 {
        return profile.group_nicknames.get(&group_id).cloned();
    }
    None
}

/// 从对话中自动提取用户事实并存储
///
/// 使用 LLM 从用户消息和 bot 回复中提取稳定事实。
pub(crate) fn extract_facts_from_conversation(user_id: u64, user_message: &str, bot_reply: &str) {
    if user_id == 0 || user_message.is_empty() {
        return;
    }

    let prompt = format!(
        "从以下对话中提取关于用户的稳定事实（如姓名、兴趣、职业、习惯等）。\n\
         只提取有明确证据的事实，不要推测。\n\n\
         用户消息：{}\n\
         Bot回复：{}\n\n\
         如果没有可提取的事实，返回空。否则返回一行一个事实。",
        user_message, bot_reply
    );

    // 无意义回复的过滤模式
    let meaningless_patterns = [
        "无明确事实",
        "没有可提取",
        "无法提取",
        "没有事实",
        "暂无",
        "无相关信息",
        "无法确定",
        "没有足够",
        "无法判断",
        "无明确",
        "（无",
        "(无",
    ];

    if let Ok(response) = crate::ai::analyze("", &prompt) {
        for line in response.lines() {
            let fact = line.trim();
            if fact.is_empty() || fact.len() <= 2 || fact.len() >= 100 {
                continue;
            }
            // 过滤无意义回复
            if meaningless_patterns.iter().any(|p| fact.contains(p)) {
                continue;
            }
            add_memory_point(user_id, fact);
        }
    }
}
