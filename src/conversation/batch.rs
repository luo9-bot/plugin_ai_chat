//! 批次处理：消息合并与过期分发
//!
//! 批次过期后按群/私聊分组：私聊独立线程直接处理，
//! 群聊进入消息队列串行处理（真正的决策在 `handler::process_group_batch`）。
//!
//! 本函数运行在 **1ms tick 的主事件循环**上，因此这里绝不能阻塞：
//! 早先版本为了"等尾部消息一起合并"在这里 `sleep(500ms)`，等于每处理
//! 一批就把整个插件的事件循环停半秒——用户消息被推迟看见、新消息在
//! 队列里堆积、回神与定时任务一起被拖慢。合并窗口现在由批次自身的
//! `batch_timeout_ms` 决定：过期才取，取出来就立刻送走。

use std::collections::HashMap;
use std::thread;

use tracing::{info, warn};

use super::handler::{GroupBatch, process_message};
use crate::{MESSAGE_QUEUE, ProcessingTask, config, processing_users, with_state};

pub fn process_expired_batches() {
    let cfg = config::get();
    let timeout = cfg.conversation.batch_timeout_ms;

    // 收集所有过期批次，跳过正在处理中的用户
    let expired: Vec<GroupBatch> = {
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
                if let Some(taken) = s.take_batch_for_processing(gid, uid) {
                    result.push(GroupBatch {
                        group_id: gid,
                        user_id: uid,
                        taken,
                    });
                }
            }
        });
        result
    };

    if expired.is_empty() {
        return;
    }

    info!(count = expired.len(), "batch: processing expired batches");

    // 按群组聚合: 同一群的所有消息一起进入表达决策
    let mut group_msgs: HashMap<u64, Vec<GroupBatch>> = HashMap::new();
    let mut private_batches: Vec<(u64, String)> = Vec::new();

    for batch in expired {
        if batch.group_id > 0 {
            group_msgs.entry(batch.group_id).or_default().push(batch);
        } else {
            private_batches.push((batch.user_id, batch.taken.messages));
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
