//! 对话处理模块：消息入口、批次处理、回复生成、上下文构建

pub mod batch;
pub mod context;
pub mod handler;
pub mod attention;

use crate::{config, with_state, with_shared_state, read_shared_state, is_admin};
use tracing::{debug, info, warn};

/// 私聊关闭时间：2026年7月14日 00:00:00 (UTC+8)
const PRIVATE_CHAT_CLOSE_TS: u64 = 1783958400; // 2026-07-14 00:00:00 UTC+8

pub fn handle_group_msg(group_id: u64, user_id: u64, msg: &str) {
    let trimmed = msg.trim();
    info!(user_id, group_id, content = trimmed, "recv: group msg");

    // ── 自动回复过滤 (完全忽略) ──
    if trimmed.starts_with("[自动回复]") {
        debug!(user_id, group_id, "ignored auto-reply message");
        return;
    }

    // ── 黑名单拦截 (完全忽略) ──
    if with_state(|s| s.is_blacklisted(user_id)) {
        debug!(user_id, group_id, "blocked message from blacklisted user");
        return;
    }

    // ── 防注入检查 (非管理员，始终开启) ──
    if !is_admin(user_id) {
        let check_result = crate::anti_injection::check_input(user_id, trimmed, &config::get().anti_injection);
        match check_result.action {
            crate::anti_injection::Action::Block | crate::anti_injection::Action::Ban => {
                warn!(
                    user_id, group_id,
                    issues = ?check_result.issues,
                    action = ?check_result.action,
                    "anti_injection: 消息被阻止"
                );
                return;
            }
            crate::anti_injection::Action::Replace => {
                // 替换模式：发送替换内容，原消息不进入对话记忆
                if let Some(msg) = check_result.sanitized {
                    crate::sender::send_msg(group_id, user_id, &msg);
                }
                warn!(
                    user_id, group_id,
                    issues = ?check_result.issues,
                    "anti_injection: 消息被替换 (不进入对话记忆)"
                );
                return;
            }
            crate::anti_injection::Action::SilentBan => {
                if let Some(msg) = check_result.sanitized {
                    crate::sender::send_msg(group_id, user_id, &msg);
                }
                info!(user_id, group_id, "anti_injection: 用户被静默封禁");
                return;
            }
            crate::anti_injection::Action::Warn => {
                warn!(
                    user_id, group_id,
                    issues = ?check_result.issues,
                    "anti_injection: 可疑消息 (允许通过，已记录违规)"
                );
            }
            crate::anti_injection::Action::CrisisExempt => {
                warn!(
                    user_id, group_id,
                    issues = ?check_result.issues,
                    "anti_injection: 危机消息豁免 (违规已记录)"
                );
            }
            _ => {}
        }
    }else{
        info!("是管理员！");
    }

    // ── 管理员专属控制命令 ──
    if is_admin(user_id) {
        match trimmed {
            "start" | "开启对话" => {
                let already = with_state(|s| s.active_groups.contains(&group_id));
                if already {
                    info!(user_id, group_id, "cmd: group already active");
                    crate::sender::send_msg(group_id, user_id, &config::get().messages.start.redo);
                    return;
                }
                with_state(|s| { s.active_groups.insert(group_id); });
                info!(user_id, group_id, "cmd: activated group");
                crate::sender::send_msg(group_id, user_id, &config::get().messages.start.success);
                return;
            }
            "end" | "关闭对话" => {
                let active = with_state(|s| s.active_groups.contains(&group_id));
                if !active {
                    info!(user_id, group_id, "cmd: group not active");
                    crate::sender::send_msg(group_id, user_id, &config::get().messages.stop.redo);
                    return;
                }
                with_state(|s| { s.active_groups.remove(&group_id); });
                info!(user_id, group_id, "cmd: deactivated group");
                crate::sender::send_msg(group_id, user_id, &config::get().messages.stop.success);
                return;
            }
            _ => {}
        }

        // 通用管理员命令 (群聊/私聊均可使用)
        if let Some(reply) = handle_admin_command(trimmed, group_id, user_id) {
            crate::sender::send_msg(group_id, user_id, &reply);
            return;
        }

        // 人格/主动对话管理命令 (管理员专属)
        if let Some(reply) = handle_personality_command(trimmed) {
            crate::sender::send_msg(group_id, user_id, &reply);
            return;
        }
        if let Some(reply) = handle_proactive_command(trimmed) {
            crate::sender::send_msg(group_id, user_id, &reply);
            return;
        }
    }

    // ── 群组未激活则不处理 ──
    let group_active = with_state(|s| s.active_groups.contains(&group_id));
    if !group_active {
        return;
    }

    // ── 记忆管理命令 (所有用户可用) ──
    if let Some(reply) = crate::memory::check_forget_command(user_id, trimmed) {
        crate::sender::send_msg(group_id, user_id, &reply);
        return;
    }

    // ── 记录用户交互 + 情绪分析 + 工作记忆 (无论是否回复) ──
    // 去除图片 CQ 码后再做情绪分析和工作记忆记录
    let text_only = crate::vision::strip_image_cq(trimmed);
    crate::proactive::record_user_reply(user_id);
    crate::emotion::analyze_user_message(user_id, &text_only);
    let record_ts = crate::working_memory::record(group_id, user_id, if text_only.is_empty() { "[图片]" } else { &text_only }, false);

    // ── 人物档案：注册/更新 ──
    crate::person_info::register_person(user_id);

    // ── 回复效果追踪：观察后续消息 ──
    crate::reply_effect::observe_message(group_id, user_id, &text_only);

    // ── 表情包自动注册（仅表情包，非普通图片）──
    if trimmed.contains("[CQ:image,") {
        let trimmed_cpy = trimmed.to_string();
        std::thread::spawn(move || {
            crate::sticker::register_from_cq(&trimmed_cpy);
        });
    }

    // ── 所有消息加入批次，由 AI 决策是否回复 ──
    with_state(|s| s.append_batch(group_id, user_id, trimmed, record_ts));
}

pub fn handle_private_msg(user_id: u64, msg: &str) {
    let trimmed = msg.trim();
    info!(user_id, content = trimmed, "recv: private msg");

    // ── 自动回复过滤 (完全忽略) ──
    if trimmed.starts_with("[自动回复]") {
        debug!(user_id, "ignored auto-reply message");
        return;
    }

    // ── 白名单/黑名单检查 (非管理员) ──
    if !is_admin(user_id) {
        let cfg = config::get();

        // 白名单优先：如果配置了白名单，只允许白名单用户
        if !cfg.whitelist.is_empty() && !cfg.whitelist.contains(&user_id) {
            debug!(user_id, "blocked: user not in whitelist");
            return;
        }

        // 黑名单检查：如果用户在黑名单中，拒绝
        if !cfg.blacklist.is_empty() && cfg.blacklist.contains(&user_id) {
            debug!(user_id, "blocked: user in blacklist");
            return;
        }

        // 运行时黑名单检查 (命令添加的)
        if with_state(|s| s.is_blacklisted(user_id)) {
            debug!(user_id, "blocked private message from blacklisted user");
            return;
        }
    }

    // ── 防注入检查 (非管理员，始终开启) ──
    if !is_admin(user_id) {
        let check_result = crate::anti_injection::check_input(user_id, trimmed, &config::get().anti_injection);
        match check_result.action {
            crate::anti_injection::Action::Block | crate::anti_injection::Action::Ban => {
                warn!(
                    user_id,
                    issues = ?check_result.issues,
                    action = ?check_result.action,
                    "anti_injection: 私聊消息被阻止"
                );
                return;
            }
            crate::anti_injection::Action::Replace => {
                if let Some(msg) = check_result.sanitized {
                    crate::sender::send_msg(0, user_id, &msg);
                }
                warn!(
                    user_id,
                    issues = ?check_result.issues,
                    "anti_injection: 私聊消息被替换 (不进入对话记忆)"
                );
                return;
            }
            crate::anti_injection::Action::SilentBan => {
                if let Some(msg) = check_result.sanitized {
                    crate::sender::send_msg(0, user_id, &msg);
                }
                info!(user_id, "anti_injection: 用户被静默封禁");
                return;
            }
            crate::anti_injection::Action::Warn => {
                warn!(
                    user_id,
                    issues = ?check_result.issues,
                    "anti_injection: 可疑私聊消息 (允许通过，已记录违规)"
                );
            }
            crate::anti_injection::Action::CrisisExempt => {
                warn!(
                    user_id,
                    issues = ?check_result.issues,
                    "anti_injection: 危机消息豁免 (违规已记录)"
                );
            }
            _ => {}
        }
    }

    // ── 私聊关闭检查 (2026-07-14 起) ──
    let now = crate::util::now_secs();
    if now >= PRIVATE_CHAT_CLOSE_TS {
        // 仅允许退出命令
        match trimmed {
            "停!" | "关闭对话" => {
                with_state(|s| {
                    s.active.remove(&user_id);
                    s.batches.remove(&(0, user_id));
                });
                info!(user_id, "cmd: deactivated private chat (after close date)");
                crate::sender::send_msg(0, user_id, &config::get().messages.stop.success);
            }
            _ => {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    info!("private chat closed: date threshold reached");
                });
                crate::sender::send_msg(0, user_id,
                    "私聊服务已于 2026年7月14日 关闭，无法进行私聊对话。\n如有需要，请在群聊中与我互动。");
            }
        }
        return;
    }

    // 控制命令
    if let Some(reply) = handle_control_command(0, user_id, trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    // 通用管理员命令
    if is_admin(user_id)
        && let Some(reply) = handle_admin_command(trimmed, 0, user_id) {
            crate::sender::send_msg(0, user_id, &reply);
            return;
        }

    if let Some(reply) = crate::memory::check_forget_command(user_id, trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = handle_personality_command(trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = handle_proactive_command(trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    if with_state(|s| s.active.contains(&user_id)) {
        crate::proactive::record_user_reply(user_id);
        crate::emotion::analyze_user_message(user_id, trimmed);

        // 表情包自动注册（同群聊逻辑）
        if trimmed.contains("[CQ:image,") {
            let trimmed_cpy = trimmed.to_string();
            std::thread::spawn(move || {
                crate::sticker::register_from_cq(&trimmed_cpy);
            });
        }

        with_state(|s| s.append_batch(0, user_id, trimmed, 0));
    }
}

// ── 控制命令 ────────────────────────────────────────────────────

pub fn handle_control_command(_group_id: u64, user_id: u64, msg: &str) -> Option<String> {
    match msg {
        "开!" | "开启对话" => {
            let already = with_state(|s| s.active.contains(&user_id));
            if already {
                info!(user_id, "cmd: already active");
                return Some(config::get().messages.start.redo.clone());
            }
            with_state(|s| { s.active.insert(user_id); });
            info!(user_id, "cmd: activated private chat");
            Some(config::get().messages.start.success.clone())
        }
        "停!" | "关闭对话" => {
            let active = with_state(|s| s.active.contains(&user_id));
            if !active {
                info!(user_id, "cmd: not active");
                return Some(config::get().messages.stop.redo.clone());
            }
            with_state(|s| {
                s.active.remove(&user_id);
                s.batches.remove(&(0, user_id));
            });
            info!(user_id, "cmd: deactivated private chat");
            Some(config::get().messages.stop.success.clone())
        }
        "遗忘对话" => {
            let history = read_shared_state(|s| s.get_history_clone(0, user_id));
            if history.is_empty() {
                info!(user_id, "cmd: no context to forget");
                return Some(config::get().messages.forget.fail.clone());
            }
            let list = history
                .iter()
                .enumerate()
                .map(|(i, (role, content))| format!("{}. [{}] {}", i + 1, role, content))
                .collect::<Vec<_>>()
                .join("\n");
            with_shared_state(|s| s.forget_user_shared(user_id));
            with_state(|s| s.forget_user_local(user_id));
            info!(user_id, "cmd: forgot conversation");
            Some(format!("{}\n\n{}", config::get().messages.forget.success, list))
        }
        "重启对话" => {
            let has = read_shared_state(|s| s.contexts.contains_key(&(0, user_id)));
            if has {
                with_shared_state(|s| s.forget_user_shared(user_id));
                with_state(|s| s.forget_user_local(user_id));
                crate::memory::forget_all(user_id);
                info!(user_id, "cmd: restarted conversation");
                Some(config::get().messages.restart.success.clone())
            } else {
                info!(user_id, "cmd: no context to restart");
                Some(config::get().messages.restart.redo.clone())
            }
        }
        _ => None,
    }
}

// ── 人格管理命令 ────────────────────────────────────────────────

pub fn handle_personality_command(msg: &str) -> Option<String> {
    if msg == "查看人格" {
        let ctx = crate::personality::get_prompt_context();
        return Some(format!("当前人格设定:\n{}", ctx));
    }

    if msg == "人格模板" {
        let templates = ["温柔体贴", "幽默风趣", "理性分析", "傲娇毒舌", "元气活泼", "安静内敛"];
        return Some(format!("可用人格模板:\n{}", templates.join("\n")));
    }

    if let Some(name) = msg.strip_prefix("切换人格:") {
        let name = name.trim();
        return Some(crate::personality::apply_template(name).unwrap_or_else(|e| e));
    }

    if let Some(rest) = msg.strip_prefix("调整特质:") {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2
            && let Ok(value) = parts[1].parse::<f32>() {
                return Some(crate::personality::adjust_trait(parts[0], value).unwrap_or_else(|e| e));
            }
        return Some("格式: 调整特质:特质名 数值 (0.0~1.0)".into());
    }

    if let Some(name) = msg.strip_prefix("保存人格:") {
        return Some(crate::personality::save_snapshot(name.trim()).unwrap_or_else(|e| e));
    }

    if let Some(name) = msg.strip_prefix("加载人格:") {
        return Some(crate::personality::load_snapshot(name.trim()).unwrap_or_else(|e| e));
    }

    if msg == "人格列表" {
        let list = crate::personality::list_snapshots();
        if list.is_empty() {
            return Some("没有保存的人格快照".into());
        }
        return Some(format!("已保存的人格:\n{}", list.join("\n")));
    }

    None
}

// ── 主动对话命令 ────────────────────────────────────────────────

pub fn handle_proactive_command(msg: &str) -> Option<String> {
    match msg {
        "开启主动对话" => {
            crate::proactive::set_enabled(true);
            return Some("已开启主动对话".into());
        }
        "关闭主动对话" => {
            crate::proactive::set_enabled(false);
            return Some("已关闭主动对话".into());
        }
        _ => {}
    }

    if let Some(rest) = msg.strip_prefix("设置免打扰:") {
        let parts: Vec<&str> = rest.splitn(2, '-').collect();
        if parts.len() == 2
            && let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                crate::proactive::set_quiet_hours(start, end);
                return Some(format!("已设置免打扰: {}时 - {}时", start, end));
            }
        return Some("格式: 设置免打扰:23-7".into());
    }

    if let Some(rest) = msg.strip_prefix("提醒我:") {
        // 提醒我:MM-DD 描述
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            crate::proactive::add_date_reminder(0, parts[0], parts[1]);
            return Some(format!("已添加日期提醒: {} {}", parts[0], parts[1]));
        }
        return Some("格式: 提醒我:MM-DD 描述".into());
    }

    None
}

// ── 通用管理员命令 (群聊/私聊均可使用) ──────────────────────────

pub fn handle_admin_command(msg: &str, _group_id: u64, user_id: u64) -> Option<String> {
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

    if let Some(res) = crate::util::parse_uid_arg(msg, "开启群聊:") {
        return Some(match res {
            Ok(group_id) => {
                if with_state(|s| s.active_groups.contains(&group_id)) {
                    format!("群{}已经是开启状态", group_id)
                } else {
                    with_state(|s| { s.active_groups.insert(group_id); });
                    format!("已开启群{}", group_id)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "关闭群聊:") {
        return Some(match res {
            Ok(group_id) => {
                if !with_state(|s| s.active_groups.contains(&group_id)) {
                    format!("群{}未开启", group_id)
                } else {
                    with_state(|s| { s.active_groups.remove(&group_id); });
                    format!("已关闭群{}", group_id)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "开启用户:") {
        return Some(match res {
            Ok(uid) => {
                if with_state(|s| s.active.contains(&uid)) {
                    format!("用户{}已开启", uid)
                } else {
                    with_state(|s| { s.active.insert(uid); });
                    format!("已开启用户{}", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "关闭用户:") {
        return Some(match res {
            Ok(uid) => {
                if !with_state(|s| s.active.contains(&uid)) {
                    format!("用户{}未开启", uid)
                } else {
                    with_state(|s| {
                        s.active.remove(&uid);
                        s.batches.remove(&(0, uid));
                    });
                    format!("已关闭用户{}", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "拉黑:") {
        return Some(match res {
            Ok(uid) => {
                if with_state(|s| s.is_blacklisted(uid)) {
                    format!("用户{}已在黑名单中", uid)
                } else {
                    with_state(|s| {
                        s.add_blacklist(uid);
                        s.active.remove(&uid);
                        s.forget_user_local(uid);
                    });
                    with_shared_state(|s| s.forget_user_shared(uid));
                    format!("已拉黑用户{}，该用户的所有消息将被忽略", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "移除黑名单:") {
        return Some(match res {
            Ok(uid) => {
                if !with_state(|s| s.is_blacklisted(uid)) {
                    format!("用户{}不在黑名单中", uid)
                } else {
                    with_state(|s| { s.remove_blacklist(uid); });
                    format!("已将用户{}移出黑名单", uid)
                }
            }
            Err(e) => e,
        });
    }

    // ── 防注入管理命令（需要权限校验） ──
    if let Some(reply) = crate::anti_injection::handle_admin_command(user_id, msg, config::get()) {
        return Some(reply);
    }

    None
}
