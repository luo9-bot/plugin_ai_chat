use serde::{Deserialize, Serialize};
use std::fs;
use std::time::SystemTime;

use crate::config;
use crate::emotion;

/// 自我反思提示词
const REFLECT_PROMPT: &str = r#"你是一个有内心世界的群聊成员。现在是你的"独处时间"，你正在回想最近发生的事情，产生一些内心想法。

根据最近的对话和你的情绪状态，产生 1-3 条内心独白。

返回 JSON（不要输出其他内容）:
{
  "thoughts": [
    {"content": "内心想法", "category": "reflection|experience|plan|feeling"}
  ],
  "share": {
    "should_share": true/false,
    "content": "如果想分享，这里是要说的话（简短自然，像突然想到的）",
    "target_group": 0
  }
}

category 说明:
- reflection: 对最近对话的反思（"刚才聊得挺开心的"、"那个人说话好有趣"）
- experience: 自身经历（"今天做了好吃的"、"看了一部电影"）
- plan: 想做的事（"想学做蛋糕"、"明天想出去走走"）
- feeling: 内心感受（"有点困了"、"心情不错"、"突然想家了"）

规则:
- 像真人一样思考，不要哲理或鸡汤，就是普通的内心想法
- 内容简短，一两句话
- 不要暴露你是 AI
- 大部分时候 should_share 为 false（约70%的情况不分享）
- 只有当想法很强烈、或者像人一样忍不住想说出来时，should_share 才为 true
- share.content 是你想主动发到群里/私聊的话，简短自然，像自言自语
- target_group: 如果想在某个群分享，填群号；0 表示私聊（不主动分享到私聊）"#;

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

#[derive(Debug, Deserialize)]
struct ReflectResult {
    thoughts: Vec<ReflectThought>,
    share: Option<ShareInfo>,
}

#[derive(Debug, Deserialize)]
struct ReflectThought {
    content: String,
    category: String,
}

#[derive(Debug, Deserialize)]
struct ShareInfo {
    should_share: bool,
    content: String,
    target_group: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("self_memory.json")
}

impl SelfMemoryStore {
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

/// 初始化时调用：返回自我记忆条数
pub fn load_count() -> usize {
    let store = SelfMemoryStore::load();
    store.thoughts.len()
}

/// 添加一条自我思考
pub fn add(content: &str, category: ThoughtCategory) {
    let mut store = SelfMemoryStore::load();
    let now = now_secs();
    store.thoughts.push(SelfThought {
        content: content.to_string(),
        category,
        created: now,
    });
    // 保留最近 100 条
    if store.thoughts.len() > 100 {
        let drain_count = store.thoughts.len() - 100;
        store.thoughts.drain(..drain_count);
    }
    store.save();
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

/// AI 驱动的自我反思
///
/// recent_context: 最近的对话上下文文本
/// active_groups: 最近活跃的群组列表
///
/// 返回: (thoughts_added, share_info)
pub fn reflect(
    recent_context: &str,
    active_groups: &[u64],
) -> (usize, Option<(String, u64)>) {
    // 构建反思上下文
    let mut context_parts = Vec::new();

    // 人格信息
    let personality = crate::personality::get_prompt_context();
    if !personality.is_empty() {
        context_parts.push(personality);
    }

    // 情绪状态 (取一个代表性的)
    let emotion_ctx = emotion::get_prompt_context(0);
    if !emotion_ctx.is_empty() {
        context_parts.push(emotion_ctx);
    }

    // 最近的自我思考 (避免重复)
    let existing = get_context(10);
    if !existing.is_empty() {
        context_parts.push(existing);
    }

    // 最近的对话
    if !recent_context.is_empty() {
        context_parts.push(format!("# 最近的对话\n{}", recent_context));
    }

    let full_context = context_parts.join("\n\n");

    match crate::ai::analyze(REFLECT_PROMPT, &full_context) {
        Ok(raw) => {
            let json_str = crate::ai::extract_json(&raw);
            let json_str = match json_str {
                Some(s) => s,
                None => {
                    eprintln!("[ai_chat] self_reflect: no JSON in response");
                    return (0, None);
                }
            };

            match serde_json::from_str::<ReflectResult>(&json_str) {
                Ok(result) => {
                    let mut count = 0;
                    for thought in &result.thoughts {
                        if thought.content.is_empty() {
                            continue;
                        }
                        let category = match thought.category.as_str() {
                            "experience" => ThoughtCategory::Experience,
                            "plan" => ThoughtCategory::Plan,
                            "feeling" => ThoughtCategory::Feeling,
                            _ => ThoughtCategory::Reflection,
                        };
                        add(&thought.content, category);
                        count += 1;
                    }

                    // 处理主动分享
                    let share = result.share.and_then(|s| {
                        if s.should_share && !s.content.is_empty() {
                            let target = if s.target_group > 0 {
                                s.target_group
                            } else if !active_groups.is_empty() {
                                // 默认分享到最近活跃的群
                                active_groups[0]
                            } else {
                                return None;
                            };
                            Some((s.content, target))
                        } else {
                            None
                        }
                    });

                    if count > 0 {
                        eprintln!("[ai_chat] self_reflect: added {} thoughts", count);
                    }

                    (count, share)
                }
                Err(e) => {
                    eprintln!("[ai_chat] self_reflect JSON parse error: {}", e);
                    (0, None)
                }
            }
        }
        Err(e) => {
            eprintln!("[ai_chat] self_reflect AI error: {}", e);
            (0, None)
        }
    }
}
