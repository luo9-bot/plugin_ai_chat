mod concerns;
mod deliberations;
mod defects;

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

// ── re-exports ────────────────────────────────────────────────

// concerns.rs
pub use concerns::{ConcernCategory, Concern, add_concern, decay_concerns};

// deliberations.rs
pub use deliberations::{Deliberation, add_deliberation, decay_deliberations};

// defects.rs
pub use defects::{DefectType, check_defect, defect_to_instruction};

// ── 持久化存储 (共享) ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct MentalStateStore {
    pub concerns: Vec<concerns::Concern>,
    pub deliberations: Vec<deliberations::Deliberation>,
    /// 上次缺陷触发时间 (unix秒)，用于全局冷却
    #[serde(default)]
    pub last_defect_ts: u64,
}

impl Default for MentalStateStore {
    fn default() -> Self {
        Self {
            concerns: Vec::new(),
            deliberations: Vec::new(),
            last_defect_ts: 0,
        }
    }
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("mental_state.json")
}

impl MentalStateStore {
    pub(crate) fn load() -> Self {
        let path = store_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub(crate) fn save(&self) {
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

// ── 文本去重 (共享) ──────────────────────────────────────────

pub(crate) fn normalize_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || (*c >= '\u{4e00}' && *c <= '\u{9fff}'))
        .collect::<String>()
        .to_lowercase()
}

pub(crate) fn is_similar(a: &str, b: &str) -> bool {
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

// ── Prompt 注入 ──────────────────────────────────────────────

/// 构建心理状态上下文，注入到 system prompt
pub fn get_prompt_context(max_concerns: usize, max_deliberations: usize) -> String {
    let store = MentalStateStore::load();
    let mut parts = Vec::new();

    // 担忧
    if !store.concerns.is_empty() {
        let mut concerns: Vec<&concerns::Concern> = store.concerns.iter().collect();
        concerns.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        let lines: Vec<String> = concerns.iter().take(max_concerns).map(|c| {
            format!("- {}（{}）", c.content, c.category.label())
        }).collect();
        parts.push(format!("# 你的担忧\n{}", lines.join("\n")));
    }

    // 考量
    if !store.deliberations.is_empty() {
        let mut delibs: Vec<&deliberations::Deliberation> = store.deliberations.iter().collect();
        delibs.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        let lines: Vec<String> = delibs.iter().take(max_deliberations).map(|d| {
            format!("- {}", d.content)
        }).collect();
        parts.push(format!("# 你的考量\n{}", lines.join("\n")));
    }

    parts.join("\n\n")
}

// ── 对话后生成 ────────────────────────────────────────────────

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
        crate::prompt::PromptManager::get().raw("mental_state_generate"),
        &full_context,
        &[crate::ai::mental_state_generate_tool()],
        Some(serde_json::json!("auto")),
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
