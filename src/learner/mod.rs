//! 表达学习系统
//!
//! 从群聊中学习语言风格和表达模式，让 bot 的回复更自然。
//! 参考 MaiBot 的 expression_learner 架构。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// 表达习惯
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionHabit {
    /// 情境描述，如 "当有人惊叹时"
    pub situation: String,
    /// 表达方式，如 "用'我嘞个xxxx'"
    pub style: String,
    /// 使用次数
    #[serde(default)]
    pub count: u32,
    /// 来源群
    #[serde(default)]
    pub source_group: u64,
}

/// 黑话/梗类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JargonType {
    /// 拼音缩写 (nb, yyds, xswl)
    Pinyin,
    /// 英文缩写 (CPU, GPU, API)
    English,
    /// 中文缩写 (社死, 内卷)
    Chinese,
}

/// 黑话/梗条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JargonEntry {
    /// 内容，如 "yyds"
    pub content: String,
    /// 类型
    pub jargon_type: JargonType,
    /// 含义解释
    pub meaning: String,
    /// 来源群
    #[serde(default)]
    pub source_group: u64,
}

/// 学习数据存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearnerStore {
    /// 表达习惯库
    pub expressions: Vec<ExpressionHabit>,
    /// 黑话库
    pub jargon: Vec<JargonEntry>,
    /// 上次学习时间 (group_id -> timestamp)
    pub last_learned: HashMap<u64, u64>,
}

static STORE: Mutex<Option<LearnerStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("learner.json")
}

fn load_store() -> LearnerStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

fn save_store(store: &LearnerStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}

/// 最短学习间隔（秒）
const LEARN_INTERVAL_SECS: u64 = 30;
/// 最少消息数才触发学习
const MIN_MESSAGES: usize = 5;
/// 表达去重相似度阈值
const SIMILARITY_THRESHOLD: f64 = 0.75;

/// 检查是否应该触发学习
pub fn should_learn(group_id: u64) -> bool {
    let store = load_store();
    let now = crate::util::now_secs();
    let last = store.last_learned.get(&group_id).copied().unwrap_or(0);
    now.saturating_sub(last) >= LEARN_INTERVAL_SECS
}

/// 从群聊消息中学习表达风格
///
/// 异步调用 AI 提取情境→表达对和黑话。
pub fn learn_from_messages(group_id: u64, messages: &[(u64, String)]) {
    if messages.len() < MIN_MESSAGES {
        return;
    }

    let now = crate::util::now_secs();
    let mut store = load_store();

    // 检查间隔
    let last = store.last_learned.get(&group_id).copied().unwrap_or(0);
    if now.saturating_sub(last) < LEARN_INTERVAL_SECS {
        return;
    }

    // 过滤掉 bot 自己的消息
    let self_qq = crate::config::get().self_qq;
    let user_messages: Vec<&(u64, String)> = messages
        .iter()
        .filter(|(uid, _)| *uid != self_qq)
        .collect();

    if user_messages.len() < MIN_MESSAGES {
        return;
    }

    // 构建消息文本
    let msg_text: Vec<String> = user_messages
        .iter()
        .map(|(uid, msg)| format!("[{}] {}", uid, msg))
        .collect();

    let prompt = crate::prompt::PromptManager::get().raw("learn_style");
    let content = format!("群聊消息:\n{}", msg_text.join("\n"));

    debug!(group_id, msg_count = user_messages.len(), "learner: starting");

    match crate::ai::analyze(prompt, &content) {
        Ok(response) => {
            if let Some(json_str) = crate::ai::extract_json(&response) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    // 解析表达习惯
                    if let Some(exprs) = parsed.get("expressions").and_then(|v| v.as_array()) {
                        for expr in exprs {
                            let situation = expr.get("situation").and_then(|v| v.as_str()).unwrap_or("");
                            let style = expr.get("style").and_then(|v| v.as_str()).unwrap_or("");
                            if situation.is_empty() || style.is_empty() {
                                continue;
                            }

                            // 去重检查
                            if let Some(existing) = store.expressions.iter_mut().find(|e| {
                                text_similarity(&e.situation, situation) > SIMILARITY_THRESHOLD
                                    && text_similarity(&e.style, style) > SIMILARITY_THRESHOLD
                            }) {
                                existing.count += 1;
                                debug!(situation, style, count = existing.count, "learner: expression updated");
                            } else {
                                store.expressions.push(ExpressionHabit {
                                    situation: situation.to_string(),
                                    style: style.to_string(),
                                    count: 1,
                                    source_group: group_id,
                                });
                                info!(situation, style, "learner: new expression");
                            }
                        }
                    }

                    // 解析黑话
                    if let Some(jargons) = parsed.get("jargon").and_then(|v| v.as_array()) {
                        for j in jargons {
                            let content = j.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let jtype = match j.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                                "pinyin" => JargonType::Pinyin,
                                "english" => JargonType::English,
                                "chinese" => JargonType::Chinese,
                                _ => continue,
                            };
                            if content.is_empty() {
                                continue;
                            }

                            // 去重
                            if !store.jargon.iter().any(|e| e.content == content) {
                                store.jargon.push(JargonEntry {
                                    content: content.to_string(),
                                    jargon_type: jtype,
                                    meaning: String::new(), // 后续可由 AI 补充
                                    source_group: group_id,
                                });
                                info!(content = %content, "learner: new jargon");
                            }
                        }
                    }
                }
            }

            store.last_learned.insert(group_id, now);
            save_store(&store);
        }
        Err(e) => {
            warn!(error = %e, group_id, "learner: AI error");
        }
    }
}

/// 获取表达习惯上下文（注入到回复 prompt 中）
pub fn get_expression_context(group_id: u64, max_count: usize) -> String {
    let store = load_store();
    let mut exprs: Vec<&ExpressionHabit> = store
        .expressions
        .iter()
        .filter(|e| e.source_group == group_id || e.source_group == 0)
        .collect();

    // 按使用次数排序，取前 N 个
    exprs.sort_by(|a, b| b.count.cmp(&a.count));
    let selected: Vec<&ExpressionHabit> = exprs.into_iter().take(max_count).collect();

    if selected.is_empty() {
        return String::new();
    }

    let mut lines = vec!["# 表达习惯参考".to_string()];
    for expr in &selected {
        lines.push(format!("- 当{}时，可以{}", expr.situation, expr.style));
    }
    lines.join("\n")
}

/// 简单的文本相似度计算（字符重叠比例）
fn text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let chars_a: Vec<char> = a.chars().collect();
    let chars_b: std::collections::HashSet<char> = b.chars().collect();
    let overlap = chars_a.iter().filter(|c| chars_b.contains(c)).count();
    overlap as f64 / chars_a.len().max(1) as f64
}
