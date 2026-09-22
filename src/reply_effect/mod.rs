//! 回复效果追踪系统

mod scoring;
mod store;

use scoring::{calculate_asi, should_finalize};
pub(crate) use store::*;

use tracing::{debug, info};

pub(crate) fn record_reply(
    group_id: u64,
    target_user: u64,
    reply_text: &str,
    archive_reply_id: Option<i64>,
) {
    let mut s = load_store();
    let now = crate::util::now_secs();
    s.records.retain(|r| {
        r.status == EffectStatus::Pending && now.saturating_sub(r.sent_at) < OBSERVATION_WINDOW * 2
    });
    if s.records.len() >= MAX_ACTIVE_RECORDS {
        s.records.remove(0);
    }
    s.records.push(ReplyEffectRecord {
        reply_text: reply_text.to_string(),
        target_user,
        group_id,
        sent_at: now,
        followups: Vec::new(),
        asi_score: None,
        status: EffectStatus::Pending,
        archive_reply_id,
    });
    save_store(&s);
    debug!(group_id, target_user, "reply_effect: recorded");
}

pub(crate) fn observe_message(group_id: u64, user_id: u64, message: &str) {
    let mut s = load_store();
    let now = crate::util::now_secs();
    let mut changed = false;
    for rec in s.records.iter_mut() {
        if rec.group_id == group_id
            && rec.target_user == user_id
            && rec.status == EffectStatus::Pending
            && now.saturating_sub(rec.sent_at) < OBSERVATION_WINDOW
            && rec.followups.len() < MAX_FOLLOWUPS
        {
            rec.followups.push(FollowupMessage {
                user_id,
                content: message.to_string(),
                timestamp: now,
            });
            changed = true;
            if should_finalize(rec) {
                rec.status = EffectStatus::Finalized;
                let rule_score = calculate_asi(rec);

                // 当规则评分偏低时，使用 LLM Judge 获得更精确的评估
                let final_score = if rule_score < 50.0 {
                    if let Some(llm_scores) = scoring::judge_with_llm(rec) {
                        let relational = scoring::calculate_relational_from_llm(&llm_scores);
                        let friction =
                            scoring::calculate_friction_from_llm(rec, llm_scores.uncanny_risk);
                        let behavior = scoring::calculate_behavior_score(rec);
                        let llm_asi =
                            ((0.45 * behavior + 0.35 * relational + 0.20 * (1.0 - friction))
                                * 100.0)
                                .round();
                        debug!(
                            group_id,
                            rule_score, llm_asi, "reply_effect: LLM judge adjusted"
                        );
                        llm_asi
                    } else {
                        rule_score
                    }
                } else {
                    rule_score
                };

                rec.asi_score = Some(final_score);
                // Reward 写回训练留档：ASI 0~100 → reward 0~1，
                // "获得互动的回复"在强化重训中概率上升（L6 闭环）
                if let Some(id) = rec.archive_reply_id {
                    let reward = (final_score / 100.0) as f32;
                    crate::mind::archive::set_reward(group_id, id, reward);
                }
                info!(
                    group_id,
                    target_user = user_id,
                    asi_score = final_score,
                    followups = rec.followups.len(),
                    "reply_effect: finalized"
                );
            }
        }
    }
    if changed {
        save_store(&s);
    }
}
