//! 内心独白系统
//!
//! 定时产生"内心想法"，不直接发送，但影响后续行为和情绪。
//! 每5-15分钟随机触发，调用LLM生成一句内心独白。
//! 想法随时间衰减（faded），但有小概率被重新想起。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

/// 内心独白
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerThought {
    /// 独白内容
    pub content: String,
    /// 创建时间
    pub timestamp: u64,
    /// 对情绪的影响 (-1.0 ~ 1.0)
    pub emotional_impact: f32,
    /// 是否可能导致行动 (0.0 ~ 1.0)
    pub action_potential: f32,
    /// 是否已淡忘
    pub faded: bool,
    /// 被重新想起的次数
    pub recall_count: u32,
}

/// 内心独白存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InnerThoughtStore {
    pub thoughts: Vec<InnerThought>,
    /// 上次生成时间
    pub last_generation: u64,
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("inner_thoughts.json")
}

impl InnerThoughtStore {
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

/// 尝试生成内心独白（定时触发）
///
/// 每5-15分钟随机触发一次
pub fn try_generate() -> Option<InnerThought> {
    let cfg = config::get();
    let h = &cfg.humanity;

    if !h.inner_thought_enabled {
        return None;
    }

    let mut store = InnerThoughtStore::load();
    let now = crate::util::now_secs();

    // 检查是否到了生成时间
    let interval_min = h.inner_thought_interval_min;
    let interval_max = h.inner_thought_interval_max;
    let elapsed = now.saturating_sub(store.last_generation);

    if elapsed < interval_min {
        return None;
    }

    // 在区间内随机触发
    let trigger_window = interval_max.saturating_sub(interval_min);
    if elapsed < interval_max {
        let random_factor = fastrand::f32();
        let progress = (elapsed - interval_min) as f32 / trigger_window as f32;
        if random_factor > progress * 2.0 {
            return None; // 越接近最大间隔越可能触发
        }
    }

    // 构建生成提示
    let context = build_generation_context();
    let thought = generate_inner_thought(&context);

    if let Some(ref t) = thought {
        // 去重：检查是否与最近的想法高度相似
        let normalized_new = normalize_thought(&t.content);
        let is_dup = store.thoughts.iter().rev().take(10).any(|existing| {
            let normalized_existing = normalize_thought(&existing.content);
            is_similar(&normalized_new, &normalized_existing)
        });
        if is_dup {
            debug!(content = %t.content, "inner_thought: skipped duplicate thought");
            store.last_generation = now;
            store.save();
            return None;
        }

        store.last_generation = now;
        store.thoughts.push(t.clone());
        // 保持最近50条
        if store.thoughts.len() > 50 {
            store.thoughts.remove(0);
        }
        store.save();

        // 可能触发情绪变化
        if t.emotional_impact.abs() > 0.3 {
            let emo_type = if t.emotional_impact > 0.0 {
                crate::emotion::EmotionType::Thinking
            } else {
                crate::emotion::EmotionType::Thinking
            };
            let mut emo_state = crate::emotion::get_state(0);
            emo_state.update_emotional_dynamics(
                Some((&emo_type, t.emotional_impact.abs() * 0.3, "内心独白", crate::emotion::TriggerType::InnerThought)),
                0.0,
            );
            crate::emotion::update_state(0, emo_state);
        }
    }

    thought
}

/// 构建生成内心独白的上下文
fn build_generation_context() -> String {
    let mut parts = Vec::new();

    // 当前情绪
    let emo = crate::emotion::get_state(0);
    parts.push(format!("当前情绪：{}", emo.describe_detailed()));

    // 最近自我记忆
    let self_mem = super::store::SelfMemoryStore::load();
    let recent: Vec<&str> = self_mem.thoughts.iter()
        .rev()
        .take(5)
        .map(|t| t.content.as_str())
        .collect();
    if !recent.is_empty() {
        parts.push(format!("最近的想法：{}", recent.join("；")));
    }

    // 活跃对话
    let active_users: Vec<u64> = crate::read_shared_state(|s| {
        s.active_users.iter().copied().collect()
    });
    if !active_users.is_empty() {
        parts.push(format!("正在和{}个用户对话", active_users.len()));
    }

    // 人员印象
    let person_ctx = crate::person_info::get_person_context(
        active_users.first().copied().unwrap_or(0)
    );
    if !person_ctx.is_empty() {
        parts.push(person_ctx);
    }

    parts.join("\n")
}

/// 调用 LLM 生成内心独白
fn generate_inner_thought(context: &str) -> Option<InnerThought> {
    let prompt = format!(
        "基于以下上下文，产生一个内心独白（一句话，不超过50字）。\n\
         内心独白可以是：\n\
         - 对当前处境的感受\n\
         - 突然想到的念头\n\
         - 对某个人的看法\n\
         - 一个随机的想法\n\n\
         {}\n\n\
         只返回内心独白的内容（一句话），并在最后用方括号标注对情绪的影响\
         [-1.0到1.0，负数=负面，正数=正面]和行动潜力[0.0-1.0]。\
         格式：「独白内容 [情绪影响] [行动潜力]」",
        context
    );

    match crate::ai::analyze("", &prompt) {
        Ok(response) => {
            let text = response.trim();
            // 提取标记
            let (content, emotional_impact, action_potential) = parse_thought_tags(text);

            if content.is_empty() || content.len() > 100 {
                return None;
            }

            let now = crate::util::now_secs();
            debug!(content, emotional_impact, action_potential, "inner_thought: generated");

            Some(InnerThought {
                content,
                timestamp: now,
                emotional_impact,
                action_potential,
                faded: false,
                recall_count: 0,
            })
        }
        Err(e) => {
            debug!(error = %e, "inner_thought: generation failed");
            None
        }
    }
}

/// 解析内心独白中的标签
fn parse_thought_tags(text: &str) -> (String, f32, f32) {
    let mut content = text.to_string();
    let mut emotional_impact = 0.0f32;
    let mut action_potential = 0.0f32;

    // 尝试提取最后的 [数字] 模式
    if let Some(last_bracket) = text.rfind(']') {
        if let Some(open_bracket) = text[..last_bracket].rfind('[') {
            let tag = &text[open_bracket + 1..last_bracket];
            let parts: Vec<&str> = tag.split_whitespace().collect();
            if parts.len() == 2 {
                if let Ok(v) = parts[0].parse::<f32>() {
                    action_potential = v.clamp(0.0, 1.0);
                }
                if let Ok(v) = parts[1].parse::<f32>() {
                    emotional_impact = v.clamp(-1.0, 1.0);
                }
                // 也有可能是 [情绪影响] [行动潜力] 或反过来
            } else if parts.len() == 1 {
                if let Ok(v) = parts[0].parse::<f32>() {
                    emotional_impact = v.clamp(-1.0, 1.0);
                }
            }
            content = text[..open_bracket].trim().to_string();
        }
    }

    (content, emotional_impact, action_potential)
}

/// 获取活跃的内心独白（未淡忘的）
pub fn get_active_thoughts(max_count: usize) -> Vec<InnerThought> {
    let store = InnerThoughtStore::load();
    let now = crate::util::now_secs();

    let mut active: Vec<InnerThought> = store.thoughts.iter()
        .filter(|t| {
            if t.faded {
                // 淡忘的想法有小概率被想起
                let elapsed_hours = now.saturating_sub(t.timestamp) as f32 / 3600.0;
                let recall_prob = 0.05 * (-elapsed_hours / 24.0).exp();
                fastrand::f32() < recall_prob
            } else {
                true
            }
        })
        .cloned()
        .collect();

    active.sort_by_key(|t| std::cmp::Reverse(t.timestamp));
    active.truncate(max_count);
    active
}

/// 衰减内心独白（超过2小时的标记为 faded）
pub fn decay_thoughts() {
    let mut store = InnerThoughtStore::load();
    let now = crate::util::now_secs();
    let mut changed = false;

    for thought in &mut store.thoughts {
        if !thought.faded && now.saturating_sub(thought.timestamp) > 7200 {
            thought.faded = true;
            changed = true;
        }
    }

    // 清理超过7天的
    let before = store.thoughts.len();
    store.thoughts.retain(|t| now.saturating_sub(t.timestamp) < 604800);
    if store.thoughts.len() != before {
        changed = true;
    }

    if changed {
        store.save();
    }
}

/// 获取内心独白上下文（注入到 prompt）
pub fn get_inner_thought_context(max_count: usize) -> String {
    let thoughts = get_active_thoughts(max_count);
    if thoughts.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = thoughts.iter().map(|t| {
        let recalled = if t.recall_count > 0 {
            " [回想起]"
        } else {
            ""
        };
        format!("- {}{}", t.content, recalled)
    }).collect();

    format!("# 你此刻的内心想法\n{}", lines.join("\n"))
}

/// 重新想起一个淡忘的想法
pub fn recall_thought(content_hint: &str) -> Option<InnerThought> {
    let mut store = InnerThoughtStore::load();
    let mut result = None;
    if let Some(thought) = store.thoughts.iter_mut()
        .find(|t| t.faded && t.content.contains(content_hint))
    {
        thought.faded = false;
        thought.recall_count += 1;
        result = Some(thought.clone());
    }
    if result.is_some() {
        store.save();
        debug!(content = %result.as_ref().unwrap().content, "inner_thought: recalled");
    }
    result
}

/// 标准化想法文本用于比较
fn normalize_thought(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || (*c >= '\u{4e00}' && *c <= '\u{9fff}'))
        .collect::<String>()
        .to_lowercase()
}

/// 检查两个标准化后的想法是否高度相似
fn is_similar(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let shorter_len = a_chars.len().min(b_chars.len());

    // 子串包含
    let (shorter, longer) = if a_chars.len() <= b_chars.len() { (&a_chars, &b_chars) } else { (&b_chars, &a_chars) };
    if longer.len() >= 6 && longer.windows(shorter.len()).any(|w| w == shorter.as_slice()) {
        return true;
    }

    // LCS 比例: 最长公共子序列占较短文本的 50% 以上
    if shorter_len >= 6 {
        let lcs = lcs_len(&a_chars, &b_chars);
        if lcs as f64 / shorter_len as f64 > 0.5 {
            return true;
        }
    }

    false
}

/// 最长公共子序列长度
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 || b_len == 0 { return 0; }
    let mut prev = vec![0usize; b_len + 1];
    for i in 1..=a_len {
        let mut curr = vec![0usize; b_len + 1];
        for j in 1..=b_len {
            if a[i - 1] == b[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        prev = curr;
    }
    prev[b_len]
}
