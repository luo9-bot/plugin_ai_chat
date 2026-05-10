use tracing::debug;
use crate::config;
use super::types::{ChatMessage, ChatRequest, ChatResponse, Tool, MemoryCorrection, PostAnalysis};
use super::tools::post_analyze_tool;

/// 创建不把 HTTP 错误状态码当作 ureq Error 的 Agent
/// 这样 4xx/5xx 响应体可以被正常读取，用于排查 API 错误原因
fn no_error_agent() -> ureq::Agent {
    let config = ureq::config::Config::builder()
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

/// 从 AI 响应中提取 JSON 对象 (处理 <think> 标签、markdown 代码块等)
pub fn extract_json(raw: &str) -> Option<String> {
    let cleaned = if let Some(pos) = raw.find("</think>") {
        raw[pos + 8..].trim()
    } else {
        raw.trim()
    };

    // 尝试直接提取 { ... }
    if let Some(start) = cleaned.find('{') {
        if let Some(end) = cleaned[start..].rfind('}') {
            return Some(cleaned[start..start + end + 1].to_string());
        }
    }

    // 尝试从 markdown 代码块提取
    if let Some(start) = cleaned.find("```json") {
        let after = &cleaned[start + 7..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    if let Some(start) = cleaned.find("```") {
        let after = &cleaned[start + 3..];
        if let Some(end) = after.find("```") {
            let inner = after[..end].trim();
            if inner.starts_with('{') {
                return Some(inner.to_string());
            }
        }
    }

    // 尝试提取 [ ... ] 数组
    if let Some(start) = cleaned.find('[') {
        if let Some(end) = cleaned[start..].rfind(']') {
            return Some(cleaned[start..start + end + 1].to_string());
        }
    }

    None
}

/// 从 JSON Value 中解析布尔值 (兼容布尔值和字符串 "true"/"false")
pub fn parse_bool(value: &serde_json::Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value.as_str().and_then(|s| match s {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
    })
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
    let now = crate::util::now_formatted_cst();
    let time_prompt = format!("\n你的时间为：{}\n", now);

    // 根据配置填充 CORE_RULES 中的风格占位符
    let omit_rule = if cfg.style.omit_subject {
        "- 说话经常省略主语，\"我\"字能省就省。不是\"我觉得很无聊\"，是\"无聊\"。不是\"我在想事情\"，是\"在想事情\""
    } else {
        "- 说话自然，不需要刻意省略主语"
    };
    let punct_rule = match cfg.style.punctuation_style.as_str() {
        "formal" => "- 使用正常的标点符号，句末加句号，问句加问号",
        _ => "- 日常发言不加句号，用换行或竖线代替停顿。问句偶尔加问号但也可以不加",
    };
    let resolved_rules = crate::prompt::PromptManager::get().raw("core_rules")
        .replace("{max_reply_chars}", &cfg.style.max_reply_chars.to_string())
        .replace("{omit_subject_rule}", omit_rule)
        .replace("{punctuation_rule}", punct_rule);

    // 组装 system prompt: 核心规则 + 用户 prompt + 记忆/人格/情绪 + 时间
    let mut full_system = format!(
        "{}\n\n{}\n\n{}\n\n{}",
        resolved_rules, base_prompt, extra_context, time_prompt
    );

    // 禁用动作描述时追加规则
    if !cfg.conversation.action_descriptions {
        full_system.push_str("\n\n# 输出格式\n完全不要用括号描述动作或描述表情（如（笑了笑）（叹气）），只输出纯对话内容。");
    }

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: Some(full_system),
        tool_calls: None,
        reasoning_content: None,
    }];

    for (role, content) in history {
        messages.push(ChatMessage {
            role: role.clone(),
            content: Some(content.clone()),
            tool_calls: None,
            reasoning_content: None,
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: Some(user_message.to_string()),
        tool_calls: None,
        reasoning_content: None,
    });

    debug!(model = %cfg.model, messages_count = messages.len(), "chat: sending API request");
    let req = ChatRequest {
        model: cfg.model.clone(),
        messages,
        frequency_penalty: cfg.ai.frequency_penalty,
        presence_penalty: cfg.ai.presence_penalty,
        temperature: cfg.ai.temperature,
        top_p: cfg.ai.top_p,
        max_tokens: cfg.ai.max_tokens,
        tools: None,
        tool_choice: None,
        thinking: Some(serde_json::json!({"type": "disabled"})),
    };

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;

    let agent = no_error_agent();
    let mut resp = agent.post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = resp.status();
    let resp_str = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("API read failed: {}", e))?;

    if !(200..300).contains(&status.as_u16()) {
        return Err(format!("API returned {}: {}", status.as_u16(), resp_str));
    }

    let body: ChatResponse = serde_json::from_str(&resp_str)
        .map_err(|e| format!("API parse failed: {}", e))?;

    let choice = body
        .choices
        .into_iter()
        .next()
        .ok_or("API returned empty choices")?;

    let mut reply = choice.message.content.unwrap_or_default();

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
            content: Some(system_prompt.to_string()),
            tool_calls: None,
            reasoning_content: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: Some(user_content.to_string()),
            tool_calls: None,
            reasoning_content: None,
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
        tools: None,
        tool_choice: None,
        thinking: Some(serde_json::json!({"type": "disabled"})),
    };

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;

    debug!(model = %cfg.model, "analyze: sending API request");
    let agent = no_error_agent();
    let mut resp = agent.post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = resp.status();
    let resp_str = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("API read failed: {}", e))?;

    if !(200..300).contains(&status.as_u16()) {
        return Err(format!("API returned {}: {}", status.as_u16(), resp_str));
    }

    let body: ChatResponse = serde_json::from_str(&resp_str)
        .map_err(|e| format!("API parse failed: {}", e))?;

    let choice = body
        .choices
        .into_iter()
        .next()
        .ok_or("API returned empty choices")?;

    let mut reply = choice.message.content.unwrap_or_default();
    debug!(reply_len = reply.len(), "analyze: received response");

    // 去除 <think> 标签
    if let Some(pos) = reply.find("</think>") {
        reply = reply[pos + 8..].trim().to_string();
    }

    Ok(reply)
}

/// 带 Function Call 的分析调用
///
/// 使用 tools 参数定义可用函数，AI 会通过 tool_calls 返回结构化数据。
/// 如果 API 没有返回 tool_calls，fallback 到从文本中提取 JSON。
/// 模型偶尔会忽略 tool_calls 直接返回文本，此时自动重试一次。
pub fn analyze_with_tools(
    system_prompt: &str,
    user_content: &str,
    tools: &[Tool],
    tool_choice: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let cfg = config::get();
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));

    // 精简日志：只显示 tools、tool_choice 和 user content，跳过 system prompt
    let tools_summary: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
    let user_content_preview = if user_content.len() > 500 {
        let end = user_content.floor_char_boundary(500);
        format!("{}...[truncated]", &user_content[..end])
    } else {
        user_content.to_string()
    };

    let tc_value = tool_choice.unwrap_or(serde_json::json!("auto"));
    let agent = no_error_agent();

    // 最多重试 2 次：模型偶尔忽略 tool_calls 返回纯文本
    for attempt in 0..2u8 {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                tool_calls: None,
                reasoning_content: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(user_content.to_string()),
                tool_calls: None,
                reasoning_content: None,
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
            tools: Some(tools.to_vec()),
            tool_choice: Some(tc_value.clone()),
            thinking: Some(serde_json::json!({"type": "disabled"})),
        };

        let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;


        // 打印tools 和 tool_choice 用于调试
        debug!(tools = ?tools_summary, tool_choice = %req.tool_choice.as_ref().map(|v| v.to_string()).unwrap_or_default(), "analyze_with_tools: tools 和 tool_choice");

        if attempt == 0 {
            debug!(
                model = %cfg.model,
                tools = ?tools_summary,
                tool_choice = %req.tool_choice.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                user_content = %user_content_preview,
                "analyze_with_tools: request"
            );
        } else {
            debug!(attempt, "analyze_with_tools: retrying (no tool_calls)");
        }

        let mut resp = agent.post(&url)
            .header("Authorization", &format!("Bearer {}", cfg.api_key))
            .header("Content-Type", "application/json")
            .send(json_body.as_bytes())
            .map_err(|e| format!("API request failed: {}", e))?;

        let status = resp.status();
        let resp_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("API read failed: {}", e))?;

        if !(200..300).contains(&status.as_u16()) {
            return Err(format!("API returned {}: {}", status.as_u16(), resp_str));
        }

        debug!("analyze_with_tools: raw response:\n{}", resp_str);

        let body: ChatResponse = serde_json::from_str(&resp_str)
            .map_err(|e| format!("API parse failed: {}", e))?;

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or("API returned empty choices")?;

        // 优先从 message.tool_calls 中提取结果
        let has_tool_calls = choice.message.tool_calls.as_ref().map_or(false, |tc| !tc.is_empty());
        let has_content = choice.message.content.as_ref().map_or(false, |c| !c.is_empty());
        debug!(has_tool_calls, has_content, "analyze_with_tools: response analysis");

        if let Some(tool_calls) = &choice.message.tool_calls {
            if let Some(first_call) = tool_calls.first() {
                debug!(name = %first_call.function.name, args_len = first_call.function.arguments.len(),
                    "analyze_with_tools: got tool call");
                let args: serde_json::Value = serde_json::from_str(&first_call.function.arguments)
                    .map_err(|e| format!("Tool call arguments parse failed: {}", e))?;
                return Ok(args);
            }
        }

        // Fallback: 从文本内容中提取 JSON (兼容旧行为)
        let mut reply = choice.message.content.unwrap_or_default();
        debug!(reply_len = reply.len(), "analyze_with_tools: falling back to text extraction");

        if let Some(pos) = reply.find("</think>") {
            reply = reply[pos + 8..].trim().to_string();
        }

        if let Some(json_str) = extract_json(&reply) {
            return serde_json::from_str(&json_str)
                .map_err(|e| format!("Fallback JSON parse failed: {}", e));
        }

        // 有内容但无 JSON → 模型未走 tool_calls，重试
        if has_content && attempt == 0 {
            continue;
        }

        return Err("No tool_calls and no JSON found in response".to_string());
    }

    unreachable!()
}

/// 带工具名称的 analyze_with_tools（返回 (tool_name, arguments)）
///
/// Timing Gate 等需要知道 AI 选择了哪个工具的场景使用。
pub fn analyze_with_tools_named(
    system_prompt: &str,
    user_content: &str,
    tools: &[Tool],
    tool_choice: Option<serde_json::Value>,
) -> Result<(String, serde_json::Value), String> {
    let cfg = config::get();
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let tc_value = tool_choice.unwrap_or(serde_json::json!("auto"));
    let agent = no_error_agent();

    for attempt in 0..2u8 {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                tool_calls: None,
                reasoning_content: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(user_content.to_string()),
                tool_calls: None,
                reasoning_content: None,
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
            tools: Some(tools.to_vec()),
            tool_choice: Some(tc_value.clone()),
            thinking: Some(serde_json::json!({"type": "disabled"})),
        };

        let json_body = serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {}", e))?;

        let mut resp = agent.post(&url)
            .header("Authorization", &format!("Bearer {}", cfg.api_key))
            .header("Content-Type", "application/json")
            .send(json_body.as_bytes())
            .map_err(|e| format!("API request failed: {}", e))?;

        let status = resp.status();
        let resp_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("API read failed: {}", e))?;

        if !(200..300).contains(&status.as_u16()) {
            return Err(format!("API returned {}: {}", status.as_u16(), resp_str));
        }

        let body: ChatResponse = serde_json::from_str(&resp_str)
            .map_err(|e| format!("API parse failed: {}", e))?;

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or("API returned empty choices")?;

        let _has_tool_calls = choice.message.tool_calls.as_ref().map_or(false, |tc| !tc.is_empty());
        let has_content = choice.message.content.as_ref().map_or(false, |c| !c.is_empty());

        if let Some(tool_calls) = &choice.message.tool_calls {
            if let Some(first_call) = tool_calls.first() {
                let name = first_call.function.name.clone();
                let args: serde_json::Value = serde_json::from_str(&first_call.function.arguments)
                    .map_err(|e| format!("Tool call arguments parse failed: {}", e))?;
                debug!(name = %name, "analyze_with_tools_named: got tool call");
                return Ok((name, args));
            }
        }

        // Fallback: 从文本中提取
        let mut reply = choice.message.content.unwrap_or_default();
        if let Some(pos) = reply.find("</think>") {
            reply = reply[pos + 8..].trim().to_string();
        }

        if has_content && attempt == 0 {
            continue;
        }

        return Err("No tool_calls found in response".to_string());
    }

    unreachable!()
}

/// 合并的后处理分析 (记忆提取 + 情绪分析 + 记忆纠错，单次 API 调用)
///
/// extra_context: 现有记忆和自我记忆的文本，供 AI 识别需要修正的内容
pub fn post_analyze(user_message: &str, ai_reply: &str, history: &[(String, String)], extra_context: &str) -> PostAnalysis {
    // 构建上下文
    let mut context_parts = Vec::new();
    if !extra_context.is_empty() {
        context_parts.push(extra_context.to_string());
    }
    let recent: Vec<_> = history.iter().rev().take(6).collect();
    for (role, content) in recent.iter().rev() {
        context_parts.push(format!("[{}]: {}", role, content));
    }
    context_parts.push(format!("[user]: {}", user_message));
    context_parts.push(format!("[assistant]: {}", ai_reply));
    let content = context_parts.join("\n");

    // 动态拼接人设到系统提示词
    let user_prompt = config::prompt();
    let personality = crate::personality::get_prompt_context();
    let mut system_prompt = String::new();
    if !user_prompt.is_empty() {
        system_prompt.push_str(&format!("# 你的身份\n{}\n\n", user_prompt));
    }
    if !personality.is_empty() {
        system_prompt.push_str(&format!("{}\n\n", personality));
    }
    system_prompt.push_str(crate::prompt::PromptManager::get().raw("post_analyze"));

    let mut analysis = PostAnalysis {
        memories: Vec::new(),
        emotion: "neutral".to_string(),
        intensity: 0.3,
        corrections: Vec::new(),
        concerns: Vec::new(),
        deliberations: Vec::new(),
    };

    match analyze_with_tools(&system_prompt, &content, &[post_analyze_tool()], None) {
        Ok(parsed) => {
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
            // 解析纠错
            if let Some(corrections) = parsed.get("corrections").and_then(|v| v.as_array()) {
                for item in corrections {
                    let old = item.get("old").and_then(|v| v.as_str()).unwrap_or("");
                    let new = item.get("new").and_then(|v| v.as_str()).unwrap_or("");
                    let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("user");
                    if !old.is_empty() {
                        analysis.corrections.push(MemoryCorrection {
                            old: old.to_string(),
                            new: new.to_string(),
                            target: target.to_string(),
                        });
                    }
                }
            }
            // 解析担忧
            if let Some(concerns) = parsed.get("concerns").and_then(|v| v.as_array()) {
                for item in concerns {
                    let c = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let cat = item.get("category").and_then(|v| v.as_str()).unwrap_or("social");
                    if !c.is_empty() {
                        analysis.concerns.push((c.to_string(), cat.to_string()));
                    }
                }
            }
            // 解析考量
            if let Some(deliberations) = parsed.get("deliberations").and_then(|v| v.as_array()) {
                for item in deliberations {
                    let d = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if !d.is_empty() {
                        analysis.deliberations.push(d.to_string());
                    }
                }
            }
        }
        Err(e) => {
            debug!(error = %e, "post_analyze failed");
        }
    }

    analysis
}
