//! 对话结束检测
//!
//! 两阶段检测：
//! 1. 关键词预筛选（快速，无 AI 调用）
//! 2. Tool 判断（AI 调用，综合上下文）


/// 告别词
const FAREWELL_PATTERNS: &[&str] = &[
    "晚安", "好了", "先这样", "拜拜", "下次聊", "去忙吧", "去睡了",
    "困了", "要睡了", "先睡了", "休息了", "挂了", "走了",
];

/// 简短确认词
const SHORT_CONFIRM: &[&str] = &[
    "好", "嗯", "行", "好的", "嗯嗯", "知道了", "ok", "嗯好",
    "好嘞", "好哒", "行吧", "好吧", "知道了知道了",
];

/// 关键词预筛选：快速判断对话可能已结束
///
/// 返回 true 表示需要进一步 AI 判断
pub fn keyword_screen(
    bot_last_message: &str,
    user_message: &str,
    current_hour: u32,
) -> bool {
    let trimmed = user_message.trim();

    // 检查是否是简短确认
    let is_short_confirm = SHORT_CONFIRM.iter().any(|&p| trimmed == p);
    if !is_short_confirm {
        return false;
    }

    // 检查 bot 是否说了告别词
    let bot_said_farewell = FAREWELL_PATTERNS.iter().any(|p| bot_last_message.contains(p));

    // 深夜时间
    let is_late_night = current_hour >= 23 || current_hour < 6;

    // 条件：bot 说了告别 + 用户确认，或者深夜 + 用户确认
    bot_said_farewell || is_late_night
}

/// 获取对话结束检测的 prompt 上下文
///
/// 注入到 Planner 中，让 AI 综合判断
pub fn get_context(bot_last_message: &str, user_message: &str) -> String {
    format!(
        "# 对话结束检测\n\
         Bot 上一条消息：{}\n\
         用户当前消息：{}\n\n\
         请判断对话是否自然结束。如果用户只是简短确认了你的告别/总结，\
         调用 finish 工具表示不回复。如果对话还在继续，调用 reply 工具。",
        bot_last_message, user_message
    )
}

/// 检查用户消息是否是简短确认（用于其他模块）
pub fn is_short_confirm(message: &str) -> bool {
    let trimmed = message.trim();
    SHORT_CONFIRM.iter().any(|&p| trimmed == p)
}
