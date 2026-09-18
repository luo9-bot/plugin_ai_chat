//! 信息觅食：她的注意力是有限的
//!
//! 群消息照常全量入流（社会世界模型的统计基础不动），
//! 但"坐下来细看"是她的主动行为：`catch_up` 是回神时她自己
//! 决定要不要执行的动作。系统只递事实——哪个群攒了多少条
//! 没细看的消息——看不看、先看哪个，由她自己定。
//!
//! 纪律：`note_message` 纯内存（主循环零 IO）；落盘由
//! 周期检查（[`flush`]）与深读时统一执行。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{info, warn};

use crate::config;
use crate::util;

/// 深读一次最多翻多少条
const CATCH_UP_LIMIT: usize = 30;
/// 攒到多少条才值得在感官里提一嘴（太少的噪音不递给她）
const MENTION_THRESHOLD: u32 = 5;

/// 一个群的未读状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UnreadState {
    /// 上次深读以来经过的消息数
    count: u32,
    /// 上次深读的时间戳（unix 秒，0 = 从没看过这个群）
    last_read_at: u64,
}

static UNREAD: Mutex<Option<HashMap<u64, UnreadState>>> = Mutex::new(None);

fn foraging_path() -> PathBuf {
    config::data_dir().join("mind").join("foraging.json")
}

fn load_state() -> HashMap<u64, UnreadState> {
    let Ok(content) = fs::read_to_string(foraging_path()) else {
        return HashMap::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "foraging: 未读状态解析失败，按空处理");
        HashMap::new()
    })
}

fn with_state<R>(f: impl FnOnce(&mut HashMap<u64, UnreadState>) -> R) -> R {
    let mut guard = UNREAD.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(load_state);
    f(map)
}

/// 把未读状态落盘（周期检查调用；深读后也会调用）
pub fn flush() {
    let snapshot = with_state(|state| state.clone());
    if snapshot.is_empty() {
        return;
    }
    if let Some(parent) = foraging_path().parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(error = %e, "foraging: 创建目录失败");
        return;
    }
    match serde_json::to_string(&snapshot) {
        Ok(json) => {
            if let Err(e) = crate::util::atomic_write(foraging_path(), json) {
                warn!(error = %e, "foraging: 未读状态落盘失败");
            }
        }
        Err(e) => warn!(error = %e, "foraging: 未读状态序列化失败"),
    }
}

/// 主线程记账：这个群又有一条消息经过（纯内存，零 IO）
pub fn note_message(group_id: u64) {
    if !config::get().humanity.foraging_enabled {
        return;
    }
    with_state(|state| {
        state.entry(group_id).or_default().count += 1;
    });
}

/// 感官事实：哪些群攒了多少条没细看的消息（回神输入用）
///
/// 只报过了门槛的群——太少的噪音不递给她。
pub fn unread_lines() -> Vec<String> {
    if !config::get().humanity.foraging_enabled {
        return Vec::new();
    }
    let snapshot: Vec<(u64, u32)> = with_state(|state| {
        state
            .iter()
            .filter(|(_, s)| s.count >= MENTION_THRESHOLD)
            .map(|(&gid, s)| (gid, s.count))
            .collect()
    });
    // 攒得最多的排前面
    let mut lines: Vec<(u64, u32)> = snapshot;
    lines.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    lines
        .into_iter()
        .map(|(gid, count)| format!("群 {gid} 里攒了 {count} 条你还没细看的消息"))
        .collect()
}

/// 深读：她翻一个群的记录（回神动作 `catch_up` 的执行体）
///
/// 返回转述好的对话（机械事实），并把计数清零、刷新深读时间。
pub fn catch_up(group_id: u64) -> String {
    let since = with_state(|state| state.entry(group_id).or_default().last_read_at);

    // 取上次深读之后的消息（多取一些再裁到最新的一段）
    let mut entries: Vec<_> = crate::working_memory::get_since(group_id, since, 200);
    if entries.len() > CATCH_UP_LIMIT {
        let drop = entries.len() - CATCH_UP_LIMIT;
        entries.drain(0..drop);
    }

    // 无论翻到什么都算"看过了"：工作记忆会过期，翻不到就是真的没有了
    let now = util::now_secs();
    with_state(|state| {
        if let Some(state) = state.get_mut(&group_id) {
            state.count = 0;
            state.last_read_at = now;
        }
    });
    flush();

    if entries.is_empty() {
        return "翻了一下，没什么没看过的。".to_string();
    }

    let lines: Vec<String> = entries
        .iter()
        .map(|e| {
            let name = if config::get().self_qq > 0 && e.user_id == config::get().self_qq {
                "你自己".to_string()
            } else {
                crate::person_info::get_display_name(e.user_id, group_id)
                    .unwrap_or_else(|| "群友".to_string())
            };
            crate::mind::sensation::transcribe_message(&name, e.timestamp, &e.content, false)
        })
        .collect();
    info!(group_id, count = lines.len(), "foraging: 深读完成");
    format!("你翻了翻群 {group_id} 的记录：\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_threshold_hides_noise() {
        // 门槛以下不进感官（unreachable 断言不成立即可）
        let small: Vec<(u64, u32)> = vec![(1, MENTION_THRESHOLD - 1)];
        let visible: Vec<&(u64, u32)> = small
            .iter()
            .filter(|(_, c)| *c >= MENTION_THRESHOLD)
            .collect();
        assert!(visible.is_empty());
        let big: Vec<(u64, u32)> = vec![(1, MENTION_THRESHOLD + 1)];
        let visible: Vec<&(u64, u32)> = big
            .iter()
            .filter(|(_, c)| *c >= MENTION_THRESHOLD)
            .collect();
        assert_eq!(visible.len(), 1);
    }
}
