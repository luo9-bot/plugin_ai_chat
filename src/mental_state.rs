use serde::{Deserialize, Serialize};
use std::fs;
use std::time::SystemTime;
use tracing::debug;

use crate::config;
use crate::emotion::EmotionType;

// ── 担忧 (Concerns) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConcernCategory {
    Social,    // 社交相关
    Task,      // 任务相关
    Emotional, // 情感相关
    Self_,     // 自我相关
}

impl ConcernCategory {
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

// ── 考量 (Deliberations) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliberation {
    pub content: String,
    pub source: String,
    pub strength: f32,
    pub created: u64,
    pub last_reinforced: u64,
}

// ── 缺陷 (Defects) ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefectType {
    Typo,         // 打错字
    AbsentMinded, // 走神/忘事
    ShortReply,   // 敷衍
    Hesitation,   // 犹豫
    Tangent,      // 跑题
}

// ── 持久化存储 ────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MentalStateStore {
    pub concerns: Vec<Concern>,
    pub deliberations: Vec<Deliberation>,
}

fn now_secs() -> u64 {
    crate::util::now_secs()
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("mental_state.json")
}

impl MentalStateStore {
    fn load() -> Self {
        let path = store_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = store_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}

/// 初始化时调用：返回心理状态条数
pub fn load_count() -> usize {
    let store = MentalStateStore::load();
    store.concerns.len() + store.deliberations.len()
}

// ── 文本去重 ─────────────────────────────────────────────

fn normalize_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || (*c >= '\u{4e00}' && *c <= '\u{9fff}'))
        .collect::<String>()
        .to_lowercase()
}

fn is_similar(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let shorter = a.len().min(b.len());
    if shorter >= 4 {
        let (s, l) = if a.len() <= b.len() { (a, b) } else { (b, a) };
        if l.contains(s) {
            return true;
        }
    }
    false
}

// ── 担忧操作 ─────────────────────────────────────────────

/// 添加担忧，自动去重。重复担忧会增强强度。
pub fn add_concern(content: &str, category: &str, trigger_user: u64, trigger_group: u64) {
    let mut store = MentalStateStore::load();
    let cat = ConcernCategory::from_str(category);
    let now = now_secs();
    let normalized = normalize_text(content);

    // 去重：检查已有担忧是否相似
    for concern in &mut store.concerns {
        let existing_norm = normalize_text(&concern.content);
        if is_similar(&normalized, &existing_norm) {
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
    if store.concerns.len() >= max {
        if let Some(weakest_idx) = store.concerns.iter()
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

// ── 考量操作 ─────────────────────────────────────────────

/// 添加考量，自动去重。重复考量会增强强度。
pub fn add_deliberation(content: &str, source: &str) {
    let mut store = MentalStateStore::load();
    let now = now_secs();
    let normalized = normalize_text(content);

    for delib in &mut store.deliberations {
        let existing_norm = normalize_text(&delib.content);
        if is_similar(&normalized, &existing_norm) {
            delib.strength = (delib.strength + 0.1).min(1.0);
            delib.last_reinforced = now;
            debug!(content, "mental_state: reinforced existing deliberation");
            store.save();
            return;
        }
    }

    let max = config::get().mental_state.deliberations_max;
    if store.deliberations.len() >= max {
        if let Some(weakest_idx) = store.deliberations.iter()
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

// ── 衰减 ─────────────────────────────────────────────────

/// 衰减所有担忧的强度，移除过低的
pub fn decay_concerns(rate_per_hour: f32) {
    let mut store = MentalStateStore::load();
    if store.concerns.is_empty() {
        return;
    }
    let now = now_secs();
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

/// 衰减所有考量的强度，移除过低的
pub fn decay_deliberations(rate_per_hour: f32) {
    let mut store = MentalStateStore::load();
    if store.deliberations.is_empty() {
        return;
    }
    let now = now_secs();
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

// ── Prompt 注入 ──────────────────────────────────────────

/// 构建心理状态上下文，注入到 system prompt
pub fn get_prompt_context(max_concerns: usize, max_deliberations: usize) -> String {
    let store = MentalStateStore::load();
    let mut parts = Vec::new();

    // 担忧
    if !store.concerns.is_empty() {
        let mut concerns: Vec<&Concern> = store.concerns.iter().collect();
        concerns.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        let lines: Vec<String> = concerns.iter().take(max_concerns).map(|c| {
            format!("- {}（{}）", c.content, c.category.label())
        }).collect();
        parts.push(format!("# 你的担忧\n{}", lines.join("\n")));
    }

    // 考量
    if !store.deliberations.is_empty() {
        let mut delibs: Vec<&Deliberation> = store.deliberations.iter().collect();
        delibs.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        let lines: Vec<String> = delibs.iter().take(max_deliberations).map(|d| {
            format!("- {}", d.content)
        }).collect();
        parts.push(format!("# 你的考量\n{}", lines.join("\n")));
    }

    parts.join("\n\n")
}

// ── 缺陷系统 ─────────────────────────────────────────────

/// 基于情绪状态和随机概率检查是否触发缺陷
pub fn check_defect(emotion: EmotionType, intensity: f32, base_probability: f32) -> Option<DefectType> {
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

// ── 对话后生成 ────────────────────────────────────────────

const MENTAL_STATE_PROMPT: &str = r#"你是有身份和人设的（见上方"你的身份"）。审视以下对话记录，产生你作为这个角色的内心反应。

任务1 - 担忧: 如果对话中出现了让你担心的事情，生成担忧条目。
- 社交担忧: 担心用户心情不好、担心关系变差
- 任务担忧: 记挂着没回答好的问题、担心遗漏了什么
- 情感担忧: 在意自己说错话了、担心伤害了对方
- 自我担忧: 觉得自己状态不好、能力不足
- 如果没有值得担忧的，返回空数组

任务2 - 要考量: 如果对话中出现了值得记住的行事准则或人际洞察，生成考量条目。
- 例如: "这个人说话直接，别往心里去"、"这个话题敏感，注意分寸"
- 如果没有新的考量，返回空数组

规则:
- 基于你的人设来判断什么是你真正在意的
- 不要过度担忧，只提取确实值得注意的
- 内容简短，一两句话
- 不要暴露你是 AI"#;

/// 从对话上下文中生成担忧和考量
pub fn generate_from_conversation(group_id: u64, messages_text: &str) {
    let mut context_parts = Vec::new();

    let user_prompt = config::prompt();
    if !user_prompt.is_empty() {
        context_parts.push(format!("# 你的身份\n{}", user_prompt));
    }

    let personality = crate::personality::get_prompt_context();
    if !personality.is_empty() {
        context_parts.push(personality);
    }

    // 注入现有担忧/考量，避免重复
    let existing = get_prompt_context(
        config::get().mental_state.concerns_max,
        config::get().mental_state.deliberations_max,
    );
    if !existing.is_empty() {
        context_parts.push(existing);
    }

    context_parts.push(format!("# 对话记录\n{}", messages_text));
    let full_context = context_parts.join("\n\n");

    match crate::ai::analyze_with_tools(
        MENTAL_STATE_PROMPT,
        &full_context,
        &[crate::ai::mental_state_generate_tool()],
        None,
    ) {
        Ok(parsed) => {
            let mut count = 0;

            // 解析担忧
            if let Some(concerns) = parsed.get("concerns").and_then(|v| v.as_array()) {
                for item in concerns {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let category = item.get("category").and_then(|v| v.as_str()).unwrap_or("social");
                    if !content.is_empty() {
                        add_concern(content, category, 0, group_id);
                        count += 1;
                    }
                }
            }

            // 解析考量
            if let Some(deliberations) = parsed.get("deliberations").and_then(|v| v.as_array()) {
                for item in deliberations {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if !content.is_empty() {
                        add_deliberation(content, "conversation");
                        count += 1;
                    }
                }
            }

            if count > 0 {
                debug!(group_id, count, "mental_state: generated from conversation");
            }
        }
        Err(e) => {
            debug!(error = %e, "mental_state: generate_from_conversation AI error");
        }
    }
}
