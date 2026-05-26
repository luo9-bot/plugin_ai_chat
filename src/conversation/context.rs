//! 上下文构建：组装注入到 system prompt 的额外上下文
//!
//! 根据关系深度、注意力状态、电量选择性注入内容。
//! 不是把所有信息平等注入，而是根据当前状态决定注入什么和注入多少。

use crate::{
    circadian, config, emotion, memory, mental_state, person_info, personality,
    schedule, self_memory, social_battery, working_memory, read_shared_state,
};
use super::attention;

/// 向量检索记忆的冷却缓存：每轮对话最多检索一次
static LAST_SEMANTIC_SEARCH: std::sync::Mutex<Option<std::collections::HashMap<u64, u64>>> =
    std::sync::Mutex::new(None);

const SEMANTIC_SEARCH_COOLDOWN: u64 = 120; // 同用户 2 分钟内不重复检索

/// 构建注入到 system prompt 的额外上下文
///
/// 选择性注入原则：
/// - 关系越深，注入越多个性化内容（共享记忆、inside jokes）
/// - 注意力越低，注入越少记忆
/// - 电量越低，注入行为简化提示
pub fn build_context(user_id: u64, group_id: u64, history: &[(String, String)]) -> String {
    let mut parts = Vec::new();
    let cfg = config::get();

    // 获取当前状态用于选择性注入
    let relationship = person_info::relationship::get_relationship(user_id);
    let attention_level = if cfg.humanity.attention_enabled {
        attention::load_attention().attention_level
    } else {
        0.7
    };
    let battery_level_frac = if cfg.humanity.social_battery_enabled {
        social_battery::load().level / cfg.humanity.battery_capacity
    } else {
        0.7
    };

    // ── 基础层：始终注入 ──

    // 对话场景标识：明确告知 AI 当前是私聊还是群聊
    let scene_info = if group_id == 0 {
        format!(
            "# 当前对话场景\n你现在正在和 user_id:{} 进行一对一私聊。\
             这是私人对话，只有你们两个人。\
             不要使用群聊用语（如「有人一起吗」「大家」等），\
             不要@任何人，像朋友单独聊天一样自然交流。",
            user_id
        )
    } else {
        format!(
            "# 当前对话场景\n你现在在群 {} 中和 user_id:{} 对话。\
             群聊中有多个人，你的回复可能被群里所有人看到。\
             注意区分群聊语境，可以适当简短，不需要每次都深入回复。",
            group_id, user_id
        )
    };

    // 当前对话用户标识 (让 AI 知道在和谁说话)
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
    parts.push(format!("{}{}", scene_info, darling_info));

    // 自我记忆 (bot 的内心想法)
    let self_mem = self_memory::get_context(cfg.self_reflection.max_thoughts.min(8));
    if !self_mem.is_empty() {
        parts.push(self_mem);
    }

    // 内心独白上下文
    if cfg.humanity.inner_thought_enabled {
        let thought_ctx = self_memory::inner_thought::get_inner_thought_context(3);
        if !thought_ctx.is_empty() {
            parts.push(thought_ctx);
        }
    }

    // ── 关系层：根据亲密度选择性注入 ──

    // 关系上下文
    let rel_ctx = person_info::relationship::get_relationship_context(user_id);
    if !rel_ctx.is_empty() {
        parts.push(rel_ctx);
    }

    // 亲密度 > 0.3：注入共享记忆
    if relationship.intimacy > 0.3 {
        let shared_ctx = person_info::relationship::get_shared_memories_context(user_id, 3);
        if !shared_ctx.is_empty() {
            parts.push(shared_ctx);
        }
    }

    // 亲密度 > 0.6：注入 inside jokes
    if relationship.intimacy > 0.6 {
        let jokes_ctx = person_info::relationship::get_inside_jokes_context(user_id);
        if !jokes_ctx.is_empty() {
            parts.push(jokes_ctx);
        }
    }

    // 人物档案上下文
    let person_ctx = person_info::get_person_context(user_id);
    if !person_ctx.is_empty() {
        parts.push(person_ctx);
    }

    // ── 记忆层：根据注意力决定注入量 ──

    // 向量检索相关记忆：用最近一条用户消息查询最相关的记忆（2 分钟冷却）
    if let Some((_, last_user_msg)) = history.iter().rev().find(|(role, _)| role == "user") {
        let now = crate::util::now_secs();
        let should_search = {
            let guard = LAST_SEMANTIC_SEARCH.lock().unwrap();
            guard.as_ref().and_then(|m| m.get(&user_id))
                .map_or(true, |&last| now.saturating_sub(last) >= SEMANTIC_SEARCH_COOLDOWN)
        };
        if should_search {
            // 根据注意力调整检索数量
            let memory_count = (attention_level * 8.0) as usize;
            let memory_count = memory_count.max(2); // 至少检索2条
            let relevant = crate::memory::search_memories(user_id, group_id, last_user_msg, memory_count);
            if !relevant.is_empty() {
                let rel_lines: Vec<String> = relevant.iter().map(|r| {
                    format!("- {}", r.content)
                }).collect();
                parts.push(format!("# 相关记忆（与当前对话相关）\n{}", rel_lines.join("\n")));
            }
            let mut guard = LAST_SEMANTIC_SEARCH.lock().unwrap();
            guard.get_or_insert_with(std::collections::HashMap::new).insert(user_id, now);
        }
    }

    // 记忆上下文（全量，但受冷却控制）
    let mem = memory::get_context(user_id, group_id);
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

    // ── 人格层 ──

    let pers = personality::get_prompt_context();
    if !pers.is_empty() {
        parts.push(pers);
    }

    // ── 状态层：情绪 + 注意力 + 电量 + 节律 ──

    // 对话历史摘要（AI 注意力机制压缩的长期记忆）
    let summary = crate::read_shared_state(|s| {
        s.contexts.get(&(group_id, user_id))
            .map(|ctx| ctx.conversation_summary.clone())
            .unwrap_or_default()
    });
    if !summary.is_empty() {
        parts.push(format!("# 对话历史摘要\n以下是对之前对话中重要内容的回顾：\n{}", summary));
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

    // 注意力状态上下文
    if cfg.humanity.attention_enabled {
        let attn_state = attention::load_attention();
        let attn_ctx = attention::get_attention_context(&attn_state);
        if !attn_ctx.is_empty() {
            parts.push(attn_ctx);
        }
    }

    // 社交电量上下文
    if cfg.humanity.social_battery_enabled {
        let battery = social_battery::load();
        let battery_ctx = social_battery::get_battery_context(&battery);
        if !battery_ctx.is_empty() {
            parts.push(battery_ctx);
        }
    }

    // 昼夜节律上下文
    if cfg.humanity.circadian_enabled {
        let rhythm = circadian::calculate();
        let rhythm_ctx = circadian::get_circadian_context(&rhythm);
        if !rhythm_ctx.is_empty() {
            parts.push(rhythm_ctx);
        }
    }

    // 心理状态上下文 (担忧 + 要考量)
    let mental_ctx = mental_state::get_prompt_context(
        cfg.mental_state.concerns_max,
        cfg.mental_state.deliberations_max,
    );
    if !mental_ctx.is_empty() {
        parts.push(mental_ctx);
    }

    // ── 行为暗示层：电量/注意力驱动的行为提示 ──

    // 对话阶段提示
    let interaction_count = history.len();
    if interaction_count > 20 {
        parts.push("- 你们已经聊了很久了，关系很亲近，可以更自然随意".into());
    } else if interaction_count > 10 {
        parts.push("- 你们已经有一定的了解了".into());
    }

    // 电量相关的行为暗示（自然语言，非数值）
    if battery_level_frac < 0.3 {
        parts.push("- 你现在有些疲惫，可以简短回复，不用每条都说很多。如果不想说话可以用表情包代替".into());
    } else if battery_level_frac < 0.15 {
        parts.push("- 你现在非常累，不想回复的话可以简单说「累了」之类的话".into());
    }

    // 注意力相关的行为暗示
    if attention_level < 0.3 {
        parts.push("- 你现在注意力不太集中，不需要进行分析或深入思考，简单回应即可".into());
    }

    // Bot 自己最近的消息 (帮助保持一致性，群聊和私聊都需要)
    let bot_msgs = read_shared_state(|s| {
        s.get_recent_bot_messages(group_id, 600, 5)
    });
    if !bot_msgs.is_empty() {
        let label = if group_id > 0 { format!("在群{}里", group_id) } else { String::new() };
        parts.push(format!("# 你最近说过的消息{}\n{}", label, bot_msgs.join("\n")));
    }

    // 工作记忆 (消息流，含时间戳帮助 AI 区分新旧消息)
    let wm_ctx = working_memory::get_context(group_id, 3600);
    if !wm_ctx.is_empty() {
        parts.push(wm_ctx);
    }

    parts.join("\n\n")
}
