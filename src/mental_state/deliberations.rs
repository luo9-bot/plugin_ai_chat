use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliberation {
    pub content: String,
    pub source: String,
    pub strength: f32,
    pub created: u64,
    pub last_reinforced: u64,
}

/// 添加考量，自动去重。重复考量会增强强度。
pub fn add_deliberation(content: &str, source: &str) {
    let mut store = super::MentalStateStore::load();
    let now = crate::util::now_secs();
    let normalized = super::normalize_text(content);

    for delib in &mut store.deliberations {
        let existing_norm = super::normalize_text(&delib.content);
        if super::is_similar(&normalized, &existing_norm) {
            delib.strength = (delib.strength + 0.1).min(1.0);
            delib.last_reinforced = now;
            debug!(content, "mental_state: reinforced existing deliberation");
            store.save();
            return;
        }
    }

    let max = config::get().mental_state.deliberations_max;
    if store.deliberations.len() >= max
        && let Some(weakest_idx) = store.deliberations.iter()
            .enumerate()
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
        {
            if store.deliberations[weakest_idx].strength < 0.2 {
                store.deliberations.remove(weakest_idx);
            } else {
                debug!(content, "mental_state: deliberations full, skipping");
                return;
            }
        }

    debug!(content, source, "mental_state: added deliberation");
    store.deliberations.push(Deliberation {
        content: content.to_string(),
        source: source.to_string(),
        strength: 0.5,
        created: now,
        last_reinforced: now,
    });
    store.save();
}

/// 衰减所有考量的强度，移除过低的
pub fn decay_deliberations(rate_per_hour: f32) {
    let mut store = super::MentalStateStore::load();
    if store.deliberations.is_empty() {
        return;
    }
    let now = crate::util::now_secs();
    let before = store.deliberations.len();

    for delib in &mut store.deliberations {
        let elapsed = now.saturating_sub(delib.last_reinforced) as f32;
        if elapsed < 60.0 {
            continue;
        }
        let decay = rate_per_hour * (elapsed / 3600.0);
        delib.strength = (delib.strength - decay).max(0.0);
    }

    store.deliberations.retain(|d| d.strength >= 0.05);
    let removed = before - store.deliberations.len();
    if removed > 0 {
        debug!(removed, "mental_state: decayed deliberations");
    }
    store.save();
}
