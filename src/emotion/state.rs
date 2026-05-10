use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::info;

// 从 crisis 模块重新导出 CrisisLevel，保持向后兼容
pub use crate::crisis::CrisisLevel;

/// Severe 降级到 Mild：需要 2 小时
pub(crate) const CRISIS_SEVERE_COOLDOWN_SECS: u64 = 7200;
/// Mild 降级到 None：需要 1 小时
pub(crate) const CRISIS_MILD_COOLDOWN_SECS: u64 = 3600;

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

pub fn describe(user_id: u64) -> String {
    let state = get_state(user_id);
    format!("{}({:.1})", state.current.description(), state.intensity)
}
