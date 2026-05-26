use tracing::{debug, info};

use super::state::{EmotionType, TriggerType, get_state, update_state};

// 危机检测、状态更新、干预指令已移至 crisis 模块
// 通过 emotion::detect_crisis 等重新导出保持向后兼容
pub use crate::crisis::{detect_crisis, detect_crisis_ai, update_crisis, get_crisis_context};

pub fn analyze_user_message(user_id: u64, message: &str) -> bool {
    info!(user_id, message = %message.chars().take(30).collect::<String>(), "emotion: 分析用户消息");
    let mut state = get_state(user_id);
    let now = crate::util::now_secs();

    let time_since_last = now.saturating_sub(state.last_interaction) as f32;
    state.last_interaction = now;
    if time_since_last > 0.0 && time_since_last < 3600.0 {
        let rate = 3600.0 / time_since_last;
        state.interaction_rate = state.interaction_rate * 0.7 + rate * 0.3;
    }

    // 关键词检测情绪
    let (detected, delta) = detect_emotion(message);
    if delta > 0.1 {
        // 截断消息作为source
        let source: String = message.chars().take(30).collect();
        state.update_emotional_dynamics(
            Some((&detected, delta, &source, TriggerType::UserMessage)),
            0.0,
        );
    }

    // 高频互动带来正向情绪
    if state.interaction_rate > crate::config::get().emotion.affinity_threshold {
        state.update_emotional_dynamics(
            Some((&EmotionType::Happy, 0.02, "高频互动", TriggerType::UserMessage)),
            0.0,
        );
    }

    update_state(user_id, state);

    // 危机信号检测
    let crisis = detect_crisis(message);
    update_crisis(user_id, crisis)
}

/// AI 驱动的情绪分析 (发送回复后调用，用于更精准的情绪状态更新)
pub fn ai_analyze(user_id: u64, user_message: &str, ai_reply: &str) {
    let content = format!("用户消息: {}\nAI回复: {}", user_message, ai_reply);

    let result = crate::ai::analyze(crate::prompt::PromptManager::get().raw("emotion_analyze"), &content);
    match result {
        Ok(raw) => {
            let json_str = if let Some(start) = raw.find('{') {
                if let Some(end) = raw[start..].find('}') {
                    &raw[start..start + end + 1]
                } else {
                    &raw
                }
            } else {
                &raw
            };

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                let emotion_str = parsed.get("emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral");
                let intensity = parsed.get("intensity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3) as f32;

                let detected = EmotionType::from_str(emotion_str);
                let mut state = get_state(user_id);

                // 使用情绪动力学更新替代直接替换
                state.update_emotional_dynamics(
                    Some((&detected, intensity, "AI情绪分析", TriggerType::SelfReflection)),
                    0.0,
                );
                update_state(user_id, state);
            }
        }
        Err(e) => {
            debug!(error = %e, "emotion AI analysis failed, falling back to keyword");
            let (detected, delta) = detect_emotion(user_message);
            if delta > 0.1 {
                let mut state = get_state(user_id);
                let source: String = user_message.chars().take(30).collect();
                state.update_emotional_dynamics(
                    Some((&detected, delta, &source, TriggerType::UserMessage)),
                    0.0,
                );
                update_state(user_id, state);
            }
        }
    }
}

/// 从后处理分析结果更新情绪状态
pub fn update_from_analysis(user_id: u64, emotion_str: &str, intensity: f32) {
    let detected = EmotionType::from_str(emotion_str);
    let mut state = get_state(user_id);

    debug!(user_id, from = ?state.current, to = ?detected, intensity, "emotion: state changed");

    // 使用情绪动力学更新替代直接替换
    state.update_emotional_dynamics(
        Some((&detected, intensity, "对话反思", TriggerType::SelfReflection)),
        0.0,
    );
    update_state(user_id, state);
}

pub fn parse_from_reply(user_id: u64, reply: &str) -> String {
    let mut state = get_state(user_id);
    let mut cleaned = reply.to_string();

    let markers = ["[emotion:", "[Emotion:", "[EMOTION:"];
    for marker in &markers {
        if let Some(start) = cleaned.find(marker) {
            let tag_start = start + marker.len();
            if let Some(end) = cleaned[tag_start..].find(']') {
                let emotion_str = cleaned[tag_start..tag_start + end].trim();
                let detected = EmotionType::from_str(emotion_str);

                // 使用情绪动力学更新
                state.update_emotional_dynamics(
                    Some((&detected, 0.2, "AI自我报告情绪", TriggerType::SelfReflection)),
                    0.0,
                );
                update_state(user_id, state);

                let full_end = tag_start + end + 1;
                let mut remove_start = start;
                if remove_start > 0 && cleaned.as_bytes().get(remove_start - 1) == Some(&b' ') {
                    remove_start -= 1;
                }
                cleaned.replace_range(remove_start..full_end, "");
                break;
            }
        }
    }
    cleaned
}

fn detect_emotion(message: &str) -> (EmotionType, f32) {
    let pairs: &[(&[&str], EmotionType, f32)] = &[
        (&["哈哈", "嘻嘻", "开心", "高兴", "太好了", "棒", "赞", "爱", "喜欢", "嘿嘿", "哇", "感动", "幸福", "谢谢", "感谢"], EmotionType::Happy, 0.3),
        (&["兴奋", "激动", "太棒了", "爽", "绝了", "666", "厉害"], EmotionType::Excited, 0.4),
        (&["难过", "伤心", "哭", "呜呜", "唉", "可惜", "遗憾", "失望", "孤独", "寂寞"], EmotionType::Sad, 0.3),
        (&["生气", "愤怒", "烦", "讨厌", "气死", "恼火", "受够了", "滚"], EmotionType::Angry, 0.4),
        (&["担心", "焦虑", "紧张", "害怕", "恐惧", "不安", "慌"], EmotionType::Worried, 0.3),
        (&["嗯", "哦", "这样", "好吧", "知道了", "了解"], EmotionType::Neutral, 0.1),
        (&["想", "思考", "为什么", "怎么", "如何", "吗", "呢", "？"], EmotionType::Thinking, 0.15),
        (&["惊", "啊", "天", "不会吧", "真的吗", "居然", "竟然", "没想到"], EmotionType::Surprised, 0.25),
        (&["害羞", "脸红", "不好意思", "讨厌啦", "才不是", "哼"], EmotionType::Shy, 0.3),
        (&["累", "困", "疲", "想睡", "没精神", "懒", "不想动"], EmotionType::Tired, 0.25),
    ];

    let mut best_emotion = EmotionType::Neutral;
    let mut best_delta = 0.0f32;

    for (keywords, emotion, delta) in pairs {
        for kw in *keywords {
            if message.contains(kw)
                && *delta > best_delta {
                    best_delta = *delta;
                    best_emotion = *emotion;
                }
        }
    }

    let exclaim_count = message.matches('!').count() + message.matches('！').count();
    if exclaim_count >= 2 && best_delta < 0.2 {
        best_delta = 0.2;
        best_emotion = EmotionType::Excited;
    }

    (best_emotion, best_delta)
}
