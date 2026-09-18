//! 认知偏差系统
//!
//! 在 BM25 + 向量检索之后，对结果应用认知偏差权重修正。
//! 模拟人类记忆检索中的确认偏误、近因效应、情绪一致性、
//! 锚定效应和可得性启发。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

use super::retrieval::RetrievalResult;
use crate::config;

/// 认知偏差状态（持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CognitiveBiases {
    /// 确认偏误：与当前隐含立场一致的记忆加分
    pub confirmation_bias: f32,
    /// 情绪一致性：当前情绪影响记忆检索（悲伤时更容易想起悲伤的事）
    pub mood_congruence: f32,
    /// 锚定效应：首次印象对后续判断的影响
    pub anchoring_strength: f32,
    /// 可得性启发：近期被频繁检索的记忆被认为更重要
    pub availability_heuristic: f32,
    /// 上次漂移时间
    pub last_drift: u64,
    /// 记忆访问频率追踪: memory_id -> access_count
    #[serde(default)]
    pub access_frequency: HashMap<String, u32>,
    /// 锚定记忆: 首次形成的强印象 (topic -> memory_id)
    #[serde(default)]
    pub anchors: HashMap<String, String>,
}

impl Default for CognitiveBiases {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity.cognitive_biases;
        Self {
            confirmation_bias: h.confirmation_bias,
            mood_congruence: h.mood_congruence,
            anchoring_strength: h.anchoring_strength,
            availability_heuristic: h.availability_heuristic,
            last_drift: crate::util::now_secs(),
            access_frequency: HashMap::new(),
            anchors: HashMap::new(),
        }
    }
}

/// 加载认知偏差
///
/// 读状态库里**自己那一行**。原先它与注意力状态共用一个
/// `cognitive_state.json`，双方各自"读整份 → 改自己那一半 → 写回整份"，
/// 交错写入会整段抹掉对方刚做的改动。分表后这个失败模式不存在了。
pub(crate) fn load_biases() -> CognitiveBiases {
    let stored = crate::db::db().singleton_state(crate::db::SingletonState::CognitiveBiases);
    match stored {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        Ok(None) => CognitiveBiases::default(),
        Err(error) => {
            tracing::warn!(%error, "cognitive_biases: 读取失败，按默认值处理");
            CognitiveBiases::default()
        }
    }
}

/// 保存认知偏差（只写自己那一行）
pub(crate) fn save_biases(biases: &CognitiveBiases) {
    let json = match serde_json::to_string(biases) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(%error, "cognitive_biases: 序列化失败，未写入");
            return;
        }
    };
    if let Err(error) =
        crate::db::db().set_singleton_state(crate::db::SingletonState::CognitiveBiases, &json)
    {
        tracing::warn!(%error, "cognitive_biases: 写库失败");
    }
}

/// 情绪到效价的映射（用于情绪一致性偏差）
fn emotion_valence(emotion_type: &crate::emotion::EmotionType) -> f32 {
    match emotion_type {
        crate::emotion::EmotionType::Happy => 0.8,
        crate::emotion::EmotionType::Excited => 0.7,
        crate::emotion::EmotionType::Like => 0.6,
        crate::emotion::EmotionType::Neutral => 0.0,
        crate::emotion::EmotionType::Surprised => 0.1,
        crate::emotion::EmotionType::Thinking => -0.1,
        crate::emotion::EmotionType::Shy => 0.2,
        crate::emotion::EmotionType::Worried => -0.5,
        crate::emotion::EmotionType::Tired => -0.3,
        crate::emotion::EmotionType::Sad => -0.7,
        crate::emotion::EmotionType::Angry => -0.6,
    }
}

/// 简单的情感极性估算（基于关键词）
fn estimate_content_valence(content: &str) -> f32 {
    let positive: &[&str] = &[
        "开心", "高兴", "喜欢", "好", "棒", "爱", "成功", "有趣", "温暖", "感动", "幸福", "美好",
        "惊喜", "期待",
    ];
    let negative: &[&str] = &[
        "难过", "伤心", "生气", "讨厌", "烦", "累", "无聊", "失败", "痛苦", "焦虑", "害怕", "失望",
        "后悔", "孤独",
    ];

    let pos_count = positive.iter().filter(|w| content.contains(*w)).count() as f32;
    let neg_count = negative.iter().filter(|w| content.contains(*w)).count() as f32;
    let total = pos_count + neg_count;
    if total == 0.0 {
        return 0.0;
    }
    (pos_count - neg_count) / total
}

/// 对检索结果应用认知偏差权重，重新排序
pub(crate) fn apply_cognitive_biases(
    results: Vec<RetrievalResult>,
    current_emotion: &crate::emotion::EmotionType,
    biases: &mut CognitiveBiases,
) -> Vec<RetrievalResult> {
    if results.is_empty() {
        return results;
    }

    // 偏差值随时间缓慢漂移（均匀分布随机扰动，近似正态分布效果）
    let now = crate::util::now_secs();
    let drift_elapsed = now.saturating_sub(biases.last_drift) as f32;
    if drift_elapsed > 3600.0 {
        // 每小时漂移一次，使用 Box-Muller 近似的简化版
        let drift_scale = (drift_elapsed / 3600.0).min(24.0) * 0.02;
        let drift = |v: &mut f32| {
            // 用两个均匀分布的和近似正态分布（中心极限定理）
            let d = (fastrand::f32() + fastrand::f32() - 1.0) * drift_scale;
            *v = (*v + d).clamp(0.0, 1.0);
        };
        drift(&mut biases.confirmation_bias);
        drift(&mut biases.mood_congruence);
        drift(&mut biases.anchoring_strength);
        drift(&mut biases.availability_heuristic);
        biases.last_drift = now;
        save_biases(biases);
        debug!(
            confirmation = biases.confirmation_bias,
            mood = biases.mood_congruence,
            anchoring = biases.anchoring_strength,
            availability = biases.availability_heuristic,
            "cognitive_biases: drifted"
        );
    }

    let emotion_val = emotion_valence(current_emotion);

    let mut adjusted: Vec<RetrievalResult> = results
        .into_iter()
        .map(|mut r| {
            let content_valence = estimate_content_valence(&r.content);
            let mut bonus = 0.0f32;

            // 1. 情绪一致性：情绪效价与记忆效价一致时加分
            let mood_match = 1.0 - (emotion_val - content_valence).abs() / 2.0;
            bonus += biases.mood_congruence * mood_match * 0.3;

            // 2. 可得性启发：近期被频繁检索的记忆得分更高
            let access_count = *biases.access_frequency.get(&r.id).unwrap_or(&0);
            if access_count > 0 {
                bonus += biases.availability_heuristic
                    * (1.0 - (-0.5 * access_count as f32).exp())
                    * 0.2;
            }

            // 3. 锚定效应：强锚定记忆获得持久加分
            if biases.anchors.values().any(|a| a == &r.id) {
                bonus += biases.anchoring_strength * 0.25;
            }

            // 4. 确认偏误：与当前情感倾向一致的记忆加分
            if (emotion_val > 0.3 && content_valence > 0.3)
                || (emotion_val < -0.3 && content_valence < -0.3)
            {
                bonus += biases.confirmation_bias * 0.2;
            }

            // 偏差是**有界的相对调整**，不是把分数整体抬到上界的加法。
            //
            // 曾经这里是 `(score + bonus).clamp(0,1)`：在归一化之前，
            // 无条件项就有 0.06（recency_bias 0.4 × 0.15），而融合分数上界
            // 只有 0.0164，于是每条结果都被顶到 1.0——排序完全由稳定排序
            // 留下的旧次序决定，偏差模块贡献了零个有效信号。
            //
            // 近因效应不在这里：它曾经对**每条**候选加同一个常数，
            // 而同一个常数不携带任何排序信息（只会在上界处抹掉差异）。
            // 时间衰减由 `retrieval::forgetting` 的 retention 分量承担。
            r.score = (r.score * (1.0 + bonus as f64)).clamp(0.0, 1.0);

            // 更新访问频率
            *biases.access_frequency.entry(r.id.clone()).or_insert(0) += 1;

            r
        })
        .collect();

    // 按调整后的分数重新排序
    adjusted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 定期清理访问频率（超过1000条时削减）
    if biases.access_frequency.len() > 1000 {
        biases.access_frequency.retain(|_, v| {
            *v = v.saturating_sub(1);
            *v > 0
        });
    }

    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::retrieval::RetrievalResult;

    /// 所有偏差拉满：这是偏差能施加的最大影响
    fn maxed_biases() -> CognitiveBiases {
        CognitiveBiases {
            confirmation_bias: 1.0,
            mood_congruence: 1.0,
            anchoring_strength: 1.0,
            availability_heuristic: 1.0,
            last_drift: crate::util::now_secs(),
            access_frequency: HashMap::new(),
            anchors: HashMap::new(),
        }
    }

    fn result(id: &str, score: f64, content: &str) -> RetrievalResult {
        RetrievalResult {
            id: id.to_string(),
            content: content.to_string(),
            score,
            source: "test",
        }
    }

    /// 偏差可以微调次序，但**不能**让一条明显不相关的记忆翻越一条高度相关的记忆。
    ///
    /// 这是 F1.5 的回归测试：曾经偏差是 `(score + bonus).clamp(0,1)` 的加法，
    /// 而无条件项就有 0.06、上限约 0.43，融合分数上界只有 0.0164，
    /// 于是每条结果都被顶到 1.0，真实相关性被完全抹掉。
    #[test]
    fn bias_cannot_overtake_a_much_more_relevant_memory() {
        let emotion = crate::emotion::EmotionType::Happy;
        let mut biases = maxed_biases();

        let ranked = apply_cognitive_biases(
            vec![
                result("relevant", 1.0, "今天天气不错"),
                result("irrelevant", 0.05, "好开心好高兴好喜欢"),
            ],
            &emotion,
            &mut biases,
        );

        assert_eq!(ranked[0].id, "relevant", "偏差不该抹掉相关性差异");
        assert!(ranked[0].score >= ranked[1].score);
    }

    /// 分数始终留在 [0,1] 内——下游和 admin 都按这个区间解释它
    #[test]
    fn adjusted_scores_stay_in_unit_range() {
        let emotion = crate::emotion::EmotionType::Sad;
        let mut biases = maxed_biases();
        let ranked = apply_cognitive_biases(
            vec![
                result("a", 1.0, "难过伤心"),
                result("b", 0.5, "难过伤心"),
                result("c", 0.0, "难过伤心"),
            ],
            &emotion,
            &mut biases,
        );
        assert!(
            ranked.iter().all(|r| (0.0..=1.0).contains(&r.score)),
            "分数越界：{:?}",
            ranked.iter().map(|r| r.score).collect::<Vec<_>>()
        );
    }

    /// 偏差确实有可测效果：不相关的记忆被提权（但不是无条件抬到 1.0）
    #[test]
    fn bias_still_promotes_mood_congruent_memories() {
        let emotion = crate::emotion::EmotionType::Happy;
        let mut biases = maxed_biases();
        let before = 0.4;

        let ranked = apply_cognitive_biases(
            vec![result("mood_match", before, "好开心好高兴")],
            &emotion,
            &mut biases,
        );

        assert!(
            ranked[0].score > before,
            "情绪一致的记忆应被提权：{} !> {before}",
            ranked[0].score
        );
        assert!(ranked[0].score <= 1.0);
    }
}
