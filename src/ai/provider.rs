use super::types::{ChatMessage, ChatRequest, ChatResponse, Tool};
use crate::config;
use tracing::debug;

/// 记录 token 用量
pub(crate) fn track_usage(body: &ChatResponse, prompt_name: &str, model: &str) {
    if let Some(ref u) = body.usage {
        crate::tracking::record_call(
            prompt_name,
            model,
            u.prompt_tokens,
            u.completion_tokens,
            u.total_tokens,
            u.prompt_cache_hit_tokens,
            u.prompt_cache_miss_tokens,
        );
    }
}

/// 创建不把 HTTP 错误状态码当作 ureq Error 的 Agent
/// 这样 4xx/5xx 响应体可以被正常读取，用于排查 API 错误原因
///
/// 超时来自 `ai.request_timeout`：这条 agent 用在主回复路径上，而消息处理
/// 是单队列串行的，没有超时意味着一个黑洞连接会让所有群和所有私聊停摆。
pub(crate) fn no_error_agent() -> ureq::Agent {
    let timeout_secs = config::get().ai.request_timeout;
    crate::util::agent(crate::util::AgentSpec::reading_error_body(timeout_secs))
}

/// 从 AI 响应中提取 JSON 对象 (处理 <think> 标签、markdown 代码块等)
pub(crate) fn extract_json(raw: &str) -> Option<String> {
    let cleaned = if let Some(pos) = raw.find("</think>") {
        raw[pos + 8..].trim()
    } else {
        raw.trim()
    };

    // 尝试直接提取 { ... }
    if let Some(start) = cleaned.find('{')
        && let Some(end) = cleaned[start..].rfind('}')
    {
        return Some(cleaned[start..start + end + 1].to_string());
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
    if let Some(start) = cleaned.find('[')
        && let Some(end) = cleaned[start..].rfind(']')
    {
        return Some(cleaned[start..start + end + 1].to_string());
    }

    None
}

/// 尝试修复被截断的 JSON（如 finish_reason="length" 时工具调用参数不完整）
///
/// 策略：逐字符追踪 JSON 结构，补全未关闭的字符串、数组和对象
fn repair_truncated_json(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut repaired = String::with_capacity(trimmed.len() + 32);
    let mut in_string = false;
    let mut escape_next = false;
    let mut depth: Vec<char> = Vec::new(); // 追踪 [ 或 {

    for &ch in &chars {
        if escape_next {
            repaired.push(ch);
            escape_next = false;
            continue;
        }

        if in_string {
            if ch == '\\' {
                repaired.push(ch);
                escape_next = true;
            } else if ch == '"' {
                repaired.push(ch);
                in_string = false;
            } else {
                repaired.push(ch);
            }
            continue;
        }

        match ch {
            '"' => {
                repaired.push(ch);
                in_string = true;
            }
            '[' | '{' => {
                repaired.push(ch);
                depth.push(ch);
            }
            ']' => {
                if depth.last() == Some(&'[') {
                    repaired.push(ch);
                    depth.pop();
                }
                // 忽略不匹配的闭合
            }
            '}' => {
                if depth.last() == Some(&'{') {
                    repaired.push(ch);
                    depth.pop();
                }
            }
            _ => repaired.push(ch),
        }
    }

    // 补全未关闭的字符串
    if in_string {
        repaired.push('"');
    }

    // 补全未关闭的数组和对象
    while let Some(open) = depth.pop() {
        match open {
            '[' => repaired.push(']'),
            '{' => repaired.push('}'),
            _ => {}
        }
    }

    serde_json::from_str(&repaired).ok()
}

/// 从 JSON Value 中解析布尔值 (兼容布尔值和字符串 "true"/"false")
pub(crate) fn parse_bool(value: &serde_json::Value) -> Option<bool> {
    value.as_bool().or_else(|| {
        value.as_str().and_then(|s| match s {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
    })
}

/// 渲染 core_rules（占位符已按配置填充）——对话与回神共用的唯一渲染路径
pub(crate) fn rendered_core_rules() -> String {
    let cfg = config::get();
    let bot_name = &cfg.bot_name;

    let omit_rule = if cfg.style.omit_subject {
        "- 说话经常省略主语，\"我\"字能省就省。不是\"我觉得很无聊\"，是\"无聊\"。不是\"我在想事情\"，是\"在想事情\""
    } else {
        "- 说话自然，不需要刻意省略主语"
    };
    let punct_rule = match cfg.style.punctuation_style.as_str() {
        "formal" => "- 使用正常的标点符号，句末加句号，问句加问号",
        _ => "- 日常发言不加句号，用换行或竖线代替停顿。问句偶尔加问号但也可以不加",
    };

    let mut vars = std::collections::HashMap::new();
    vars.insert("bot_name", bot_name.as_str());
    let max_chars_str = &cfg.style.max_reply_chars.to_string();
    vars.insert("max_reply_chars", max_chars_str.as_str());
    vars.insert("omit_subject_rule", omit_rule);
    vars.insert("punctuation_rule", punct_rule);
    crate::prompt::PromptRenderer::render_simple(
        crate::prompt::PromptManager::get().raw("core_rules"),
        &vars,
    )
}

/// 唯一的一次"只要一句话"的模型调用（带重试与熔断）
///
/// 所有单轮调用都走这里：请求构造留在各自函数里，而**网络、重试、熔断、
/// 用量统计、取首个 choice、剥离 `<think>`** 只有这一份实现。
/// 需要检查 `tool_calls` 的调用直接用 `client::chat_completion`。
fn completion_text(req: &ChatRequest, prompt_name: &str) -> Result<String, String> {
    let body = crate::ai::client::chat_completion(req).map_err(|e| e.to_string())?;
    track_usage(&body, prompt_name, &req.model);

    let mut reply = body
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .unwrap_or_default();

    // 剥离思维链标签：她真正的输出在 </think> 之后
    if let Some(pos) = reply.find("</think>") {
        reply = reply[pos + 8..].trim().to_string();
    }
    Ok(reply)
}

/// 调用 AI API，注入记忆/人格/情绪上下文
///
/// 返回 (reply, detected_emotion)
pub(crate) fn chat(
    base_prompt: &str,
    extra_context: &str,
    history: &[(String, String)],
    user_message: &str,
) -> Result<(String, String), String> {
    let cfg = config::get();
    let now = crate::util::now_formatted_cst();
    let time_prompt = format!("\n你的时间为：{}\n", now);

    let resolved_rules = rendered_core_rules();

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

    let reply = completion_text(&req, "chat")?;
    Ok((reply, String::new()))
}

/// 轻量级 AI 分析调用 (记忆提取、情绪分析等)
///
/// 使用更低的 max_tokens 和 temperature，快速返回结构化结果
pub(crate) fn analyze(system_prompt: &str, user_content: &str) -> Result<String, String> {
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

    completion_text(&req, "analyze")
}

/// 带 Function Call 的分析调用
///
/// 使用 tools 参数定义可用函数，AI 会通过 tool_calls 返回结构化数据。
/// 如果 API 没有返回 tool_calls，fallback 到从文本中提取 JSON。
/// 模型偶尔会忽略 tool_calls 直接返回文本，此时自动重试一次。
pub(crate) fn analyze_with_tools(
    system_prompt: &str,
    user_content: &str,
    tools: &[Tool],
    tool_choice: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let cfg = config::get();
    analyze_with_tools_cfg(
        system_prompt,
        user_content,
        tools,
        tool_choice,
        cfg.ai.analysis_temperature,
        0.3,
    )
}

/// 带温度/采样参数的分析调用
///
/// 与 `analyze_with_tools` 逻辑完全一致，仅允许调用方指定
/// temperature 与 top_p（如主动消息生成需要更高随机性时使用）。
pub(crate) fn analyze_with_tools_cfg(
    system_prompt: &str,
    user_content: &str,
    tools: &[Tool],
    tool_choice: Option<serde_json::Value>,
    temperature: f64,
    top_p: f64,
) -> Result<serde_json::Value, String> {
    let cfg = config::get();

    // 精简日志：只显示 tools、tool_choice 和 user content，跳过 system prompt
    let tools_summary: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
    let user_content_preview = if user_content.len() > 500 {
        let end = user_content.floor_char_boundary(500);
        format!("{}...[truncated]", &user_content[..end])
    } else {
        user_content.to_string()
    };

    let tc_value = tool_choice.unwrap_or(serde_json::json!("auto"));

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
            temperature,
            top_p,
            max_tokens: cfg.ai.analysis_max_tokens,
            tools: Some(tools.to_vec()),
            tool_choice: Some(tc_value.clone()),
            thinking: Some(serde_json::json!({"type": "disabled"})),
        };

        debug!(
            attempt,
            model = %cfg.model,
            tools = ?tools_summary,
            tool_choice = %req.tool_choice.as_ref().map(|v| v.to_string()).unwrap_or_default(),
            user_content = %user_content_preview,
            "analyze_with_tools: request"
        );

        // 走唯一入口：网络、重试、熔断、状态校验都在 client 里
        let body = match crate::ai::client::chat_completion(&req) {
            Ok(body) => body,
            Err(error) => return Err(error.to_string()),
        };
        let prompt_name = tools_summary.first().copied().unwrap_or("analysis");
        track_usage(&body, prompt_name, &cfg.model);

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or("API returned empty choices")?;

        // 优先从 message.tool_calls 中提取结果
        let has_tool_calls = choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|tc| !tc.is_empty());
        let has_content = choice
            .message
            .content
            .as_ref()
            .is_some_and(|c| !c.is_empty());
        debug!(
            has_tool_calls,
            has_content, "analyze_with_tools: response analysis"
        );

        if let Some(tool_calls) = &choice.message.tool_calls
            && let Some(first_call) = tool_calls.first()
        {
            debug!(name = %first_call.function.name, args_len = first_call.function.arguments.len(),
                    "analyze_with_tools: got tool call");
            let args_str = &first_call.function.arguments;
            // 先尝试正常解析
            match serde_json::from_str::<serde_json::Value>(args_str) {
                Ok(args) => return Ok(args),
                Err(e) => {
                    // 尝试修复被截断的 JSON（finish_reason="length" 时常见）
                    debug!(error = %e, "analyze_with_tools: JSON parse failed, attempting repair");
                    if let Some(repaired) = repair_truncated_json(args_str) {
                        debug!("analyze_with_tools: JSON repaired successfully");
                        return Ok(repaired);
                    }
                    return Err(format!("Tool call arguments parse failed: {}", e));
                }
            }
        }

        // Fallback: 从文本内容中提取 JSON (兼容旧行为)
        let mut reply = choice.message.content.unwrap_or_default();
        debug!(
            reply_len = reply.len(),
            "analyze_with_tools: falling back to text extraction"
        );

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

        // 两次重试后仍无 tool_calls → 尝试按工具格式包裹纯文本
        if has_content {
            let reply_trimmed = reply.trim();
            if !reply_trimmed.is_empty()
                && let Ok(wrapped) = try_wrap_text_for_tools(reply_trimmed, tools)
            {
                return Ok(wrapped);
            }
        }

        return Err("No tool_calls and no JSON found in response".to_string());
    }

    unreachable!()
}

/// 尝试将纯文本包装为工具的 JSON 参数
///
/// 兼容模型直接输出消息文本而不调用 tool_calls 的情况，
/// 适用于 memory_review、decide_reply 等场景。
fn try_wrap_text_for_tools(text: &str, tools: &[Tool]) -> Result<serde_json::Value, ()> {
    for tool in tools {
        let params = &tool.function.parameters;
        let required = params.get("required").and_then(|r| r.as_array());
        let props = params.get("properties").and_then(|p| p.as_object());

        let Some(required) = required else { continue };
        let Some(props) = props else { continue };

        // 情况 1: 只有一个 required 参数且为 string → { param_name: text }
        if required.len() == 1 {
            let key = required[0].as_str().unwrap_or("");
            if let Some(schema) = props.get(key)
                && schema.get("type").and_then(|t| t.as_str()) == Some("string")
            {
                let mut map = serde_json::Map::new();
                map.insert(key.to_string(), serde_json::Value::String(text.to_string()));
                debug!(tool = %tool.function.name, key, "try_wrap_text_for_tools: wrapped as single param");
                return Ok(serde_json::Value::Object(map));
            }
        }

        // 情况 2: memory_review — { action: "keep", reason: text }
        if tool.function.name == "memory_review" {
            let wrapped = serde_json::json!({
                "action": "keep",
                "reason": text
            });
            debug!("try_wrap_text_for_tools: wrapped as memory_review");
            return Ok(wrapped);
        }

        // 情况 3: decide_reply — AI 直接输出文本而非调用工具
        if tool.function.name == "decide_reply" {
            let trimmed = text.trim();
            // 检测 AI 是否在表达"不想回复"的意图
            let silent_keywords = [
                "不回复",
                "不想回",
                "不应该回",
                "不接了",
                "不参与",
                "没必要回",
                "不需要回",
                "就不回",
                "我就不回",
                "不插嘴",
                "不凑热闹",
                "不搭话",
            ];
            let should_not_reply = silent_keywords.iter().any(|k| trimmed.contains(k));
            if should_not_reply {
                let wrapped = serde_json::json!({"reply": false, "reason": format!("fallback: AI表达不想回复 - {}", &trimmed[..trimmed.len().min(80)])});
                debug!("try_wrap_text_for_tools: wrapped as decide_reply (no reply)");
                return Ok(wrapped);
            }
            // 默认认为想回复（AI 输出了内容，通常意味着想说什么）
            let wrapped = serde_json::json!({"reply": true, "reason": "fallback: AI直接输出文本"});
            debug!("try_wrap_text_for_tools: wrapped as decide_reply (reply)");
            return Ok(wrapped);
        }
    }
    Err(())
}
