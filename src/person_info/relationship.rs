//! 关系动力学系统
//!
//! 在人物档案基础上扩展为完整的关系模型。
//! 模拟人类关系的自然演化：信任缓慢建立快速崩塌、亲密度有天花板、
//! 缺席冷却、共享记忆和inside jokes。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// 关系类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelationshipType {
    /// 陌生人
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

impl Default for RelationshipType {
    fn default() -> Self { Self::Stranger }
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

    pub fn from_str(s: &str) -> Self {
        match s {
            "acquaintance" => Self::Acquaintance,
            "regular" => Self::Regular,
            "close" => Self::Close,
            "confidant" => Self::Confidant,
            "antagonistic" => Self::Antagonistic,
            "admiring" => Self::Admiring,
            _ => Self::Stranger,
        }
    }
}

/// 用户交流风格偏好
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommStyle {
    /// 默认
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

impl Default for CommStyle {
    fn default() -> Self { Self::Default }
}

/// 共享记忆 / inside joke
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMemory {
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

/// 关系数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub user_id: u64,
    /// 信任度——缓慢建立，快速崩塌（非对称）
    pub trust: f32,
    /// 亲密度——交互频率和深度
    pub intimacy: f32,
    /// 默契度——越少话能理解越多
    pub rapport: f32,
    /// 好感度
    pub affection: f32,
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
            impression: String::new(),
            communication_style: CommStyle::Default,
            shared_memories: Vec::new(),
            perceived_attitude: "neutral".to_string(),
            last_interaction: now,
            absence_cooling_rate: 0.01,
            relationship_type: RelationshipType::Stranger,
            created_at: now,
            updated_at: now,
            interaction_count: 0,
            positive_interactions: 0,
            negative_interactions: 0,
        }
    }
}

/// 关系存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelationshipStore {
    pub relationships: HashMap<u64, Relationship>,
}

use std::sync::Mutex;
pub(crate) static REL_STORE: Mutex<Option<RelationshipStore>> = Mutex::new(None);

fn rel_path() -> std::path::PathBuf {
    crate::config::data_dir().join("relationships.json")
}

fn load_rel_store() -> RelationshipStore {
    let mut g = REL_STORE.lock().unwrap();
    if g.is_none() {
        *g = Some(crate::util::load_json(&rel_path()));
    }
    g.clone().unwrap_or_default()
}

fn save_rel_store(store: &RelationshipStore) {
    let mut g = REL_STORE.lock().unwrap();
    *g = Some(store.clone());
    crate::util::save_json(&rel_path(), store);
}

/// 获取或初始化关系
pub fn get_relationship(user_id: u64) -> Relationship {
    let store = load_rel_store();
    store.relationships.get(&user_id).cloned().unwrap_or_else(|| {
        let mut rel = Relationship::default();
        rel.user_id = user_id;
        let now = crate::util::now_secs();
        rel.created_at = now;
        rel.updated_at = now;
        rel
    })
}

/// 保存关系
pub fn save_relationship(rel: &Relationship) {
    let mut store = load_rel_store();
    store.relationships.insert(rel.user_id, rel.clone());
    save_rel_store(&store);
}

/// 记录一次交互，更新关系动力学
pub fn record_interaction(user_id: u64, positive: bool) {
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
        // 信任缓慢建立
        rel.trust = (rel.trust + 0.01).min(1.0);
    } else {
        // 信任崩塌更快
        rel.trust = (rel.trust - 0.05).max(0.0);
    }

    // 2. 亲密度更新（天花板效应）
    let intimacy_gain = if positive { 0.02 } else { -0.01 };
    // 天花板效应：亲密度越高，增长越慢
    let ceiling_factor = 1.0 - rel.intimacy * 0.8;
    rel.intimacy = (rel.intimacy + intimacy_gain * ceiling_factor).clamp(0.0, 1.0);

    // 3. 好感度更新
    rel.affection = (rel.affection + if positive { 0.015 } else { -0.03 }).clamp(0.0, 1.0);

    // 4. 默契度更新（互动越多默契越高）
    if rel.interaction_count > 10 {
        rel.rapport = (rel.rapport + 0.005).min(1.0);
    }

    // 5. 关系类型自动升级
    rel.relationship_type = if rel.intimacy > 0.8 && rel.trust > 0.7 {
        RelationshipType::Confidant
    } else if rel.intimacy > 0.6 {
        RelationshipType::Close
    } else if rel.intimacy > 0.3 || rel.interaction_count > 20 {
        RelationshipType::Regular
    } else if rel.interaction_count > 3 {
        RelationshipType::Acquaintance
    } else {
        RelationshipType::Stranger
    };

    rel.updated_at = now;

    debug!(
        user_id,
        trust = rel.trust,
        intimacy = rel.intimacy,
        rapport = rel.rapport,
        interactions = rel.interaction_count,
        rel_type = ?rel.relationship_type,
        "relationship: updated"
    );

    save_relationship(&rel);
}

/// 缺席冷却：长时间不互动后更新关系
pub fn apply_absence_cooling(user_id: u64) {
    let mut rel = get_relationship(user_id);
    let now = crate::util::now_secs();
    let elapsed_days = now.saturating_sub(rel.last_interaction) as f32 / 86400.0;

    if elapsed_days < 1.0 {
        return; // 少于1天不冷却
    }

    // 亲密度下降
    let cooling = elapsed_days * rel.absence_cooling_rate;
    rel.intimacy = (rel.intimacy - cooling).max(0.0);

    // 亲和度也轻微下降
    rel.affection = (rel.affection - cooling * 0.5).max(0.1);

    // 信任几乎不变（信任不会因为不见面就消失）
    rel.trust = (rel.trust - cooling * 0.05).max(0.1);

    rel.updated_at = now;
    save_relationship(&rel);

    debug!(
        user_id,
        elapsed_days,
        intimacy = rel.intimacy,
        "relationship: absence cooling applied"
    );
}

/// 记录共享记忆或 inside joke
pub fn record_shared_memory(
    user_id: u64,
    summary: &str,
    emotional_significance: f32,
    inside_joke: Option<&str>,
) {
    let mut rel = get_relationship(user_id);
    let now = crate::util::now_secs();

    // 检查是否已存在类似记忆（去重）
    let exists = rel.shared_memories.iter().any(|m| {
        m.summary == summary
    });

    if !exists {
        rel.shared_memories.push(SharedMemory {
            summary: summary.to_string(),
            emotional_significance,
            inside_joke: inside_joke.map(|s| s.to_string()),
            callback_count: 0,
            created_at: now,
        });

        // 共享记忆增加亲密度
        rel.intimacy = (rel.intimacy + 0.03).min(1.0);
        rel.updated_at = now;

        save_relationship(&rel);
        debug!(user_id, summary, "relationship: shared memory recorded");
    }
}

/// 回调共享记忆（增加引用计数）
pub fn callback_shared_memory(user_id: u64, memory_summary: &str) {
    let mut rel = get_relationship(user_id);
    let mut found = false;
    let mut new_count = 0u32;
    if let Some(mem) = rel.shared_memories.iter_mut()
        .find(|m| m.summary.contains(memory_summary) || memory_summary.contains(&m.summary))
    {
        mem.callback_count += 1;
        new_count = mem.callback_count;
        found = true;
    }
    if found {
        rel.updated_at = crate::util::now_secs();
        save_relationship(&rel);
        debug!(user_id, summary = %memory_summary, count = new_count, "relationship: callback recorded");
    }
}

/// 获取共享记忆上下文（按重要性排序）
pub fn get_shared_memories_context(user_id: u64, max_count: usize) -> String {
    let rel = get_relationship(user_id);
    if rel.shared_memories.is_empty() {
        return String::new();
    }

    let mut sorted = rel.shared_memories.clone();
    sorted.sort_by(|a, b| {
        let a_score = a.emotional_significance + a.callback_count as f32 * 0.1;
        let b_score = b.emotional_significance + b.callback_count as f32 * 0.1;
        b_score.partial_cmp(&a_score).unwrap()
    });

    let lines: Vec<String> = sorted.iter().take(max_count).map(|m| {
        let joke = if let Some(ref joke) = m.inside_joke {
            format!(" (内部梗: {})", joke)
        } else {
            String::new()
        };
        format!("- {}{} [重要性:{:.1}]", m.summary, joke, m.emotional_significance)
    }).collect();

    format!("# 你和这个人的共同回忆\n{}", lines.join("\n"))
}

/// 获取亲密度足够的共享记忆中的 inside jokes
pub fn get_inside_jokes_context(user_id: u64) -> String {
    let rel = get_relationship(user_id);
    let jokes: Vec<&str> = rel.shared_memories.iter()
        .filter_map(|m| m.inside_joke.as_deref())
        .collect();

    if jokes.is_empty() {
        return String::new();
    }

    format!("# 你和这个人之间的内部梗\n{}",
        jokes.iter().map(|j| format!("- {}", j)).collect::<Vec<_>>().join("\n"))
}

/// 获取关系自然语言上下文（用于 prompt 注入）
pub fn get_relationship_context(user_id: u64) -> String {
    let rel = get_relationship(user_id);
    let mut lines = Vec::new();

    match rel.relationship_type {
        RelationshipType::Stranger => {
            if rel.interaction_count > 0 {
                lines.push("- 你们刚认识不久，保持礼貌友好的态度".to_string());
            }
        }
        RelationshipType::Acquaintance => {
            lines.push("- 你们算认识的人了，可以稍微随意一些".to_string());
        }
        RelationshipType::Regular => {
            lines.push("- 你们已经是常聊的朋友了，可以更自然随意地交流".to_string());
        }
        RelationshipType::Close => {
            lines.push("- 你们关系很亲近，可以放心地表达自己的想法和情绪".to_string());
            lines.push("- 可以更主动地关心对方，分享自己的日常".to_string());
        }
        RelationshipType::Confidant => {
            lines.push("- 你们是知己，几乎无话不谈。可以在对方面前展示真实的一面".to_string());
            lines.push("- 你们已经形成了默契，不需要过多解释就能理解彼此".to_string());
        }
        RelationshipType::Antagonistic => {
            lines.push("- 你们的关系有些紧张，保持礼貌但可以保持距离".to_string());
        }
        RelationshipType::Admiring => {
            lines.push("- 你对这个人有好感，会更温柔、更主动".to_string());
        }
    }

    // 信任度提示
    if rel.trust > 0.8 {
        lines.push("- 你非常信任这个人，愿意分享更多个人想法".to_string());
    } else if rel.trust < 0.2 {
        lines.push("- 你对这个人的信任度较低，对Ta的话会保持一定警惕".to_string());
    }

    // 默契度提示
    if rel.rapport > 0.7 {
        lines.push("- 你们之间很有默契，一个眼神就能懂对方的意思".to_string());
    }

    // 印象
    if !rel.impression.is_empty() {
        lines.push(format!("- 你对Ta的印象: {}", rel.impression));
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("# 你和这个人的关系\n{}", lines.join("\n"))
    }
}

/// 设置对用户的印象
pub fn set_impression(user_id: u64, impression: &str) {
    let mut rel = get_relationship(user_id);
    rel.impression = impression.to_string();
    rel.updated_at = crate::util::now_secs();
    save_relationship(&rel);
}

/// 更新沟通风格偏好
pub fn update_communication_style(user_id: u64, style: CommStyle) {
    let mut rel = get_relationship(user_id);
    rel.communication_style = style;
    rel.updated_at = crate::util::now_secs();
    save_relationship(&rel);
}
