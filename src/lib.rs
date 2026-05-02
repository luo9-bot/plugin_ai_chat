pub mod ai;
pub mod archive;
pub mod config;
pub mod cron;
pub mod emotion;
pub mod memory;
pub mod personality;
pub mod proactive;
pub mod sender;
pub mod state;
pub mod working_memory;

use luo9_sdk::bus::Bus;
use luo9_sdk::payload::*;
use std::cell::RefCell;
use std::thread;
use std::time::Duration;

/// AI 群聊回复决策提示词
const DECIDE_REPLY_PROMPT: &str = r#"你是一个群聊中的成员，需要判断是否要回复当前消息。

根据以下信息做判断:
1. 消息是否是发给你的（@你、提到你、回复你的消息）
2. 消息是否是你正在参与的对话的延续
3. 消息内容是否有趣到值得你插话
4. 你的人格特质和当前情绪
5. 你刚说过的话是否被对方回应（非常重要！）

返回 JSON（不要输出其他内容）:
{"reply": true/false, "reason": "简短原因"}

判断标准:
- 如果消息明显是发给你的 → reply: true
- 如果你刚刚说了某句话（见"你在群里最近的消息"），而现在有人对你的话做出回应、追问、评论 → reply: true
- 如果消息是两个人之间的对话，不是发给你的 → reply: false
- 如果消息是一般性的群聊内容（牢骚、感叹、转发等）→ 根据你的人格决定是否参与
- 如果你和发送者正在聊天 → reply: true

特别注意: 如果"你在群里最近的消息"中包含你刚说过的话题，而当前消息明显是在回应或追问那个话题，你应该回复。比如你刚说了"吃夜宵"，别人问"吃的什么"，这明显是对你说话的回应。

注意: 不要每次都回复，像真人一样选择性参与对话。"#;

thread_local! {
    static STATE: RefCell<state::State> = RefCell::new(state::State::new());
}

pub(crate) fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut state::State) -> R,
{
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// 上次主动消息检查时间
static mut LAST_PROACTIVE_CHECK: u64 = 0;
/// 上次记忆审查时间
static mut LAST_MEMORY_REVIEW: u64 = 0;

// ── 插件入口 ────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    config::init();
    println!("[ai_chat] plugin loaded, model: {}", config::get().model);

    let msg_sub = Bus::topic("luo9_message").subscribe().unwrap();
    let task_sub = Bus::topic("luo9_task").subscribe().unwrap();
    let msg_topic = Bus::topic("luo9_message");
    let task_topic = Bus::topic("luo9_task");

    loop {
        if let Some(json) = msg_topic.pop(msg_sub) {
            if let Some(BusPayload::Message(msg)) = BusPayload::parse(&json) {
                match msg.message_type {
                    MsgType::Group => {
                        handle_group_msg(msg.group_id.unwrap_or(0), msg.user_id, &msg.message);
                    }
                    MsgType::Private => {
                        handle_private_msg(msg.user_id, &msg.message);
                    }
                    _ => {}
                }
            }
        }

        if let Some(json) = task_topic.pop(task_sub) {
            cron::handle_task_event(&json);
        }

        process_expired_batches();

        // 每60秒检查一次主动消息和情绪衰减
        check_periodic();

        thread::sleep(Duration::from_millis(1));
    }
}

// ── 周期性检查 ──────────────────────────────────────────────────

fn check_periodic() {
    let now = now_secs();
    let last = unsafe { LAST_PROACTIVE_CHECK };
    let interval = config::get().proactive.check_interval;
    if now.saturating_sub(last) < interval {
        return;
    }
    unsafe { LAST_PROACTIVE_CHECK = now; }

    // 情绪衰减 + 主动消息检查
    with_state(|s| {
        // 私聊活跃用户
        let private_users: Vec<(u64, u64)> = s.active.iter().map(|&uid| (uid, 0u64)).collect();
        // 群聊: 从批次中获取最近交互的用户
        let group_users: Vec<(u64, u64)> = s.batches.iter()
            .map(|(&uid, batch)| (uid, batch.group_id))
            .collect();

        let mut all_users: Vec<(u64, u64)> = private_users;
        all_users.extend(group_users);
        all_users.sort_unstable();
        all_users.dedup();

        for (user_id, group_id) in all_users {
            emotion::decay(user_id);
            proactive::check_proactive_messages(user_id, group_id);
        }
    });

    // 定期记忆审查 (每小时一次)
    let last_review = unsafe { LAST_MEMORY_REVIEW };
    if now.saturating_sub(last_review) >= 3600 {
        unsafe { LAST_MEMORY_REVIEW = now; }
        memory::ai_review_all();
    }

    // 工作记忆清理 (每次周期检查都执行，轻量级)
    let expire_hours = config::get().memory.working_memory_expire_hours;
    working_memory::cleanup(expire_hours * 3600);
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── 消息处理 ────────────────────────────────────────────────────

/// 检查用户是否是管理员
fn is_admin(user_id: u64) -> bool {
    let admin = config::get().admin_qq;
    admin == 0 || admin == user_id
}

/// 检查群消息是否需要回复
///
/// 判断逻辑:
/// 1. @了机器人 → true
/// 2. 机器人刚在群里回过话 (群级跟进时间内) → true
/// 3. 其他 → false
fn should_reply_in_group(group_id: u64, _user_id: u64, msg: &str) -> bool {
    let cfg = config::get();

    // 未配置 self_qq 时，回复所有消息 (向后兼容)
    if cfg.self_qq == 0 {
        return true;
    }

    // 检查是否 @了机器人
    let at_pattern = format!("[CQ:at,qq={}]", cfg.self_qq);
    if msg.contains(&at_pattern) {
        return true;
    }

    // 检查群级对话跟进: 机器人刚在群里回过话，任何人都可以接话
    let follow_up = cfg.conversation.reply_follow_up_secs;
    if with_state(|s| s.is_in_follow_up(group_id, 0, follow_up)) {
        return true;
    }

    false
}

fn handle_group_msg(group_id: u64, user_id: u64, msg: &str) {
    let trimmed = msg.trim();

    // ── 管理员专属控制命令 ──
    if is_admin(user_id) {
        match trimmed {
            "开!" | "开启对话" => {
                let already = with_state(|s| s.active_groups.contains(&group_id));
                if already {
                    sender::send_msg(group_id, user_id, &config::get().messages.start.redo);
                    return;
                }
                with_state(|s| { s.active_groups.insert(group_id); });
                sender::send_msg(group_id, user_id, &config::get().messages.start.success);
                return;
            }
            "停!" | "关闭对话" => {
                let active = with_state(|s| s.active_groups.contains(&group_id));
                if !active {
                    sender::send_msg(group_id, user_id, &config::get().messages.stop.redo);
                    return;
                }
                with_state(|s| { s.active_groups.remove(&group_id); });
                sender::send_msg(group_id, user_id, &config::get().messages.stop.success);
                return;
            }
            _ => {}
        }

        // 人格/主动对话管理命令 (管理员专属)
        if let Some(reply) = handle_personality_command(trimmed) {
            sender::send_msg(group_id, user_id, &reply);
            return;
        }
        if let Some(reply) = handle_proactive_command(trimmed) {
            sender::send_msg(group_id, user_id, &reply);
            return;
        }
    }

    // ── 群组未激活则不处理 ──
    let group_active = with_state(|s| s.active_groups.contains(&group_id));
    if !group_active {
        return;
    }

    // ── 记忆管理命令 (所有用户可用) ──
    if let Some(reply) = memory::check_forget_command(user_id, trimmed) {
        sender::send_msg(group_id, user_id, &reply);
        return;
    }

    // ── 记录用户交互 + 情绪分析 + 工作记忆 (无论是否回复) ──
    proactive::record_user_reply(user_id);
    emotion::analyze_user_message(user_id, trimmed);
    working_memory::record(group_id, user_id, trimmed, false);

    // ── 判断是否需要回复 ──
    if !should_reply_in_group(group_id, user_id, trimmed) {
        return;
    }

    // 加入批次
    with_state(|s| s.append_batch(user_id, group_id, trimmed));
}

fn handle_private_msg(user_id: u64, msg: &str) {
    let trimmed = msg.trim();

    // 控制命令
    if let Some(reply) = handle_control_command(0, user_id, trimmed) {
        sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = memory::check_forget_command(user_id, trimmed) {
        sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = handle_personality_command(trimmed) {
        sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = handle_proactive_command(trimmed) {
        sender::send_msg(0, user_id, &reply);
        return;
    }

    if with_state(|s| s.active.contains(&user_id)) {
        proactive::record_user_reply(user_id);
        emotion::analyze_user_message(user_id, trimmed);
        with_state(|s| s.append_batch(user_id, 0, trimmed));
    }
}

// ── 控制命令 ────────────────────────────────────────────────────

fn handle_control_command(_group_id: u64, user_id: u64, msg: &str) -> Option<String> {
    match msg {
        "开!" | "开启对话" => {
            let already = with_state(|s| s.active.contains(&user_id));
            if already {
                return Some(config::get().messages.start.redo.clone());
            }
            with_state(|s| { s.active.insert(user_id); });
            Some(config::get().messages.start.success.clone())
        }
        "停!" | "关闭对话" => {
            let active = with_state(|s| s.active.contains(&user_id));
            if !active {
                return Some(config::get().messages.stop.redo.clone());
            }
            with_state(|s| {
                s.active.remove(&user_id);
                s.batches.remove(&user_id);
            });
            Some(config::get().messages.stop.success.clone())
        }
        "遗忘对话" => {
            let has = with_state(|s| s.contexts.contains_key(&user_id));
            if !has {
                return Some(config::get().messages.forget.fail.clone());
            }
            let list = with_state(|s| {
                let history = &s.contexts[&user_id].history;
                if history.is_empty() {
                    return None;
                }
                let list = history
                    .iter()
                    .enumerate()
                    .map(|(i, (role, content))| format!("{}. [{}] {}", i + 1, role, content))
                    .collect::<Vec<_>>()
                    .join("\n");
                s.forget_user(user_id);
                Some(list)
            });
            match list {
                Some(list) => Some(format!("{}\n\n{}", config::get().messages.forget.success, list)),
                None => Some(config::get().messages.forget.fail.clone()),
            }
        }
        "重启对话" => {
            let has = with_state(|s| s.contexts.contains_key(&user_id));
            if has {
                with_state(|s| s.forget_user(user_id));
                memory::forget_all(user_id);
                Some(config::get().messages.restart.success.clone())
            } else {
                Some(config::get().messages.restart.redo.clone())
            }
        }
        _ => None,
    }
}

// ── 人格管理命令 ────────────────────────────────────────────────

fn handle_personality_command(msg: &str) -> Option<String> {
    if msg == "查看人格" {
        let ctx = personality::get_prompt_context();
        return Some(format!("当前人格设定:\n{}", ctx));
    }

    if msg == "人格模板" {
        let templates = ["温柔体贴", "幽默风趣", "理性分析", "傲娇毒舌", "元气活泼", "安静内敛"];
        return Some(format!("可用人格模板:\n{}", templates.join("\n")));
    }

    if let Some(name) = msg.strip_prefix("切换人格:") {
        let name = name.trim();
        return Some(personality::apply_template(name).unwrap_or_else(|e| e));
    }

    if let Some(rest) = msg.strip_prefix("调整特质:") {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            if let Ok(value) = parts[1].parse::<f32>() {
                return Some(personality::adjust_trait(parts[0], value).unwrap_or_else(|e| e));
            }
        }
        return Some("格式: 调整特质:特质名 数值 (0.0~1.0)".into());
    }

    if let Some(name) = msg.strip_prefix("保存人格:") {
        return Some(personality::save_snapshot(name.trim()).unwrap_or_else(|e| e));
    }

    if let Some(name) = msg.strip_prefix("加载人格:") {
        return Some(personality::load_snapshot(name.trim()).unwrap_or_else(|e| e));
    }

    if msg == "人格列表" {
        let list = personality::list_snapshots();
        if list.is_empty() {
            return Some("没有保存的人格快照".into());
        }
        return Some(format!("已保存的人格:\n{}", list.join("\n")));
    }

    None
}

// ── 主动对话命令 ────────────────────────────────────────────────

fn handle_proactive_command(msg: &str) -> Option<String> {
    match msg {
        "开启主动对话" => {
            proactive::set_enabled(true);
            return Some("已开启主动对话".into());
        }
        "关闭主动对话" => {
            proactive::set_enabled(false);
            return Some("已关闭主动对话".into());
        }
        _ => {}
    }

    if let Some(rest) = msg.strip_prefix("设置免打扰:") {
        let parts: Vec<&str> = rest.splitn(2, '-').collect();
        if parts.len() == 2 {
            if let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                proactive::set_quiet_hours(start, end);
                return Some(format!("已设置免打扰: {}时 - {}时", start, end));
            }
        }
        return Some("格式: 设置免打扰:23-7".into());
    }

    if let Some(rest) = msg.strip_prefix("提醒我:") {
        // 提醒我:MM-DD 描述
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            proactive::add_date_reminder(0, parts[0], parts[1]);
            return Some(format!("已添加日期提醒: {} {}", parts[0], parts[1]));
        }
        return Some("格式: 提醒我:MM-DD 描述".into());
    }

    None
}

// ── 批次处理 ────────────────────────────────────────────────────

fn process_expired_batches() {
    let cfg = config::get();
    let timeout = cfg.conversation.batch_timeout_ms;

    // 收集所有过期批次: 先扫描获取 (uid, group_id)，再取出 messages
    let expired: Vec<(u64, u64, String)> = {
        let mut result = Vec::new();
        with_state(|s| {
            let expired_uids: Vec<(u64, u64)> = s.batches.iter()
                .filter(|(_, batch)| batch.last_update.elapsed().as_millis() >= timeout as u128)
                .map(|(&uid, batch)| (uid, batch.group_id))
                .collect();
            for (uid, group_id) in expired_uids {
                if let Some(msgs) = s.take_batch_for_processing(uid) {
                    result.push((uid, group_id, msgs));
                }
            }
        });
        result
    };

    if expired.is_empty() {
        return;
    }

    // 按群组聚合: 同一群的所有消息一起做 AI 决策
    let self_qq = cfg.self_qq;
    let at_pattern = if self_qq > 0 { format!("[CQ:at,qq={}]", self_qq) } else { String::new() };

    let mut group_msgs: std::collections::HashMap<u64, Vec<(u64, String)>> = std::collections::HashMap::new();
    let mut private_batches: Vec<(u64, String)> = Vec::new();

    for (user_id, group_id, messages) in expired {
        if group_id > 0 {
            group_msgs.entry(group_id).or_default().push((user_id, messages));
        } else {
            private_batches.push((user_id, messages));
        }
    }

    // 处理私聊批次 (直接回复)
    for (user_id, messages) in private_batches {
        process_message(user_id, 0, &messages);
        // 检查处理期间是否有新消息
        check_new_messages(user_id, 0);
    }

    // 处理群聊批次: 把整个群的消息作为上下文一起做 AI 决策
    for (group_id, user_msgs) in group_msgs {
        let group_context: Vec<String> = user_msgs.iter()
            .map(|(uid, msg)| format!("[{}] {}", uid, msg))
            .collect();
        let context_str = group_context.join("\n");

        for (user_id, messages) in user_msgs {
            // @消息直接回复
            if self_qq > 0 && messages.contains(&at_pattern) {
                process_message(user_id, group_id, &messages);
                check_new_messages(user_id, group_id);
                continue;
            }

            // 非@消息: AI 决策 (传入整个群的上下文)
            if decide_reply(group_id, user_id, &messages, &context_str) {
                process_message(user_id, group_id, &messages);
            }
            // 检查处理期间是否有新消息到达
            check_new_messages(user_id, group_id);
        }
    }
}

/// 检查处理期间是否有新消息到达，如有则处理
/// 类似人类: 你回复完后发现对方又发了新消息，会接着回复
fn check_new_messages(user_id: u64, group_id: u64) {
    let timeout = config::get().conversation.batch_timeout_ms;
    // 短暂等待让新消息有机会到达 (类似人类回复后的停顿)
    // 最多等 2 秒，每 200ms 检查一次
    let max_wait = 2000u64.min(timeout);
    let mut waited = 0u64;
    while waited < max_wait {
        thread::sleep(Duration::from_millis(200));
        waited += 200;
        // 检查是否已有新消息
        let has_new = with_state(|s| s.batches.contains_key(&user_id));
        if has_new {
            break;
        }
    }

    let new_msgs = with_state(|s| s.take_batch_for_processing(user_id));
    if let Some(msgs) = new_msgs {
        // 新消息到达，重新判断是否需要回复
        if group_id > 0 {
            let self_qq = config::get().self_qq;
            let at_pattern = if self_qq > 0 { format!("[CQ:at,qq={}]", self_qq) } else { String::new() };
            if self_qq > 0 && msgs.contains(&at_pattern) {
                process_message(user_id, group_id, &msgs);
            } else if decide_reply(group_id, user_id, &msgs, &msgs) {
                process_message(user_id, group_id, &msgs);
            }
        } else {
            process_message(user_id, 0, &msgs);
        }
    }
}

/// AI 驱动的群聊回复决策
///
/// 综合记忆、对话上下文、人格特质和消息内容，判断是否需要回复
/// group_context: 同一群中所有待处理消息的拼接，用于理解连续对话
fn decide_reply(group_id: u64, user_id: u64, message: &str, group_context: &str) -> bool {
    let cfg = config::get();

    // self_qq 未配置时，回复所有消息
    if cfg.self_qq == 0 {
        return true;
    }

    // 构建决策上下文
    let mut context_parts = Vec::new();

    // 1. 人格信息
    let personality = personality::get_prompt_context();
    if !personality.is_empty() {
        context_parts.push(personality);
    }

    // 2. 情绪状态
    let emotion_ctx = emotion::get_prompt_context(user_id);
    if !emotion_ctx.is_empty() {
        context_parts.push(emotion_ctx);
    }

    // 3. 相关记忆
    let memories = memory::get_context(user_id);
    if !memories.is_empty() {
        context_parts.push(memories);
    }

    // 3.5 群内其他成员的记忆 (解决"A提到B时不知道B是谁"的问题)
    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            context_parts.push(group_mem);
        }
    }

    // 4. 与该用户的历史对话
    let recent_history = with_state(|s| {
        s.get_or_create_context(user_id).history.iter()
            .rev()
            .take(6)
            .map(|(role, content)| format!("[{}]: {}", role, content))
            .collect::<Vec<_>>()
    });
    if !recent_history.is_empty() {
        context_parts.push(format!("# 与该用户的历史对话\n{}", recent_history.join("\n")));
    }

    // 5. 机器人在群里最近的消息 (关键：让用户回应能被识别为"接话")
    let bot_msgs = with_state(|s| {
        s.get_recent_bot_messages(group_id, 600, 5)
            .into_iter().map(|m| m.to_string()).collect::<Vec<_>>()
    });
    if !bot_msgs.is_empty() {
        context_parts.push(format!("# 你在群里最近的消息\n{}", bot_msgs.join("\n")));
    }

    // 6. 群聊工作记忆 (所有消息，无论是否回复)
    let wm_ctx = working_memory::get_context(group_id, 3600);
    if !wm_ctx.is_empty() {
        context_parts.push(wm_ctx);
    }

    // 7. 当前群的实时消息流 (包含多人对话上下文)
    if !group_context.is_empty() {
        context_parts.push(format!("# 当前群聊消息流\n{}", group_context));
    }

    let context = context_parts.join("\n\n");
    let weight_desc = if cfg.conversation.intrusiveness_weight < 0.2 {
        "你非常话痨，几乎什么话题都想参与"
    } else if cfg.conversation.intrusiveness_weight < 0.5 {
        "你会适度参与群聊，不是每条消息都回"
    } else if cfg.conversation.intrusiveness_weight < 0.8 {
        "你比较内敛，只在消息明显和你相关时才回复"
    } else {
        "你非常安静，几乎不主动参与群聊"
    };

    let content = format!(
        "你的性格倾向: {}\n\n需要判断是否回复的当前消息:\n[{}] {}",
        weight_desc, user_id, message
    );

    let full_prompt = format!("{}\n\n{}", DECIDE_REPLY_PROMPT, context);

    match ai::analyze(&full_prompt, &content) {
        Ok(raw) => {
            let json_str = if let Some(start) = raw.find('{') {
                if let Some(end) = raw[start..].find('}') {
                    &raw[start..start + end + 1]
                } else { "null" }
            } else { "null" };

            serde_json::from_str::<serde_json::Value>(json_str)
                .and_then(|v| {
                    let reply = v.get("reply").and_then(|r| r.as_bool()).unwrap_or(false);
                    let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                    if !reply {
                        eprintln!("[ai_chat] decided not to reply to user {} in group {}: {}", user_id, group_id, reason);
                    }
                    Ok(reply)
                })
                .unwrap_or(true) // 解析失败时默认回复
        }
        Err(e) => {
            eprintln!("[ai_chat] decide_reply AI error: {}, defaulting to reply", e);
            true // AI 调用失败时默认回复 (向后兼容)
        }
    }
}

fn process_message(user_id: u64, group_id: u64, message: &str) {
    let max_history = config::get().conversation.max_history;

    // 追加用户消息到对话历史
    with_state(|s| s.push_history(user_id, "user", message, max_history));

    let history = with_state(|s| {
        s.get_or_create_context(user_id).history.clone()
    });

    // 组装额外上下文: 记忆 + 人格 + 情绪
    let extra_context = build_context(user_id, group_id, &history);

    // 调用 AI
    match ai::chat(config::prompt(), &extra_context, &history, message) {
        Ok((reply, _)) => {
            // 从回复中解析情绪标签 (AI 自报告)
            let cleaned_reply = emotion::parse_from_reply(user_id, &reply);

            // 追加 AI 回复到历史
            with_state(|s| s.push_history(user_id, "assistant", &cleaned_reply, max_history));

            // 处理定时任务嵌入
            let final_reply = cron::handle_cron_in_reply(&cleaned_reply, group_id);

            // 先发送回复 (用户不用等分析完成)
            sender::send_with_typing(group_id, user_id, &final_reply);

            // 记录回复时间 (用于群聊对话跟进判断)
            with_state(|s| {
                s.record_reply(group_id, user_id);
                if group_id > 0 {
                    s.record_bot_message(group_id, &final_reply);
                }
            });

            // 标记工作记忆中该用户的消息为已回复
            working_memory::mark_replied(group_id, user_id);

            // ── AI 驱动的后处理 (发送后执行，不阻塞用户) ──

            // 合并分析: 记忆提取 + 情绪分析 (单次 API 调用)
            let analysis = ai::post_analyze(message, &cleaned_reply, &history);

            // 保存提取的记忆
            for (content, importance_str) in &analysis.memories {
                let importance = match importance_str.as_str() {
                    "permanent" => memory::Importance::Permanent,
                    "important" => memory::Importance::Important,
                    _ => memory::Importance::Normal,
                };
                memory::add(user_id, content, importance);
            }

            // 更新情绪状态
            emotion::update_from_analysis(user_id, &analysis.emotion, analysis.intensity);

            // 定期自动摘要
            if history.len() > 10 && history.len() % 10 == 0 {
                memory::auto_summarize(user_id, &history);
            }
        }
        Err(e) => {
            eprintln!("[ai_chat] AI error for user {}: {}", user_id, e);
            sender::send_msg(group_id, user_id, "睡着了...");
        }
    }
}

/// 构建注入到 system prompt 的额外上下文
fn build_context(user_id: u64, group_id: u64, history: &[(String, String)]) -> String {
    let mut parts = Vec::new();

    // 记忆上下文
    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        parts.push(mem);
    }

    // 群内其他成员的记忆 (群聊时)
    if group_id > 0 {
        let group_mem = memory::get_group_context(group_id, user_id);
        if !group_mem.is_empty() {
            parts.push(group_mem);
        }
    }

    // 人格上下文
    let pers = personality::get_prompt_context();
    if !pers.is_empty() {
        parts.push(pers);
    }

    // 情绪上下文
    let emo = emotion::get_prompt_context(user_id);
    if !emo.is_empty() {
        parts.push(emo);
    }

    // 对话状态指令
    let interaction_count = history.len();
    if interaction_count > 20 {
        parts.push("- 你们已经聊了很久了，关系很亲近，可以更自然随意".into());
    } else if interaction_count > 10 {
        parts.push("- 你们已经有一定的了解了".into());
    }

    parts.join("\n\n")
}
