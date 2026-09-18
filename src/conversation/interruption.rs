//! 概率式中断：她话说到一半，群里又炸出新消息
//!
//! 她正在生成回复时，新消息照常入批等待下一轮。但等这轮生成完、
//! 还没开口的那一刻，如果处理期间又涌进大量新消息，话题可能已经
//! 变了——按消息量与已用时长计算概率，把到嘴边的话咽回去，
//! 让她带着完整的最新消息重新看一眼场面。
//!
//! 概率公式（借鉴 AIRI interruption）：`p = 消息量比 × (1 − 处理时长比)`。
//! 刚开始处理（<1s）必不打断——短回复让它说完；处理太久（>30s）必打断——
//! 说出来大概率驴唇不对马嘴。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tracing::debug;

/// 基准消息量：处理期间新到 5 条即视为"话题可能已经变了"
const BASELINE_NEW_MESSAGES: f32 = 5.0;
/// 基准处理时长：超过 10 秒后消息量带来的打断概率逐渐失效
const BASELINE_PROCESSING_MS: f32 = 10_000.0;
/// 短于这个时长的处理必不打断（短回复让它说完）
const MIN_PROCESSING_MS: u128 = 1_000;
/// 长于这个时长的处理必打断
const MAX_PROCESSING_MS: u128 = 30_000;

/// 纯函数决策：处理期间新到 `new_messages` 条、已处理 `processing_ms` 毫秒、
/// 随机数 `rand`（0~1）——要不要打断
///
/// 处理期没有新消息时永不打断：没有新信息，"话题可能已经变了"这个前提
/// 就不成立。旧实现只看时长，于是模型慢（或超时重试）超过 30 秒就会把
/// 一句本来完全正确的话咽回去——慢的是她自己的网络，不是群里的对话。
pub(crate) fn should_interrupt(new_messages: u32, processing_ms: u128, rand: f32) -> bool {
    if new_messages == 0 {
        return false;
    }
    if processing_ms < MIN_PROCESSING_MS {
        return false;
    }
    if processing_ms > MAX_PROCESSING_MS {
        return true;
    }
    let message_ratio = (new_messages as f32 / BASELINE_NEW_MESSAGES).min(1.0);
    let time_ratio = ((processing_ms as f32) / BASELINE_PROCESSING_MS).min(1.0);
    let probability = message_ratio * (1.0 - time_ratio);
    rand < probability
}

// ── 处理期状态（主线程记账，队列线程读） ────────────────────────

struct PendingState {
    since: Instant,
    new_messages: u32,
}

static PENDING: OnceLock<Mutex<HashMap<u64, PendingState>>> = OnceLock::new();

fn pending_map() -> &'static Mutex<HashMap<u64, PendingState>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 她开始处理这个群的一批消息：清空处理期新消息计数
pub(crate) fn begin(group_id: u64) {
    if let Ok(mut map) = pending_map().lock() {
        map.insert(
            group_id,
            PendingState {
                since: Instant::now(),
                new_messages: 0,
            },
        );
    }
}

/// 她处理期间这个群又有一条消息到达（主线程调用）
pub(crate) fn note_arrival(group_id: u64) {
    if let Ok(mut map) = pending_map().lock()
        && let Some(state) = map.get_mut(&group_id)
    {
        state.new_messages += 1;
    }
}

/// 她收尾（无论发没发）：清掉处理期状态
pub(crate) fn end(group_id: u64) {
    if let Ok(mut map) = pending_map().lock() {
        map.remove(&group_id);
    }
}

/// 发送前的最终决策：要不要把到嘴边的话咽回去
///
/// 未开启打断或处理期没有记账时一律返回 false。
pub(crate) fn should_swallow(group_id: u64) -> bool {
    let Some((new_messages, elapsed_ms)) = (|| {
        let map = pending_map().lock().ok()?;
        let state = map.get(&group_id)?;
        Some((state.new_messages, state.since.elapsed().as_millis()))
    })() else {
        return false;
    };
    let swallow = should_interrupt(new_messages, elapsed_ms, fastrand::f32());
    if swallow {
        debug!(
            group_id,
            new_messages, elapsed_ms, "interruption: 话到嘴边又咽了回去"
        );
    }
    swallow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_processes_never_interrupt() {
        assert!(!should_interrupt(50, 999, 0.0));
        assert!(!should_interrupt(50, 0, 0.0));
    }

    #[test]
    fn very_long_processes_always_interrupt() {
        assert!(should_interrupt(1, 30_001, 0.99));
        assert!(should_interrupt(3, 60_000, 0.99));
    }

    #[test]
    fn silence_during_generation_never_interrupts() {
        // 没有新消息 = 没有新信息。慢是她自己的网络慢，不是话题变了。
        assert!(!should_interrupt(0, 30_001, 0.0));
        assert!(!should_interrupt(0, 60_000, 0.0));
        assert!(!should_interrupt(0, 10 * 60_000, 0.0));
    }

    #[test]
    fn probability_follows_message_volume() {
        // 处理了 5 秒（time_ratio=0.5），新到 5 条（message_ratio=1.0）→ p=0.5
        assert!(!should_interrupt(5, 5_000, 0.6));
        assert!(should_interrupt(5, 5_000, 0.4));
        // 只来了 1 条（message_ratio=0.2）→ p=0.1
        assert!(!should_interrupt(1, 5_000, 0.2));
        assert!(should_interrupt(1, 5_000, 0.05));
        // 没有新消息 → 永不打断
        assert!(!should_interrupt(0, 5_000, 0.0));
    }
}
