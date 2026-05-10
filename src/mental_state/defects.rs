use std::time::SystemTime;
use tracing::debug;

use crate::emotion::EmotionType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefectType {
    Typo,         // 打错字
    AbsentMinded, // 走神/忘事
    ShortReply,   // 敷衍
    Hesitation,   // 犹豫
    Tangent,      // 跑题
}

/// 基于情绪状态和随机概率检查是否触发缺陷（带全局冷却）
pub fn check_defect(emotion: EmotionType, intensity: f32, base_probability: f32) -> Option<DefectType> {
    // 全局冷却：120秒内不重复触发缺陷
    let now = crate::util::now_secs();
    {
        let store = super::MentalStateStore::load();
        if now.saturating_sub(store.last_defect_ts) < 120 {
            return None;
        }
    }

    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let roll = (nanos % 10000) as f32 / 10000.0;

    // 每种缺陷在不同情绪下的基础概率
    let (typo_p, absent_p, short_p, hesitate_p, tangent_p) = match emotion {
        EmotionType::Tired =>   (0.20, 0.15, 0.20, 0.15, 0.05),
        EmotionType::Angry =>   (0.05, 0.05, 0.25, 0.05, 0.05),
        EmotionType::Thinking => (0.05, 0.10, 0.05, 0.20, 0.15),
        EmotionType::Excited => (0.10, 0.05, 0.02, 0.05, 0.20),
        EmotionType::Sad =>     (0.05, 0.10, 0.20, 0.15, 0.05),
        EmotionType::Worried => (0.05, 0.10, 0.10, 0.15, 0.05),
        EmotionType::Happy =>   (0.05, 0.02, 0.02, 0.02, 0.10),
        _ =>                    (0.05, 0.05, 0.05, 0.05, 0.05),
    };

    let scale = intensity * (base_probability / 0.1);

    let types_and_probs = [
        (DefectType::Typo, typo_p * scale),
        (DefectType::AbsentMinded, absent_p * scale),
        (DefectType::ShortReply, short_p * scale),
        (DefectType::Hesitation, hesitate_p * scale),
        (DefectType::Tangent, tangent_p * scale),
    ];

    let mut cumulative = 0.0f32;
    for (dtype, prob) in &types_and_probs {
        cumulative += prob;
        if roll < cumulative {
            debug!(?dtype, roll, cumulative, "mental_state: defect triggered");
            // 记录触发时间，启动冷却
            let mut store = super::MentalStateStore::load();
            store.last_defect_ts = now;
            store.save();
            return Some(*dtype);
        }
    }
    None
}

/// 将缺陷类型转换为 prompt 指令
pub fn defect_to_instruction(d: DefectType) -> &'static str {
    match d {
        DefectType::Typo =>
            "- 你刚才打字有点急，可能会有一两个错别字，不用刻意纠正，自然就好",
        DefectType::AbsentMinded =>
            "- 你刚才有点走神，可能没完全理解对方的意思，回复可能有点答非所问",
        DefectType::ShortReply =>
            "- 你现在没什么聊天兴致，回复会比较简短敷衍",
        DefectType::Hesitation =>
            "- 你现在有点犹豫不决，回复可能会带出不确定的语气，比如'嗯...'、'可能吧'",
        DefectType::Tangent =>
            "- 你现在思绪有点飘，可能会突然岔开话题或者说些不太相关的话",
    }
}
