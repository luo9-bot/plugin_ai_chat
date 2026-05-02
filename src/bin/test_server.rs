use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::SystemTime;

// 引入插件的数据模块 (不引入 sender/cron 等需要 SDK 的模块)
use top_drluo_luo9_ai_chat::config;
use top_drluo_luo9_ai_chat::memory;
use top_drluo_luo9_ai_chat::working_memory;
use top_drluo_luo9_ai_chat::emotion;
use top_drluo_luo9_ai_chat::personality;
use top_drluo_luo9_ai_chat::ai;
use top_drluo_luo9_ai_chat::self_memory;
use top_drluo_luo9_ai_chat::state::State;
use top_drluo_luo9_ai_chat::DECIDE_REPLY_PROMPT;

// 测试日志
struct TestLog {
    entries: Mutex<Vec<serde_json::Value>>,
}

impl TestLog {
    fn new() -> Self {
        Self { entries: Mutex::new(Vec::new()) }
    }

    fn log(&self, entry: serde_json::Value) {
        self.entries.lock().unwrap().push(entry);
    }

    fn to_json(&self) -> String {
        serde_json::to_string_pretty(&*self.entries.lock().unwrap()).unwrap_or_else(|_| "[]".into())
    }
}

static LOG: once_cell::sync::Lazy<TestLog> = once_cell::sync::Lazy::new(|| TestLog::new());

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// AI 决定是否回复
fn test_decide_reply(group_id: u64, user_id: u64, message: &str, state: &mut State) -> (bool, String) {
    let cfg = config::get();

    // self_qq 未配置时，回复所有消息
    if cfg.self_qq == 0 {
        return (true, "self_qq=0, always reply".into());
    }

    // 检查是否 @了机器人 (CQ 码格式)
    let at_pattern = format!("[CQ:at,qq={}]", cfg.self_qq);
    if message.contains(&at_pattern) {
        return (true, "directed at me (CQ at)".into());
    }

    let mut context_parts = Vec::new();

    let personality = personality::get_prompt_context();
    if !personality.is_empty() {
        context_parts.push(personality);
    }

    let emotion_ctx = emotion::get_prompt_context(user_id);
    if !emotion_ctx.is_empty() {
        context_parts.push(emotion_ctx);
    }

    let memories = memory::get_context(user_id);
    if !memories.is_empty() {
        context_parts.push(memories);
    }

    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            context_parts.push(group_mem);
        }
    }

    let recent_history: Vec<String> = state.get_or_create_context(group_id, user_id).history.iter()
        .rev()
        .take(6)
        .map(|(role, content)| format!("[{}]: {}", role, content))
        .collect();
    if !recent_history.is_empty() {
        context_parts.push(format!("# 与该用户的历史对话\n{}", recent_history.join("\n")));
    }

    let bot_msgs: Vec<String> = state.get_recent_bot_messages(group_id, 600, 5)
        .into_iter().map(|m| m.to_string()).collect();
    if !bot_msgs.is_empty() {
        context_parts.push(format!("# 你在群里最近的消息\n{}", bot_msgs.join("\n")));
    }

    let wm_ctx = working_memory::get_context(group_id, 3600);
    if !wm_ctx.is_empty() {
        context_parts.push(wm_ctx);
    }

    // 检查是否在 follow-up 窗口内
    let in_follow_up = state.is_in_follow_up(group_id, 0, cfg.conversation.reply_follow_up_secs);

    // 从人格特质获取 verbosity 作为回复倾向指导
    let verbosity = personality::get_verbosity();
    let personality_hint = if verbosity > 0.7 {
        "你很喜欢聊天，大部分话题都想参与"
    } else if verbosity > 0.4 {
        "你适度参与群聊，选择性回复感兴趣的话题"
    } else {
        "你比较安静，只在明显相关时才回复"
    };

    let full_prompt = format!("{}\n\n{}", DECIDE_REPLY_PROMPT, context_parts.join("\n\n"));
    let content = format!(
        "{}\n\n需要判断是否回复的当前消息:\n[{}] {}",
        personality_hint, user_id, message
    );

    match ai::analyze(&full_prompt, &content) {
        Ok(raw) => {
            match ai::extract_json(&raw) {
                Some(json_str) => {
                    match serde_json::from_str::<serde_json::Value>(&json_str) {
                        Ok(v) => {
                            let reply = v.get("reply").and_then(|r| r.as_bool()).unwrap_or(in_follow_up);
                            let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
                            (reply, reason)
                        }
                        Err(_) => (in_follow_up, "JSON parse failed".into()),
                    }
                }
                None => (in_follow_up, "no JSON in response".into()),
            }
        }
        Err(e) => {
            eprintln!("[test_server] decide_reply AI error: {}, follow_up={}", e, in_follow_up);
            (in_follow_up, format!("AI error: {}", e))
        }
    }
}

/// 构建 decide_reply 的完整上下文 (复用 lib.rs 的逻辑)
fn build_decide_context(group_id: u64, user_id: u64, _message: &str, state: &mut State) -> Vec<String> {
    let mut parts = Vec::new();

    let personality_ctx = personality::get_prompt_context();
    if !personality_ctx.is_empty() {
        parts.push(personality_ctx);
    }

    let emotion_ctx = emotion::get_prompt_context(user_id);
    if !emotion_ctx.is_empty() {
        parts.push(emotion_ctx);
    }

    let memories = memory::get_context(user_id);
    if !memories.is_empty() {
        parts.push(memories);
    }

    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            parts.push(group_mem);
        }
    }

    let recent_history: Vec<String> = state.get_or_create_context(group_id, user_id).history.iter()
        .rev()
        .take(6)
        .map(|(role, content)| format!("[{}]: {}", role, content))
        .collect();
    if !recent_history.is_empty() {
        parts.push(format!("# 与该用户的历史对话\n{}", recent_history.join("\n")));
    }

    let bot_msgs: Vec<String> = state.get_recent_bot_messages(group_id, 600, 5)
        .into_iter().map(|m| m.to_string()).collect();
    if !bot_msgs.is_empty() {
        parts.push(format!("# 你在群里最近的消息\n{}", bot_msgs.join("\n")));
    }

    let wm_ctx = working_memory::get_context(group_id, 3600);
    if !wm_ctx.is_empty() {
        parts.push(wm_ctx);
    }

    parts
}

/// 构建 process_message 的 system prompt 上下文
fn build_process_context(user_id: u64, group_id: u64, history: &[(String, String)], state: &State) -> Vec<String> {
    let mut parts = Vec::new();

    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        parts.push(mem);
    }

    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            parts.push(group_mem);
        }
    }

    let pers = personality::get_prompt_context();
    if !pers.is_empty() {
        parts.push(pers);
    }

    let emo = emotion::get_prompt_context(user_id);
    if !emo.is_empty() {
        parts.push(emo);
    }

    let interaction_count = history.len();
    if interaction_count > 20 {
        parts.push("- 你们已经聊了很久了，关系很亲近，可以更自然随意".into());
    } else if interaction_count > 10 {
        parts.push("- 你们已经有一定的了解了".into());
    }

    // Bot 自己最近的消息 (帮助保持一致性)
    let bot_msgs: Vec<String> = state.get_recent_bot_messages(group_id, 600, 5)
        .into_iter().map(|m| m.to_string()).collect();
    if !bot_msgs.is_empty() {
        parts.push(format!("# 你在群里最近发过的消息\n{}", bot_msgs.join("\n")));
    }

    // 工作记忆 (群聊最近消息流)
    let wm_ctx = working_memory::get_context(group_id, 3600);
    if !wm_ctx.is_empty() {
        parts.push(wm_ctx);
    }

    parts
}

/// 处理模拟消息请求
fn handle_simulate(body: &str, state: &mut State) -> serde_json::Value {
    let req: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": format!("Invalid JSON: {}", e)}),
    };

    let group_id = req["group_id"].as_u64().unwrap_or(0);
    let user_id = req["user_id"].as_u64().unwrap_or(0);
    let message = req["message"].as_str().unwrap_or("");
    let nickname = req["nickname"].as_str().unwrap_or("");

    if user_id == 0 || message.is_empty() {
        return serde_json::json!({"error": "user_id and message are required"});
    }

    let start = now_millis();

    // 1. 记录工作记忆
    if group_id > 0 {
        working_memory::record(group_id, user_id, message, false);
    }

    // 2. 情绪分析 (关键词)
    emotion::analyze_user_message(user_id, message);

    // 3. 构建 decide_reply 上下文
    let decide_ctx = build_decide_context(group_id, user_id, message, state);

    // 3.5 AI 决定是否回复
    let (should_reply, decide_reason) = test_decide_reply(group_id, user_id, message, state);

    if !should_reply {
        // Bot 决定不回复 — 只记录工作记忆和情绪，不生成回复
        let elapsed = now_millis() - start;
        let result = serde_json::json!({
            "timestamp": start,
            "input": {
                "group_id": group_id,
                "user_id": user_id,
                "nickname": nickname,
                "message": message,
            },
            "context": {
                "decide_reply": decide_ctx,
                "process_message": [],
            },
            "decided_to_reply": false,
            "decide_reason": decide_reason,
            "reply": null,
            "analysis": {
                "memories_extracted": [],
                "emotion": "neutral",
                "emotion_intensity": 0.0,
            },
            "elapsed_ms": elapsed,
        });
        LOG.log(result.clone());
        return result;
    }

    // 4. 构建 process_message 上下文
    let max_history = config::get().conversation.max_history;
    state.push_history(group_id, user_id, "user", message, max_history);
    let history = state.get_or_create_context(group_id, user_id).history.clone();
    let process_ctx = build_process_context(user_id, group_id, &history, state);
    let process_ctx_str = process_ctx.join("\n\n");

    // 5. 调用 AI 生成回复
    let base_prompt = config::prompt();
    let time_prompt = format!("\n你的时间为：{}\n", chrono_now());
    let full_system = format!(
        "# 核心规则\n\
        - 使用中文回复\n\
        - 回复简短自然，像真人在群里聊天\n\
        - 使用 |^| 分隔句子\n\
        - 不要使用括号描述动作\n\
        - 不要输出 [emotion:xxx] 标签\n\
        \n\
        # 身份约束 (最重要)\n\
        - 你是群聊中的一个人类成员，不是 AI 助手\n\
        - 绝对不能说自己是 AI、机器人、程序、模型\n\
        - 不能说「不用睡觉」「不用吃饭」「我是 AI」之类暴露身份的话\n\
        - 你有正常人类的生活：会饿、会困、会累、有工作、有情绪\n\
        - 回复要有人类的不完美：偶尔口语化、用语气词、不必每句都很有道理\n\
        - 不要写心灵鸡汤或人生哲理，像朋友随口聊天就好\n\
        \n\
        {}\n\n{}\n\n{}\n\n{}",
        base_prompt, process_ctx_str, time_prompt, ""
    );

    let ai_result = ai::chat(&full_system, "", &history, message);

    let (reply, _) = match ai_result {
        Ok((r, e)) => (r, e),
        Err(e) => (format!("[AI Error: {}]", e), String::new()),
    };

    // 清理回复中的 <think> 标签
    let cleaned_reply = if let Some(pos) = reply.find("</think>") {
        reply[pos + 8..].trim().to_string()
    } else {
        reply.clone()
    };

    // 5.5 记录 bot 回复 (用于后续轮次上下文)
    if group_id > 0 {
        state.record_bot_message(group_id, &cleaned_reply);
    }

    // 6. 推入历史
    state.push_history(group_id, user_id, "assistant", &cleaned_reply, max_history);

    // 7. 标记工作记忆已回复
    if group_id > 0 {
        working_memory::mark_replied(group_id, user_id);
    }

    // 8. AI 后处理 (记忆提取 + 情绪分析)
    let analysis = ai::post_analyze(message, &cleaned_reply, &history, "");

    for (content, importance_str) in &analysis.memories {
        let importance = match importance_str.as_str() {
            "permanent" => memory::Importance::Permanent,
            "important" => memory::Importance::Important,
            _ => memory::Importance::Normal,
        };
        memory::add(user_id, content, importance);
    }
    emotion::update_from_analysis(user_id, &analysis.emotion, analysis.intensity);

    // 记忆纠错
    for correction in &analysis.corrections {
        match correction.target.as_str() {
            "self" => { self_memory::correct(&correction.old, &correction.new); }
            _ => { memory::correct(user_id, &correction.old, &correction.new); }
        }
    }

    let elapsed = now_millis() - start;

    let result = serde_json::json!({
        "timestamp": start,
        "input": {
            "group_id": group_id,
            "user_id": user_id,
            "nickname": nickname,
            "message": message,
        },
        "context": {
            "decide_reply": decide_ctx,
            "process_message": process_ctx,
        },
        "decided_to_reply": true,
        "decide_reason": decide_reason,
        "reply": cleaned_reply,
        "analysis": {
            "memories_extracted": analysis.memories,
            "emotion": analysis.emotion,
            "emotion_intensity": analysis.intensity,
        },
        "elapsed_ms": elapsed,
    });

    LOG.log(result.clone());
    result
}

/// 直接调用 AI API (不经过 ai::analyze 的低 token/temperature 限制)
/// 用于需要更可靠输出的场景 (orchestrator、generate_message)
fn ai_raw_call(system_prompt: &str, user_prompt: &str, max_tokens: u32, temperature: f64) -> Result<String, String> {
    use serde::{Serialize, Deserialize};

    #[derive(Serialize)]
    struct Req { model: String, messages: Vec<Msg>, max_tokens: u32, temperature: f64 }
    #[derive(Serialize, Deserialize, Clone)]
    struct Msg { role: String, content: String }
    #[derive(Deserialize)]
    struct Resp { choices: Vec<Choice> }
    #[derive(Deserialize)]
    struct Choice { message: Msg }

    let cfg = config::get();
    let req = Req {
        model: cfg.model.clone(),
        messages: vec![
            Msg { role: "system".into(), content: system_prompt.into() },
            Msg { role: "user".into(), content: user_prompt.into() },
        ],
        max_tokens,
        temperature,
    };
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    println!("[ai_raw] POST {} ({} bytes, max_tokens={}, temp={})", url, body.len(), max_tokens, temperature);
    let mut resp = ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(body.as_bytes())
        .map_err(|e| {
            eprintln!("[ai_raw] HTTP error: {}", e);
            format!("HTTP error: {}", e)
        })?;
    let text = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    // Safe truncation for UTF-8 (don't cut in middle of multi-byte char)
    let preview: String = text.chars().take(300).collect();
    println!("[ai_raw] response ({} bytes): {}", text.len(), preview);
    let parsed: Resp = serde_json::from_str(&text).map_err(|e| {
        let safe: String = text.chars().take(300).collect();
        format!("Parse error: {} | raw: {}", e, safe)
    })?;
    let reply = parsed.choices.into_iter().next().ok_or("empty choices")?.message.content;

    // 去除 <think> 标签
    let cleaned = if let Some(pos) = reply.find("</think>") {
        reply[pos + 8..].trim().to_string()
    } else {
        reply.trim().to_string()
    };

    if cleaned.is_empty() {
        return Err("AI returned empty response (possibly only thinking output)".into());
    }
    Ok(cleaned)
}

fn chrono_now() -> String {
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    let (year, month, day) = ai::days_to_ymd(days);
    format!("{}年{:02}月{:02}日{:02}时{:02}分{:02}秒", year, month, day, hour, minute, second)
}

/// AI 生成测试角色的下一句话
fn handle_generate_message(body: &str) -> serde_json::Value {
    let req: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": format!("Invalid JSON: {}", e)}),
    };

    let nickname = req["nickname"].as_str().unwrap_or("用户");
    let persona = req["persona"].as_str().unwrap_or("");
    let recent_chat = req["recent_chat"].as_str().unwrap_or("");
    let topic_hint = req["topic_hint"].as_str().unwrap_or("");

    let system_prompt = format!(
        r#"你正在模拟一个群聊中的用户，用于测试 AI 聊天机器人的表现。

你的身份:
- 昵称: {}
- 性格描述: {}

规则:
- 你是一个真实的人类用户，不是 AI
- 说话自然随意，像真人在群聊里聊天
- 可以用简短的句子、口语化表达
- 不要用括号描述动作
- 不要暴露自己是 AI 或在扮演角色
- 只输出你要发的那一条消息，不要加任何前缀或解释
- 消息长度通常在 1-30 字之间，偶尔可以长一点"#,
        nickname, if persona.is_empty() { "普通群友" } else { persona }
    );

    let user_prompt = if !recent_chat.is_empty() {
        format!(
            "以下是最近的群聊记录:\n{}\n\n{}现在你想说什么？只输出你的消息内容。",
            recent_chat,
            if !topic_hint.is_empty() { format!("话题方向: {}\n", topic_hint) } else { String::new() }
        )
    } else {
        format!(
            "{}群里刚开聊，你想说什么？只输出你的消息内容。",
            if !topic_hint.is_empty() { format!("话题方向: {}\n", topic_hint) } else { String::new() }
        )
    };

    // 重试 3 次
    for _attempt in 0..3 {
        match ai_raw_call(&system_prompt, &user_prompt, 500, 1.0) {
            Ok(reply) => {
                let cleaned = if let Some(pos) = reply.find("</think>") {
                    reply[pos + 8..].trim().to_string()
                } else {
                    reply.trim().to_string()
                };
                // 去掉可能的引号包裹
                let msg = cleaned.trim_matches(|c| c == '"' || c == '"' || c == '"').trim();
                if !msg.is_empty() {
                    return serde_json::json!({"message": msg});
                }
            }
            Err(e) => {
                eprintln!("[test_server] generate_message attempt failed: {}", e);
            }
        }
    }
    serde_json::json!({"error": "AI generate failed after 3 attempts"})
}

/// AI 自动测试调度：决定谁说话、说什么
fn handle_auto_play_step(body: &str) -> serde_json::Value {
    println!("[auto_play_step] received body ({} bytes)", body.len());

    let req: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[auto_play_step] JSON parse error: {}", e);
            return serde_json::json!({"error": format!("Invalid JSON: {}", e)});
        }
    };

    let characters = match req.get("characters").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return serde_json::json!({"error": "characters array required"}),
    };
    let default_arr1 = vec![];
    let default_arr2 = vec![];
    let default_arr3 = vec![];
    let chat_history = req.get("chat_history").and_then(|v| v.as_array()).unwrap_or(&default_arr1);
    let objectives = req.get("objectives").and_then(|v| v.as_array()).unwrap_or(&default_arr2);
    let completed = req.get("completed").and_then(|v| v.as_array()).unwrap_or(&default_arr3);
    println!("[auto_play_step] chars={}, history={}, objectives={}, completed={}", characters.len(), chat_history.len(), objectives.len(), completed.len());

    // 构建角色描述
    let char_desc: Vec<String> = characters.iter().enumerate().map(|(i, c)| {
        let name = c["name"].as_str().unwrap_or("?");
        let persona = c["persona"].as_str().unwrap_or("");
        format!("{}. {} — {}", i + 1, name, if persona.is_empty() { "普通群友" } else { persona })
    }).collect();

    // 构建聊天记录摘要
    let history_lines: Vec<String> = chat_history.iter().rev().take(15).map(|m| {
        let role = m["role"].as_str().unwrap_or("?");
        let name = m["name"].as_str().unwrap_or(role);
        let content = m["content"].as_str().unwrap_or("");
        format!("{}: {}", name, content)
    }).rev().collect();

    // 未完成的测试目标
    let remaining: Vec<String> = objectives.iter()
        .filter(|o| !completed.contains(o))
        .map(|o| o.as_str().unwrap_or("").to_string())
        .collect();

    // 找出上一个发言的角色
    let last_speaker = chat_history.iter().rev().find(|m| {
        m["role"].as_str() == Some("char")
    }).and_then(|m| m["name"].as_str()).unwrap_or("(无)");

    let self_qq = config::get().self_qq;
    let at_code = format!("[CQ:at,qq={}]", self_qq);

    let system_prompt = format!(r#"你是一个群聊测试调度器。你的任务是决定下一步谁说话、说什么，以覆盖所有测试目标。

严格规则：
1. 你必须选择一个角色来发言，并生成该角色会说的话
2. 对话要自然真实，像真人聊天——有接话、有转折、有闲聊、有互怼
3. 【严格】同一个角色不能连续发言两次，必须轮换！如果上一条是角色A发的，这次必须选B或C
4. 有时候 A 和 B 在聊天，不涉及 bot；有时候直接对 bot 说话
5. 要创造多样化的场景：闲聊、问问题、表达情绪、提到别人、连续发消息等
6. 生成的消息应该简短自然，10-30字居多
7. 当角色想直接对 bot 说话、提问 bot、回复 bot 时，必须在消息中使用 {} 来 @ bot，例如: "{} 你喜欢什么？"
8. 【严格】每3条消息中最多只有1条 @ bot，大部分消息是角色之间的对话，不需要 @ bot
9. 角色之间也可以聊天，不需要每次都涉及 bot

返回 JSON（不要输出其他内容）:
{{
  "speaker_index": 0,
  "message": "角色要说的话",
  "reasoning": "为什么选这个角色说这句话（简短）"
}}"#, at_code, at_code);

    let user_prompt = format!(
        "群聊角色:\n{}\n\n最近聊天记录:\n{}\n\n上一个发言者: {}\n\n需要测试的功能 (未完成):\n{}\n\n已覆盖的功能:\n{}\n\n\
        请决定下一步：谁说话？说什么？\n\n\
        重要：【严格】不能选 {} 再次发言！必须选其他角色。\n\n\
        根据未完成的测试目标设计对话：\n\
        - memory_test: 让角色告诉 bot 个人信息（名字、喜好、生日等），稍后让另一个角色问 bot 关于那个人的事\n\
        - emotion_detection: 让角色表达强烈情绪（生气、难过、震惊等），不是简单的开心\n\
        - group_memory_sharing: 让角色 A 对 bot 提到角色 B 的信息，看 bot 是否知道\n\
        - follow_up: 在 bot 回复后，让另一个角色接话评论，形成自然对话\n\
        - cross_conversation: 让两个角色聊一个话题（不涉及 bot），同时另一个角色突然问 bot 问题\n\
        - batch_messages: 让同一角色连续发2-3条短消息（每条都要单独生成，这次只生成第一条）\n\
        - working_memory: 让角色发消息但不 @ bot（bot 可能不回复），稍后验证 bot 是否记得",
        char_desc.join("\n"),
        if history_lines.is_empty() { "(刚开始，还没有对话)".to_string() } else { history_lines.join("\n") },
        last_speaker,
        if remaining.is_empty() { "(全部完成，继续自然聊天测试边界情况)".to_string() } else { remaining.join(", ") },
        if completed.is_empty() { "(无)".to_string() } else {
            completed.iter().map(|c| c.as_str().unwrap_or("")).collect::<Vec<_>>().join(", ")
        },
        last_speaker
    );

    // 最多重试 3 次
    let mut last_err = String::new();
    for attempt in 0..3u32 {
        println!("[auto_play_step] AI call attempt {}/3...", attempt + 1);
        let result = ai_raw_call(&system_prompt, &user_prompt, 204800 , 0.8);
        match result {
            Ok(raw) => {
                println!("[auto_play_step] AI responded ({} chars): {}", raw.len(), raw.chars().take(200).collect::<String>());
                let json_str = match ai::extract_json(&raw) {
                    Some(j) => {
                        println!("[auto_play_step] extracted JSON ({} chars)", j.len());
                        j
                    }
                    None => {
                        last_err = format!("AI response has no JSON: {}", raw.chars().take(200).collect::<String>());
                        eprintln!("[auto_play_step] {}", last_err);
                        continue;
                    }
                };
                match serde_json::from_str::<serde_json::Value>(&json_str) {
                    Ok(parsed) => {
                        let speaker_idx = parsed["speaker_index"].as_u64().unwrap_or(0) as usize;
                        let message = parsed["message"].as_str().unwrap_or("").to_string();
                        let reasoning = parsed["reasoning"].as_str().unwrap_or("").to_string();

                        if speaker_idx >= characters.len() {
                            return serde_json::json!({"error": format!("speaker_index {} out of range", speaker_idx)});
                        }
                        if message.is_empty() {
                            last_err = "AI generated empty message".to_string();
                            continue;
                        }

                        let speaker = &characters[speaker_idx];
                        return serde_json::json!({
                            "speaker_index": speaker_idx,
                            "speaker_name": speaker["name"].as_str().unwrap_or("?"),
                            "speaker_id": speaker["id"].as_u64().unwrap_or(0),
                            "message": message,
                            "reasoning": reasoning,
                        });
                    }
                    Err(e) => {
                        last_err = format!("JSON parse failed: {}", e);
                        eprintln!("[auto_play_step] {}", last_err);
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = format!("AI error: {}", e);
                eprintln!("[auto_play_step] {}", last_err);
                continue;
            }
        }
    }
    serde_json::json!({"error": format!("Orchestrator failed after 3 attempts: {}", last_err)})
}

/// 获取当前状态
fn handle_state() -> serde_json::Value {
    let mem_count = memory::load_user_count();
    let wm_groups = working_memory::group_count();

    // 读取各 JSON 文件
    let data_dir = config::data_dir();
    let read_json = |name: &str| -> serde_json::Value {
        let path = data_dir.join(name);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null)
    };

    serde_json::json!({
        "memory": read_json("memory.json"),
        "working_memory": read_json("working_memory.json"),
        "emotion": read_json("emotion.json"),
        "archive": read_json("archive.json"),
        "stats": {
            "memory_users": mem_count,
            "working_memory_groups": wm_groups,
        }
    })
}

/// 简易 HTTP 解析
fn parse_request(request: &str) -> (String, String, String) {
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let parts: Vec<&str> = lines[0].split_whitespace().collect();
    let method = parts.get(0).unwrap_or(&"").to_string();
    let path = parts.get(1).unwrap_or(&"").to_string();

    // 找 Content-Length
    let mut content_length = 0;
    for line in &lines[1..] {
        if line.to_lowercase().starts_with("content-length:") {
            if let Some(val) = line.split(':').nth(1) {
                content_length = val.trim().parse().unwrap_or(0);
            }
        }
    }

    // 找 body (空行之后)
    let body = if let Some(pos) = request.find("\r\n\r\n") {
        let raw_body = &request[pos + 4..];
        if raw_body.len() >= content_length {
            raw_body[..content_length].to_string()
        } else {
            raw_body.to_string()
        }
    } else {
        String::new()
    };

    (method, path, body)
}

fn response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n{}",
        status, content_type, body.len(), body
    )
}

fn main() {
    config::init();
    println!("[test_server] config loaded, model: {}", config::get().model);
    println!("[test_server] data dir: {:?}", config::data_dir());

    let state = Mutex::new(State::new());
    let listener = TcpListener::bind("127.0.0.1:18923").expect("Failed to bind port 18923");
    println!("[test_server] listening on http://127.0.0.1:18923");
    println!("[test_server] open test/debug.html in browser and click 'Connect'");

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[test_server] accept error: {}", e);
                continue;
            }
        };

        // Set read timeout so we don't hang on broken connections
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok();
        stream.set_write_timeout(Some(std::time::Duration::from_secs(10))).ok();

        let mut buf = vec![0u8; 131072];
        let mut total = 0;
        // Read headers first
        loop {
            let n = match stream.read(&mut buf[total..]) {
                Ok(n) if n > 0 => n,
                Ok(_) => break,
                Err(_) => break,
            };
            total += n;
            // Check if we've found the end of headers
            if total >= 4 && buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if total >= buf.len() { break; }
        }

        if total == 0 {
            eprintln!("[test_server] empty read, skipping");
            continue;
        }

        let request = String::from_utf8_lossy(&buf[..total]).to_string();
        let (method, path, mut body) = parse_request(&request);

        // If we have Content-Length but didn't read the full body, read more
        let content_length = request.lines()
            .find(|l| l.to_lowercase().starts_with("content-length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);

        if content_length > body.len() {
            // Need to read more body data
            let body_start = request.find("\r\n\r\n").map(|p| p + 4).unwrap_or(total);
            let already_read = total - body_start;
            let remaining = content_length.saturating_sub(already_read);
            if remaining > 0 {
                let mut extra = vec![0u8; remaining];
                let mut extra_total = 0;
                while extra_total < remaining {
                    match stream.read(&mut extra[extra_total..]) {
                        Ok(0) => break,
                        Ok(n) => extra_total += n,
                        Err(_) => break,
                    }
                }
                let mut full_body = body.into_bytes();
                full_body.extend_from_slice(&extra[..extra_total]);
                body = String::from_utf8_lossy(&full_body).to_string();
            }
        }

        println!("[test_server] {} {} (headers: {} bytes, body: {} bytes, content_length: {})", method, path, total, body.len(),
            request.lines().find(|l| l.to_lowercase().starts_with("content-length:")).unwrap_or("?"));

        let resp = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match (method.as_str(), path.as_str()) {
                ("OPTIONS", _) => {
                    response("200 OK", "text/plain", "")
                }
                ("POST", "/api/simulate") => {
                    println!("[test_server] /api/simulate starting...");
                    let mut s = state.lock().unwrap();
                    let result = handle_simulate(&body, &mut s);
                    let json = result.to_string();
                    println!("[test_server] /api/simulate done ({} bytes)", json.len());
                    response("200 OK", "application/json", &json)
                }
                ("GET", "/api/state") => {
                    let result = handle_state();
                    response("200 OK", "application/json", &result.to_string())
                }
                ("GET", "/api/log") => {
                    response("200 OK", "application/json", &LOG.to_json())
                }
                ("POST", "/api/reset") => {
                    *state.lock().unwrap() = State::new();
                    response("200 OK", "application/json", r#"{"ok":true}"#)
                }
                ("GET", "/api/config") => {
                    let cfg = config::get();
                    let info = serde_json::json!({
                        "model": cfg.model,
                        "self_qq": cfg.self_qq,
                        "admin_qq": cfg.admin_qq,
                    });
                    response("200 OK", "application/json", &info.to_string())
                }
                ("POST", "/api/generate_message") => {
                    println!("[test_server] /api/generate_message starting...");
                    let result = handle_generate_message(&body);
                    let json = result.to_string();
                    println!("[test_server] /api/generate_message done ({} bytes)", json.len());
                    response("200 OK", "application/json", &json)
                }
                ("POST", "/api/auto_play_step") => {
                    println!("[test_server] /api/auto_play_step starting...");
                    let result = handle_auto_play_step(&body);
                    let json = result.to_string();
                    println!("[test_server] /api/auto_play_step done ({} bytes): {}", json.len(), json.chars().take(200).collect::<String>());
                    response("200 OK", "application/json", &json)
                }
                _ => {
                    response("404 Not Found", "text/plain", "Not Found")
                }
            }
        }));

        match resp {
            Ok(resp) => {
                if let Err(e) = stream.write_all(resp.as_bytes()) {
                    eprintln!("[test_server] write error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("[test_server] PANIC in handler: {:?}", e);
                let err_resp = response("500 Internal Server Error", "application/json",
                    r#"{"error":"server panic, check terminal logs"}"#);
                let _ = stream.write_all(err_resp.as_bytes());
            }
        }
    }
}
