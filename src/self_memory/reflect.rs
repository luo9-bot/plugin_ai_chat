use tracing::{debug, info};

use crate::config;
use crate::emotion;

use super::store::{ThoughtCategory, add, get_context};

/// 群组画像：AI 用来判断往哪个群分享
pub struct GroupProfile {
    pub group_id: u64,
    pub recent_messages: String,  // 格式化的最近消息摘要
}

/// AI 驱动的自我反思
///
/// recent_context: 最近的对话上下文文本
/// group_profiles: 各群的画像 (群号 -> 最近消息摘要)，AI 用来判断往哪个群分享
///
/// 返回: (thoughts_added, share_info)
pub fn reflect(
    recent_context: &str,
    group_profiles: &[GroupProfile],
) -> (usize, Option<(String, u64)>) {
    info!("self_reflect: 开始自我反思");
    // 构建反思上下文
    let mut context_parts = Vec::new();

    // 用户 prompt (人设定义)
    let user_prompt = config::prompt();
    if !user_prompt.is_empty() {
        context_parts.push(user_prompt.to_string());
    }

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
    let existing = get_context(20);
    if !existing.is_empty() {
        context_parts.push(existing);
    }

    // 最近的对话
    if !recent_context.is_empty() {
        context_parts.push(format!("# 最近的对话\n{}", recent_context));
    }

    // 各群的画像 (让 AI 了解每个群是干什么的)
    if !group_profiles.is_empty() {
        let profiles_text: Vec<String> = group_profiles.iter().map(|p| {
            format!("## 群{}\n{}", p.group_id, p.recent_messages)
        }).collect();
        context_parts.push(format!("# 你所在的群\n{}", profiles_text.join("\n\n")));
    }

    let full_context = context_parts.join("\n\n");

    match crate::ai::analyze_with_tools(
        crate::prompt::PromptManager::get().raw("self_reflect"),
        &full_context,
        &[crate::ai::self_reflect_tool()],
        Some(serde_json::json!("auto"))
    ) {
        Ok(parsed) => {
            // 解析 thoughts
            let mut count = 0;
            if let Some(thoughts) = parsed.get("thoughts").and_then(|v| v.as_array()) {
                for thought in thoughts {
                    let content = thought.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if content.is_empty() {
                        continue;
                    }
                    let category = match thought.get("category").and_then(|v| v.as_str()).unwrap_or("") {
                        "experience" => ThoughtCategory::Experience,
                        "plan" => ThoughtCategory::Plan,
                        "feeling" => ThoughtCategory::Feeling,
                        _ => ThoughtCategory::Reflection,
                    };
                    add(content, category);
                    count += 1;
                }
            }

            // 解析担忧
            if let Some(concerns) = parsed.get("concerns").and_then(|v| v.as_array()) {
                for item in concerns {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let category = item.get("category").and_then(|v| v.as_str()).unwrap_or("social");
                    if !content.is_empty() {
                        crate::mental_state::add_concern(content, category, 0, 0);
                    }
                }
            }
            // 解析考量
            if let Some(deliberations) = parsed.get("deliberations").and_then(|v| v.as_array()) {
                for item in deliberations {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if !content.is_empty() {
                        crate::mental_state::add_deliberation(content, "reflection");
                    }
                }
            }

            // 处理主动分享
            let share = parsed.get("share").and_then(|s| {
                let should_share = s.get("should_share").and_then(crate::ai::parse_bool).unwrap_or(false);
                let content = s.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let target = s.get("target_group_id").and_then(|v| v.as_u64()).unwrap_or(0);
                if should_share && !content.is_empty() && target > 0 {
                    Some((content.to_string(), target))
                } else {
                    None
                }
            });

            if count > 0 {
                debug!(count, "self_reflect: added thoughts");
            }

            (count, share)
        }
        Err(e) => {
            debug!(error = %e, "self_reflect: AI error");
            (0, None)
        }
    }
}
