//! 叙事自我系统
//!
//! 将离散的自我记忆升级为连贯的自我叙事。
//! 维护：
//! - 核心自我认知（缓慢变化的身份感）
//! - 当前叙事线（"最近我在……"）
//! - 内在价值观（我真正在意什么）
//! - 时间线事件（我的"经历"）

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

// ── 数据结构 ────────────────────────────────────────────────────

/// 叙事自我状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeSelf {
    /// 核心自我认知（一句话描述"我是谁"，缓慢变化）
    pub core_identity: String,
    /// 当前叙事线（"最近我在……"的故事）
    pub current_narrative: String,
    /// 内在价值观（我真正在意的事，不是配置，是内生的）
    pub values: Vec<Value>,
    /// 持续关注的事
    pub ongoing_concerns: Vec<OngoingConcern>,
    /// 时间线事件（我的"经历"）
    pub timeline: Vec<TimelineEvent>,
    /// 上次更新时间
    pub last_update: u64,
    /// 上次叙事刷新时间
    pub last_narrative_refresh: u64,
}

/// 内在价值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Value {
    /// 价值描述
    pub content: String,
    /// 强度 (0.0-1.0)
    pub strength: f32,
    /// 来源（从什么经历中形成的）
    pub source: Option<String>,
    /// 形成时间
    pub created_at: u64,
}

/// 持续关注
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OngoingConcern {
    /// 关注内容
    pub content: String,
    /// 关注强度 (0.0-1.0)
    pub strength: f32,
    /// 关注类型
    pub concern_type: ConcernType,
    /// 开始时间
    pub started_at: u64,
    /// 上次强化时间
    pub last_reinforced: u64,
}

/// 关注类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConcernType {
    /// 对某人的关注
    Person,
    /// 对某件事的关注
    Event,
    /// 对某个话题的关注
    Topic,
    /// 对自身状态的关注
    SelfState,
}

/// 时间线事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// 事件描述
    pub content: String,
    /// 事件类型
    pub event_type: TimelineEventType,
    /// 情感重要性 (0.0-1.0)
    pub emotional_significance: f32,
    /// 相关用户（如果有）
    pub related_user: Option<u64>,
    /// 发生时间
    pub created_at: u64,
    /// 是否已被整合到叙事中
    pub integrated: bool,
}

/// 时间线事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimelineEventType {
    /// 对话事件
    Conversation,
    /// 情感事件
    Emotional,
    /// 学习事件（学到了新东西）
    Learning,
    /// 关系事件（关系变化）
    Relationship,
    /// 自我认知事件（对自己有了新认识）
    SelfDiscovery,
    /// 日常事件
    Daily,
}

// ── 持久化 ──────────────────────────────────────────────────────

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("narrative_self.json")
}

impl NarrativeSelf {
    pub fn load() -> Self {
        let path = store_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = store_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}

impl Default for NarrativeSelf {
    fn default() -> Self {
        Self {
            core_identity: String::new(),
            current_narrative: String::new(),
            values: Vec::new(),
            ongoing_concerns: Vec::new(),
            timeline: Vec::new(),
            last_update: 0,
            last_narrative_refresh: 0,
        }
    }
}

// ── 公开接口 ────────────────────────────────────────────────────

/// 添加时间线事件
pub fn add_timeline_event(
    content: &str,
    event_type: TimelineEventType,
    emotional_significance: f32,
    related_user: Option<u64>,
) {
    let mut ns = NarrativeSelf::load();

    // 去重：检查是否有高度相似的最近事件
    let is_dup = ns.timeline.iter().rev().take(5).any(|e| {
        e.content == content || is_content_similar(&e.content, content)
    });
    if is_dup {
        debug!(content, "narrative_self: skipped duplicate timeline event");
        return;
    }

    ns.timeline.push(TimelineEvent {
        content: content.to_string(),
        event_type,
        emotional_significance: emotional_significance.clamp(0.0, 1.0),
        related_user,
        created_at: crate::util::now_secs(),
        integrated: false,
    });

    // 保留最近100条时间线事件
    if ns.timeline.len() > 100 {
        ns.timeline.remove(0);
    }

    ns.last_update = crate::util::now_secs();
    ns.save();
    debug!(content, "narrative_self: timeline event added");
}

/// 添加内在价值
pub fn add_value(content: &str, strength: f32, source: Option<&str>) {
    let mut ns = NarrativeSelf::load();

    // 去重
    if ns.values.iter().any(|v| v.content == content) {
        return;
    }

    ns.values.push(Value {
        content: content.to_string(),
        strength: strength.clamp(0.0, 1.0),
        source: source.map(|s| s.to_string()),
        created_at: crate::util::now_secs(),
    });

    // 保留最多20个价值
    if ns.values.len() > 20 {
        // 移除最弱的
        if let Some(min_idx) = ns.values.iter().enumerate()
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap())
            .map(|(i, _)| i) {
            ns.values.remove(min_idx);
        }
    }

    ns.last_update = crate::util::now_secs();
    ns.save();
    debug!(content, "narrative_self: value added");
}

/// 添加持续关注
pub fn add_ongoing_concern(content: &str, concern_type: ConcernType, strength: f32) {
    let mut ns = NarrativeSelf::load();
    let now = crate::util::now_secs();

    // 如果已有类似关注，强化它
    if let Some(existing) = ns.ongoing_concerns.iter_mut()
        .find(|c| c.content == content || is_content_similar(&c.content, content))
    {
        existing.strength = (existing.strength + 0.1).min(1.0);
        existing.last_reinforced = now;
        ns.last_update = now;
        ns.save();
        return;
    }

    ns.ongoing_concerns.push(OngoingConcern {
        content: content.to_string(),
        strength: strength.clamp(0.0, 1.0),
        concern_type,
        started_at: now,
        last_reinforced: now,
    });

    // 保留最多15个关注
    if ns.ongoing_concerns.len() > 15 {
        // 移除最弱的
        if let Some(min_idx) = ns.ongoing_concerns.iter().enumerate()
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap())
            .map(|(i, _)| i) {
            ns.ongoing_concerns.remove(min_idx);
        }
    }

    ns.last_update = now;
    ns.save();
    debug!(content, "narrative_self: ongoing concern added");
}

/// 更新核心自我认知
pub fn update_core_identity(identity: &str) {
    let mut ns = NarrativeSelf::load();
    if identity.is_empty() || identity == ns.core_identity {
        return;
    }
    ns.core_identity = identity.to_string();
    ns.last_update = crate::util::now_secs();
    ns.save();
    debug!(identity, "narrative_self: core identity updated");
}

/// 更新当前叙事线
pub fn update_current_narrative(narrative: &str) {
    let mut ns = NarrativeSelf::load();
    if narrative.is_empty() {
        return;
    }
    ns.current_narrative = narrative.to_string();
    ns.last_narrative_refresh = crate::util::now_secs();
    ns.last_update = crate::util::now_secs();
    ns.save();
    debug!("narrative_self: current narrative updated");
}

/// 衰减关注强度（每天调用一次）
pub fn decay_concerns() {
    let mut ns = NarrativeSelf::load();
    let now = crate::util::now_secs();
    let mut changed = false;

    for concern in &mut ns.ongoing_concerns {
        let elapsed_days = now.saturating_sub(concern.last_reinforced) as f32 / 86400.0;
        if elapsed_days > 1.0 {
            let decay = 0.05 * elapsed_days;
            concern.strength = (concern.strength - decay).max(0.0);
            changed = true;
        }
    }

    // 移除已衰减到零的关注
    let before = ns.ongoing_concerns.len();
    ns.ongoing_concerns.retain(|c| c.strength > 0.01);
    if ns.ongoing_concerns.len() != before {
        changed = true;
    }

    // 价值也会缓慢衰减
    for value in &mut ns.values {
        let elapsed_days = now.saturating_sub(value.created_at) as f32 / 86400.0;
        if elapsed_days > 30.0 {
            value.strength = (value.strength - 0.001 * elapsed_days).max(0.1);
            changed = true;
        }
    }

    if changed {
        ns.last_update = now;
        ns.save();
    }
}

/// 获取叙事自我上下文（注入到 system prompt）
pub fn get_narrative_context() -> String {
    let ns = NarrativeSelf::load();
    let mut parts = Vec::new();

    // 核心自我认知
    if !ns.core_identity.is_empty() {
        parts.push(format!("# 你的自我认知\n{}", ns.core_identity));
    }

    // 当前叙事线
    if !ns.current_narrative.is_empty() {
        parts.push(format!("# 你最近的状态\n{}", ns.current_narrative));
    }

    // 内在价值观（取最强的3个）
    if !ns.values.is_empty() {
        let mut sorted = ns.values.clone();
        sorted.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap());
        let lines: Vec<String> = sorted.iter().take(3)
            .map(|v| format!("- {}（{:.0}%）", v.content, v.strength * 100.0))
            .collect();
        parts.push(format!("# 你在乎的事\n{}", lines.join("\n")));
    }

    // 持续关注（取最强的3个）
    if !ns.ongoing_concerns.is_empty() {
        let mut sorted = ns.ongoing_concerns.clone();
        sorted.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap());
        let lines: Vec<String> = sorted.iter().take(3)
            .map(|c| format!("- {}", c.content))
            .collect();
        parts.push(format!("# 你一直在关注的事\n{}", lines.join("\n")));
    }

    // 最近时间线（取最近5条未整合的）
    let recent: Vec<&TimelineEvent> = ns.timeline.iter()
        .rev()
        .filter(|e| !e.integrated)
        .take(5)
        .collect();
    if !recent.is_empty() {
        let lines: Vec<String> = recent.iter()
            .map(|e| format!("- {}", e.content))
            .collect();
        parts.push(format!("# 你最近的经历\n{}", lines.join("\n")));
    }

    parts.join("\n\n")
}

/// 获取叙事自我摘要（用于 WebUI）
pub fn get_narrative_summary() -> serde_json::Value {
    let ns = NarrativeSelf::load();
    serde_json::json!({
        "core_identity": ns.core_identity,
        "current_narrative": ns.current_narrative,
        "values": ns.values.iter().map(|v| serde_json::json!({
            "content": v.content,
            "strength": v.strength,
            "source": v.source,
            "created_at": v.created_at,
        })).collect::<Vec<_>>(),
        "ongoing_concerns": ns.ongoing_concerns.iter().map(|c| serde_json::json!({
            "content": c.content,
            "strength": c.strength,
            "concern_type": format!("{:?}", c.concern_type),
            "started_at": c.started_at,
            "last_reinforced": c.last_reinforced,
        })).collect::<Vec<_>>(),
        "timeline": ns.timeline.iter().rev().take(20).map(|e| serde_json::json!({
            "content": e.content,
            "event_type": format!("{:?}", e.event_type),
            "emotional_significance": e.emotional_significance,
            "related_user": e.related_user,
            "created_at": e.created_at,
            "integrated": e.integrated,
        })).collect::<Vec<_>>(),
        "stats": {
            "values_count": ns.values.len(),
            "concerns_count": ns.ongoing_concerns.len(),
            "timeline_count": ns.timeline.len(),
            "last_update": ns.last_update,
            "last_narrative_refresh": ns.last_narrative_refresh,
        },
    })
}

/// 从对话后分析中提取叙事元素
pub fn extract_from_conversation(_group_id: u64, messages_text: &str) {
    let _user_prompt = config::prompt();
    let prompt = format!(
        "你是{}。分析以下对话，提取可能影响你自我认知的元素。\n\n\
         对话记录：\n{}\n\n\
         请识别：\n\
         1. 是否有值得记录的时间线事件（情感强烈的对话、学到新东西、关系变化）\n\
         2. 是否有新的内在价值被触发（你发现自己在意什么）\n\
         3. 是否有新的持续关注点\n\n\
         返回 JSON：\n\
         {{\n\
           \"timeline_events\": [{{\"content\": \"...\", \"significance\": 0.0-1.0}}],\n\
           \"values\": [{{\"content\": \"...\", \"strength\": 0.0-1.0}}],\n\
           \"concerns\": [{{\"content\": \"...\", \"type\": \"Person|Event|Topic|SelfState\"}}]\n\
         }}",
        crate::config::get().bot_name,
        messages_text,
    );

    match crate::ai::analyze("", &prompt) {
        Ok(response) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                // 提取时间线事件
                if let Some(events) = parsed.get("timeline_events").and_then(|v| v.as_array()) {
                    for event in events {
                        let content = event.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let significance = event.get("significance").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                        if !content.is_empty() {
                            add_timeline_event(content, TimelineEventType::Conversation, significance, None);
                        }
                    }
                }

                // 提取价值
                if let Some(values) = parsed.get("values").and_then(|v| v.as_array()) {
                    for val in values {
                        let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let strength = val.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                        if !content.is_empty() {
                            add_value(content, strength, Some("conversation"));
                        }
                    }
                }

                // 提取关注
                if let Some(concerns) = parsed.get("concerns").and_then(|v| v.as_array()) {
                    for concern in concerns {
                        let content = concern.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let type_str = concern.get("type").and_then(|v| v.as_str()).unwrap_or("Topic");
                        let concern_type = match type_str {
                            "Person" => ConcernType::Person,
                            "Event" => ConcernType::Event,
                            "SelfState" => ConcernType::SelfState,
                            _ => ConcernType::Topic,
                        };
                        if !content.is_empty() {
                            add_ongoing_concern(content, concern_type, 0.5);
                        }
                    }
                }
            }
        }
        Err(e) => {
            debug!(error = %e, "narrative_self: extract_from_conversation failed");
        }
    }
}

// ── 工具函数 ────────────────────────────────────────────────────

fn is_content_similar(a: &str, b: &str) -> bool {
    if a == b { return true; }
    if a.is_empty() || b.is_empty() { return false; }
    // 简单的包含检查
    if a.len() > 6 && b.len() > 6 {
        if a.contains(b) || b.contains(a) { return true; }
    }
    false
}
