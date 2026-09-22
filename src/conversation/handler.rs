//! 消息处理：私聊表达路径 + 群聊表达调度
//!
//! 群聊和私聊都走同一条表达管线（`voice`）：
//! 同一个"她"读完场面后，在同一口气里决定说话、沉默或发表情包。
//! 本模块负责感知准备（视觉、历史、注意力）、调度与回复落地簿记。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::util::MutexExt;
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
///
/// 图片描述来自 VLM，是对**不可信图片**的转述（表情包里可以写任何
/// 字样），因此与网页搜索结果同级：拼进感知之前先过滤壳，命中即替换成
/// 中性的 `[图片]` 并留一条安全日志。没有这道过滤，一张图片就能把指令
/// 送进她的 prompt——`check_input` 只看文本消息，看不到图像描述。
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
            .map(|(i, d)| {
                if crate::anti_injection::check_memory_entry(d).passed {
                    format!("[图片{}: {}]", i + 1, d)
                } else {
                    crate::mind::security::log_event(0, "vision_perception", "rejected", d);
                    format!("[图片{}]", i + 1)
                }
            })
            .collect();
        if text.is_empty() {
            img_ctx.join("\n")
        } else {
            format!("{}\n{}", img_ctx.join("\n"), text)
        }
    }
}

// ── 多模态统一记忆 ──────────────────────────────────────────────

/// 每个用户两次图片记忆之间的最小间隔（防刷屏，不防真心分享）
const IMAGE_MEMORY_COOLDOWN_SECS: u64 = 3600;
/// 描述短于这个长度的不值得记（VLM 兜话或没识别出内容）
const IMAGE_MEMORY_MIN_DESC_CHARS: usize = 12;

static LAST_IMAGE_MEMORY: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();

/// 把图片描述沉淀为文本记忆，与文字记忆进同一个向量空间
///
/// 图片是不可信输入：描述先过与搜索结果同源的防注入检测，命中即丢弃。
/// 表情包是语气不是信息，由调用方负责不送进来。
fn remember_images(user_id: u64, group_id: u64, descriptions: &[String]) {
    if !config::get().vision.memory_images || descriptions.is_empty() {
        return;
    }
    // 每人冷却：主线程轻查，写入在后台
    let now = crate::util::now_secs();
    let last = LAST_IMAGE_MEMORY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|m| m.get(&user_id).copied())
        .unwrap_or(0);
    if now.saturating_sub(last) < IMAGE_MEMORY_COOLDOWN_SECS {
        return;
    }

    let usable: Vec<String> = descriptions
        .iter()
        .filter(|d| d.chars().count() >= IMAGE_MEMORY_MIN_DESC_CHARS)
        .filter(|d| crate::anti_injection::check_memory_entry(d).passed)
        .cloned()
        .collect();
    if usable.is_empty() {
        return;
    }
    if let Ok(mut m) = LAST_IMAGE_MEMORY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        m.insert(user_id, now);
    }

    let name = crate::person_info::get_display_name(user_id, group_id)
        .unwrap_or_else(|| "有人".to_string());
    std::thread::spawn(move || {
        for desc in usable {
            let content = format!("（{name}给我看过一张图：{desc}）");
            crate::memory::add(
                user_id,
                group_id,
                &content,
                crate::memory::Importance::Normal,
            );
        }
        debug!(user_id, group_id, "vision: 图片描述沉淀为记忆");
    });
}

/// 把 QQ 号翻成她认得的名字；她自己一定认得
///
/// 她本人不一定在 `person_info` 里有档案（那是给"别人"建的），
/// 所以对自己单独兜底：否则"别人 @ 她"会被标成"你不认识这个人"。
fn resolve_name(qq: u64, group_id: u64) -> Option<String> {
    let cfg = config::get();
    if qq == cfg.self_qq && cfg.self_qq > 0 {
        return Some(cfg.bot_name);
    }
    crate::person_info::get_display_name(qq, group_id)
}

/// 感知一条批次消息：图片描述回写工作记忆，返回组装好的感知文本
///
/// CQ 码在这里统一归一（见 [`crate::conversation::perception`]）：
/// `[CQ:markdown,…]` 这类富文本此前完全没被处理过，一坨几百字符的
/// 原始 markup（HTML 转义、`mqqapi://` 链接、图片链接、代码块）直接进了
/// 她的 prompt——她正是在这种消息里"看不清谁 @ 了谁"。
fn perceive_batch_message(group_id: u64, user_id: u64, message: &str, entry_ids: &[u64]) -> String {
    let (descriptions, text_only) = perceive_images(user_id, message);
    if !descriptions.is_empty() && group_id > 0 {
        // 用精确时间戳把工作记忆中的 [图片] 替换为实际描述
        crate::working_memory::update_image_content(group_id, user_id, &descriptions, entry_ids);
    }
    // 表情包是语气不是信息：只有普通图片才沉淀记忆
    if !crate::sticker::is_sticker_cq(message) {
        remember_images(user_id, group_id, &descriptions);
    }
    // 归一富文本：@ 还原成人名，转义解码，markup 剥掉
    let normalized =
        crate::conversation::perception::normalize(&text_only, &|qq| resolve_name(qq, group_id));
    compose_perception(&descriptions, &normalized)
}

// ── 私聊 ────────────────────────────────────────────────────────

/// 私聊消息处理：感知 → 表达 → 落地
pub(crate) fn process_message(user_id: u64, message: &str) {
    // 标记用户为处理中，防止并发处理同一用户的消息
    {
        let mut processing = processing_users().lock_recover();
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
    // 表情包是语气不是信息：只有普通图片才沉淀记忆
    if !crate::sticker::is_sticker_cq(message) {
        remember_images(user_id, 0, &descriptions);
    }

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
        let mut event = crate::mind::StreamEvent::new(
            crate::mind::StreamKind::Sensation,
            line.clone(),
        )
        .with_about(user_id);
        if let Some(source) = crate::mind::recall::source_for(&line) {
            event = event.with_recall(
                crate::mind::recall::recall_id(user_id, 0, &line),
                source,
            );
        }
        crate::mind::stream::push(event);
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
    if !crate::sender::safe_send(0, user_id, reply, user_message) {
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
        crate::personal_tasks::extract_from_conversation(user_id, 0, &msg);
        crate::person_info::extract_facts_from_conversation(user_id, &msg, &rep);
        let history = read_shared_state(|s| s.get_history_clone(0, user_id));
        crate::memory::ai_extract(user_id, 0, &msg, &rep, &history);
        crate::memory::auto_summarize(user_id, 0, &history);
    });
}

// ── 群聊 ────────────────────────────────────────────────────────

/// 一批待处理的群消息：某个用户在一个批次窗口里说的话
///
/// 结构化而不是元组：字段多了以后 `(u64, String, Vec<u64>, Vec<u64>)`
/// 在调用点完全读不出含义，传参顺序写错编译器也帮不上忙。
#[derive(Debug, Clone)]
pub(crate) struct GroupBatch {
    /// 群号（私聊为 0）
    pub group_id: u64,
    pub user_id: u64,
    /// 消息本体与时间信息
    pub taken: crate::state::TakenBatch,
}

impl GroupBatch {
    /// 这批消息最早到达的时刻（秒）
    pub(crate) fn first_arrival(&self) -> u64 {
        self.taken.first_arrival()
    }

    /// 排序用的到达时刻（毫秒）
    pub(crate) fn sort_key_ms(&self) -> u64 {
        self.taken.sort_key_ms()
    }
}

/// 群聊批次处理：危机筛选 → 配额 → 表达 → 落地
pub(crate) fn process_group_batch(group_id: u64, user_msgs: &[GroupBatch]) {
    let cfg = config::get();
    let self_qq = cfg.self_qq;

    // ── 危机消息强制回应（绕过配额与沉默冷却） ──
    let mut forced_users: Vec<u64> = Vec::new();
    let mut crisis_utterances: Vec<GroupUtterance> = Vec::new();

    for batch in user_msgs {
        let user_id = &batch.user_id;
        let messages = &batch.taken.messages;
        let timestamps = &batch.taken.entry_ids;
        let mut level = crate::emotion::get_state(*user_id).crisis_level;
        if !level.is_crisis()
            && crate::crisis::detect_crisis(messages).is_crisis()
            && let Some(detected) = crate::crisis::detect_crisis_ai(messages)
        {
            level = detected;
            crate::crisis::update_crisis(*user_id, level);
        }
        if level.is_crisis() {
            warn!(user_id = *user_id, group_id, level = ?level, "crisis: 群聊危机信号，强制回应");
            let perceived = perceive_batch_message(group_id, *user_id, messages, timestamps);
            crisis_utterances.push(GroupUtterance {
                user_id: *user_id,
                text: perceived,
                // @ 必须从原始文本取：perceive 已把 CQ 码还原成人名
                at_targets: crate::conversation::turn::at_targets(messages),
                ts: batch.first_arrival(),
                ts_ms: batch.sort_key_ms(),
            });
            forced_users.push(*user_id);
        }
    }

    if !crisis_utterances.is_empty() {
        // 危机路径必然回应，焦点只用来定"回给谁"
        let crisis_focus = crate::conversation::turn::focus_batch(
            &crisis_utterances,
            self_qq,
            &cfg.bot_name,
            &|_| false,
        );
        speak_and_deliver_group(
            group_id,
            &crisis_utterances,
            &crisis_focus,
            true,
            false,
            true,
        );
        return;
    }

    // ── 剩余消息 ──
    let remaining: Vec<&GroupBatch> = user_msgs
        .iter()
        .filter(|batch| !forced_users.contains(&batch.user_id))
        .collect();
    if remaining.is_empty() {
        return;
    }

    // ── 配额记账 ──
    for batch in &remaining {
        crate::quota::log_segment_message(group_id, batch.user_id, &batch.taken.messages);
    }

    let at_pattern = if self_qq > 0 {
        format!("[CQ:at,qq={self_qq}]")
    } else {
        String::new()
    };
    let joined: String = remaining
        .iter()
        .map(|batch| batch.taken.messages.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let addressed = !at_pattern.is_empty()
        && remaining
            .iter()
            .any(|batch| batch.taken.messages.contains(&at_pattern));

    // ── 配额门槛：有余量先说话后扣账；耗尽时只有高优先级（@/darling）能突破 ──
    let quota_available = crate::quota::has_quota(group_id);
    if !quota_available {
        let bypass = crate::quota::try_reply(
            group_id,
            remaining[0].user_id,
            &joined,
            &at_pattern,
            cfg.darling_qq,
        );
        if !bypass {
            debug!(group_id, "quota: 配额耗尽且优先级不足，跳过");
            return;
        }
    }

    // 沉默冷却的判定放在轮次焦点之后（见下）——被点名时冷却不能挡

    let utterances: Vec<GroupUtterance> = remaining
        .iter()
        .map(|batch| GroupUtterance {
            user_id: batch.user_id,
            // perceive_batch_message 已把 CQ 富文本归一、把 @ 还原成人名
            text: perceive_batch_message(
                group_id,
                batch.user_id,
                &batch.taken.messages,
                &batch.taken.entry_ids,
            ),
            // @ 的判定必须走原始文本，归一化后 parse 不出 `[CQ:at,qq=…]`
            at_targets: crate::conversation::turn::at_targets(&batch.taken.messages),
            // 用真实到达时刻排序：工作记忆时间戳只有秒级精度，
            // 同一秒内两个人的话谁先谁后会退化成哈希顺序
            ts: batch.first_arrival(),
            ts_ms: batch.sort_key_ms(),
        })
        .collect();

    // 群级现场在沉默冷却和表达决策前记录，下一轮才能接住她错过的上下文。
    for u in &utterances {
        let text_only = crate::vision::strip_image_cq(&u.text);
        let stored = if text_only.is_empty() { u.text.clone() } else { text_only };
        let name = crate::person_info::get_display_name(u.user_id, group_id)
            .unwrap_or_else(|| "群友".to_string());
        with_shared_state(|s| {
            s.push_group_history(
                group_id,
                "user",
                &format!("[{name}] {stored}"),
                cfg.conversation.max_history,
            );
        });
    }

    // 轮次焦点：这批消息在跟谁说话（确定性判定，只用 @ / 名字 / 跟进关系）。
    // 回复目标由这里定，而不是"哪个用户的批次先到期"——批次是按
    // (群, 用户) 切出来的，取 first() 会答错人（实测 15.1% 的回复对象
    // 不是最后一位发文字的人）。
    let focus =
        crate::conversation::turn::focus_batch(&utterances, self_qq, &cfg.bot_name, &|uid| {
            read_shared_state(|s| {
                s.is_in_follow_up(group_id, uid, cfg.conversation.reply_follow_up_secs)
            })
        });

    // 冷却是"这会儿不太想插话"，不是"听不见"：点名/叫名字必须能穿透
    if focus.is_solely_for_others() {
        debug!(group_id, "voice: 本批明确对其他群友说话，继续旁听");
        return;
    }
    if silence_cooling(group_id) && !focus.is_called() && !addressed && focus.followed_up_by.is_empty() {
        debug!(group_id, "voice: silence cooldown, skipping");
        return;
    }

    speak_and_deliver_group(
        group_id,
        &utterances,
        &focus,
        addressed,
        quota_available,
        false,
    );

    // ── 表达学习：从群聊消息中学习语言风格（后台） ──
    if crate::learner::should_learn(group_id) {
        let learn_msgs: Vec<(u64, String)> = remaining
            .iter()
            .map(|batch| (batch.user_id, batch.taken.messages.clone()))
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
    focus: &crate::conversation::turn::TurnFocus,
    force_reply: bool,
    consume_quota_on_reply: bool,
    crisis: bool,
) {
    let cfg = config::get();
    let max_history = cfg.conversation.max_history;
    let primary = focus.primary;

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

    // ── SpeakScore 门控：只在"完全没被点到"时才用来省 API ──
    //
    // 被 @、被叫名字（`focus.is_called()`）与危机一样直接叫醒她：这类
    // 消息必须由她自己决定说什么或不说，不能由一个分数替她拒绝。
    //
    // 实测标定：被拦样本最高分 0.2948，而门限是 0.30——裕度几乎为零。
    // 分数被"刚说过话"的两个惩罚项主导，导致密集群里越是聊得热闹越
    // 说不上话。现在被点名加成进入评分，门限下调到只挡真正的噪声。
    if !force_reply && !focus.is_called() {
        let utterance_refs: Vec<(u64, &str)> = utterances
            .iter()
            .map(|u| (u.user_id, u.text.as_str()))
            .collect();
        let breakdown = crate::mind::social::speak_score(
            group_id,
            &utterance_refs,
            primary,
            focus.addressing_strength(),
        );
        let gate = crate::mind::social::speak_gate();
        // 结构化单行：门限该定在哪，只能靠真实分布回答，不能靠猜。
        // 这一行带全部评分分量，可直接从日志回放复算（方案 §4 要求）。
        debug!(
            group_id,
            primary,
            utterances = utterances.len(),
            gate,
            called = focus.is_called(),
            result = if breakdown.total >= gate { "pass" } else { "silent" },
            detail = %breakdown.log_line(),
            "voice: gate decision"
        );
        if breakdown.total < gate {
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
        let mut event = crate::mind::StreamEvent::new(
            crate::mind::StreamKind::Sensation,
            line.clone(),
        )
        .with_about(primary);
        if let Some(source) = crate::mind::recall::source_for(&line) {
            event = event.with_recall(
                crate::mind::recall::recall_id(primary, group_id, &line),
                source,
            );
        }
        crate::mind::stream::push(event);
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

    match voice::speak_group(group_id, utterances, focus, force_reply) {
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
    // 先登记再排队发送：发送现在是异步的（不阻塞决策线程），
    // 若等到发完才登记，相邻两轮可能都通过去重检查、把同一句话发两遍。
    crate::runtime::reply_dedup::record(group_id, reply);

    let incoming: String = utterances
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if !crate::sender::safe_send(group_id, primary, reply, &incoming) {
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
            crate::personal_tasks::extract_from_conversation(uid, gid, &msg);
            crate::person_info::extract_facts_from_conversation(uid, &msg, &rep);
            let history = read_shared_state(|s| s.get_history_clone(gid, uid));
            crate::memory::ai_extract(uid, gid, &msg, &rep, &history);
            crate::memory::auto_summarize(uid, gid, &history);
        }
    });
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
