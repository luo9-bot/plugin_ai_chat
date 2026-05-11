//! 表达提取：从群聊消息中学习语言风格

use tracing::{debug, info, warn};
use super::store::*;

pub fn should_learn(group_id: u64) -> bool {
    let s = load_store();
    let now = crate::util::now_secs();
    now.saturating_sub(s.last_learned.get(&group_id).copied().unwrap_or(0)) >= LEARN_INTERVAL_SECS
}

pub fn learn_from_messages(group_id: u64, messages: &[(u64, String)]) {
    if messages.len() < MIN_MESSAGES { return; }
    let now = crate::util::now_secs();
    let mut s = load_store();
    if now.saturating_sub(s.last_learned.get(&group_id).copied().unwrap_or(0)) < LEARN_INTERVAL_SECS { return; }

    let self_qq = crate::config::get().self_qq;
    // 过滤：排除 bot 自身消息、纯 emoji 消息、剥离 Unicode emoji
    let user_msgs: Vec<(u64, String)> = messages.iter()
        .filter(|(u, _)| *u != self_qq)
        .map(|(u, m)| (*u, crate::emoji::strip_emoji(m)))
        .filter(|(_, m)| !m.trim().is_empty() && !crate::emoji::is_emoji_only(m))
        .collect();
    if user_msgs.len() < MIN_MESSAGES { return; }

    let msg_text: Vec<String> = user_msgs.iter().map(|(u,m)| format!("[{}] {}", u, m)).collect();

    // 构建人设约束
    let persona = crate::config::prompt();
    let persona_constraint = if persona.is_empty() {
        "只提取符合一般社交礼仪的表达。".to_string()
    } else {
        format!(
            "重要约束：只提取符合以下人设的表达，不符合的直接忽略：\n{}\n\
             提取的表达应该与上述人设的性格、语气、风格一致。\
             如果某个表达与人设冲突（比如温柔的人设不应该学毒舌），不要提取。",
            persona
        )
    };

    let prompt_template = crate::prompt::PromptManager::get().raw("learn_style");
    let prompt = prompt_template.replace("{persona_constraint}", &persona_constraint);
    let content = format!("群聊消息:\n{}", msg_text.join("\n"));

    debug!(group_id, msg_count = user_msgs.len(), "learner: starting");
    match crate::ai::analyze(&prompt, &content) {
        Ok(response) => {
            if let Some(json_str) = crate::ai::extract_json(&response) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(exprs) = parsed.get("expressions").and_then(|v| v.as_array()) {
                        for expr in exprs {
                            let sit = expr.get("situation").and_then(|v| v.as_str()).unwrap_or("");
                            let sty = expr.get("style").and_then(|v| v.as_str()).unwrap_or("");
                            if sit.is_empty() || sty.is_empty() { continue; }
                            if let Some(e) = s.expressions.iter_mut().find(|e| text_sim(&e.situation, sit) > SIMILARITY_THRESHOLD && text_sim(&e.style, sty) > SIMILARITY_THRESHOLD) {
                                e.count += 1;
                            } else {
                                s.expressions.push(ExpressionHabit { situation: sit.into(), style: sty.into(), count: 1, source_group: group_id });
                                info!(situation = %sit, style = %sty, "learner: new expression");
                            }
                        }
                    }
                    if let Some(jargons) = parsed.get("jargon").and_then(|v| v.as_array()) {
                        for j in jargons {
                            let c = j.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let t = match j.get("type").and_then(|v| v.as_str()).unwrap_or("") { "pinyin" => JargonType::Pinyin, "english" => JargonType::English, "chinese" => JargonType::Chinese, _ => continue };
                            if c.is_empty() { continue; }
                            if !s.jargon.iter().any(|e| e.content == c) {
                                s.jargon.push(JargonEntry { content: c.into(), jargon_type: t, meaning: String::new(), source_group: group_id });
                            }
                        }
                    }
                }
            }
            s.last_learned.insert(group_id, now);
            save_store(&s);
        }
        Err(e) => { warn!(error = %e, group_id, "learner: AI error"); }
    }
}

fn text_sim(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let ca: Vec<char> = a.chars().collect();
    let cb: std::collections::HashSet<char> = b.chars().collect();
    ca.iter().filter(|c| cb.contains(c)).count() as f64 / ca.len().max(1) as f64
}
