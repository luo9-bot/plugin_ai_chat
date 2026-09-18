//! 关系动力学系统
//!
//! 在人物档案基础上扩展为完整的关系模型。
//! 模拟人类关系的自然演化：信任缓慢建立快速崩塌、亲密度有天花板、
//! 缺席冷却、共享记忆和inside jokes。

use serde::{Deserialize, Serialize};
use tracing::debug;

/// 关系类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub(crate) enum RelationshipType {
    /// 陌生人
    #[default]
    Stranger,
    /// 认识的人
    Acquaintance,
    /// 常客
    Regular,
    /// 亲近
    Close,
    /// 知己
    Confidant,
    /// 对立
    Antagonistic,
    /// 仰慕
    Admiring,
}

impl RelationshipType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stranger => "stranger",
            Self::Acquaintance => "acquaintance",
            Self::Regular => "regular",
            Self::Close => "close",
            Self::Confidant => "confidant",
            Self::Antagonistic => "antagonistic",
            Self::Admiring => "admiring",
        }
    }
}

/// 用户交流风格偏好
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) enum CommStyle {
    /// 默认
    #[default]
    Default,
    /// 喜欢直来直去
    Direct,
    /// 喜欢温柔委婉
    Gentle,
    /// 喜欢幽默
    Humorous,
    /// 喜欢简短
    Brief,
    /// 喜欢深入讨论
    Deep,
}

/// 共享记忆 / inside joke
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SharedMemory {
    /// 记忆摘要
    pub summary: String,
    /// 情感重要性 (0.0-1.0)
    pub emotional_significance: f32,
    /// 内部笑话（如果有）
    pub inside_joke: Option<String>,
    /// 被引用次数——越引用越重要
    pub callback_count: u32,
    /// 创建时间
    pub created_at: u64,
}

/// 关系事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) enum RelationEvent {
    /// 被忽视（bot 主动说话但没被回复）
    Ignored,
    /// 被关心（对方主动关心 bot）
    Cared,
    /// 冲突（争吵、不愉快）
    Conflict,
    /// 和解（冲突后的修复）
    Reconciliation,
    /// 新鲜信息（对方分享了新东西）
    NewInfo,
    /// 重复话题（对方又说了同样的话）
    Repetitive,
    /// 共同经历（一起做了什么）
    SharedExperience,
    /// 被尊重（对方认真对待 bot 的话）
    Respected,
}

/// 关系数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Relationship {
    pub user_id: u64,
    /// 信任度——缓慢建立，快速崩塌（非对称）
    pub trust: f32,
    /// 亲密度——交互频率和深度
    pub intimacy: f32,
    /// 默契度——越少话能理解越多
    pub rapport: f32,
    /// 好感度
    pub affection: f32,
    /// 互惠感——对方是否也在主动（0.0=完全被动，1.0=非常主动）
    #[serde(default)]
    pub reciprocity: f32,
    /// 紧张度——冲突后上升，和解后下降
    #[serde(default)]
    pub tension: f32,
    /// 烦躁度——对重复行为的容忍度下降
    #[serde(default)]
    pub annoyance: f32,
    /// 对这个人的好奇心
    #[serde(default)]
    pub curiosity: f32,
    /// 对用户的"印象"（自然语言）
    pub impression: String,
    /// 用户喜欢的交流方式
    pub communication_style: CommStyle,
    /// 共享经历、inside jokes
    pub shared_memories: Vec<SharedMemory>,
    /// 感知到的用户态度
    pub perceived_attitude: String,
    /// 上次交互时间
    pub last_interaction: u64,
    /// 上次 bot 主动说话但没被回复的时间
    #[serde(default)]
    pub last_ignored_at: u64,
    /// 缺席冷却速率
    pub absence_cooling_rate: f32,
    /// 关系类型
    pub relationship_type: RelationshipType,
    /// 创建时间
    pub created_at: u64,
    /// 更新时间
    pub updated_at: u64,
    /// 交互总次数
    pub interaction_count: u32,
    /// 积极交互次数
    pub positive_interactions: u32,
    /// 消极交互次数
    pub negative_interactions: u32,
    /// 连续被忽视次数
    #[serde(default)]
    pub ignore_streak: u32,
    /// 最近关系事件（保留最近20条）
    #[serde(default)]
    pub recent_events: Vec<RelationEventRecord>,
}

/// 关系事件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RelationEventRecord {
    pub event: RelationEvent,
    pub timestamp: u64,
    pub detail: Option<String>,
}

impl Default for Relationship {
    fn default() -> Self {
        let now = crate::util::now_secs();
        Self {
            user_id: 0,
            trust: 0.3,
            intimacy: 0.0,
            rapport: 0.0,
            affection: 0.3,
            reciprocity: 0.5,
            tension: 0.0,
            annoyance: 0.0,
            curiosity: 0.3,
            impression: String::new(),
            communication_style: CommStyle::Default,
            shared_memories: Vec::new(),
            perceived_attitude: "neutral".to_string(),
            last_interaction: now,
            last_ignored_at: 0,
            absence_cooling_rate: 0.01,
            relationship_type: RelationshipType::Stranger,
            created_at: now,
            updated_at: now,
            interaction_count: 0,
            positive_interactions: 0,
            negative_interactions: 0,
            ignore_streak: 0,
            recent_events: Vec::new(),
        }
    }
}

/// 读一个用户的关系；没有记录时返回 `None`
///
/// 关系按用户一行存在状态库里（原先在 `relationships.json` 的一整张 map 里）。
fn load_relationship(user_id: u64) -> Option<Relationship> {
    match crate::db::db().per_user_state(crate::db::PerUserState::Relationship, user_id) {
        Ok(Some(json)) => serde_json::from_str(&json).ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, user_id, "relationship: 读取失败");
            None
        }
    }
}

/// 获取或初始化关系
pub(crate) fn get_relationship(user_id: u64) -> Relationship {
    if let Some(rel) = load_relationship(user_id) {
        return rel;
    }
    let now = crate::util::now_secs();
    Relationship {
        user_id,
        created_at: now,
        updated_at: now,
        ..Default::default()
    }
}

/// 保存关系（单行 UPSERT）
pub(crate) fn save_relationship(rel: &Relationship) {
    let json = match serde_json::to_string(rel) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(%error, user_id = rel.user_id, "relationship: 序列化失败，未写入");
            return;
        }
    };
    if let Err(error) = crate::db::db().set_per_user_state(
        crate::db::PerUserState::Relationship,
        rel.user_id,
        &json,
    ) {
        tracing::warn!(%error, user_id = rel.user_id, "relationship: 写库失败");
    }
}

/// 记录一次交互，更新关系动力学
pub(crate) fn record_interaction(user_id: u64, positive: bool) {
    let mut rel = get_relationship(user_id);
    let now = crate::util::now_secs();

    rel.last_interaction = now;
    rel.interaction_count += 1;

    if positive {
        rel.positive_interactions += 1;
    } else {
        rel.negative_interactions += 1;
    }

    // 1. 信任更新（非对称：建立慢，崩塌快）
    if positive {
        rel.trust = (rel.trust + 0.01).min(1.0);
    } else {
        rel.trust = (rel.trust - 0.05).max(0.0);
    }

    // 2. 亲密度更新（天花板效应）
    let intimacy_gain = if positive { 0.02 } else { -0.01 };
    let ceiling_factor = 1.0 - rel.intimacy * 0.8;
    rel.intimacy = (rel.intimacy + intimacy_gain * ceiling_factor).clamp(0.0, 1.0);

    // 3. 好感度更新
    rel.affection = (rel.affection + if positive { 0.015 } else { -0.03 }).clamp(0.0, 1.0);

    // 4. 默契度更新（互动越多默契越高）
    if rel.interaction_count > 10 {
        rel.rapport = (rel.rapport + 0.005).min(1.0);
    }

    // 5. 紧张度自然衰减（每次正常互动降低一点）
    rel.tension = (rel.tension - 0.02).max(0.0);

    // 6. 烦躁度自然衰减
    rel.annoyance = (rel.annoyance - 0.01).max(0.0);

    // 7. 好奇心：积极互动增加好奇心
    if positive {
        rel.curiosity = (rel.curiosity + 0.01).min(1.0);
    }

    // 8. 关系类型自动升级（考虑新维度）
    rel.relationship_type = compute_relationship_type(&rel);

    rel.updated_at = now;

    debug!(
        user_id,
        trust = rel.trust,
        intimacy = rel.intimacy,
        rapport = rel.rapport,
        reciprocity = rel.reciprocity,
        tension = rel.tension,
        annoyance = rel.annoyance,
        interactions = rel.interaction_count,
        rel_type = ?rel.relationship_type,
        "relationship: updated"
    );

    save_relationship(&rel);
}

/// 根据多维度计算关系类型
fn compute_relationship_type(rel: &Relationship) -> RelationshipType {
    // 对立关系：高紧张度
    if rel.tension > 0.6 && rel.affection < 0.3 {
        return RelationshipType::Antagonistic;
    }
    // 仰慕关系：高好感 + 低互惠（对方不太主动但 bot 很喜欢）
    if rel.affection > 0.7 && rel.reciprocity < 0.3 {
        return RelationshipType::Admiring;
    }
    // 知己：高亲密 + 高信任 + 高默契
    if rel.intimacy > 0.8 && rel.trust > 0.7 && rel.rapport > 0.5 {
        return RelationshipType::Confidant;
    }
    // 亲近
    if rel.intimacy > 0.6 && rel.tension < 0.3 {
        return RelationshipType::Close;
    }
    // 常客
    if rel.intimacy > 0.3 || rel.interaction_count > 20 {
        return RelationshipType::Regular;
    }
    // 认识
    if rel.interaction_count > 3 {
        return RelationshipType::Acquaintance;
    }
    RelationshipType::Stranger
}

/// 获取关系摘要（用于 WebUI 显示）
pub(crate) fn get_relationship_summary(user_id: u64) -> serde_json::Value {
    let rel = get_relationship(user_id);
    serde_json::json!({
        "user_id": rel.user_id,
        "trust": rel.trust,
        "intimacy": rel.intimacy,
        "rapport": rel.rapport,
        "affection": rel.affection,
        "reciprocity": rel.reciprocity,
        "tension": rel.tension,
        "annoyance": rel.annoyance,
        "curiosity": rel.curiosity,
        "relationship_type": rel.relationship_type.as_str(),
        "interaction_count": rel.interaction_count,
        "positive_interactions": rel.positive_interactions,
        "negative_interactions": rel.negative_interactions,
        "ignore_streak": rel.ignore_streak,
        "last_interaction": rel.last_interaction,
        "impression": rel.impression,
        "shared_memories_count": rel.shared_memories.len(),
        "recent_events": rel.recent_events.iter().rev().take(5).map(|e| {
            serde_json::json!({
                "event": format!("{:?}", e.event),
                "timestamp": e.timestamp,
                "detail": e.detail,
            })
        }).collect::<Vec<_>>(),
    })
}
