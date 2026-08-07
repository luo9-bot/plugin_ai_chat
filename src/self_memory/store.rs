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

    // 按主题抽样，避免最近想法堆叠在同一主题（如反复出现"吃饭"）
    let picked = pick_diverse_thoughts(&store.thoughts, max_count);
    let lines: Vec<String> = picked
        .iter()
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

/// 轻量主题分类：用于避免最近记忆堆叠在同一个主题上
fn thought_topic(content: &str) -> &'static str {
    const FOOD_WORDS: [&str; 12] = ["吃", "饭", "饿", "西瓜", "火锅", "水果", "猪脚", "麻辣",
        "番茄", "夜宵", "宵夜", "早餐"];
    if FOOD_WORDS.iter().any(|w| content.contains(w)) {
        return "food";
    }
    const HEALTH_WORDS: [&str; 12] = ["嗓子", "睡", "累", "病", "疼", "感冒", "水", "药",
        "困", "不舒服", "头疼", "嗓子疼"];
    if HEALTH_WORDS.iter().any(|w| content.contains(w)) {
        return "health";
    }
    const PEOPLE_WORDS: [&str; 12] = ["他", "她", "豆", "群", "消息", "回", "人", "洛屿",
        "土豆", "大家", "朋友", "队友"];
    if PEOPLE_WORDS.iter().any(|w| content.contains(w)) {
        return "people";
    }
    const PLAN_WORDS: [&str; 10] = ["计划", "明天", "打算", "安排", "准备", "记得",
        "要去", "得去", "该去", "安排"];
    if PLAN_WORDS.iter().any(|w| content.contains(w)) {
        return "plan";
    }
    "other"
}

/// 从记忆里按主题抽样：同一主题最多取 2 条，再按最新补齐，保证上下文主题多样
pub(super) fn pick_diverse_thoughts(thoughts: &[SelfThought], max_count: usize) -> Vec<&SelfThought> {
    use std::collections::{HashMap, HashSet};

    let mut picked: Vec<&SelfThought> = Vec::new();
    let mut picked_idx: HashSet<usize> = HashSet::new();
    let mut topic_count: HashMap<&'static str, usize> = HashMap::new();
    let indices: Vec<usize> = (0..thoughts.len()).rev().collect();

    // 第一轮：按主题多样选取，每个主题最多 2 条
    for &idx in &indices {
        if picked.len() >= max_count {
            break;
        }
        let topic = thought_topic(&thoughts[idx].content);
        let used = topic_count.entry(topic).or_insert(0);
        if *used >= 2 {
            continue;
        }
        *used += 1;
        picked.push(&thoughts[idx]);
        picked_idx.insert(idx);
    }

    // 第二轮：仍不足时按最新补齐（保证多样性优先，也不浪费可用记忆）
    for &idx in &indices {
        if picked.len() >= max_count {
            break;
        }
        if picked_idx.contains(&idx) {
            continue;
        }
        picked.push(&thoughts[idx]);
    }

    picked
}
