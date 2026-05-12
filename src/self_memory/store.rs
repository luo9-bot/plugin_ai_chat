use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtCategory {
    Reflection,
    Experience,
    Plan,
    Feeling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfThought {
    pub content: String,
    pub category: ThoughtCategory,
    pub created: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SelfMemoryStore {
    pub thoughts: Vec<SelfThought>,
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("self_memory.json")
}

impl SelfMemoryStore {
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

/// 初始化时调用：返回自我记忆条数
pub fn load_count() -> usize {
    let store = SelfMemoryStore::load();
    store.thoughts.len()
}

/// 添加一条自我思考 (永久保存，不会自动淘汰)
/// 自动去重：如果已有高度相似的想法，跳过
pub fn add(content: &str, category: ThoughtCategory) {
    let mut store = SelfMemoryStore::load();

    // 去重：检查是否已有相似内容
    let normalized = normalize_thought(content);
    for existing in &store.thoughts {
        let existing_norm = normalize_thought(&existing.content);
        if is_similar(&normalized, &existing_norm) {
            debug!(content, "self_memory: skipped duplicate thought");
            return;
        }
    }

    let now = crate::util::now_secs();
    debug!(content, ?category, "self_memory: added thought");
    store.thoughts.push(SelfThought {
        content: content.to_string(),
        category,
        created: now,
    });
    store.save();

    // 远程同步 (fire-and-forget)
    if crate::config::get().sync.enabled
        && let Some(thought) = store.thoughts.last() {
            super::sync::sync_to_remote(thought);
        }
}

/// 标准化想法文本用于比较
fn normalize_thought(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c >= '\u{4e00}' && *c <= '\u{9fff}')
        .collect::<String>()
        .to_lowercase()
}

/// 检查两个标准化后的想法是否高度相似
/// 使用最长公共子序列(LCS)比例判断
fn is_similar(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }

    // 完全相同
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

/// 总共保存了多少条自我记忆
pub fn total_count() -> usize {
    let store = SelfMemoryStore::load();
    store.thoughts.len()
}

/// 修正自我记忆：根据 old 模糊匹配，替换为 new (new 为空则删除)
/// 返回修正的条数
pub fn correct(old: &str, new: &str) -> usize {
    let mut store = SelfMemoryStore::load();
    let mut count = 0;

    if new.is_empty() {
        let before = store.thoughts.len();
        store.thoughts.retain(|t| !t.content.contains(old));
        count = before - store.thoughts.len();
    } else {
        for thought in &mut store.thoughts {
            if thought.content.contains(old) {
                thought.content = new.to_string();
                count += 1;
            }
        }
    }

    if count > 0 {
        store.save();
        debug!(old, new, count, "self_memory: corrected entries");
    }
    count
}

/// 获取最近的自我思考上下文 (注入到 system prompt)
pub fn get_context(max_count: usize) -> String {
    let store = SelfMemoryStore::load();
    if store.thoughts.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = store.thoughts
        .iter()
        .rev()
        .take(max_count)
        .map(|t| {
            let cat = match t.category {
                ThoughtCategory::Reflection => "反思",
                ThoughtCategory::Experience => "经历",
                ThoughtCategory::Plan => "计划",
                ThoughtCategory::Feeling => "感受",
            };
            format!("- [{}] {}", cat, t.content)
        })
        .collect();

    format!("# 你最近的想法\n{}", lines.join("\n"))
}
