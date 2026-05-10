//! 回复效果追踪系统
//!
//! 发送回复后观察用户反应，评估回复质量。
//! 参考 MaiBot 的 reply_effect 架构。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tracing::{debug, info};

/// 回复效果状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EffectStatus {
    /// 观察中
    Pending,
    /// 已终结
    Finalized,
}

/// 后续消息快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowupMessage {
    pub user_id: u64,
    pub content: String,
    pub timestamp: u64,
}

/// 回复效果记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyEffectRecord {
    /// 回复内容
    pub reply_text: String,
    /// 目标用户
    pub target_user: u64,
    /// 群聊 ID
    pub group_id: u64,
    /// 发送时间
    pub sent_at: u64,
    /// 后续消息
    pub followups: Vec<FollowupMessage>,
    /// ASI 评分 (0-100)
    pub asi_score: Option<f64>,
    /// 状态
    pub status: EffectStatus,
}

/// 效果追踪存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EffectStore {
    /// 活跃记录 (key: group_id_user_id)
    pub records: Vec<ReplyEffectRecord>,
}

static STORE: Mutex<Option<EffectStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("reply_effects.json")
}

fn load_store() -> EffectStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

fn save_store(store: &EffectStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}

/// 观察窗口（秒）
const OBSERVATION_WINDOW: u64 = 600; // 10 分钟
/// 最大后续消息数
const MAX_FOLLOWUPS: usize = 10;
/// 最大活跃记录数
const MAX_ACTIVE_RECORDS: usize = 20;

/// 记录一条回复
pub fn record_reply(group_id: u64, target_user: u64, reply_text: &str) {
    let mut store = load_store();

    // 清理过期记录
    let now = crate::util::now_secs();
    store.records.retain(|r| {
        r.status == EffectStatus::Pending
            && now.saturating_sub(r.sent_at) < OBSERVATION_WINDOW * 2
    });

    // 限制活跃记录数
    if store.records.len() >= MAX_ACTIVE_RECORDS {
        store.records.remove(0);
    }

    store.records.push(ReplyEffectRecord {
        reply_text: reply_text.to_string(),
        target_user,
        group_id,
        sent_at: now,
        followups: Vec::new(),
        asi_score: None,
        status: EffectStatus::Pending,
    });

    save_store(&store);
    debug!(group_id, target_user, "reply_effect: recorded");
}

/// 观察后续消息
pub fn observe_message(group_id: u64, user_id: u64, message: &str) {
    let mut store = load_store();
    let now = crate::util::now_secs();

    let mut changed = false;
    for record in store.records.iter_mut() {
        if record.group_id == group_id
            && record.target_user == user_id
            && record.status == EffectStatus::Pending
            && now.saturating_sub(record.sent_at) < OBSERVATION_WINDOW
            && record.followups.len() < MAX_FOLLOWUPS
        {
            record.followups.push(FollowupMessage {
                user_id,
                content: message.to_string(),
                timestamp: now,
            });
            changed = true;

            // 检查终结条件
            if should_finalize(record) {
                record.status = EffectStatus::Finalized;
                let score = calculate_asi(record);
                record.asi_score = Some(score);
                info!(
                    group_id,
                    target_user = user_id,
                    asi_score = score,
                    followups = record.followups.len(),
                    "reply_effect: finalized"
                );
            }
        }
    }

    if changed {
        save_store(&store);
    }
}

/// 检查是否应该终结观察
fn should_finalize(record: &ReplyEffectRecord) -> bool {
    let now = crate::util::now_secs();

    // 超时终结
    if now.saturating_sub(record.sent_at) >= OBSERVATION_WINDOW {
        return true;
    }

    // 目标用户发送 2+ 条后续消息
    let target_followups = record
        .followups
        .iter()
        .filter(|f| f.user_id == record.target_user)
        .count();
    if target_followups >= 2 {
        return true;
    }

    // 总后续消息达到 5 条
    if record.followups.len() >= 5 {
        return true;
    }

    // 明确负面反馈
    let negative_patterns = ["你没懂", "算了", "无语", "不是这个意思", "听不懂"];
    for followup in &record.followups {
        if followup.user_id == record.target_user {
            for pattern in &negative_patterns {
                if followup.content.contains(pattern) {
                    return true;
                }
            }
        }
    }

    // 修复循环检测
    let repair_patterns = ["我是说", "你理解错", "我问的是", "不是这个"];
    for followup in &record.followups {
        if followup.user_id == record.target_user {
            for pattern in &repair_patterns {
                if followup.content.contains(pattern) {
                    return true;
                }
            }
        }
    }

    false
}

/// 计算 ASI 评分 (0-100)
///
/// ASI = (0.45 * 行为分 + 0.35 * 关系分 + 0.20 * (1 - 摩擦分)) * 100
fn calculate_asi(record: &ReplyEffectRecord) -> f64 {
    let behavior = calculate_behavior_score(record);
    let relational = calculate_relational_score(record);
    let friction = calculate_friction_score(record);

    ((0.45 * behavior + 0.35 * relational + 0.20 * (1.0 - friction)) * 100.0).round()
}

/// 行为分数：对话是否继续、用户情绪、消息长度、无纠正、无放弃
fn calculate_behavior_score(record: &ReplyEffectRecord) -> f64 {
    let target_followups: Vec<&FollowupMessage> = record
        .followups
        .iter()
        .filter(|f| f.user_id == record.target_user)
        .collect();

    // 对话是否继续 (0 or 1)
    let continued = if target_followups.is_empty() { 0.0 } else { 1.0 };

    // 用户情绪 (简单判断：有正面词=1，有负面词=0，中性=0.5)
    let sentiment = target_followups
        .iter()
        .map(|f| {
            if f.content.contains('哈') || f.content.contains('笑') || f.content.contains("好的")
            {
                1.0
            } else if f.content.contains('唉') || f.content.contains("无语") {
                0.0
            } else {
                0.5
            }
        })
        .sum::<f64>()
        / target_followups.len().max(1) as f64;

    // 用户消息长度 (平均长度归一化)
    let avg_len: f64 = target_followups
        .iter()
        .map(|f| f.content.len() as f64)
        .sum::<f64>()
        / target_followups.len().max(1) as f64;
    let expansion = (avg_len / 50.0).min(1.0);

    // 无纠正
    let no_correction = if target_followups.iter().any(|f| {
        f.content.contains("我是说") || f.content.contains("你理解错")
    }) {
        0.0
    } else {
        1.0
    };

    // 无放弃
    let no_abort = if target_followups
        .iter()
        .any(|f| f.content.contains("算了"))
    {
        0.0
    } else {
        1.0
    };

    0.30 * continued + 0.25 * sentiment + 0.20 * expansion + 0.15 * no_correction + 0.10 * no_abort
}

/// 关系分数：社交存在感、温暖度、能力感、恰当性
fn calculate_relational_score(_record: &ReplyEffectRecord) -> f64 {
    // 简化实现：基于回复长度和是否有后续对话
    // 完整实现需要 AI 评判
    0.5 // 默认中性
}

/// 摩擦分数：明确负面、修复循环、诡异风险
fn calculate_friction_score(record: &ReplyEffectRecord) -> f64 {
    let negative_patterns = ["你没懂", "算了", "无语", "不是这个意思"];
    let repair_patterns = ["我是说", "你理解错", "我问的是"];

    let explicit_negative = record
        .followups
        .iter()
        .any(|f| negative_patterns.iter().any(|p| f.content.contains(p)));
    let repair_loop = record
        .followups
        .iter()
        .any(|f| repair_patterns.iter().any(|p| f.content.contains(p)));

    let neg_score = if explicit_negative { 1.0 } else { 0.0 };
    let repair_score = if repair_loop { 1.0 } else { 0.0 };
    let uncanny = 0.0; // 需要 AI 评判

    0.40 * neg_score + 0.30 * repair_score + 0.30 * uncanny
}
