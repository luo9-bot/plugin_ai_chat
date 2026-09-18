use serde::{Deserialize, Serialize};
use tracing::info;

use crate::crisis::CrisisLevel;

/// Severe 降级到 Mild：需要 2 小时
pub(crate) const CRISIS_SEVERE_COOLDOWN_SECS: u64 = 7200;
/// Mild 降级到 None：需要 1 小时
pub(crate) const CRISIS_MILD_COOLDOWN_SECS: u64 = 3600;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub(crate) enum EmotionType {
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
    Like, // 喜欢/心动
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EmotionState {
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

fn default_emotional_inertia() -> f32 {
    0.5
}
fn default_resilience() -> f32 {
    0.4
}
fn default_empathy_resonance() -> f32 {
    0.3
}

/// 情绪触发事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EmotionTrigger {
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
pub(crate) enum TriggerType {
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
    pub(crate) fn update_emotional_dynamics(
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
                    if self.secondary.is_none() || self.secondary.as_ref() == Some(emotion) {
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
            if matches!(
                self.current,
                EmotionType::Sad | EmotionType::Angry | EmotionType::Worried
            ) {
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
        self.trigger_chain
            .retain(|t| now.saturating_sub(t.timestamp) < 7200);

        self.last_update = crate::util::now_secs();
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

/// 读一个用户的情绪状态
///
/// 状态库按 user_id 主键存**一行一个用户**。此前这里是"读整个
/// `emotion.json` 再取其中一个键"：`get_state` 在每条消息的路径上被调用，
/// 而 `update_state` 更是"全量读 → 改一个键 → 全量写"——
/// 一个用户的情绪波动会重写所有用户的记录。
pub(crate) fn get_state(user_id: u64) -> EmotionState {
    let stored = crate::db::db().emotion_state(user_id);
    match stored {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        Ok(None) => EmotionState::default(),
        Err(error) => {
            tracing::warn!(%error, user_id, "emotion: 读取状态失败，按默认情绪处理");
            EmotionState::default()
        }
    }
}

/// 写一个用户的情绪状态（单行 UPSERT）
pub(crate) fn update_state(user_id: u64, state: EmotionState) {
    let json = match serde_json::to_string(&state) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(%error, user_id, "emotion: 序列化失败，状态未写入");
            return;
        }
    };
    if let Err(error) = crate::db::db().set_emotion_state(user_id, &json) {
        tracing::warn!(%error, user_id, "emotion: 状态写库失败");
    }
}

/// 有情绪状态记录的用户数量
pub(crate) fn user_count() -> usize {
    crate::db::db().emotion_user_count().unwrap_or(0)
}

/// 单个用户状态的时间推进（纯计算，不碰磁盘）
///
/// 返回 `false` 表示还没到衰减延迟、状态未变。
fn advance_decay(state: &mut EmotionState, user_id: u64, now: u64, decay_delay_secs: u64) -> bool {
    let elapsed = now.saturating_sub(state.last_update) as f32;
    if elapsed < decay_delay_secs as f32 {
        return false;
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
    true
}

/// 批量推进多个用户的情绪衰减
///
/// 周期检查需要推进**所有已知用户**，而 `emotion.json` 是"每用户一个条目的
/// 单一文件"。逐用户调用 [`decay`] 会让 1ms 主循环上的文件操作数量变成 3N
/// （每用户一次全量读 + 一次全量读改写）。这里一次载入、一次落盘。
pub(crate) fn decay_many(user_ids: &[u64]) {
    if user_ids.is_empty() {
        return;
    }
    let decay_delay_secs = crate::config::get().emotion.decay_delay_secs;
    let now = crate::util::now_secs();
    let db = crate::db::db();

    // 逐用户读他**那一行**，只把真正变化的收集起来一次写入：
    // 此前这里是"整份 emotion.json 读进来 → 全量写回去"。
    let mut changed: Vec<(u64, String)> = Vec::new();
    for user_id in user_ids {
        let mut state = match db.emotion_state(*user_id) {
            Ok(Some(json)) => match serde_json::from_str::<EmotionState>(&json) {
                Ok(state) => state,
                Err(error) => {
                    tracing::warn!(%error, user_id, "emotion: 状态解析失败，跳过衰减");
                    continue;
                }
            },
            // 没有状态记录就不必凭空造一个：默认状态本来也没有衰减可言
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(%error, user_id, "emotion: 读取状态失败，跳过衰减");
                continue;
            }
        };
        if !advance_decay(&mut state, *user_id, now, decay_delay_secs) {
            continue;
        }
        match serde_json::to_string(&state) {
            Ok(json) => changed.push((*user_id, json)),
            Err(error) => tracing::warn!(%error, user_id, "emotion: 序列化失败，跳过写入"),
        }
    }

    if let Err(error) = db.set_emotion_states(&changed) {
        tracing::warn!(%error, count = changed.len(), "emotion: 批量写库失败");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未到衰减延迟时不推进——批量推进必须保留这条门（否则所有状态
    /// 每轮维护都会被无意义地改写一次）
    #[test]
    fn advance_decay_respects_the_delay_gate() {
        let mut state = EmotionState {
            last_update: 1_000,
            ..EmotionState::default()
        };

        // 距上次更新 10 秒 < 延迟 60 秒
        assert!(!advance_decay(&mut state, 7, 1_010, 60));
        // 恰好到延迟边界即推进
        assert!(advance_decay(&mut state, 7, 1_060, 60));
    }

    /// 推进之后 `last_update` 前移，因此同一次维护里重复推进不会叠加衰减
    #[test]
    fn advance_decay_moves_last_update_forward() {
        let mut state = EmotionState {
            last_update: 1_000,
            ..EmotionState::default()
        };

        assert!(advance_decay(&mut state, 7, 5_000, 60));
        let advanced_to = state.last_update;
        assert!(
            advanced_to > 1_000,
            "推进后 last_update 必须前移，否则衰减会被重复应用"
        );

        // 紧接着再推进一次：因为 last_update 已经前移，延迟门会挡住
        assert!(!advance_decay(&mut state, 7, 5_000, 60));
    }

    /// 危机等级的时间衰减是保底清理，必须能在推进中被降级
    #[test]
    fn advance_decay_relaxes_stale_crisis_levels() {
        let mut state = EmotionState {
            last_update: 1_000,
            crisis_level: CrisisLevel::Mild,
            last_crisis_detected: 1_000,
            ..EmotionState::default()
        };

        // 距上次检测超过 Mild 冷却的三倍
        let now = 1_000 + CRISIS_MILD_COOLDOWN_SECS * 3 + 1;
        assert!(advance_decay(&mut state, 7, now, 60));
        assert_eq!(state.crisis_level, CrisisLevel::None);
    }
}
