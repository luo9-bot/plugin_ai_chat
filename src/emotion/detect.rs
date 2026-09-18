use tracing::info;

use super::state::{EmotionType, TriggerType, get_state, update_state};
use crate::crisis::{detect_crisis, update_crisis};

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
            Some((
                &EmotionType::Happy,
                0.02,
                "高频互动",
                TriggerType::UserMessage,
            )),
            0.0,
        );
    }

    update_state(user_id, state);

    // 危机信号检测
    let crisis = detect_crisis(message);
    update_crisis(user_id, crisis)
}

fn detect_emotion(message: &str) -> (EmotionType, f32) {
    let pairs: &[(&[&str], EmotionType, f32)] = &[
        (
            &[
                "哈哈",
                "嘻嘻",
                "开心",
                "高兴",
                "太好了",
                "棒",
                "赞",
                "爱",
                "喜欢",
                "嘿嘿",
                "哇",
                "感动",
                "幸福",
                "谢谢",
                "感谢",
            ],
            EmotionType::Happy,
            0.3,
        ),
        (
            &["兴奋", "激动", "太棒了", "爽", "绝了", "666", "厉害"],
            EmotionType::Excited,
            0.4,
        ),
        (
            &[
                "难过", "伤心", "哭", "呜呜", "唉", "可惜", "遗憾", "失望", "孤独", "寂寞",
            ],
            EmotionType::Sad,
            0.3,
        ),
        (
            &["生气", "愤怒", "烦", "讨厌", "气死", "恼火", "受够了", "滚"],
            EmotionType::Angry,
            0.4,
        ),
        (
            &["担心", "焦虑", "紧张", "害怕", "恐惧", "不安", "慌"],
            EmotionType::Worried,
            0.3,
        ),
        (
            &["嗯", "哦", "这样", "好吧", "知道了", "了解"],
            EmotionType::Neutral,
            0.1,
        ),
        (
            &["想", "思考", "为什么", "怎么", "如何", "吗", "呢", "？"],
            EmotionType::Thinking,
            0.15,
        ),
        (
            &[
                "惊",
                "啊",
                "天",
                "不会吧",
                "真的吗",
                "居然",
                "竟然",
                "没想到",
            ],
            EmotionType::Surprised,
            0.25,
        ),
        (
            &["害羞", "脸红", "不好意思", "讨厌啦", "才不是", "哼"],
            EmotionType::Shy,
            0.3,
        ),
        (
            &["累", "困", "疲", "想睡", "没精神", "懒", "不想动"],
            EmotionType::Tired,
            0.25,
        ),
    ];

    let mut best_emotion = EmotionType::Neutral;
    let mut best_delta = 0.0f32;

    for (keywords, emotion, delta) in pairs {
        for kw in *keywords {
            if message.contains(kw) && *delta > best_delta {
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
