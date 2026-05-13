use tracing::debug;

use crate::config;
use crate::emotion;
use crate::memory;
use crate::self_memory;
use crate::working_memory;

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

    // 主动消息的自我记忆（更长时间窗口）
    let self_thoughts = self_memory::get_context(10);
    if !self_thoughts.is_empty() {
        ctx.push(self_thoughts);
    }

    // 关于用户的记忆
    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        ctx.push(format!("# 关于用户 user_id:{}\n{}", user_id, mem));
    }

    // 包含 bot 自己的历史回复（因为 raw_send_msg 中会 record_bot_reply）
    if group_id > 0 {
        let wm = working_memory::get_context(group_id, 7200);
        if !wm.is_empty() {
            ctx.push(format!("# 群聊最近动态 (group_id:{})\n{}", group_id, wm));
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
