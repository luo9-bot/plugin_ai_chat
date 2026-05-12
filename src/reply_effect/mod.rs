//! 回复效果追踪系统

mod store;
mod scoring;

pub use store::*;
use scoring::{calculate_asi, should_finalize};

use tracing::{debug, info};

pub fn record_reply(group_id: u64, target_user: u64, reply_text: &str) {
    let mut s = load_store();
    let now = crate::util::now_secs();
    s.records.retain(|r| r.status == EffectStatus::Pending && now.saturating_sub(r.sent_at) < OBSERVATION_WINDOW * 2);
    if s.records.len() >= MAX_ACTIVE_RECORDS { s.records.remove(0); }
    s.records.push(ReplyEffectRecord {
        reply_text: reply_text.to_string(), target_user, group_id, sent_at: now,
        followups: Vec::new(), asi_score: None, status: EffectStatus::Pending,
    });
    save_store(&s);
    debug!(group_id, target_user, "reply_effect: recorded");
}

pub fn observe_message(group_id: u64, user_id: u64, message: &str) {
    let mut s = load_store();
    let now = crate::util::now_secs();
    let mut changed = false;
    for rec in s.records.iter_mut() {
        if rec.group_id == group_id && rec.target_user == user_id && rec.status == EffectStatus::Pending
            && now.saturating_sub(rec.sent_at) < OBSERVATION_WINDOW && rec.followups.len() < MAX_FOLLOWUPS
        {
            rec.followups.push(FollowupMessage { user_id, content: message.to_string(), timestamp: now });
            changed = true;
            if should_finalize(rec) {
                rec.status = EffectStatus::Finalized;
                let score = calculate_asi(rec);
                rec.asi_score = Some(score);
                info!(group_id, target_user = user_id, asi_score = score, followups = rec.followups.len(), "reply_effect: finalized");
            }
        }
    }
    if changed { save_store(&s); }
}

/// ASI 趋势
#[derive(Debug, Clone)]
pub struct AsiTrend {
    pub avg_score: f64,
    pub sample_count: usize,
    pub trend: TrendDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrendDirection {
    Rising,
    Stable,
    Falling,
}

/// 获取最近 N 条回复的 ASI 趋势
pub fn get_recent_asi_trend(limit: usize) -> Option<AsiTrend> {
    let s = load_store();
    let scores: Vec<f64> = s.records.iter()
        .filter(|r| r.status == EffectStatus::Finalized && r.asi_score.is_some())
        .rev()
        .take(limit)
        .filter_map(|r| r.asi_score)
        .collect();

    if scores.len() < 2 {
        return None;
    }

    let avg = scores.iter().sum::<f64>() / scores.len() as f64;

    // 趋势判断：比较前半和后半的平均分
    let mid = scores.len() / 2;
    let first_half_avg: f64 = scores[..mid].iter().sum::<f64>() / mid as f64;
    let second_half_avg: f64 = scores[mid..].iter().sum::<f64>() / (scores.len() - mid) as f64;

    let trend = if second_half_avg > first_half_avg + 5.0 {
        TrendDirection::Rising
    } else if second_half_avg < first_half_avg - 5.0 {
        TrendDirection::Falling
    } else {
        TrendDirection::Stable
    };

    Some(AsiTrend {
        avg_score: avg,
        sample_count: scores.len(),
        trend,
    })
}

/// 获取 ASI 反馈提示（用于注入到回复上下文）
///
/// 当 ASI 评分持续偏低时，返回调整建议
pub fn get_asi_feedback_hint() -> Option<String> {
    let trend = get_recent_asi_trend(10)?;

    if trend.avg_score < 40.0 {
        Some(format!(
            "最近回复效果不佳（平均 ASI: {:.0}，趋势: {:?}）。请调整：更简短自然、更贴近用户语气、避免过度热情。",
            trend.avg_score, trend.trend
        ))
    } else if trend.trend == TrendDirection::Falling && trend.avg_score < 60.0 {
        Some(format!(
            "回复效果有下降趋势（平均 ASI: {:.0}）。请注意回复的自然度。",
            trend.avg_score
        ))
    } else {
        None
    }
}
