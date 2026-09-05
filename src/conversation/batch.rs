//! 批次处理：消息合并与过期分发
//!
//! 批次过期后按群/私聊分组：私聊独立线程直接处理，
//! 群聊进入消息队列串行处理（真正的决策在 `handler::process_group_batch`）。

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use tracing::{info, warn};

use super::handler::process_message;
use crate::{MESSAGE_QUEUE, ProcessingTask, config, processing_users, with_state};

pub fn process_expired_batches() {
    let cfg = config::get();
    let timeout = cfg.conversation.batch_timeout_ms;

    // 收集所有过期批次，跳过正在处理中的用户: (group_id, user_id, messages, record_timestamps)
    let expired: Vec<(u64, u64, String, Vec<u64>)> = {
        let mut result = Vec::new();
        let processing = processing_users().lock().unwrap();
        with_state(|s| {
            let expired_keys: Vec<(u64, u64)> = s
                .batches
                .iter()
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
        if let Some((extra, extra_ts)) =
            with_state(|s| s.take_batch_for_processing(group_id, user_id))
        {
            final_msgs.push('\n');
            final_msgs.push_str(&extra);
            timestamps.extend(extra_ts);
        }
        merged.push((group_id, user_id, final_msgs, timestamps));
    }

    // 按群组聚合: 同一群的所有消息一起进入语音决策
    let mut group_msgs: HashMap<u64, Vec<(u64, String, Vec<u64>)>> = HashMap::new();
    let mut private_batches: Vec<(u64, String)> = Vec::new();

    for (group_id, user_id, messages, timestamps) in merged {
        if group_id > 0 {
            group_msgs
                .entry(group_id)
                .or_default()
                .push((user_id, messages, timestamps));
        } else {
            private_batches.push((user_id, messages));
        }
    }

    // 处理私聊批次 (独立线程，不阻塞主循环)
    for (user_id, messages) in private_batches {
        thread::spawn(move || {
            process_message(user_id, &messages);
        });
    }

    // 处理群聊批次: 通过消息队列串行化处理，避免并发混乱
    for (group_id, user_msgs) in group_msgs {
        if let Some(queue) = MESSAGE_QUEUE.get()
            && queue
                .tx
                .send(ProcessingTask {
                    group_id,
                    user_msgs,
                })
                .is_err()
        {
            warn!(group_id, "queue: 发送失败");
        }
    }
}
