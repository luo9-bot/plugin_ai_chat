//! 批次处理：消息合并、过期批次检测、群组串行化处理

use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::{
    config, emotion, learner, quota, timing_gate, util, working_memory,
    with_state, with_shared_state, read_shared_state,
    processing_users, MESSAGE_QUEUE, ProcessingTask,
};
use super::handler::process_message;

pub fn process_expired_batches() {
    let cfg = config::get();
    let timeout = cfg.conversation.batch_timeout_ms;

    // 收集所有过期批次，跳过正在处理中的用户: (group_id, user_id, messages, record_timestamps)
    let expired: Vec<(u64, u64, String, Vec<u64>)> = {
        let mut result = Vec::new();
        let processing = processing_users().lock().unwrap();
        with_state(|s| {
            let expired_keys: Vec<(u64, u64)> = s.batches.iter()
                .filter(|(_, batch)| batch.last_update.elapsed().as_millis() >= timeout as u128)
                .filter(|(key, _)| !processing.contains(key))
                .map(|(&key, _)| key)
                .collect();
            for (gid, uid) in expired_keys {
                if let Some((msgs, timestamps)) = s.take_batch_for_processing(gid, uid) {
                    result.push((gid, uid, msgs, timestamps));
                }
            }
        });
        result
    };

    if expired.is_empty() {
        return;
    }

    info!(count = expired.len(), "batch: processing expired batches");

    // 预合并: 短等待让尾部消息到达 (用户连发多条时的合并窗口)
    thread::sleep(Duration::from_millis(500));
    let mut merged: Vec<(u64, u64, String, Vec<u64>)> = Vec::new();
    for (group_id, user_id, messages, mut timestamps) in expired {
        let mut final_msgs = messages;
        if let Some((extra, extra_ts)) = with_state(|s| s.take_batch_for_processing(group_id, user_id)) {
            final_msgs.push('\n');
            final_msgs.push_str(&extra);
            timestamps.extend(extra_ts);
        }
        merged.push((group_id, user_id, final_msgs, timestamps));
    }

    // 按群组聚合: 同一群的所有消息一起做 AI 决策
    let mut group_msgs: HashMap<u64, Vec<(u64, String, Vec<u64>)>> = HashMap::new();
    let mut private_batches: Vec<(u64, String, Vec<u64>)> = Vec::new();

    for (group_id, user_id, messages, timestamps) in merged {
        if group_id > 0 {
            group_msgs.entry(group_id).or_default().push((user_id, messages, timestamps));
        } else {
            private_batches.push((user_id, messages, timestamps));
        }
    }

    // 处理私聊批次 (独立线程，不阻塞主循环)
    for (user_id, messages, timestamps) in private_batches {
        thread::spawn(move || {
            process_message(user_id, 0, &messages, &timestamps);
        });
    }

    // 处理群聊批次: 通过消息队列串行化处理，避免并发混乱
    for (group_id, user_msgs) in group_msgs {
        if let Some(queue) = MESSAGE_QUEUE.get()
            && queue.tx.send(ProcessingTask {
                group_id,
                user_msgs,
            }).is_err()
        {
            warn!(group_id, "queue: 发送失败");
        }
    }
}

/// 串行处理单个群组的消息批次（由消息队列 worker 调用）
pub fn process_group_batch(group_id: u64, user_msgs: &[(u64, String, Vec<u64>)]) {
    // ── 第一步：危机消息强制回复 ──
    // analyze_user_message() 已在 handle_group_msg 中调用，更新了危机状态
    // 这里读取已更新的状态，无需重复关键词检测
    let mut handled_users: HashSet<u64> = HashSet::new();

    for (user_id, messages, timestamps) in user_msgs {
        let crisis_level = emotion::get_state(*user_id).crisis_level;
        if crisis_level.is_crisis() {
            tracing::warn!(user_id = *user_id, group_id, level = ?crisis_level, "crisis: 群聊危机信号(已检测)，强制回复");
            process_message(*user_id, group_id, messages, timestamps);
            handled_users.insert(*user_id);
        }
    }

    // ── 第二步：收集剩余消息 ──
    let remaining: Vec<&(u64, String, Vec<u64>)> = user_msgs.iter()
        .filter(|(uid, _, _)| !handled_users.contains(uid))
        .collect();

    if remaining.is_empty() {
        with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
        return;
    }

    // ── 第三步：AI 辅助危机检测（关键词未命中时，检测隐晦表达） ──
    for item in remaining.iter() {
        let (user_id, messages, timestamps): &(u64, String, Vec<u64>) = item;
        if let Some(level) = emotion::detect_crisis_ai(messages) {
            tracing::warn!(user_id = *user_id, group_id, level = ?level, "crisis: 群聊危机信号(AI检测)，强制回复");
            emotion::update_crisis(*user_id, level);
            process_message(*user_id, group_id, messages, timestamps);
            handled_users.insert(*user_id);
        }
    }

    // 更新 remaining（排除危机用户）
    let remaining: Vec<&(u64, String, Vec<u64>)> = user_msgs.iter()
        .filter(|(uid, _, _)| !handled_users.contains(uid))
        .collect();

    if remaining.is_empty() {
        with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
        return;
    }

    // ── 第四步：@bot 强制回复（跳过 Timing Gate） ──
    let self_qq = config::get().self_qq;
    let at_pattern = if self_qq > 0 { format!("[CQ:at,qq={}]", self_qq) } else { String::new() };
    let darling_qq = config::get().darling_qq;
    let mut at_handled = false;

    for (uid, msg, ts) in &remaining {
        if self_qq > 0 && msg.contains(&at_pattern) {
            debug!(user_id = *uid, group_id, "timing_gate: @bot detected, force reply");
            if quota::try_reply(group_id, *uid, msg, &at_pattern, darling_qq) {
                process_message(*uid, group_id, msg, ts);
                at_handled = true;
            }
        }
    }

    if at_handled {
        with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
        return;
    }

    // ── 第五步：配额检查 + 段兴趣追踪 ──
    quota::check_and_review_segment(group_id);
    for (uid, msg, _) in &remaining {
        quota::log_segment_message(group_id, *uid, msg);
    }

    if !quota::has_quota(group_id) {
        debug!(group_id, "quota: 配额耗尽，跳过 Timing Gate");
        with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
        return;
    }

    // ── 第六步：Timing Gate 决策 ──
    // 冷却期内跳过（刚决定沉默就不再评估）
    if timing_gate::is_in_cooldown(group_id) {
        debug!(group_id, "timing_gate: in cooldown, skipping");
        with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
        return;
    }

    let gate_context = timing_gate::GateContext {
        identity: config::prompt().to_string(),
        recent_bot_messages: read_shared_state(|s| s.get_recent_bot_messages(group_id, 600, 5)),
        working_memory: working_memory::get_context(group_id, 3600),
        self_qq,
        is_group: true,
    };

    let decision = timing_gate::run_timing_gate(group_id, &remaining, &gate_context);

    match decision {
        timing_gate::GateDecision::Continue => {
            // Timing Gate 批准：对所有剩余用户尝试回复
            let mut replied = 0u32;
            let mut quota_exhausted = false;
            for (uid, msg, ts) in &remaining {
                if quota_exhausted || !quota::try_reply(group_id, *uid, msg, &at_pattern, darling_qq) {
                    quota_exhausted = true;
                    debug!(uid, group_id, "timing_gate: 配额耗尽，跳过回复");
                    continue;
                }
                process_message(*uid, group_id, msg, ts);
                replied += 1;
            }
            if replied > 0 {
                debug!(group_id, replied, "timing_gate: continue -> replied");
            }
        }
        timing_gate::GateDecision::NoReply => {
            // TODO
        }
        timing_gate::GateDecision::Wait(_seconds) => {
            // TODO: 实现延迟重新评估
        }
    }

    // ── 表达学习：从群聊消息中学习语言风格 ──
    if learner::should_learn(group_id) {
        let learn_msgs: Vec<(u64, String)> = remaining
            .iter()
            .map(|(uid, msg, _)| (*uid, msg.clone()))
            .collect();
        // 在后台线程学习，不阻塞主流程
        std::thread::spawn(move || {
            learner::learn_from_messages(group_id, &learn_msgs);
        });
    }

    // 记录对话活跃时间
    with_shared_state(|s| s.record_conversation(group_id, util::now_secs()));
}
