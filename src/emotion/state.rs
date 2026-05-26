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

    #[allow(clippy::should_implement_trait)]
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
    /// 混合情绪（次要情绪，如"开心但有点担忧"）
    #[serde(default)]
    pub secondary: Option<EmotionType>,
    pub intensity: f32,
    /// 情绪惯性——不会瞬间切换，越大越"固执"
    #[serde(default = "default_emotional_inertia")]
    pub inertia: f32,
    /// 情绪触发链——是什么事件导致了当前情绪
    #[serde(default)]
    pub trigger_chain: Vec<EmotionTrigger>,
    /// 长期情绪基线（正值=乐观，负值=悲观）
    #[serde(default)]
    pub baseline: f32,
    /// 从负面情绪恢复的速度 (0.0-1.0)
    #[serde(default = "default_resilience")]
    pub resilience: f32,
    /// 被他人情绪影响的程度 (0.0-1.0)
    #[serde(default = "default_empathy_resonance")]
    pub empathy_resonance: f32,
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

fn default_emotional_inertia() -> f32 { 0.5 }
fn default_resilience() -> f32 { 0.4 }
fn default_empathy_resonance() -> f32 { 0.3 }

/// 情绪触发事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionTrigger {
    /// 触发类型
    pub trigger_type: TriggerType,
    /// 触发源描述
    pub source: String,
    /// 发生时间
    pub timestamp: u64,
    /// 导致的情绪
    pub caused_emotion: EmotionType,
    /// 触发强度
    pub intensity: f32,
}

/// 情绪触发类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TriggerType {
    /// 用户说了什么
    UserMessage,
    /// 自我反思
    SelfReflection,
    /// 记忆唤起
    MemoryRecall,
    /// 环境/时间变化
    Environmental,
    /// 情绪感染（被他人情绪影响）
    EmotionalContagion,
    /// 内心独白
    InnerThought,
}

impl EmotionState {
    /// 情绪动力学更新
    ///
    /// 不是状态机，是动力学系统：
    /// 1. 自然衰减——所有情绪强度随时间向基线回归
    /// 2. 新刺激叠加（不是替换，是混合）
    /// 3. 基线引力——缓慢拉向人格决定的基线情绪
    /// 4. 清理过期触发链
    pub fn update_emotional_dynamics(
        &mut self,
        new_stimulus: Option<(&EmotionType, f32, &str, TriggerType)>,
        delta_secs: f32,
    ) {
        // 1. 自然衰减——向基线回归
        let decay_factor = (-delta_secs / 3600.0 * (1.0 - self.inertia * 0.5)).exp();
        self.intensity = (self.intensity * decay_factor).max(0.05);

        // 当强度过低时回到Neutral
        if self.intensity < 0.1 && self.current != EmotionType::Neutral {
            self.current = EmotionType::Neutral;
            self.secondary = None;
            self.intensity = 0.1;
        }

        // 2. 新刺激叠加（混合而非替换）
        if let Some((emotion, stim_intensity, source, trigger_type)) = new_stimulus {
            // 如果与当前情绪一致，加强
            if *emotion == self.current {
                self.intensity = (self.intensity + stim_intensity * 0.3).min(1.0);
            } else if self.intensity < 0.3 || stim_intensity > 0.5 {
                // 强刺激可以改变情绪，但受惯性阻碍
                let switch_pressure = stim_intensity * (1.0 - self.inertia * 0.6);
                if switch_pressure > self.intensity * 1.2 {
                    // 旧情绪变为次要情绪
                    self.secondary = Some(self.current);
                    self.current = *emotion;
                    self.intensity = stim_intensity.clamp(0.1, 1.0);
                } else {
                    // 不足以改变主情绪，但可能产生混合情绪
                    if self.secondary.is_none()
                        || self.secondary.as_ref() == Some(emotion)
                    {
                        self.secondary = Some(*emotion);
                    }
                    // 微调强度
                    self.intensity = (self.intensity + stim_intensity * 0.1).min(1.0);
                }
            } else {
                // 弱刺激，仅微调
                self.intensity = (self.intensity + stim_intensity * 0.05).min(1.0);
            }

            // 记录触发链
            self.trigger_chain.push(EmotionTrigger {
                trigger_type,
                source: source.to_string(),
                timestamp: crate::util::now_secs(),
                caused_emotion: *emotion,
                intensity: stim_intensity,
            });
        }

        // 3. 基线引力——缓慢拉向基线
        let baseline_pull = self.baseline * delta_secs / 86400.0; // 每天
        if self.baseline > 0.0 {
            // 乐观基线，倾向积极情绪
            if matches!(self.current, EmotionType::Sad | EmotionType::Angry | EmotionType::Worried) {
                // 从负面情绪恢复，受resilience影响
                let recovery = baseline_pull.abs() * (1.0 + self.resilience);
                self.intensity -= recovery;
                if self.intensity < 0.15 {
                    self.current = EmotionType::Neutral;
                    self.secondary = None;
                }
            }
        } else if self.baseline < 0.0 {
            // 悲观基线，更容易陷入负面情绪
            if matches!(self.current, EmotionType::Happy | EmotionType::Excited) {
                self.intensity -= baseline_pull.abs() * 0.5;
            }
        }

        // 4. 清理过期触发链（保留2小时）
        let now = crate::util::now_secs();
        self.trigger_chain.retain(|t| now.saturating_sub(t.timestamp) < 7200);

        self.last_update = crate::util::now_secs();
    }

    /// 情绪感染——被他人的情绪状态影响
    pub fn emotional_contagion(&mut self, other_emotion: &EmotionType, other_intensity: f32) {
        let resonance = self.empathy_resonance * other_intensity * 0.3;
        if resonance > 0.1 {
            self.update_emotional_dynamics(
                Some((other_emotion, resonance, "情绪感染", TriggerType::EmotionalContagion)),
                0.0,
            );
        }
    }

    /// 获取当前情绪的描述
    pub fn describe_detailed(&self) -> String {
        let mut desc = match self.current {
            EmotionType::Neutral => "平静".to_string(),
            EmotionType::Happy => "开心".to_string(),
            EmotionType::Sad => "难过".to_string(),
            EmotionType::Thinking => "沉思".to_string(),
            EmotionType::Surprised => "惊讶".to_string(),
            EmotionType::Angry => "有些不悦".to_string(),
            EmotionType::Shy => "害羞".to_string(),
            EmotionType::Worried => "担忧".to_string(),
            EmotionType::Tired => "疲惫".to_string(),
            EmotionType::Excited => "兴奋".to_string(),
            EmotionType::Like => "心动".to_string(),
        };

        if let Some(secondary) = &self.secondary {
            let sec_desc = match secondary {
                EmotionType::Happy => "开心",
                EmotionType::Sad => "难过",
                EmotionType::Worried => "担忧",
                EmotionType::Excited => "兴奋",
                EmotionType::Thinking => "思考",
                _ => "",
            };
            if !sec_desc.is_empty() {
                desc = format!("{}但有点{}", desc, sec_desc);
            }
        }

        format!("{}(强度:{:.1})", desc, self.intensity)
    }
}

impl Default for EmotionState {
    fn default() -> Self {
        let now = crate::util::now_secs();
        Self {
            current: EmotionType::Neutral,
            secondary: None,
            intensity: 0.3,
            inertia: default_emotional_inertia(),
            trigger_chain: Vec::new(),
            baseline: 0.1, // 轻微乐观
            resilience: default_resilience(),
            empathy_resonance: default_empathy_resonance(),
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

    // 使用情绪动力学更新（替代旧的线性衰减）
    state.update_emotional_dynamics(None, elapsed);

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
