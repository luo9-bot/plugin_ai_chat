use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConcernCategory {
    Social,    // 社交相关
    Task,      // 任务相关
    Emotional, // 情感相关
    Self_,     // 自我相关
}

impl ConcernCategory {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "task" => Self::Task,
            "emotional" => Self::Emotional,
            "self" => Self::Self_,
            _ => Self::Social,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Social => "社交",
            Self::Task => "任务",
            Self::Emotional => "情感",
            Self::Self_ => "自我",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concern {
    pub content: String,
    pub category: ConcernCategory,
    pub strength: f32,
    pub created: u64,
    pub last_reinforced: u64,
    pub trigger_user: u64,
    pub trigger_group: u64,
}

/// 添加担忧，自动去重。重复担忧会增强强度。
pub fn add_concern(content: &str, category: &str, trigger_user: u64, trigger_group: u64) {
    let mut store = super::MentalStateStore::load();
    let cat = ConcernCategory::from_str(category);
    let now = crate::util::now_secs();
    let normalized = super::normalize_text(content);

    // 去重：检查已有担忧是否相似
    for concern in &mut store.concerns {
        let existing_norm = super::normalize_text(&concern.content);
        if super::is_similar(&normalized, &existing_norm) {
            // 增强已有担忧
            concern.strength = (concern.strength + 0.15).min(1.0);
            concern.last_reinforced = now;
            debug!(content, "mental_state: reinforced existing concern");
            store.save();
            return;
        }
    }

    // 容量检查：超过上限时替换最弱的
    let max = config::get().mental_state.concerns_max;
    if store.concerns.len() >= max
        && let Some(weakest_idx) = store.concerns.iter()
            .enumerate()
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
        {
            if store.concerns[weakest_idx].strength < 0.3 {
                store.concerns.remove(weakest_idx);
            } else {
                debug!(content, "mental_state: concerns full, skipping weak concern");
                return;
            }
        }

    debug!(content, ?cat, "mental_state: added concern");
    store.concerns.push(Concern {
        content: content.to_string(),
        category: cat,
        strength: 0.5,
        created: now,
        last_reinforced: now,
        trigger_user,
        trigger_group,
    });
    store.save();
}

/// 衰减所有担忧的强度，移除过低的
pub fn decay_concerns(rate_per_hour: f32) {
    let mut store = super::MentalStateStore::load();
    if store.concerns.is_empty() {
        return;
    }
    let now = crate::util::now_secs();
    let before = store.concerns.len();

    for concern in &mut store.concerns {
        let elapsed = now.saturating_sub(concern.last_reinforced) as f32;
        if elapsed < 60.0 {
            continue; // 60秒宽限期
        }
        let decay = rate_per_hour * (elapsed / 3600.0);
        concern.strength = (concern.strength - decay).max(0.0);
    }

    store.concerns.retain(|c| c.strength >= 0.05);
    let removed = before - store.concerns.len();
    if removed > 0 {
        debug!(removed, "mental_state: decayed concerns");
    }
    store.save();
}
