//! ASI 评分计算

use super::store::{ReplyEffectRecord, FollowupMessage};

pub fn calculate_asi(record: &ReplyEffectRecord) -> f64 {
    let behavior = calculate_behavior_score(record);
    let relational = calculate_relational_score(record);
    let friction = calculate_friction_score(record);
    ((0.45 * behavior + 0.35 * relational + 0.20 * (1.0 - friction)) * 100.0).round()
}

fn calculate_behavior_score(record: &ReplyEffectRecord) -> f64 {
    let target: Vec<&FollowupMessage> = record.followups.iter().filter(|f| f.user_id == record.target_user).collect();
    let continued = if target.is_empty() { 0.0 } else { 1.0 };
    let sentiment = target.iter().map(|f| {
        if f.content.contains('哈') || f.content.contains('笑') || f.content.contains("好的") { 1.0 }
        else if f.content.contains('唉') || f.content.contains("无语") { 0.0 }
        else { 0.5 }
    }).sum::<f64>() / target.len().max(1) as f64;
    let avg_len: f64 = target.iter().map(|f| f.content.len() as f64).sum::<f64>() / target.len().max(1) as f64;
    let expansion = (avg_len / 50.0).min(1.0);
    let no_correction = if target.iter().any(|f| f.content.contains("我是说") || f.content.contains("你理解错")) { 0.0 } else { 1.0 };
    let no_abort = if target.iter().any(|f| f.content.contains("算了")) { 0.0 } else { 1.0 };
    0.30 * continued + 0.25 * sentiment + 0.20 * expansion + 0.15 * no_correction + 0.10 * no_abort
}

fn calculate_relational_score(_record: &ReplyEffectRecord) -> f64 { 0.5 }

fn calculate_friction_score(record: &ReplyEffectRecord) -> f64 {
    let neg = ["你没懂", "算了", "无语", "不是这个意思"];
    let rep = ["我是说", "你理解错", "我问的是"];
    let n = if record.followups.iter().any(|f| neg.iter().any(|p| f.content.contains(p))) { 1.0 } else { 0.0 };
    let r = if record.followups.iter().any(|f| rep.iter().any(|p| f.content.contains(p))) { 1.0 } else { 0.0 };
    0.40 * n + 0.30 * r
}

pub fn should_finalize(record: &ReplyEffectRecord) -> bool {
    let now = crate::util::now_secs();
    if now.saturating_sub(record.sent_at) >= super::store::OBSERVATION_WINDOW { return true; }
    if record.followups.iter().filter(|f| f.user_id == record.target_user).count() >= 2 { return true; }
    if record.followups.len() >= 5 { return true; }
    let neg = ["你没懂", "算了", "无语", "不是这个意思", "听不懂"];
    if record.followups.iter().any(|f| f.user_id == record.target_user && neg.iter().any(|p| f.content.contains(p))) { return true; }
    let rep = ["我是说", "你理解错", "我问的是", "不是这个"];
    if record.followups.iter().any(|f| f.user_id == record.target_user && rep.iter().any(|p| f.content.contains(p))) { return true; }
    false
}
