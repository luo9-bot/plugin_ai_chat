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
