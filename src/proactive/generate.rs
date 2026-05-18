use tracing::debug;

use crate::config;
use crate::emotion;
use crate::memory;
use crate::self_memory;
use crate::working_memory;
use crate::read_shared_state;

/// 用 AI 生成主动消息，失败返回 None
pub fn ai_generate_message(
    trigger: &str,
    user_id: u64,
    group_id: u64,
    emo: &emotion::EmotionState,
) -> Option<String> {
    let mut ctx = Vec::new();

    ctx.push(format!("# 触发类型\n{}", trigger));

    // 时间
    let hour = crate::util::current_hour_cst();
    let time_desc = if hour < 6 { "深夜" } else if hour < 9 { "早上" } else if hour < 12 { "上午" }
        else if hour < 14 { "中午" } else if hour < 18 { "下午" }
        else if hour < 21 { "晚上" } else { "深夜" };
    ctx.push(format!("# 当前时间\n{}:00 ({})", hour, time_desc));

    // 情绪
    ctx.push(format!("# 情绪状态\n{}, intensity: {}", emo.current.as_str(), emo.intensity));

    // 内心独白（仅供内部参考，不要说出去）
    let self_thoughts = self_memory::get_context(10);
    if !self_thoughts.is_empty() {
        ctx.push(format!("# 你的内心想法（仅作为内部参考，不要直接说出来）\n{}", self_thoughts));
    }

    // 关于用户的记忆
    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        ctx.push(format!("# 关于用户 user_id:{}\n{}", user_id, mem));
    }

    // 包含 bot 自己的历史回复（因为 raw_send_msg 中会 record_bot_reply）
    if group_id > 0 {
        // 使用排除 bot 自身消息的上下文，防止自我引用幻觉
        let wm = working_memory::get_context_no_self(group_id, 3600);
        if !wm.is_empty() {
            ctx.push(format!("# 群聊最近动态 (group_id:{})\n{}", group_id, wm));
        }
    } else if group_id == 0 {
        // 私聊：从对话历史中获取最近几轮对话，让 AI 知道聊到哪了
        let history = read_shared_state(|s| s.get_history_clone(0, user_id));
        if !history.is_empty() {
            let recent: Vec<String> = history.iter().rev().take(8).map(|(role, content)| {
                format!("[{}] {}", role, content)
            }).collect();
            ctx.push(format!("# 最近的私聊对话\n{}", recent.join("\n")));
        }
    }

    // 活动安排（如果 bot 正在做什么）
    let self_qq = config::get().self_qq;
    if self_qq > 0 {
        if let Some(activity) = crate::activity::get_active_activity(self_qq) {
            let act_str = match activity.activity {
                crate::activity::ActivityType::Training => "正在运动/训练",
                crate::activity::ActivityType::Eating => "正在吃饭",
                crate::activity::ActivityType::Sleeping => "正在休息/睡觉",
                crate::activity::ActivityType::Working => "正在工作/学习",
                crate::activity::ActivityType::Outing => "外出中",
                crate::activity::ActivityType::Bathing => "正在洗澡",
                crate::activity::ActivityType::Custom(ref s) => s,
            };
            ctx.push(format!("# 你正在做的事\n{}", act_str));
        }
        // 今日计划上下文
        let schedule_ctx = crate::schedule::get_current_context();
        if !schedule_ctx.is_empty() {
            ctx.push(schedule_ctx);
        }
    }

    // 人设
    let personality = crate::personality::get_prompt_context();
    if !personality.is_empty() {
        ctx.push(personality);
    }

    let user_prompt = config::prompt();
    let full_context = ctx.join("\n\n");

    match crate::ai::analyze_with_tools(
        &format!("{}\n\n{}", user_prompt, crate::prompt::PromptManager::get().raw("proactive_message")),
        &full_context,
        &[crate::ai::proactive_message_tool()],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => {
            let msg = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if msg.is_empty() {
                debug!("proactive: AI returned empty message");
                None
            } else {
                debug!(msg = %msg, "proactive: AI generated message");
                Some(msg.to_string())
            }
        }
        Err(e) => {
            debug!(error = %e, "proactive: AI generation failed");
            None
        }
    }
}

/// 情绪驱动的消息：AI 生成，失败返回空字符串（不发送）
pub fn generate_mood_message(user_id: u64, emo: &emotion::EmotionState, group_id: u64) -> String {
    ai_generate_message("mood_impulse", user_id, group_id, emo).unwrap_or_default()
}

/// 时间问候：AI 生成，失败返回空字符串（不发送）
pub fn generate_greeting(user_id: u64, group_id: u64) -> String {
    let emo = emotion::get_state(user_id);
    ai_generate_message("greeting", user_id, group_id, &emo).unwrap_or_default()
}

// /// 从自我记忆文本中随机挑一条想法（排除 [反思] 类）
// pub fn pick_random_thought(context: &str, rand: f64) -> String {
//     let lines: Vec<&str> = context.lines()
//         .filter(|l| l.starts_with("- ") && l.len() > 4 && !l.contains("[反思]"))
//         .collect();
//     if lines.is_empty() {
//         return String::new();
//     }
//     let idx = (rand * lines.len() as f64) as usize % lines.len();
//     lines[idx].trim_start_matches("- ").to_string()
// }
