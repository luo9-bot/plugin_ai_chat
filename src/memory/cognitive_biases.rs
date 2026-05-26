//! 认知偏差系统
//!
//! 在 BM25 + 向量检索之后，对结果应用认知偏差权重修正。
//! 模拟人类记忆检索中的确认偏误、近因效应、情绪一致性、
//! 锚定效应和可得性启发。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::debug;

use super::retrieval::RetrievalResult;
use crate::config;

/// 认知偏差状态（持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveBiases {
    /// 确认偏误：与当前隐含立场一致的记忆加分
    pub confirmation_bias: f32,
    /// 近因效应：最近的记忆权重衰减更慢
    pub recency_bias: f32,
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
            recency_bias: h.recency_bias,
            mood_congruence: h.mood_congruence,
            anchoring_strength: h.anchoring_strength,
            availability_heuristic: h.availability_heuristic,
            last_drift: crate::util::now_secs(),
            access_frequency: HashMap::new(),
            anchors: HashMap::new(),
        }
    }
}

pub(crate) fn state_path() -> std::path::PathBuf {
    config::data_dir().join("cognitive_state.json")
}

/// 认知状态存储（可扩展，目前仅含 biases）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CognitiveStateStore {
    pub biases: Option<CognitiveBiases>,
    #[serde(default)]
    pub attention_json: Option<serde_json::Value>,
}

/// 加载认知偏差
pub fn load_biases() -> CognitiveBiases {
    let path = state_path();
    match fs::read_to_string(&path) {
        Ok(content) => {
            let store: CognitiveStateStore =
                serde_json::from_str(&content).unwrap_or_default();
            store.biases.unwrap_or_default()
        }
        Err(_) => CognitiveBiases::default(),
    }
}

/// 保存认知偏差
pub fn save_biases(biases: &CognitiveBiases) {
    let path = state_path();
    let mut store: CognitiveStateStore = match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => CognitiveStateStore::default(),
    };
    store.biases = Some(biases.clone());
    if let Ok(json) = serde_json::to_string_pretty(&store) {
        fs::write(path, json).ok();
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
    let positive: &[&str] = &["开心", "高兴", "喜欢", "好", "棒", "爱", "成功", "有趣",
        "温暖", "感动", "幸福", "美好", "惊喜", "期待"];
    let negative: &[&str] = &["难过", "伤心", "生气", "讨厌", "烦", "累", "无聊",
        "失败", "痛苦", "焦虑", "害怕", "失望", "后悔", "孤独"];

    let pos_count = positive.iter().filter(|w| content.contains(*w)).count() as f32;
    let neg_count = negative.iter().filter(|w| content.contains(*w)).count() as f32;
    let total = pos_count + neg_count;
    if total == 0.0 {
        return 0.0;
    }
    (pos_count - neg_count) / total
}

/// 记忆衰减权重（基于创建时间）
#[allow(dead_code)]
fn recency_weight(created_secs: u64, bias_strength: f32) -> f32 {
    let now = crate::util::now_secs();
    let age_hours = now.saturating_sub(created_secs) as f32 / 3600.0;
    // 基础指数衰减
    let base_decay = (-age_hours / 168.0).exp(); // 7天半衰期
    // 近因效应让衰减更平缓
    let adjusted = base_decay.powf(1.0 - bias_strength * 0.5);
    adjusted.clamp(0.1, 1.0)
}

/// 对检索结果应用认知偏差权重，重新排序
pub fn apply_cognitive_biases(
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
        drift(&mut biases.recency_bias);
        drift(&mut biases.mood_congruence);
        drift(&mut biases.anchoring_strength);
        drift(&mut biases.availability_heuristic);
        biases.last_drift = now;
        save_biases(biases);
        debug!(
            confirmation = biases.confirmation_bias,
            recency = biases.recency_bias,
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

            // 5. 近因效应：通过 recency_weight 影响分数（需要 creation timestamp）
            // 从 retrieval result 的元数据中提取或使用默认的中间值
            let recency_bonus = biases.recency_bias * 0.15;
            bonus += recency_bonus;

            r.score = (r.score + bonus as f64).clamp(0.0, 1.0);

            // 更新访问频率
            *biases.access_frequency.entry(r.id.clone()).or_insert(0) += 1;

            r
        })
        .collect();

    // 按调整后的分数重新排序
    adjusted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 定期清理访问频率（超过1000条时削减）
    if biases.access_frequency.len() > 1000 {
        biases.access_frequency.retain(|_, v| {
            *v = v.saturating_sub(1);
            *v > 0
        });
    }

    adjusted
}

/// 记录锚定记忆（首次强印象）
pub fn record_anchor(topic: &str, memory_id: &str, biases: &mut CognitiveBiases) {
    if !biases.anchors.contains_key(topic) {
        biases.anchors.insert(topic.to_string(), memory_id.to_string());
        save_biases(biases);
        debug!(topic, memory_id, "cognitive_biases: anchor recorded");
    }
}
