use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::{debug, info};


// 从 crisis 模块重新导出 CrisisLevel，保持向后兼容
pub use crate::crisis::CrisisLevel;

/// Severe 降级到 Mild：需要 2 小时
const CRISIS_SEVERE_COOLDOWN_SECS: u64 = 7200;
/// Mild 降级到 None：需要 1 小时
const CRISIS_MILD_COOLDOWN_SECS: u64 = 3600;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EmotionType {
    Neutral,
    Happy,
    Sad,
    Thinking,
    Surprised,
    Angry,
    Shy,
    Worried,
    Tired,
    Excited,
    Like,    // 喜欢/心动
}

impl EmotionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Happy => "happy",
            Self::Sad => "sad",
            Self::Thinking => "thinking",
            Self::Surprised => "surprised",
            Self::Angry => "angry",
            Self::Shy => "shy",
            Self::Worried => "worried",
            Self::Tired => "tired",
            Self::Excited => "excited",
            Self::Like => "like",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "happy" | "开心" | "高兴" => Self::Happy,
            "sad" | "难过" | "伤心" => Self::Sad,
            "thinking" | "思考" | "沉思" => Self::Thinking,
            "surprised" | "惊讶" | "吃惊" => Self::Surprised,
            "angry" | "生气" | "愤怒" => Self::Angry,
            "shy" | "害羞" | "羞涩" => Self::Shy,
            "worried" | "担心" | "担忧" => Self::Worried,
            "tired" | "疲惫" | "困倦" => Self::Tired,
            "excited" | "兴奋" | "激动" => Self::Excited,
            "like" | "喜欢" | "心动" => Self::Like,
            _ => Self::Neutral,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Neutral => "平静",
            Self::Happy => "开心",
            Self::Sad => "难过",
            Self::Thinking => "沉思",
            Self::Surprised => "惊讶",
            Self::Angry => "有些不悦",
            Self::Shy => "害羞",
            Self::Worried => "担忧",
            Self::Tired => "疲惫",
            Self::Excited => "兴奋",
            Self::Like => "心动",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionState {
    pub current: EmotionType,
    pub intensity: f32,
    pub last_update: u64,
    pub last_interaction: u64,
    pub interaction_rate: f32,
    pub history: Vec<(EmotionType, u64)>,
    /// 最近一次检测到的危机等级
    #[serde(default)]
    pub crisis_level: CrisisLevel,
    /// 上次危机干预时间（用于避免短时间内重复干预）
    #[serde(default)]
    pub last_crisis_intervention: u64,
    /// 连续未检测到危机关键词的消息计数
    #[serde(default)]
    pub crisis_clean_count: u32,
    /// 最近一次实际检测到危机关键词的时间
    #[serde(default)]
    pub last_crisis_detected: u64,
}

impl Default for EmotionState {
    fn default() -> Self {
        let now = crate::util::now_secs();
        Self {
            current: EmotionType::Neutral,
            intensity: 0.3,
            last_update: now,
            last_interaction: now,
            interaction_rate: 0.0,
            history: Vec::new(),
            crisis_level: CrisisLevel::None,
            last_crisis_intervention: 0,
            crisis_clean_count: 0,
            last_crisis_detected: 0,
        }
    }
}

fn emotion_path() -> std::path::PathBuf {
    crate::config::data_dir().join("emotion.json")
}

fn load_states() -> HashMap<String, EmotionState> {
    let path = emotion_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_states(states: &HashMap<String, EmotionState>) {
    let path = emotion_path();
    if let Ok(json) = serde_json::to_string_pretty(states) {
        fs::write(path, json).ok();
    }
}

/// 初始化时调用：返回有情绪状态的用户数量
pub fn user_count() -> usize {
    let states = load_states();
    states.len()
}

pub fn get_state(user_id: u64) -> EmotionState {
    let states = load_states();
    let mut state = states.get(&user_id.to_string()).cloned().unwrap_or_default();
    // 迁移修复：last_crisis_detected == 0 说明是旧数据，crisis_level 不可信
    if state.crisis_level != CrisisLevel::None && state.last_crisis_detected == 0 {
        info!(user_id, from = ?state.crisis_level, "crisis migration: resetting stale crisis_level to None");
        state.crisis_level = CrisisLevel::None;
    }
    state
}

pub fn update_state(user_id: u64, state: EmotionState) {
    let mut states = load_states();
    states.insert(user_id.to_string(), state);
    save_states(&states);
}

pub fn decay(user_id: u64) {
    let cfg = &crate::config::get().emotion;
    let mut state = get_state(user_id);
    let now = crate::util::now_secs();
    let elapsed = now.saturating_sub(state.last_update) as f32;
    if elapsed < cfg.decay_delay_secs as f32 {
        return;
    }
    let decay = cfg.decay_rate * (elapsed / 3600.0);
    state.intensity = (state.intensity - decay).max(0.0);
    if state.intensity < cfg.neutral_threshold && state.current != EmotionType::Neutral {
        state.current = EmotionType::Neutral;
        state.intensity = 0.3;
    }
    state.last_update = now;

    // ── 危机等级时间衰减（长时间无交互时的保底清理） ──
    if state.crisis_level != CrisisLevel::None {
        let time_since_detected = now.saturating_sub(state.last_crisis_detected);
        match state.crisis_level {
            CrisisLevel::Severe if time_since_detected >= CRISIS_SEVERE_COOLDOWN_SECS * 2 => {
                info!(user_id, "crisis decay: Severe -> Mild (timeout)");
                state.crisis_level = CrisisLevel::Mild;
                state.last_crisis_detected = now;
                state.crisis_clean_count = 0;
            }
            CrisisLevel::Mild if time_since_detected >= CRISIS_MILD_COOLDOWN_SECS * 3 => {
                info!(user_id, "crisis decay: Mild -> None (timeout)");
                state.crisis_level = CrisisLevel::None;
                state.crisis_clean_count = 0;
            }
            _ => {}
        }
    }

    update_state(user_id, state);
}

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

    let (detected, delta) = detect_emotion(message);
    if delta > 0.1 {
        if detected != state.current {
            state.history.push((state.current, now));
            if state.history.len() > 5 {
                state.history.remove(0);
            }
        }
        state.current = detected;
        state.intensity = (state.intensity + delta).min(1.0);
    }

    if state.interaction_rate > crate::config::get().emotion.affinity_threshold {
        state.intensity = (state.intensity + 0.02).min(1.0);
        if state.current == EmotionType::Neutral {
            state.current = EmotionType::Happy;
        }
    }

    state.last_update = now;
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
            // 尝试从回复中提取 JSON
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
                let now = crate::util::now_secs();

                if detected != state.current {
                    state.history.push((state.current, now));
                    if state.history.len() > 5 {
                        state.history.remove(0);
                    }
                }
                state.current = detected;
                state.intensity = intensity.clamp(0.0, 1.0);
                state.last_update = now;
                update_state(user_id, state);
            }
        }
        Err(e) => {
            debug!(error = %e, "emotion AI analysis failed, falling back to keyword");
            // fallback: 关键词分析
            let (detected, delta) = detect_emotion(user_message);
            if delta > 0.1 {
                let mut state = get_state(user_id);
                let now = crate::util::now_secs();
                if detected != state.current {
                    state.history.push((state.current, now));
                    if state.history.len() > 5 {
                        state.history.remove(0);
                    }
                }
                state.current = detected;
                state.intensity = (state.intensity + delta).min(1.0);
                state.last_update = now;
                update_state(user_id, state);
            }
        }
    }
}

/// 从后处理分析结果更新情绪状态
pub fn update_from_analysis(user_id: u64, emotion_str: &str, intensity: f32) {
    let detected = EmotionType::from_str(emotion_str);
    let mut state = get_state(user_id);
    let now = crate::util::now_secs();

    if detected != state.current {
        debug!(user_id, from = ?state.current, to = ?detected, intensity, "emotion: state changed");
        state.history.push((state.current, now));
        if state.history.len() > 5 {
            state.history.remove(0);
        }
    }
    state.current = detected;
    state.intensity = intensity.clamp(0.0, 1.0);
    state.last_update = now;
    update_state(user_id, state);
}

pub fn parse_from_reply(user_id: u64, reply: &str) -> String {
    let mut state = get_state(user_id);
    let now = crate::util::now_secs();
    let mut cleaned = reply.to_string();

    let markers = ["[emotion:", "[Emotion:", "[EMOTION:"];
    for marker in &markers {
        if let Some(start) = cleaned.find(marker) {
            let tag_start = start + marker.len();
            if let Some(end) = cleaned[tag_start..].find(']') {
                let emotion_str = cleaned[tag_start..tag_start + end].trim();
                let detected = EmotionType::from_str(emotion_str);
                state.history.push((state.current, now));
                if state.history.len() > 5 {
                    state.history.remove(0);
                }
                state.current = detected;
                state.intensity = (state.intensity + 0.2).min(1.0);
                state.last_update = now;
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
            if message.contains(kw) {
                if *delta > best_delta {
                    best_delta = *delta;
                    best_emotion = *emotion;
                }
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

// 危机检测、状态更新、干预指令已移至 crisis 模块
// 通过 emotion::detect_crisis 等重新导出保持向后兼容
pub use crate::crisis::{detect_crisis, detect_crisis_ai, update_crisis, get_crisis_context};



pub fn get_prompt_context(user_id: u64) -> String {
    let state = get_state(user_id);
    let mut lines = Vec::new();
    lines.push("# 情绪状态指令".to_string());

    match state.current {
        EmotionType::Neutral => {
            lines.push("- 你当前心情平静，以自然放松的状态对话".into());
        }
        EmotionType::Happy => {
            lines.push("- 你当前心情不错，语气会更轻快积极，可能会多用一些语气词".into());
        }
        EmotionType::Sad => {
            lines.push("- 你当前有些低落，回复可能会更简短，语气稍显沉静".into());
        }
        EmotionType::Thinking => {
            lines.push("- 你当前处于思考状态，回复可能带有沉吟、停顿的感觉".into());
        }
        EmotionType::Surprised => {
            lines.push("- 你当前有些惊讶，可能会用感叹或追问来表达".into());
        }
        EmotionType::Angry => {
            lines.push("- 你当前有些不悦，但不会对用户恶语相向，而是表现为沉默或转移话题".into());
        }
        EmotionType::Shy => {
            lines.push("- 你当前有些害羞，可能会说话吞吞吐吐或用省略号".into());
        }
        EmotionType::Worried => {
            lines.push("- 你当前有些担忧，可能会更关心对方的状况".into());
        }
        EmotionType::Tired => {
            lines.push("- 你当前有些疲惫，回复可能更简短，偶尔带出困倦感".into());
        }
        EmotionType::Excited => {
            lines.push("- 你当前很兴奋，语气会更活泼热情，可能会用更多感叹号".into());
        }
        EmotionType::Like => {
            lines.push("- 你当前对这个人有好感，语气会更温柔，可能会更主动关心".into());
        }
    }

    if state.interaction_rate > 5.0 {
        lines.push("- 你们最近聊得很频繁，关系更亲近了，可以更随意一些".into());
    }

    lines.join("\n")
}

pub fn describe(user_id: u64) -> String {
    let state = get_state(user_id);
    format!("{}({:.1})", state.current.description(), state.intensity)
}

// ── 测试 ─────────────────────────────────────────────────────────
// 测试代码已移至 tests/emotion_test.rs
