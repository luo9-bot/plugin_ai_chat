//! 主动消息动机系统
//!
//! 将原来的基于间隔的主动消息改为内在动机驱动。
//! 不是随机选类型，而是根据内部状态自然产生。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

/// 主动消息动机状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProactiveMotivation {
    /// 分享欲——看到有趣东西想分享
    pub urge_to_share: f32,
    /// 关心欲——关心对方状态
    pub caring_check_in: f32,
    /// 延续欲——延续中断话题的未完成感
    pub open_thread_pull: f32,
    /// 表达欲——积压的表达欲
    pub accumulated_expression: f32,
    /// 社交需求——孤独感驱动
    pub social_need: f32,
    /// 好奇心——好奇心驱动
    pub curiosity_drive: f32,
    /// 上次更新时间
    pub last_update: u64,
    /// 积压的未表达想法
    pub pending_expressions: Vec<String>,
    /// 中断的对话话题
    pub open_threads: Vec<OpenThread>,
}

/// 未完成的对话话题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenThread {
    /// 话题摘要
    pub topic: String,
    /// 对话方 user_id
    pub user_id: u64,
    /// 群ID（0表示私聊）
    pub group_id: u64,
    /// 中断时间
    pub interrupted_at: u64,
    /// 未说完的内容
    pub unfinished_content: Option<String>,
}

fn motivation_path() -> std::path::PathBuf {
    config::data_dir().join("proactive_motivation.json")
}

fn load_motivation() -> ProactiveMotivation {
    let path = motivation_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ProactiveMotivation::default(),
    }
}

fn save_motivation(m: &ProactiveMotivation) {
    let path = motivation_path();
    if let Ok(json) = serde_json::to_string_pretty(m) {
        fs::write(path, json).ok();
    }
}

/// 更新动机状态（每周期调用）
pub fn update_motivations() {
    let mut m = load_motivation();
    let now = crate::util::now_secs();
    let elapsed_hours = now.saturating_sub(m.last_update) as f32 / 3600.0;

    if elapsed_hours < 0.1 {
        return; // 至少6分钟
    }
    m.last_update = now;

    // 1. 表达欲——随时间累积，表达后归零
    m.accumulated_expression = (m.accumulated_expression + 0.05 * elapsed_hours).min(1.0);

    // 2. 社交需求——长时间无互动时上升
    let battery = crate::social_battery::load();
    if battery.level > 30.0 {
        m.social_need = (m.social_need + 0.03 * elapsed_hours).min(1.0);
    } else {
        m.social_need = (m.social_need - 0.05 * elapsed_hours).max(0.0);
    }

    // 3. 好奇心——基于最近的对话活跃度
    let active_count = crate::read_shared_state(|s| s.active_users.len());
    if active_count > 0 {
        m.curiosity_drive = (m.curiosity_drive + 0.02 * elapsed_hours).min(1.0);
    } else {
        m.curiosity_drive = (m.curiosity_drive - 0.03 * elapsed_hours).max(0.0);
    }

    // 4. 分享欲——基于自我反思产生的想法
    let recent_thoughts = crate::self_memory::inner_thought::get_active_thoughts(5);
    m.urge_to_share = (recent_thoughts.len() as f32 * 0.15).min(0.8);

    // 5. 关心欲——基于关系亲密度
    let mut max_intimacy = 0.0f32;
    for uid in crate::read_shared_state(|s| s.active_users.clone()) {
        let rel = crate::person_info::relationship::get_relationship(uid);
        if rel.intimacy > max_intimacy {
            max_intimacy = rel.intimacy;
        }
    }
    m.caring_check_in = max_intimacy * 0.6;

    // 6. 未完成话题的吸引力
    let now_u64 = now;
    m.open_threads.retain(|t| {
        let elapsed_days = now_u64.saturating_sub(t.interrupted_at) as f32 / 86400.0;
        elapsed_days < 7.0 // 最多保留7天
    });
    m.open_thread_pull = (m.open_threads.len() as f32 * 0.2).min(0.7);

    save_motivation(&m);
}

/// 获取当前最强的动机类型
pub fn get_dominant_motivation() -> Option<(String, f32)> {
    let m = load_motivation();
    let motivations: Vec<(&str, f32)> = vec![
        ("分享欲", m.urge_to_share),
        ("关心欲", m.caring_check_in),
        ("延续话题", m.open_thread_pull),
        ("表达欲", m.accumulated_expression),
        ("社交需求", m.social_need),
        ("好奇心", m.curiosity_drive),
    ];

    motivations.into_iter()
        .filter(|(_, v)| *v > 0.3)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(name, strength)| (name.to_string(), strength))
}

/// 动机类型对应的消息风格
pub fn motivation_to_message_hint(motivation_type: &str) -> &'static str {
    match motivation_type {
        "分享欲" => "你刚想到一个有意思的事想分享给对方",
        "关心欲" => "你想关心一下对方最近的状态",
        "延续话题" => "你想起之前有个没说完的话题想继续",
        "表达欲" => "你有一些积压的想法想说出来",
        "社交需求" => "你感到有些寂寞，想和人聊聊天",
        "好奇心" => "你对对方的生活有些好奇",
        _ => "",
    }
}

/// 记录未完成的话题
pub fn record_open_thread(user_id: u64, group_id: u64, topic: &str, unfinished: Option<&str>) {
    let mut m = load_motivation();
    let now = crate::util::now_secs();

    // 去重
    m.open_threads.retain(|t| t.topic != topic || t.user_id != user_id);

    m.open_threads.push(OpenThread {
        topic: topic.to_string(),
        user_id,
        group_id,
        interrupted_at: now,
        unfinished_content: unfinished.map(|s| s.to_string()),
    });

    // 保留最近10条
    if m.open_threads.len() > 10 {
        m.open_threads.remove(0);
    }

    save_motivation(&m);
    debug!(user_id, topic, "motivation: open thread recorded");
}

/// 消耗表达欲（发送消息后）
pub fn consume_expression() {
    let mut m = load_motivation();
    m.accumulated_expression = (m.accumulated_expression - 0.3).max(0.0);
    save_motivation(&m);
}

/// 获取动机的自然语言上下文
pub fn get_motivation_context() -> String {
    if let Some((motivation_type, strength)) = get_dominant_motivation() {
        let hint = motivation_to_message_hint(&motivation_type);
        format!("# 当前动机\n{}（强度:{:.1}）", hint, strength)
    } else {
        String::new()
    }
}
