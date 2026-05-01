use serde::{Deserialize, Serialize};
use crate::config;

/// 核心规则 (始终注入 system prompt，用户无需在 prompt 文件中写这些)
const CORE_RULES: &str = r#"# 核心规则
- 使用中文回复
- 回复应该简短自然，像日常聊天一样，不要长篇大论
- 使用 |^| 分隔句子或短语，例如：你好|^|今天天气不错
- 当用户一次发了多条消息时，综合所有消息给出一个完整回复，不要逐条回复
- 不要使用括号描述动作或心理活动
- 不要输出时间戳
- 不要输出 [emotion:xxx] 标签，情绪通过语气和用词自然流露
- 不要提及"我记得你说过..."，而是在合适的时候自然地融入记忆信息

# 定时功能
当用户提到具体的时间点和需要提醒的事件时，自动设置定时提醒。
回复格式: {"cron":{"title":"标题", "exp":"秒 分 时 日 月 星期 年", "content":"提醒内容"}}|cron|回复消息
其中 cron 表达式严格满足7个参数: "秒 分 时 日 月 星期 年""#;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    frequency_penalty: f64,
    presence_penalty: f64,
    temperature: f64,
    top_p: f64,
    max_tokens: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// 调用 AI API，注入记忆/人格/情绪上下文
///
/// 返回 (reply, detected_emotion)
pub fn chat(
    base_prompt: &str,
    extra_context: &str,
    history: &[(String, String)],
    user_message: &str,
) -> Result<(String, String), String> {
    let cfg = config::get();
    let now = chrono_now();
    let time_prompt = format!("\n你的时间为：{}\n", now);

    // 组装 system prompt: 核心规则 + 回复风格 + 用户 prompt + 记忆/人格/情绪 + 时间
    let reply_style = format!("# 回复风格\n- {}", cfg.reply_style);
    let full_system = format!(
        "{}\n\n{}\n\n{}\n\n{}\n\n{}",
        CORE_RULES, reply_style, base_prompt, extra_context, time_prompt
    );

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: full_system,
    }];

    for (role, content) in history {
        messages.push(ChatMessage {
            role: role.clone(),
            content: content.clone(),
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let req = ChatRequest {
        model: cfg.model.clone(),
        messages,
        frequency_penalty: cfg.ai.frequency_penalty,
        presence_penalty: cfg.ai.presence_penalty,
        temperature: cfg.ai.temperature,
        top_p: cfg.ai.top_p,
        max_tokens: cfg.ai.max_tokens,
    };

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;

    let mut resp = ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| format!("API request failed: {}", e))?;

    let resp_str = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("API read failed: {}", e))?;

    let body: ChatResponse = serde_json::from_str(&resp_str)
        .map_err(|e| format!("API parse failed: {}", e))?;

    let choice = body
        .choices
        .into_iter()
        .next()
        .ok_or("API returned empty choices")?;

    let mut reply = choice.message.content;

    // 去除 <think> 标签
    if let Some(pos) = reply.find("</think>") {
        reply = reply[pos + 8..].trim().to_string();
    }

    Ok((reply, String::new()))
}

/// 轻量级 AI 分析调用 (记忆提取、情绪分析等)
///
/// 使用更低的 max_tokens 和 temperature，快速返回结构化结果
pub fn analyze(system_prompt: &str, user_content: &str) -> Result<String, String> {
    let cfg = config::get();

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        },
    ];

    let req = ChatRequest {
        model: cfg.model.clone(),
        messages,
        frequency_penalty: 0.0,
        presence_penalty: 0.0,
        temperature: cfg.ai.analysis_temperature,
        top_p: 0.3,
        max_tokens: cfg.ai.analysis_max_tokens,
    };

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;

    let mut resp = ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| format!("API request failed: {}", e))?;

    let resp_str = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("API read failed: {}", e))?;

    let body: ChatResponse = serde_json::from_str(&resp_str)
        .map_err(|e| format!("API parse failed: {}", e))?;

    let choice = body
        .choices
        .into_iter()
        .next()
        .ok_or("API returned empty choices")?;

    let mut reply = choice.message.content;

    // 去除 <think> 标签
    if let Some(pos) = reply.find("</think>") {
        reply = reply[pos + 8..].trim().to_string();
    }

    Ok(reply)
}

/// 后处理分析提示词 (合并记忆提取 + 情绪分析，一次 API 调用)
const POST_ANALYZE_PROMPT: &str = r#"分析以下对话，同时完成两个任务:

任务1: 提取值得长期记忆的信息
任务2: 分析用户当前的情绪状态

返回 JSON（不要输出其他内容）:
{
  "memories": [{"content":"记忆内容","importance":"permanent|important|normal"}],
  "emotion": "neutral|happy|sad|thinking|surprised|angry|shy|worried|tired|excited",
  "intensity": 0.0~1.0
}

记忆重要性:
- permanent: 用户明确要求记住的
- important: 用户个人信息（姓名、生日、喜好等）
- normal: 值得记录的一般内容
- 如果没有值得记忆的，memories 为空数组 []

情绪: 根据用户消息的语气和内容判断，intensity 为 0.0~1.0 的强度值

示例:
用户: "记住我叫小明" / AI: "好的"
→ {"memories":[{"content":"用户叫小明","importance":"permanent"}],"emotion":"neutral","intensity":0.3}

用户: "太好了！终于考过了！" / AI: "恭喜啊！"
→ {"memories":[],"emotion":"excited","intensity":0.8}"#;

/// 后处理分析结果
pub struct PostAnalysis {
    pub memories: Vec<(String, String)>, // (content, importance)
    pub emotion: String,
    pub intensity: f32,
}

/// 合并的后处理分析 (记忆提取 + 情绪分析，单次 API 调用)
pub fn post_analyze(user_message: &str, ai_reply: &str, history: &[(String, String)]) -> PostAnalysis {
    // 构建上下文
    let mut context_parts = Vec::new();
    let recent: Vec<_> = history.iter().rev().take(6).collect();
    for (role, content) in recent.iter().rev() {
        context_parts.push(format!("[{}]: {}", role, content));
    }
    context_parts.push(format!("[user]: {}", user_message));
    context_parts.push(format!("[assistant]: {}", ai_reply));
    let content = context_parts.join("\n");

    let result = analyze(POST_ANALYZE_PROMPT, &content);
    let mut analysis = PostAnalysis {
        memories: Vec::new(),
        emotion: "neutral".to_string(),
        intensity: 0.3,
    };

    match result {
        Ok(raw) => {
            let json_str = if let Some(start) = raw.find('{') {
                if let Some(end) = raw[start..].find('}') {
                    &raw[start..start + end + 1]
                } else { return analysis; }
            } else { return analysis; };

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                // 解析记忆
                if let Some(memories) = parsed.get("memories").and_then(|v| v.as_array()) {
                    for item in memories {
                        let c = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        let i = item.get("importance").and_then(|v| v.as_str()).unwrap_or("normal");
                        if !c.is_empty() {
                            analysis.memories.push((c.to_string(), i.to_string()));
                        }
                    }
                }
                // 解析情绪
                analysis.emotion = parsed.get("emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral")
                    .to_string();
                analysis.intensity = parsed.get("intensity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3) as f32;
            }
        }
        Err(e) => {
            eprintln!("[ai_chat] post_analyze failed: {}", e);
        }
    }

    analysis
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{}年{:02}月{:02}日{:02}时{:02}分{:02}秒", year, month, day, hour, minute, second)
}

pub fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    let mut year = 1970i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let days_in_month = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u32;
    for &dim in &days_in_month {
        if days < dim as i64 {
            break;
        }
        days -= dim as i64;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
