pub mod ai;
pub mod archive;
pub mod blocklist;
pub mod config;
#[cfg(feature = "plugin")]
pub mod cron;
pub mod emotion;
pub mod memory;
pub mod personality;
pub mod proactive;
pub mod self_memory;
#[cfg(feature = "plugin")]
pub mod sender;
pub mod state;
pub mod working_memory;

// ── 测试模式下的 stub ────────────────────────────────────────
#[cfg(not(feature = "plugin"))]
mod cron {
    pub fn handle_cron_in_reply(reply: &str, _group_id: u64) -> String { reply.to_string() }
    pub fn handle_task_event(_json: &str) {}
}
#[cfg(not(feature = "plugin"))]
mod sender {
    pub fn send_msg(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn send_with_typing(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn send_at_msg(_group_id: u64, _user_id: u64, _text: &str) {}
}

#[cfg(feature = "plugin")]
use luo9_sdk::bus::Bus;
#[cfg(feature = "plugin")]
use luo9_sdk::payload::*;
use std::cell::RefCell;
use std::thread;
use std::time::Duration;
use tracing::debug;

/// AI 群聊回复决策提示词
pub const DECIDE_REPLY_PROMPT: &str = r#"你是一个群聊中的成员，需要判断是否要回复当前消息。

返回 JSON（不要输出其他内容）:
{"reply": true/false, "reason": "简短原因"}

应该回复的情况 (reply: true):
- 消息直接 @你 或提到你的名字
- 你刚说了话，有人明确回应你（追问、评论、调侃、附和你的话）
- 你和对方正在进行一对一的对话（连续互相发消息）
- 消息是发给群里所有人的问题或话题，你想参与

不应该回复的情况 (reply: false):
- 消息 @了其他人而不是你（即使你认识那个人）
- 两个人在聊天，你没有参与其中
- 一般性的感叹、牢骚、自言自语（除非你特别想插话）
- 你不确定消息是否发给你的 → 不回复

关键原则:
- 群里有多个成员，消息可能是发给别人的，不要默认所有消息都和你有关
- @了别人的消息 = 发给别人的，不要抢答
- 区分"对方在和你说话" vs "对方在和别人说话" vs "对方在自言自语"
- 像真人一样判断：你会回复的消息才回复，不要过度热情"#;

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
/// 上次自我反思时间
static mut LAST_SELF_REFLECTION: u64 = 0;

// ── 插件入口 ────────────────────────────────────────────────────

#[cfg(feature = "plugin")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    // 初始化 tracing subscriber，只输出 ai_chat 的 debug 日志
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("top_drluo_luo9_ai_chat=debug,warn"))
        )
        .with_target(false)
        .with_ansi(false)
        .init();

    config::init();
    debug!(model = %config::get().model, "plugin loaded");

    // 初始化定时器，避免启动时立即触发
    let now = now_secs();
    unsafe {
        LAST_PROACTIVE_CHECK = now;
        LAST_MEMORY_REVIEW = now;
        LAST_SELF_REFLECTION = now;
    }

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
        // 群聊: 从批次键中获取最近交互的 (user_id, group_id)
        let group_users: Vec<(u64, u64)> = s.batches.iter()
            .map(|(&(gid, uid), _)| (uid, gid))
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

    // 自我反思 (从配置读取间隔)
    let reflect_interval = config::get().self_reflection.interval;
    let last_reflect = unsafe { LAST_SELF_REFLECTION };
    if now.saturating_sub(last_reflect) >= reflect_interval {
        unsafe { LAST_SELF_REFLECTION = now; }
        do_self_reflection();
    }
}

/// 执行自我反思：收集最近对话上下文，调用 AI 生成内心想法
fn do_self_reflection() {
    // 收集群组列表和私聊上下文
    let (recent_context, group_ids) = with_state(|s| {
        let mut context_parts = Vec::new();
        let mut groups = Vec::new();

        // 从活跃群组中获取
        for &gid in &s.active_groups {
            if gid > 0 {
                groups.push(gid);
            }
        }

        // 取最近有对话的用户的私聊历史
        for (&(gid, uid), ctx) in &s.contexts {
            if gid == 0 && !ctx.history.is_empty() {
                let recent: Vec<String> = ctx.history.iter()
                    .rev()
                    .take(4)
                    .map(|(role, content)| format!("[{}] {}", role, content))
                    .collect();
                if !recent.is_empty() {
                    context_parts.push(format!("用户{}的私聊:\n{}", uid, recent.join("\n")));
                }
            }
        }

        (context_parts.join("\n\n"), groups)
    });

    // 构建群组画像：每个群的最近消息，让 AI 理解每个群是干什么的
    let group_profiles: Vec<self_memory::GroupProfile> = group_ids.iter().map(|&gid| {
        let entries = working_memory::get_recent(gid, 7200, 20);
        let recent_messages = if entries.is_empty() {
            "(最近没有消息)".to_string()
        } else {
            let lines: Vec<String> = entries.iter().map(|e| {
                format!("[用户{}] {}", e.user_id, e.content)
            }).collect();
            lines.join("\n")
        };
        self_memory::GroupProfile { group_id: gid, recent_messages }
    }).collect();

    let (count, share) = self_memory::reflect(&recent_context, &group_profiles);

    // 如果反思产生了想分享的想法，主动发送 (只发到激活的群)
    if let Some((content, group_id)) = share {
        let is_active = with_state(|s| s.active_groups.contains(&group_id));
        if is_active {
            debug!(group_id, content, "self_reflect: sharing thought");
            sender::send_msg(group_id, 0, &content);
        } else {
            debug!(group_id, "self_reflect: skipping share to inactive group");
        }
    }

    debug!(count, "self_reflect completed");
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

fn handle_group_msg(group_id: u64, user_id: u64, msg: &str) {
    let trimmed = msg.trim();

    // ── 黑名单拦截 (完全忽略) ──
    if with_state(|s| s.is_blacklisted(user_id)) {
        debug!(user_id, group_id, "blocked message from blacklisted user");
        return;
    }

    // ── 管理员专属控制命令 ──
    if is_admin(user_id) {
        match trimmed {
            "start" | "开启对话" => {
                let already = with_state(|s| s.active_groups.contains(&group_id));
                if already {
                    sender::send_msg(group_id, user_id, &config::get().messages.start.redo);
                    return;
                }
                with_state(|s| { s.active_groups.insert(group_id); });
                sender::send_msg(group_id, user_id, &config::get().messages.start.success);
                return;
            }
            "end" | "关闭对话" => {
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

        // 通用管理员命令 (群聊/私聊均可使用)
        if let Some(reply) = handle_admin_command(trimmed) {
            sender::send_msg(group_id, user_id, &reply);
            return;
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

    // ── 所有消息加入批次，由 AI 决策是否回复 ──
    with_state(|s| s.append_batch(group_id, user_id, trimmed));
}

fn handle_private_msg(user_id: u64, msg: &str) {
    let trimmed = msg.trim();

    // ── 黑名单拦截 (完全忽略) ──
    if with_state(|s| s.is_blacklisted(user_id)) {
        debug!(user_id, "blocked private message from blacklisted user");
        return;
    }

    // 控制命令
    if let Some(reply) = handle_control_command(0, user_id, trimmed) {
        sender::send_msg(0, user_id, &reply);
        return;
    }

    // 通用管理员命令
    if is_admin(user_id) {
        if let Some(reply) = handle_admin_command(trimmed) {
            sender::send_msg(0, user_id, &reply);
            return;
        }
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
        with_state(|s| s.append_batch(0, user_id, trimmed));
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
                s.batches.remove(&(0, user_id));
            });
            Some(config::get().messages.stop.success.clone())
        }
        "遗忘对话" => {
            let has = with_state(|s| s.contexts.contains_key(&(0, user_id)));
            if !has {
                return Some(config::get().messages.forget.fail.clone());
            }
            let list = with_state(|s| {
                let history = &s.contexts[&(0, user_id)].history;
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
            let has = with_state(|s| s.contexts.contains_key(&(0, user_id)));
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

// ── 通用管理员命令 (群聊/私聊均可使用) ──────────────────────────

fn handle_admin_command(msg: &str) -> Option<String> {
    match msg {
        "查看群聊" => {
            let groups = with_state(|s| s.active_groups.iter().copied().collect::<Vec<u64>>());
            if groups.is_empty() {
                return Some("当前没有开启的群聊".into());
            }
            let list: Vec<String> = groups.iter().map(|g| g.to_string()).collect();
            return Some(format!("已开启的群聊 ({}):\n{}", list.len(), list.join("\n")));
        }
        "查看用户" => {
            let users = with_state(|s| s.active.iter().copied().collect::<Vec<u64>>());
            if users.is_empty() {
                return Some("当前没有开启私聊的用户".into());
            }
            let list: Vec<String> = users.iter().map(|u| u.to_string()).collect();
            return Some(format!("已开启的用户 ({}):\n{}", list.len(), list.join("\n")));
        }
        "查看黑名单" => {
            let blocked = with_state(|s| s.blacklist.iter().copied().collect::<Vec<u64>>());
            if blocked.is_empty() {
                return Some("黑名单为空".into());
            }
            let list: Vec<String> = blocked.iter().map(|u| u.to_string()).collect();
            return Some(format!("黑名单用户 ({}):\n{}", list.len(), list.join("\n")));
        }
        _ => {}
    }

    if let Some(rest) = msg.strip_prefix("开启群聊:") {
        if let Ok(group_id) = rest.trim().parse::<u64>() {
            let already = with_state(|s| s.active_groups.contains(&group_id));
            if already {
                return Some(format!("群{}已经是开启状态", group_id));
            }
            with_state(|s| { s.active_groups.insert(group_id); });
            return Some(format!("已开启群{}", group_id));
        }
        return Some("格式: 开启群聊:群号".into());
    }

    if let Some(rest) = msg.strip_prefix("关闭群聊:") {
        if let Ok(group_id) = rest.trim().parse::<u64>() {
            let active = with_state(|s| s.active_groups.contains(&group_id));
            if !active {
                return Some(format!("群{}未开启", group_id));
            }
            with_state(|s| { s.active_groups.remove(&group_id); });
            return Some(format!("已关闭群{}", group_id));
        }
        return Some("格式: 关闭群聊:群号".into());
    }

    if let Some(rest) = msg.strip_prefix("开启用户:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            let already = with_state(|s| s.active.contains(&uid));
            if already {
                return Some(format!("用户{}已开启", uid));
            }
            with_state(|s| { s.active.insert(uid); });
            return Some(format!("已开启用户{}", uid));
        }
        return Some("格式: 开启用户:QQ号".into());
    }

    if let Some(rest) = msg.strip_prefix("关闭用户:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            let active = with_state(|s| s.active.contains(&uid));
            if !active {
                return Some(format!("用户{}未开启", uid));
            }
            with_state(|s| {
                s.active.remove(&uid);
                s.batches.remove(&(0, uid));
            });
            return Some(format!("已关闭用户{}", uid));
        }
        return Some("格式: 关闭用户:QQ号".into());
    }

    if let Some(rest) = msg.strip_prefix("拉黑:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            let already = with_state(|s| s.is_blacklisted(uid));
            if already {
                return Some(format!("用户{}已在黑名单中", uid));
            }
            with_state(|s| {
                s.add_blacklist(uid);
                // 同时关闭该用户的私聊和清理批次
                s.active.remove(&uid);
                s.forget_user(uid);
            });
            return Some(format!("已拉黑用户{}，该用户的所有消息将被忽略", uid));
        }
        return Some("格式: 拉黑:QQ号".into());
    }

    if let Some(rest) = msg.strip_prefix("移除黑名单:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            let blocked = with_state(|s| s.is_blacklisted(uid));
            if !blocked {
                return Some(format!("用户{}不在黑名单中", uid));
            }
            with_state(|s| { s.remove_blacklist(uid); });
            return Some(format!("已将用户{}移出黑名单", uid));
        }
        return Some("格式: 移除黑名单:QQ号".into());
    }

    None
}

// ── 批次处理 ────────────────────────────────────────────────────

fn process_expired_batches() {
    let cfg = config::get();
    let timeout = cfg.conversation.batch_timeout_ms;

    // 收集所有过期批次: key = (group_id, user_id)
    let expired: Vec<(u64, u64, String)> = {
        let mut result = Vec::new();
        with_state(|s| {
            let expired_keys: Vec<(u64, u64)> = s.batches.iter()
                .filter(|(_, batch)| batch.last_update.elapsed().as_millis() >= timeout as u128)
                .map(|(&key, _)| key)
                .collect();
            for (gid, uid) in expired_keys {
                if let Some(msgs) = s.take_batch_for_processing(gid, uid) {
                    result.push((gid, uid, msgs));
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

    for (group_id, user_id, messages) in expired {
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
        check_new_messages_for_user(user_id);
    }

    // 处理群聊批次: 把整个群的消息作为上下文一起做 AI 决策
    for (group_id, user_msgs) in group_msgs {
        let group_context: Vec<String> = user_msgs.iter()
            .map(|(uid, msg)| format!("[{}] {}", uid, msg))
            .collect();
        let context_str = group_context.join("\n");

        for (user_id, messages) in &user_msgs {
            // @机器人 → 直接回复 (唯一快速通道)
            if self_qq > 0 && messages.contains(&at_pattern) {
                process_message(*user_id, group_id, messages);
                continue;
            }

            // 所有非@消息: AI 决策 (传入群组上下文)
            if decide_reply(group_id, *user_id, messages, &context_str) {
                process_message(*user_id, group_id, messages);
            }
        }

        // 处理完成后检查整个群是否有新消息 (而非逐用户检查)
        check_new_messages_for_group(group_id);
    }
}

/// 检查私聊处理期间是否有新消息到达，如有则处理
fn check_new_messages_for_user(user_id: u64) {
    let timeout = config::get().conversation.batch_timeout_ms;
    let max_wait = 2000u64.min(timeout);
    let mut waited = 0u64;
    while waited < max_wait {
        thread::sleep(Duration::from_millis(200));
        waited += 200;
        let has_new = with_state(|s| s.batches.contains_key(&(0, user_id)));
        if has_new {
            break;
        }
    }

    let new_msgs = with_state(|s| s.take_batch_for_processing(0, user_id));
    if let Some(msgs) = new_msgs {
        process_message(user_id, 0, &msgs);
    }
}

/// 检查群聊处理期间是否有新消息到达 (检查整个群的所有用户)
/// 类似人类: 你回复完后发现群里又有人发了新消息，会接着回复
fn check_new_messages_for_group(group_id: u64) {
    let timeout = config::get().conversation.batch_timeout_ms;
    let self_qq = config::get().self_qq;
    let at_pattern = if self_qq > 0 { format!("[CQ:at,qq={}]", self_qq) } else { String::new() };

    // 短暂等待让新消息有机会到达 (类似人类回复后的停顿)
    let max_wait = 2000u64.min(timeout);
    let mut waited = 0u64;
    while waited < max_wait {
        thread::sleep(Duration::from_millis(200));
        waited += 200;
        // 检查该群是否有人有新消息
        let has_new = with_state(|s| s.batches.keys().any(|&(gid, _)| gid == group_id));
        if has_new {
            break;
        }
    }

    // 收取该群所有新消息
    let new_batches: Vec<(u64, String)> = with_state(|s| {
        let keys: Vec<(u64, u64)> = s.batches.keys()
            .filter(|&&(gid, _)| gid == group_id)
            .copied()
            .collect();
        keys.into_iter()
            .filter_map(|(gid, uid)| s.take_batch_for_processing(gid, uid).map(|msgs| (uid, msgs)))
            .collect()
    });

    if new_batches.is_empty() {
        return;
    }

    // 构建群上下文
    let group_context: Vec<String> = new_batches.iter()
        .map(|(uid, msg)| format!("[{}] {}", uid, msg))
        .collect();
    let context_str = group_context.join("\n");

    for (user_id, messages) in new_batches {
        if self_qq > 0 && messages.contains(&at_pattern) {
            process_message(user_id, group_id, &messages);
        } else if decide_reply(group_id, user_id, &messages, &context_str) {
            process_message(user_id, group_id, &messages);
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

    // 检查是否在 follow-up 窗口内 (机器人刚在群里回过话)
    let in_follow_up = with_state(|s| {
        s.is_in_follow_up(group_id, 0, cfg.conversation.reply_follow_up_secs)
    });

    // 构建决策上下文
    let mut context_parts = Vec::new();

    // 1. 人格信息
    let personality_ctx = personality::get_prompt_context();
    if !personality_ctx.is_empty() {
        context_parts.push(personality_ctx);
    }

    // 1.5 自我记忆 (bot 的内心想法)
    let self_mem = self_memory::get_context(config::get().self_reflection.max_thoughts.min(8));
    if !self_mem.is_empty() {
        context_parts.push(self_mem);
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
        s.get_or_create_context(group_id, user_id).history.iter()
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
            let json_str = ai::extract_json(&raw);
            match json_str {
                Some(s) => {
                    serde_json::from_str::<serde_json::Value>(&s)
                        .map(|v| {
                            let reply = v.get("reply").and_then(|r| r.as_bool()).unwrap_or(in_follow_up);
                            let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                            if !reply {
                                debug!(user_id, group_id, reason, "decided not to reply");
                            }
                            reply
                        })
                        .unwrap_or(in_follow_up) // JSON 解析失败 → follow-up 时回复
                }
                None => {
                    debug!(in_follow_up, "decide_reply: no JSON in response");
                    in_follow_up // follow-up 窗口内默认回复
                }
            }
        }
        Err(e) => {
            debug!(error = %e, in_follow_up, "decide_reply: AI error");
            in_follow_up
        }
    }
}

fn process_message(user_id: u64, group_id: u64, message: &str) {
    let max_history = config::get().conversation.max_history;

    // 追加用户消息到对话历史
    with_state(|s| s.push_history(group_id, user_id, "user", message, max_history));

    let history = with_state(|s| {
        s.get_or_create_context(group_id, user_id).history.clone()
    });

    // 组装额外上下文: 记忆 + 人格 + 情绪
    let extra_context = build_context(user_id, group_id, &history);

    // 调用 AI
    match ai::chat(config::prompt(), &extra_context, &history, message) {
        Ok((reply, _)) => {
            // 从回复中解析情绪标签 (AI 自报告)
            let cleaned_reply = emotion::parse_from_reply(user_id, &reply);

            // 追加 AI 回复到历史
            with_state(|s| s.push_history(group_id, user_id, "assistant", &cleaned_reply, max_history));

            // 处理定时任务嵌入
            let final_reply = cron::handle_cron_in_reply(&cleaned_reply, group_id);

            // 先发送回复 (用户不用等分析完成)
            if group_id > 0 {
                // 群聊: @回复用户，让对方明确知道 bot 在回复谁
                send_group_reply(group_id, user_id, &final_reply);
            } else {
                sender::send_with_typing(0, user_id, &final_reply);
            }

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

            // 合并分析: 记忆提取 + 情绪分析 + 记忆纠错 (单次 API 调用)
            let analysis_context = {
                let mut parts = Vec::new();
                let mem = memory::get_context(user_id);
                if !mem.is_empty() { parts.push(mem); }
                let self_mem = self_memory::get_context(10);
                if !self_mem.is_empty() { parts.push(self_mem); }
                parts.join("\n\n")
            };
            let analysis = ai::post_analyze(message, &cleaned_reply, &history, &analysis_context);

            debug!(
                user_id,
                group_id,
                memories_count = analysis.memories.len(),
                emotion = %analysis.emotion,
                intensity = analysis.intensity,
                corrections_count = analysis.corrections.len(),
                "post_analyze completed"
            );

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

            // 记忆纠错
            for correction in &analysis.corrections {
                match correction.target.as_str() {
                    "self" => { self_memory::correct(&correction.old, &correction.new); }
                    _ => { memory::correct(user_id, &correction.old, &correction.new); }
                }
            }

            // 定期自动摘要
            if history.len() > 10 && history.len() % 10 == 0 {
                memory::auto_summarize(user_id, &history);
            }
        }
        Err(e) => {
            debug!(user_id, group_id, error = %e, "AI chat error");
            sender::send_msg(group_id, user_id, "睡着了...");
        }
    }
}

/// 群聊回复：模拟打字延迟，分段发送
fn send_group_reply(group_id: u64, user_id: u64, reply: &str) {
    let cfg = config::get();
    let segments: Vec<&str> = reply.split("|^|").filter(|s| !s.trim().is_empty()).collect();

    for (i, segment) in segments.iter().enumerate() {
        sender::send_msg(group_id, user_id, segment);
        // 段间打字延迟
        if i < segments.len() - 1 {
            let delay_secs = segment.chars().count() as f64 / cfg.conversation.typing_speed;
            let delay_ms = (delay_secs * 1000.0) as u64;
            let delay_ms = delay_ms.min(cfg.conversation.max_typing_delay_ms);
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }
}

/// 构建注入到 system prompt 的额外上下文
fn build_context(user_id: u64, group_id: u64, history: &[(String, String)]) -> String {
    let mut parts = Vec::new();

    // 自我记忆 (bot 的内心想法)
    let self_mem = self_memory::get_context(config::get().self_reflection.max_thoughts.min(8));
    if !self_mem.is_empty() {
        parts.push(self_mem);
    }

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

    // Bot 自己最近的消息 (帮助保持一致性)
    if group_id > 0 {
        let bot_msgs: Vec<String> = with_state(|s| {
            s.get_recent_bot_messages(group_id, 600, 5)
                .into_iter().map(|m| m.to_string()).collect()
        });
        if !bot_msgs.is_empty() {
            parts.push(format!("# 你在群里最近发过的消息\n{}", bot_msgs.join("\n")));
        }
    }

    // 工作记忆 (群聊最近消息流)
    if group_id > 0 {
        let wm_ctx = working_memory::get_context(group_id, 3600);
        if !wm_ctx.is_empty() {
            parts.push(wm_ctx);
        }
    }

    parts.join("\n\n")
}
