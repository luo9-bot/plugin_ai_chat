use super::store::{current_max_replies, current_segment_start};
use crate::config;
use crate::util::now_secs;
use tracing::debug;

// ── 核心 API ────────────────────────────────────────────────

/// 检查当前配额段是否还有余量（不扣减）
pub fn has_quota(group_id: u64) -> bool {
    if !config::get().quota.enabled {
        return true;
    }
    let Some(max) = resolve_max_replies() else {
        return false;
    };
    let segment_start = current_segment_start();
    let used = crate::db::db()
        .quota_segment_count(group_id, segment_start)
        .unwrap_or(0);
    used < max
}

/// 当前小时的配额上限；无时段覆盖时按 0 处理并**留痕**
///
/// 留痕是必须的：否则"配置漏了这个小时"与"配额用尽"在日志里长得一样，
/// 而前者是配置错误、需要人来修。
fn resolve_max_replies() -> Option<u32> {
    match current_max_replies() {
        Some(max) => Some(max),
        None => {
            debug!(
                hour = crate::util::hour_cst_at(now_secs()),
                "quota: 当前小时没有任何时段覆盖（配置有空档），按 0 余量处理"
            );
            None
        }
    }
}

/// 检查配额并扣减。返回 true 表示允许回复。
///
/// 检查与扣减在状态库里是一个事务：否则两个线程同时看到"还剩一个名额"
/// 会都放行（原先靠进程内锁保证，迁到库之后必须靠事务）。
pub fn check_and_consume(group_id: u64) -> bool {
    if !config::get().quota.enabled {
        return true;
    }
    let Some(max) = resolve_max_replies() else {
        return false;
    };
    let segment_start = current_segment_start();
    match crate::db::db().quota_consume(group_id, segment_start, max) {
        Ok(allowed) => allowed,
        Err(error) => {
            tracing::warn!(%error, group_id, "quota: 扣减失败，按放行处理");
            true
        }
    }
}

// ── 段日志记录 ────────────────────────────────────────────────

pub fn log_segment_message(group_id: u64, user_id: u64, message: &str) {
    let segment_start = current_segment_start();
    if let Err(error) = crate::db::db().quota_log_message(
        group_id,
        segment_start,
        user_id,
        message,
        now_secs() as i64,
    ) {
        tracing::warn!(%error, group_id, "quota: 段日志写入失败");
    }
}
