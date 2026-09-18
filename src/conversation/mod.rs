//! 对话处理模块：消息入口、批次处理、回复生成

pub mod attention;
pub mod batch;
pub mod handler;
pub mod interruption;
pub mod perception;
pub mod turn;

use crate::{batches, config, gate_read, is_admin, mind, read_shared_state, with_shared_state};
use tracing::{debug, info, warn};

/// 零容忍（方案书 §7.3）：确认注入一次 = 永久拉黑 + 残留清洗。
/// Block/Ban/SilentBan 均视为确认；Warn 灰区走"玻璃瓶"由她自己产生厌恶。
fn enforce_zero_tolerance(user_id: u64, action: &crate::anti_injection::Action, issues: &[String]) {
    if !matches!(
        action,
        crate::anti_injection::Action::Block
            | crate::anti_injection::Action::Ban
            | crate::anti_injection::Action::SilentBan
    ) {
        return;
    }
    // 永久拉黑（运行时 + 持久）
    crate::set_blacklisted(crate::db::Actor::Command, user_id, true);
    crate::anti_injection::ban_user(user_id);
    // 残留清洗：他不能再留在她的世界里
    mind::persons::purge_user_want_to_say(user_id);
    mind::diary::purge_about(user_id);
    mind::wake::purge_loops_about(user_id);
    mind::social::purge_user(user_id);
    mind::security::log_event(
        user_id,
        "perception",
        "zero_tolerance_blacklist",
        &issues.join(";"),
    );
    warn!(user_id, "零容忍：确认注入，永久拉黑并清洗残留");
}

pub fn handle_group_msg(group_id: u64, user_id: u64, msg: &str) {
    let trimmed = msg.trim();
    info!(user_id, group_id, content = trimmed, "recv: group msg");

    // ── 自身消息处理：记录到工作记忆，但不触发回复 ──
    let self_qq = config::get().self_qq;
    if self_qq > 0 && user_id == self_qq {
        let text_only = crate::vision::strip_image_cq(trimmed);
        crate::working_memory::record_bot_reply(
            group_id,
            if text_only.is_empty() {
                "[图片]"
            } else {
                &text_only
            },
        );
        debug!(user_id, group_id, "self message recorded to working memory");
        return;
    }

    // ── 自动回复过滤 (完全忽略) ──
    if trimmed.starts_with("[自动回复]") {
        debug!(user_id, group_id, "ignored auto-reply message");
        return;
    }

    // ── 黑名单拦截 (完全忽略) ──
    if gate_read(|g| g.is_blacklisted(user_id)) {
        debug!(user_id, group_id, "blocked message from blacklisted user");
        return;
    }

    // ── 防注入检查 (非管理员，始终开启) ──
    if !is_admin(user_id) {
        let check_result =
            crate::anti_injection::check_input(user_id, trimmed, &config::get().anti_injection);
        let issue_names: Vec<String> = check_result
            .issues
            .iter()
            .map(|i| format!("{i:?}"))
            .collect();
        enforce_zero_tolerance(user_id, &check_result.action, &issue_names);
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
    }

    // ── 管理员专属控制命令 ──
    if is_admin(user_id) {
        match trimmed {
            "start" | "开启对话" => {
                if !crate::toggle_group_chat(crate::db::Actor::Command, group_id, true) {
                    info!(user_id, group_id, "cmd: group already active");
                    crate::sender::send_msg(group_id, user_id, &config::get().messages.start.redo);
                    return;
                }
                info!(user_id, group_id, "cmd: activated group");
                crate::sender::send_msg(group_id, user_id, &config::get().messages.start.success);
                return;
            }
            "end" | "关闭对话" => {
                if !crate::toggle_group_chat(crate::db::Actor::Command, group_id, false) {
                    info!(user_id, group_id, "cmd: group not active");
                    crate::sender::send_msg(group_id, user_id, &config::get().messages.stop.redo);
                    return;
                }
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
    }

    // ── 群组未激活则不处理 ──
    if !gate_read(|g| g.is_group_active(group_id)) {
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
    crate::emotion::analyze_user_message(user_id, &text_only);
    let entry_id = crate::working_memory::record(
        group_id,
        user_id,
        if text_only.is_empty() {
            "[图片]"
        } else {
            &text_only
        },
        false,
    );

    // ── 训练数据留档：人类说话语料（防注入放行的才进库） ──
    let archive_name = crate::person_info::get_display_name(user_id, group_id).unwrap_or_default();
    crate::mind::archive::record_message(
        group_id,
        user_id,
        &archive_name,
        if text_only.is_empty() {
            "[图片]"
        } else {
            &text_only
        },
    );

    // ── 社会世界模型：这条消息改变群里的势（纯内存观察，主循环零 IO） ──
    crate::mind::social::observe_message(
        group_id,
        user_id,
        if text_only.is_empty() {
            "[图片]"
        } else {
            &text_only
        },
        entry_id,
    );

    // ── 概率式中断记账：她正在生成回复时又来了新消息 ──
    crate::conversation::interruption::note_arrival(group_id);

    // ── 信息觅食记账：这个群又攒了一条没细看的（纯内存） ──
    crate::mind::foraging::note_message(group_id);

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
    batches(|b| b.append(group_id, user_id, trimmed, entry_id));
}

pub fn handle_private_msg(user_id: u64, msg: &str) {
    let trimmed = msg.trim();
    info!(user_id, content = trimmed, "recv: private msg");

    // ── 自身消息处理：记录到工作记忆，但不触发回复 ──
    let self_qq = config::get().self_qq;
    if self_qq > 0 && user_id == self_qq {
        crate::working_memory::record_bot_reply(0, trimmed);
        debug!(user_id, "self message recorded to working memory");
        return;
    }

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
        if gate_read(|g| g.is_blacklisted(user_id)) {
            debug!(user_id, "blocked private message from blacklisted user");
            return;
        }
    }

    // ── 防注入检查 (非管理员，始终开启) ──
    if !is_admin(user_id) {
        let check_result =
            crate::anti_injection::check_input(user_id, trimmed, &config::get().anti_injection);
        let issue_names: Vec<String> = check_result
            .issues
            .iter()
            .map(|i| format!("{i:?}"))
            .collect();
        enforce_zero_tolerance(user_id, &check_result.action, &issue_names);
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

    // 控制命令
    if let Some(reply) = handle_control_command(0, user_id, trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    // 通用管理员命令
    if is_admin(user_id)
        && let Some(reply) = handle_admin_command(trimmed, 0, user_id)
    {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    if let Some(reply) = crate::memory::check_forget_command(user_id, trimmed) {
        crate::sender::send_msg(0, user_id, &reply);
        return;
    }

    if gate_read(|g| g.is_private_active(user_id)) {
        crate::emotion::analyze_user_message(user_id, trimmed);

        // 表情包自动注册（同群聊逻辑）
        if trimmed.contains("[CQ:image,") {
            let trimmed_cpy = trimmed.to_string();
            std::thread::spawn(move || {
                crate::sticker::register_from_cq(&trimmed_cpy);
            });
        }

        batches(|b| b.append(0, user_id, trimmed, 0));
    }
}

// ── 控制命令 ────────────────────────────────────────────────────

pub fn handle_control_command(_group_id: u64, user_id: u64, msg: &str) -> Option<String> {
    match msg {
        "开!" | "开启对话" => {
            if !crate::toggle_private_chat(crate::db::Actor::Command, user_id, true) {
                info!(user_id, "cmd: already active");
                return Some(config::get().messages.start.redo.clone());
            }
            info!(user_id, "cmd: activated private chat");
            Some(config::get().messages.start.success.clone())
        }
        "停!" | "关闭对话" => {
            if !crate::toggle_private_chat(crate::db::Actor::Command, user_id, false) {
                info!(user_id, "cmd: not active");
                return Some(config::get().messages.stop.redo.clone());
            }
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
            batches(|b| b.forget_user(user_id));
            info!(user_id, "cmd: forgot conversation");
            Some(format!(
                "{}\n\n{}",
                config::get().messages.forget.success,
                list
            ))
        }
        "重启对话" => {
            let has = read_shared_state(|s| s.contexts.contains_key(&(0, user_id)));
            if has {
                with_shared_state(|s| s.forget_user_shared(user_id));
                batches(|b| b.forget_user(user_id));
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

// ── 通用管理员命令 (群聊/私聊均可使用) ──────────────────────────

pub fn handle_admin_command(msg: &str, _group_id: u64, user_id: u64) -> Option<String> {
    match msg {
        "查看群聊" => {
            let groups = crate::get_active_groups();
            if groups.is_empty() {
                return Some("当前没有开启的群聊".into());
            }
            let list: Vec<String> = groups.iter().map(|g| g.to_string()).collect();
            return Some(format!(
                "已开启的群聊 ({}):\n{}",
                list.len(),
                list.join("\n")
            ));
        }
        "查看用户" => {
            let users = crate::get_active_users();
            if users.is_empty() {
                return Some("当前没有开启私聊的用户".into());
            }
            let list: Vec<String> = users.iter().map(|u| u.to_string()).collect();
            return Some(format!(
                "已开启的用户 ({}):\n{}",
                list.len(),
                list.join("\n")
            ));
        }
        "查看黑名单" => {
            let blocked = crate::get_blacklist();
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
                if crate::toggle_group_chat(crate::db::Actor::Command, group_id, true) {
                    format!("已开启群{}", group_id)
                } else {
                    format!("群{}已经是开启状态", group_id)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "关闭群聊:") {
        return Some(match res {
            Ok(group_id) => {
                if crate::toggle_group_chat(crate::db::Actor::Command, group_id, false) {
                    format!("已关闭群{}", group_id)
                } else {
                    format!("群{}未开启", group_id)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "开启用户:") {
        return Some(match res {
            Ok(uid) => {
                if crate::toggle_private_chat(crate::db::Actor::Command, uid, true) {
                    format!("已开启用户{}", uid)
                } else {
                    format!("用户{}已开启", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "关闭用户:") {
        return Some(match res {
            Ok(uid) => {
                if crate::toggle_private_chat(crate::db::Actor::Command, uid, false) {
                    format!("已关闭用户{}", uid)
                } else {
                    format!("用户{}未开启", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "拉黑:") {
        return Some(match res {
            Ok(uid) => {
                if gate_read(|g| g.is_blacklisted(uid)) {
                    format!("用户{}已在黑名单中", uid)
                } else {
                    crate::set_blacklisted(crate::db::Actor::Command, uid, true);
                    crate::toggle_private_chat(crate::db::Actor::Command, uid, false);
                    batches(|b| b.forget_user(uid));
                    with_shared_state(|s| s.forget_user_shared(uid));
                    crate::mind::social::purge_user(uid);
                    format!("已拉黑用户{}，该用户的所有消息将被忽略", uid)
                }
            }
            Err(e) => e,
        });
    }

    if let Some(res) = crate::util::parse_uid_arg(msg, "移除黑名单:") {
        return Some(match res {
            Ok(uid) => {
                if !gate_read(|g| g.is_blacklisted(uid)) {
                    format!("用户{}不在黑名单中", uid)
                } else {
                    crate::set_blacklisted(crate::db::Actor::Command, uid, false);
                    format!("已将用户{}移出黑名单", uid)
                }
            }
            Err(e) => e,
        });
    }

    // ── 防注入管理命令（需要权限校验） ──
    if let Some(reply) = crate::anti_injection::handle_admin_command(user_id, msg, &config::get()) {
        return Some(reply);
    }

    None
}
