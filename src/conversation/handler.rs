//! 消息处理：私聊表达路径 + 群聊表达调度
//!
//! 群聊和私聊都走同一条表达管线（`voice`）：
//! 同一个"她"读完场面后，在同一口气里决定说话、沉默或发表情包。
//! 本模块负责感知准备（视觉、历史、注意力）、调度与回复落地簿记。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::voice::{self, GroupUtterance, VoiceAction};
use crate::{ProcessingGuard, config, processing_users, read_shared_state, with_shared_state};

// ── 群聊沉默冷却 ────────────────────────────────────────────────

/// 她刚决定不说话，短时间内不再重新权衡。
/// 省 API 调用，也符合"刚看过一眼群里，没什么想说的"的心理状态。
static LAST_SILENCE: OnceLock<Mutex<HashMap<u64, Instant>>> = OnceLock::new();

fn silence_cooling(group_id: u64) -> bool {
    let cooldown = config::get().conversation.silence_cooldown_secs;
    LAST_SILENCE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&group_id).copied())
        .map(|t| t.elapsed().as_secs() < cooldown)
        .unwrap_or(false)
}

fn mark_silence(group_id: u64) {
    if let Ok(mut m) = LAST_SILENCE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        m.insert(group_id, Instant::now());
    }
}

// ── 视觉感知 ────────────────────────────────────────────────────

/// 提取消息中的图片描述（表情包走持久化缓存路径）
///
/// 返回 (图片描述列表, 去除图片 CQ 码后的纯文本)
fn perceive_images(user_id: u64, message: &str) -> (Vec<String>, String) {
    let cfg = config::get();
    let vision_disabled = crate::anti_injection::is_vision_disabled(user_id);
    if !cfg.vision.enabled() || vision_disabled {
        return (Vec::new(), crate::vision::strip_image_cq(message));
    }

    let is_sticker_msg = crate::sticker::is_sticker_cq(message);
    let descriptions: Vec<String> = crate::vision::extract_image_urls(message)
        .iter()
        .filter_map(|url| {
            if is_sticker_msg {
                // 表情包走持久化缓存（下载→哈希→查 stickers.json→VLM）
                if let Some(desc) = crate::sticker::describe_sticker_cq(message) {
                    debug!("vision: got sticker description via hash cache");
                    return Some(desc);
                }
            }
            crate::vision::recognize_for_user(url, user_id)
        })
        .collect();

    (descriptions, crate::vision::strip_image_cq(message))
}

/// 把图片描述和文字组装成给她的感知内容
fn compose_perception(descriptions: &[String], text: &str) -> String {
    if descriptions.is_empty() {
        if text.is_empty() {
            "[图片]".to_string()
        } else {
            text.to_string()
        }
    } else {
        let img_ctx: Vec<String> = descriptions
            .iter()
            .enumerate()
            .map(|(i, d)| format!("[图片{}: {}]", i + 1, d))
            .collect();
        if text.is_empty() {
            img_ctx.join("\n")
        } else {
            format!("{}\n{}", img_ctx.join("\n"), text)
        }
    }
}

/// 感知一条批次消息：图片描述回写工作记忆，返回组装好的感知文本
fn perceive_batch_message(
    group_id: u64,
    user_id: u64,
    message: &str,
    record_timestamps: &[u64],
) -> String {
    let (descriptions, text_only) = perceive_images(user_id, message);
    if !descriptions.is_empty() && group_id > 0 {
        // 用精确时间戳把工作记忆中的 [图片] 替换为实际描述
        crate::working_memory::update_image_content(
            group_id,
            user_id,
            &descriptions,
            record_timestamps,
        );
    }
    compose_perception(&descriptions, &text_only)
}

// ── 私聊 ────────────────────────────────────────────────────────

/// 私聊消息处理：感知 → 表达 → 落地
pub fn process_message(user_id: u64, message: &str) {
    // 标记用户为处理中，防止并发处理同一用户的消息
    {
        let mut processing = processing_users().lock().unwrap();
        if processing.contains(&(0, user_id)) {
            info!(user_id, "process_message: 用户消息正在处理中，跳过");
            return;
        }
        processing.insert((0, user_id));
    }
    let _guard = ProcessingGuard {
        group_id: 0,
        user_id,
    };

    let cfg = config::get();
    let max_history = cfg.conversation.max_history;

    let (descriptions, text_only) = perceive_images(user_id, message);
    let ai_message = compose_perception(&descriptions, &text_only);

    // 追加用户消息到对话历史
    with_shared_state(|s| s.push_history(0, user_id, "user", &ai_message, max_history));

    // ── 感知入流 + 夜间门控：夜间是睡眠，不是免打扰（危机除外） ──
    let crisis_level = crate::emotion::get_state(user_id).crisis_level;
    let asleep = crate::mind::is_night() && !crisis_level.is_crisis();
    let mut perception = crate::mind::transcribe_message(
        &crate::person_info::get_display_name(user_id, 0).unwrap_or_else(|| "有人".into()),
        crate::util::now_secs(),
        &ai_message,
        false,
    );
    if asleep {
        perception.push_str("（她在睡梦中，还没看到这条）");
    }
    crate::mind::stream::push(
        crate::mind::StreamEvent::new(crate::mind::StreamKind::Sensation, perception)
            .with_about(user_id),
    );
    if asleep {
        debug!(user_id, "handler: 她在睡觉，消息留到早上");
        return;
    }

    // 联想：她能想起什么（转述入流，成为她的经历）
    for line in crate::mind::recall::recall_for(&ai_message, user_id, 0) {
        crate::mind::stream::push(
            crate::mind::StreamEvent::new(crate::mind::StreamKind::Sensation, line)
                .with_about(user_id),
        );
    }

    // 注意力模型
    if cfg.humanity.attention_enabled {
        let mut attn = crate::conversation::attention::load_attention();
        if !attn.focused_topic.is_empty() && !ai_message.contains(&attn.focused_topic) {
            crate::conversation::attention::interrupt_flow(&mut attn, &ai_message);
        } else if attn.focused_topic.is_empty() {
            attn.focused_topic = ai_message.clone();
        }
        crate::conversation::attention::update_attention(&mut attn, user_id, true);
        crate::conversation::attention::save_attention(&attn);
    }

    let history = read_shared_state(|s| s.get_history_clone(0, user_id));

    // 对话结束检测：关键词预筛选 + 上下文注入
    let extra_system = history.last().and_then(|(bot_last, _)| {
        let hour = crate::util::current_hour_cst();
        if crate::conversation_end::keyword_screen(bot_last, &ai_message, hour) {
            debug!(
                user_id,
                "conversation_end: keyword triggered, injecting context"
            );
            Some(crate::conversation_end::get_context(bot_last, &ai_message))
        } else {
            None
        }
    });

    match voice::speak_private(user_id, &ai_message, &history, extra_system.as_deref()) {
        VoiceAction::Reply(reply) => {
            crate::mind::stream::push(
                crate::mind::StreamEvent::new(crate::mind::StreamKind::Acted, reply.clone())
                    .with_about(user_id),
            );
            capture_last_plan(0, user_id, user_id);
            finish_private_reply(user_id, &ai_message, &reply);
        }
        VoiceAction::Silent => {
            // 沉默也是一次被记录的决策（私聊关系里，沉默有分量）
            crate::mind::stream::push(
                crate::mind::StreamEvent::new(
                    crate::mind::StreamKind::Acted,
                    "（她看了一眼，没有说话）",
                )
                .with_about(user_id),
            );
            debug!(user_id, "voice: private silent");
        }
    }
}

/// 私聊回复落地：发送 + 簿记
fn finish_private_reply(user_id: u64, user_message: &str, reply: &str) {
    if !crate::sender::safe_send(0, user_id, reply) {
        return;
    }
    info!(user_id, reply, "voice: private reply sent");

    let cfg = config::get();
    with_shared_state(|s| {
        s.push_history(0, user_id, "assistant", reply, cfg.conversation.max_history);
        s.record_reply(0, user_id);
    });

    // 社交电量：主动回复消耗
    if cfg.humanity.social_battery_enabled {
        let mut battery = crate::social_battery::load();
        let emo = crate::emotion::get_state(user_id);
        crate::social_battery::set_emotion_modifier(&mut battery, &emo.current);
        crate::social_battery::record_active_reply(&mut battery);
        crate::social_battery::save(&battery);
    }

    crate::person_info::relationship::record_interaction(user_id, true);
    crate::reply_effect::record_reply(0, user_id, reply, None);
    crate::working_memory::mark_replied(0, user_id);
    crate::activity::check_bot_message(user_id, reply);

    // 后处理任务不阻塞，放入后台线程
    let msg = user_message.to_string();
    let rep = reply.to_string();
    std::thread::spawn(move || {
        crate::personal_tasks::note_user_message(user_id, 0, &msg);
        crate::personal_tasks::extract_from_conversation(user_id, 0, &msg, &rep);
        crate::person_info::extract_facts_from_conversation(user_id, &msg, &rep);
        let history = read_shared_state(|s| s.get_history_clone(0, user_id));
        crate::memory::ai_extract(user_id, 0, &msg, &rep, &history);
        crate::memory::auto_summarize(user_id, 0, &history);
    });
}

// ── 群聊 ────────────────────────────────────────────────────────

/// 群聊批次处理：危机筛选 → 配额 → 表达 → 落地
pub fn process_group_batch(group_id: u64, user_msgs: &[(u64, String, Vec<u64>)]) {
    let cfg = config::get();
    let self_qq = cfg.self_qq;

    // ── 危机消息强制回应（绕过配额与沉默冷却） ──
    let mut forced_users: Vec<u64> = Vec::new();
    let mut crisis_utterances: Vec<GroupUtterance> = Vec::new();

    for (user_id, messages, timestamps) in user_msgs {
        let mut level = crate::emotion::get_state(*user_id).crisis_level;
        if !level.is_crisis()
            && crate::emotion::detect_crisis(messages).is_crisis()
            && let Some(detected) = crate::emotion::detect_crisis_ai(messages)
        {
            level = detected;
            crate::emotion::update_crisis(*user_id, level);
        }
        if level.is_crisis() {
            warn!(user_id = *user_id, group_id, level = ?level, "crisis: 群聊危机信号，强制回应");
            let perceived = perceive_batch_message(group_id, *user_id, messages, timestamps);
            crisis_utterances.push(GroupUtterance {
                user_id: *user_id,
                text: perceived,
                ts: timestamps
                    .first()
                    .copied()
                    .unwrap_or_else(crate::util::now_secs),
            });
            forced_users.push(*user_id);
        }
    }

    if !crisis_utterances.is_empty() {
        speak_and_deliver_group(group_id, &crisis_utterances, true, false, true);
        record_group_activity(group_id);
        return;
    }

    // ── 剩余消息 ──
    let remaining: Vec<&(u64, String, Vec<u64>)> = user_msgs
        .iter()
        .filter(|(uid, _, _)| !forced_users.contains(uid))
        .collect();
    if remaining.is_empty() {
        record_group_activity(group_id);
        return;
    }

    // ── 配额记账 ──
    crate::quota::check_and_review_segment(group_id);
    for (uid, msg, _) in &remaining {
        crate::quota::log_segment_message(group_id, *uid, msg);
    }

    let at_pattern = if self_qq > 0 {
        format!("[CQ:at,qq={self_qq}]")
    } else {
        String::new()
    };
    let joined: String = remaining
        .iter()
        .map(|(_, m, _)| m.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let addressed =
        !at_pattern.is_empty() && remaining.iter().any(|(_, m, _)| m.contains(&at_pattern));

    // ── 配额门槛：有余量先说话后扣账；耗尽时只有高优先级（@/darling）能突破 ──
    let quota_available = crate::quota::has_quota(group_id);
    if !quota_available {
        let bypass = crate::quota::try_reply(
            group_id,
            remaining[0].0,
            &joined,
            &at_pattern,
            cfg.darling_qq,
        );
        if !bypass {
            debug!(group_id, "quota: 配额耗尽且优先级不足，跳过");
            record_group_activity(group_id);
            return;
        }
    }

    // ── 沉默冷却：她刚决定不说话，短期内不再权衡 ──
    if !addressed && silence_cooling(group_id) {
        debug!(group_id, "voice: silence cooldown, skipping");
        record_group_activity(group_id);
        return;
    }

    let utterances: Vec<GroupUtterance> = remaining
        .iter()
        .map(|(uid, msg, ts)| GroupUtterance {
            user_id: *uid,
            text: perceive_batch_message(group_id, *uid, msg, ts),
            ts: ts.first().copied().unwrap_or_else(crate::util::now_secs),
        })
        .collect();

    speak_and_deliver_group(group_id, &utterances, addressed, quota_available, false);
    record_group_activity(group_id);

    // ── 表达学习：从群聊消息中学习语言风格（后台） ──
    if crate::learner::should_learn(group_id) {
        let learn_msgs: Vec<(u64, String)> = remaining
            .iter()
            .map(|(uid, msg, _)| (*uid, msg.clone()))
            .collect();
        std::thread::spawn(move || {
            crate::learner::learn_from_messages(group_id, &learn_msgs);
        });
    }
}

/// 表达调用 + 回复落地/沉默簿记
///
/// `crisis` 为 true 时绕过夜间门控（真正的危机不会被"她睡着了"挡住）。
fn speak_and_deliver_group(
    group_id: u64,
    utterances: &[GroupUtterance],
    force_reply: bool,
    consume_quota_on_reply: bool,
    crisis: bool,
) {
    let cfg = config::get();
    let max_history = cfg.conversation.max_history;
    let primary = utterances.first().map(|u| u.user_id).unwrap_or(0);

    // 概率式中断记账：从这里到开口，期间新到的消息都可能让话题变掉
    crate::conversation::interruption::begin(group_id);

    // ── 感知入流（含夜间标记）+ 夜间门控：她真的睡了 ──
    let asleep = crate::mind::is_night() && !crisis;
    for u in utterances {
        let mut perception = crate::mind::transcribe_message(
            &crate::person_info::get_display_name(u.user_id, group_id)
                .unwrap_or_else(|| "群友".into()),
            u.ts,
            &u.text,
            false,
        );
        if asleep {
            perception.push_str("（她在睡梦中，还没看到这条）");
        }
        crate::mind::stream::push(
            crate::mind::StreamEvent::new(crate::mind::StreamKind::Sensation, perception)
                .with_about(u.user_id),
        );
    }
    if asleep {
        debug!(group_id, "handler: 她在睡觉，群消息留到早上");
        return;
    }

    // ── SpeakScore 门控：开口势低于阈值 → 这轮她没注意到（批次已照常入流，
    //    下次回神翻流时依然看得见；@/危机等 forced 路径不会走到这里） ──
    if !force_reply {
        let utterance_refs: Vec<(u64, &str)> = utterances
            .iter()
            .map(|u| (u.user_id, u.text.as_str()))
            .collect();
        let score = crate::mind::social::speak_score(group_id, &utterance_refs, primary);
        if score < crate::mind::social::SPEAK_GATE {
            debug!(
                group_id,
                score,
                gate = crate::mind::social::SPEAK_GATE,
                "voice: speak score below gate, staying quiet"
            );
            mark_silence(group_id);
            if cfg.humanity.social_battery_enabled {
                let mut battery = crate::social_battery::load();
                crate::social_battery::record_passive_participation(&mut battery);
                crate::social_battery::save(&battery);
            }
            return;
        }
    }

    // 用户消息进入各自历史（摘要压缩依赖它）
    for u in utterances {
        let text_only = crate::vision::strip_image_cq(&u.text);
        let stored = if text_only.is_empty() {
            u.text.clone()
        } else {
            text_only
        };
        with_shared_state(|s| {
            s.push_history(group_id, u.user_id, "user", &stored, max_history);
        });
    }

    // 联想：她能想起什么（以在场话题为线索，转述入流）
    let joined_text: String = utterances
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for line in crate::mind::recall::recall_for(&joined_text, primary, group_id) {
        crate::mind::stream::push(
            crate::mind::StreamEvent::new(crate::mind::StreamKind::Sensation, line)
                .with_about(primary),
        );
    }

    // 注意力模型（以主要发言人计）
    if primary > 0 && cfg.humanity.attention_enabled {
        let joined: String = utterances
            .iter()
            .map(|u| u.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut attn = crate::conversation::attention::load_attention();
        if !attn.focused_topic.is_empty() && !joined.contains(&attn.focused_topic) {
            crate::conversation::attention::interrupt_flow(&mut attn, &joined);
        } else if attn.focused_topic.is_empty() {
            attn.focused_topic = joined.clone();
        }
        crate::conversation::attention::update_attention(&mut attn, primary, true);
        crate::conversation::attention::save_attention(&attn);
    }

    match voice::speak_group(group_id, utterances, force_reply) {
        VoiceAction::Reply(reply) => {
            // 概率式中断：生成完、开口前的最后一刻，若处理期间涌进大量新消息，
            // 话题可能已经变了——把到嘴边的话咽回去，带着最新消息重新看一眼
            let swallowed = !force_reply
                && cfg.conversation.interruption_enabled
                && crate::conversation::interruption::should_swallow(group_id);
            if swallowed {
                debug!(group_id, "voice: interrupted before speaking");
                mark_silence(group_id);
                if cfg.humanity.social_battery_enabled {
                    let mut battery = crate::social_battery::load();
                    crate::social_battery::record_passive_participation(&mut battery);
                    crate::social_battery::save(&battery);
                }
            } else {
                // 她说的话成为她的经历
                crate::mind::stream::push(
                    crate::mind::StreamEvent::new(crate::mind::StreamKind::Acted, reply.clone())
                        .with_about(primary),
                );
                capture_last_plan(group_id, primary, primary);
                if consume_quota_on_reply {
                    crate::quota::check_and_consume(group_id);
                }
                finish_group_reply(group_id, primary, utterances, &reply);
            }
        }
        VoiceAction::Silent => {
            // 群聊沉默不逐次入流（会淹没她的经历），只做冷却与电量记账
            debug!(group_id, "voice: group silent");
            mark_silence(group_id);
            if cfg.humanity.social_battery_enabled {
                let mut battery = crate::social_battery::load();
                crate::social_battery::record_passive_participation(&mut battery);
                crate::social_battery::save(&battery);
            }
        }
    }
    crate::conversation::interruption::end(group_id);
}

/// 群聊回复落地：发送 + 簿记
fn finish_group_reply(group_id: u64, primary: u64, utterances: &[GroupUtterance], reply: &str) {
    // 去重：短时间内不发送相同回复
    if crate::runtime::reply_dedup::is_duplicate(group_id, reply) {
        info!(group_id, "dedup: 检测到重复回复，跳过发送");
        return;
    }

    if !crate::sender::safe_send(group_id, primary, reply) {
        return;
    }
    info!(
        group_id,
        user_id = primary,
        reply,
        "voice: group reply sent"
    );

    let cfg = config::get();
    with_shared_state(|s| {
        s.push_history(
            group_id,
            primary,
            "assistant",
            reply,
            cfg.conversation.max_history,
        );
        s.push_group_history(group_id, "assistant", reply, cfg.conversation.max_history);
        s.record_reply(group_id, primary);
        s.record_bot_message(group_id, reply);
    });

    // 社交电量：主动回复消耗
    if cfg.humanity.social_battery_enabled {
        let mut battery = crate::social_battery::load();
        crate::social_battery::record_active_reply(&mut battery);
        crate::social_battery::save(&battery);
    }

    for u in utterances {
        crate::person_info::relationship::record_interaction(u.user_id, true);
        crate::working_memory::mark_replied(group_id, u.user_id);
    }

    // ── 训练数据留档：(触发, 回复) 配对——离线风格学习的监督信号 ──
    // 行 id 传给回复效果追踪：ASI 定稿后 reward 精确写回这一行
    let trigger: String = utterances
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let archive_reply_id =
        crate::mind::archive::record_reply(group_id, primary, &trigger, reply, false);

    // ── 社会世界模型：她的话挂进最热线程——
    //    她下一眼能看见自己刚说过什么，并由此开始观察有没有人接她的话 ──
    crate::mind::social::record_bot_speech(group_id, reply);

    crate::reply_effect::record_reply(group_id, primary, reply, archive_reply_id);
    crate::runtime::reply_dedup::record(group_id, primary, reply);
    crate::activity::check_bot_message(primary, reply);

    // 后处理任务不阻塞，逐用户放入后台线程
    let rep = reply.to_string();
    let gid = group_id;
    let batch: Vec<(u64, String)> = utterances
        .iter()
        .map(|u| {
            let text_only = crate::vision::strip_image_cq(&u.text);
            (
                u.user_id,
                if text_only.is_empty() {
                    u.text.clone()
                } else {
                    text_only
                },
            )
        })
        .collect();

    std::thread::spawn(move || {
        for (uid, msg) in batch {
            crate::personal_tasks::note_user_message(uid, gid, &msg);
            crate::personal_tasks::extract_from_conversation(uid, gid, &msg, &rep);
            crate::person_info::extract_facts_from_conversation(uid, &msg, &rep);
            let history = read_shared_state(|s| s.get_history_clone(gid, uid));
            crate::memory::ai_extract(uid, gid, &msg, &rep, &history);
            crate::memory::auto_summarize(uid, gid, &history);
        }
    });
}

/// 记录群活跃时间
fn record_group_activity(group_id: u64) {
    with_shared_state(|s| s.record_conversation(group_id, crate::util::now_secs()));
}

/// 把她在表达里留下的"想起"（plan_next）写进意图堆
fn capture_last_plan(group_id: u64, target_user: u64, about_user: u64) {
    let Some((in_secs, reason)) = voice::take_last_plan() else {
        return;
    };
    let due_at = crate::util::now_secs() + in_secs.max(60);
    // 十分钟内的想起算"稍后想"：到期更早被兑现，失败也更早重试
    let urgency = if in_secs <= 600 {
        crate::mind::Urgency::Soon
    } else {
        crate::mind::Urgency::Later
    };
    let mut plan = crate::mind::WakePlan::new(crate::mind::WakeKind::Idle, due_at, reason)
        .with_about(about_user)
        .with_urgency(urgency);
    if group_id > 0 {
        plan.target_group = Some(group_id);
    } else {
        plan.target_user = Some(target_user);
    }
    crate::mind::add_wake_plan(plan);
}
