//! 不可预测性注入
//!
//! 在现有记忆管理和人格系统中添加概率性漂移。
//! 包括：观点漂移、自然遗忘、联想跳跃。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

/// 不可预测性引擎状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpredictabilityState {
    /// 心血来潮概率
    pub whim_probability: f32,
    /// 观点漂移率
    pub opinion_drift_rate: f32,
    /// 自然遗忘率
    pub forgetting_rate: f32,
    /// 联想跳跃概率
    pub association_jump_probability: f32,
    /// 上次遗忘扫描时间
    pub last_forgetting_scan: u64,
    /// 观点漂移记录
    pub opinion_drifts: Vec<OpinionDrift>,
}

/// 观点漂移记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpinionDrift {
    /// 话题
    pub topic: String,
    /// 旧立场
    pub old_stance: String,
    /// 新立场
    pub new_stance: String,
    /// 漂移强度
    pub drift_strength: f32,
    /// 原因
    pub reason: Option<String>,
    /// 时间
    pub created_at: u64,
}

impl Default for UnpredictabilityState {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity;
        Self {
            whim_probability: h.whim_probability,
            opinion_drift_rate: h.opinion_drift_rate,
            forgetting_rate: h.forgetting_rate,
            association_jump_probability: h.association_jump_probability,
            last_forgetting_scan: 0,
            opinion_drifts: Vec::new(),
        }
    }
}

fn state_path() -> std::path::PathBuf {
    config::data_dir().join("unpredictability.json")
}

fn load_state() -> UnpredictabilityState {
    let path = state_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => UnpredictabilityState::default(),
    }
}

fn save_state(state: &UnpredictabilityState) {
    let path = state_path();
    if let Ok(json) = serde_json::to_string_pretty(state) {
        fs::write(path, json).ok();
    }
}

/// 每日遗忘扫描
///
/// 随机标记低重要性记忆为"模糊"（不是删除，检索时降权）
pub fn run_forgetting_scan() {
    let cfg = config::get();
    if !cfg.humanity.unpredictability_enabled {
        return;
    }

    let mut state = load_state();
    let now = crate::util::now_secs();

    // 每天只运行一次
    if now.saturating_sub(state.last_forgetting_scan) < 86400 {
        return;
    }
    state.last_forgetting_scan = now;

    let forgetting_rate = state.forgetting_rate;
    let mut forgotten_count = 0u32;

    // 扫描所有用户记忆，概率性标记重要性降级
    // 这里使用记忆存储的接口
    let all_users = crate::memory::store::all_user_ids();
    for user_id in all_users {
        let memory = crate::memory::store::load_user_memory(user_id);
        let mut modified = false;
        let mut new_entries = memory.entries.clone();

        for entry in &mut new_entries {
            // 只对Normal重要性的记忆进行遗忘
            if matches!(entry.importance, crate::memory::Importance::Normal)
                && fastrand::f32() < forgetting_rate
            {
                // 降级为Normal但标记为更低的access_count（相当于降权）
                entry.access_count = entry.access_count.saturating_sub(
                    (entry.access_count as f32 * 0.5) as u32
                );
                modified = true;
                forgotten_count += 1;
            }
        }

        if modified {
            crate::memory::store::save_user_memory(user_id, &crate::memory::store::MemoryFile {
                entries: new_entries,
            });
        }
    }

    save_state(&state);
    debug!(forgotten_count, "unpredictability: forgetting scan completed");
}

/// 检查是否应该触发联想跳跃
///
/// 对话中有概率突然联想到不直接相关但有记忆关联的话题
pub fn should_association_jump() -> bool {
    let cfg = config::get();
    if !cfg.humanity.unpredictability_enabled {
        return false;
    }
    let state = load_state();
    fastrand::f32() < state.association_jump_probability
}

/// 记录观点漂移
///
/// 每次生成回复后，检查是否暗示了与现有偏好矛盾的倾向
pub fn record_opinion_drift(
    topic: &str,
    old_stance: &str,
    new_stance: &str,
    reason: Option<&str>,
) {
    let cfg = config::get();
    if !cfg.humanity.unpredictability_enabled {
        return;
    }

    let mut state = load_state();
    if fastrand::f32() >= state.opinion_drift_rate {
        return;
    }

    state.opinion_drifts.push(OpinionDrift {
        topic: topic.to_string(),
        old_stance: old_stance.to_string(),
        new_stance: new_stance.to_string(),
        drift_strength: fastrand::f32() * 0.3 + 0.1,
        reason: reason.map(|s| s.to_string()),
        created_at: crate::util::now_secs(),
    });

    // 保留最近20条
    if state.opinion_drifts.len() > 20 {
        state.opinion_drifts.remove(0);
    }

    save_state(&state);
    debug!(topic, old_stance, new_stance, "unpredictability: opinion drift recorded");
}

/// 心血来潮检查
pub fn should_whim() -> bool {
    let cfg = config::get();
    if !cfg.humanity.unpredictability_enabled {
        return false;
    }
    let state = load_state();
    fastrand::f32() < state.whim_probability
}
