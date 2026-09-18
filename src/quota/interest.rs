use tracing::debug;

use super::segment::{check_and_consume, has_quota};
use super::store::{SegmentLogEntry, SegmentMessage};
use crate::config;

// ── 回复优先级 ───────────────────────────────────────────────

/// 计算消息优先级分 (0.0~1.0)
///
/// 设计：
/// - darling + bot 刚回复(2min内) → 0.45，无需 @即可延续对话
/// - 2 分钟后窗口关闭 → 0.25，需要 @bot 才能突破
/// - 普通用户 @bot + bot 活跃 → 0.35，仍无法突破（阈值 0.45）
pub fn calculate_priority(
    user_id: u64,
    group_id: u64,
    message: &str,
    at_pattern: &str,
    darling_qq: u64,
) -> f32 {
    let is_darling = darling_qq > 0 && user_id == darling_qq;
    let is_at_bot = !at_pattern.is_empty() && message.contains(at_pattern);

    // bot 最近在群里发过消息（2 分钟内）= 对话窗口
    let bot_recently_active =
        crate::read_shared_state(|s| !s.get_recent_bot_messages(group_id, 120, 1).is_empty());

    let mut score = 0.0f32;

    if bot_recently_active {
        if is_darling {
            // darling 在活跃对话中：无需 @ 也能延续
            score += 0.45;
            if is_at_bot {
                score += 0.10;
            }
        } else if is_at_bot {
            // 普通用户 @bot：有基础分但不足以突破
            score += 0.20;
        }
    } else {
        // 对话窗口已关闭：只有 @bot 才有分
        if is_at_bot {
            score += 0.40;
            if is_darling {
                score += 0.05;
            }
        }
    }

    score.min(1.00)
}

/// 综合判断：优先级 + 配额，成功时自动消费配额。返回 true 表示允许回复。
///
/// - 配额充足 → 消费配额，返回 true
/// - 配额耗尽 + 优先级 >= 0.45 → 突破配额（不消费），返回 true
/// - 配额耗尽 + 优先级 < 0.45 → 返回 false
pub fn try_reply(
    group_id: u64,
    user_id: u64,
    message: &str,
    at_pattern: &str,
    darling_qq: u64,
) -> bool {
    let cfg = &config::get().quota;
    if !cfg.enabled {
        return true;
    }

    // 先检查配额
    if has_quota(group_id) {
        return check_and_consume(group_id);
    }

    // 配额耗尽，检查优先级
    let priority = calculate_priority(user_id, group_id, message, at_pattern, darling_qq);
    let threshold = 0.45f32;
    if priority >= threshold {
        debug!(user_id, group_id, priority, "quota: 配额耗尽但优先级突破");
        return true;
    }

    debug!(user_id, group_id, priority, "quota: 配额耗尽，跳过回复");
    false
}

// ── Admin API ──────────────────────────────────────────────────

/// 某个群最近若干段的段日志（段起始时间倒序）
pub fn get_segment_logs(group_id: u64, limit: usize) -> Vec<SegmentLogEntry> {
    let messages = crate::db::db()
        .quota_messages(group_id, limit)
        .unwrap_or_default();

    // 记录已按「段倒序、段内按写入顺序」返回，这里保持该顺序分组
    let mut entries: Vec<SegmentLogEntry> = Vec::new();
    for row in messages {
        match entries.last_mut() {
            Some(entry) if entry.segment_start == row.segment_start => {
                entry.messages.push(SegmentMessage {
                    user_id: row.user_id,
                    message: row.message,
                    timestamp: row.ts.max(0) as u64,
                });
            }
            _ => entries.push(SegmentLogEntry {
                segment_start: row.segment_start,
                messages: vec![SegmentMessage {
                    user_id: row.user_id,
                    message: row.message,
                    timestamp: row.ts.max(0) as u64,
                }],
            }),
        }
    }
    entries
}

/// 有段日志的群
pub fn get_groups_with_logs() -> Vec<u64> {
    crate::db::db()
        .quota_groups_with_messages()
        .unwrap_or_default()
}
