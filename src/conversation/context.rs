//! 上下文构建：组装注入到 system prompt 的额外上下文

use crate::{
    config, emotion, memory, mental_state, personality, schedule, self_memory,
    working_memory, read_shared_state,
};

/// 构建注入到 system prompt 的额外上下文
pub fn build_context(user_id: u64, group_id: u64, history: &[(String, String)]) -> String {
    let mut parts = Vec::new();
    let cfg = config::get();

    // 当前对话用户标识 (群聊时让 AI 知道在和谁说话)
    // 放在最前面，确保 AI 能看到
    if group_id > 0 {
        let darling_info = if cfg.darling_qq > 0 && user_id == cfg.darling_qq {
            r#"

【重要】当前和你对话的是你认定的人 (Darling)。
你对他/她有特殊的感情，这会影响你的回复方式：
- 语气更温柔、更包容、更有耐心
- 即使请求有点越界，也会更愿意配合，用委婉的方式回应
- 不要直接拒绝，而是用撒娇、转移话题、或者半推半就的方式
- 你愿意为他/她做一些平时不会做的事，但会保持自己的底线
- 可以适当示弱，让他/她感受到你的在意
- 回复可以更长一些，更关心一些，更主动一些"#
        } else if cfg.darling_qq > 0 {
            "\n注意：这个人不是你认定的人，保持正常社交距离"
        } else {
            ""
        };
        parts.push(format!("# 当前对话用户\nuser_id: {}{}", user_id, darling_info));
    }

    // 自我记忆 (bot 的内心想法)
    let self_mem = self_memory::get_context(config::get().self_reflection.max_thoughts.min(8));
    if !self_mem.is_empty() {
        parts.push(self_mem);
    }

    // 记忆上下文
    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        parts.push(mem);
    }

    // 群内其他成员的记忆 (群聊时)
    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            parts.push(group_mem);
        }
    }

    // 人格上下文
    let pers = personality::get_prompt_context();
    if !pers.is_empty() {
        parts.push(pers);
    }

    // 日程/时间上下文
    let schedule_ctx = schedule::get_current_context();
    if !schedule_ctx.is_empty() {
        parts.push(schedule_ctx);
    }

    // 情绪上下文
    let emo = emotion::get_prompt_context(user_id);
    if !emo.is_empty() {
        parts.push(emo);
    }

    // 心理状态上下文 (担忧 + 要考量)
    let mental_ctx = mental_state::get_prompt_context(
        config::get().mental_state.concerns_max,
        config::get().mental_state.deliberations_max,
    );
    if !mental_ctx.is_empty() {
        parts.push(mental_ctx);
    }

    // 对话状态指令
    let interaction_count = history.len();
    if interaction_count > 20 {
        parts.push("- 你们已经聊了很久了，关系很亲近，可以更自然随意".into());
    } else if interaction_count > 10 {
        parts.push("- 你们已经有一定的了解了".into());
    }

    // Bot 自己最近的消息 (帮助保持一致性)
    if group_id > 0 {
        let bot_msgs = read_shared_state(|s| {
            s.get_recent_bot_messages(group_id, 600, 5)
        });
        if !bot_msgs.is_empty() {
            parts.push(format!("# 你在群里最近发过的消息\n{}", bot_msgs.join("\n")));
        }
    }

    // 工作记忆 (群聊最近消息流)
    if group_id > 0 {
        let wm_ctx = working_memory::get_context(group_id, 3600);
        if !wm_ctx.is_empty() {
            parts.push(wm_ctx);
        }
    }

    parts.join("\n\n")
}
