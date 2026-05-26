//! 社交电量系统
//!
//! 模拟人类的社交能量波动。不是硬性的"能不能回复"开关，
//! 而是柔性影响回复质量、风格和主动性的连续模型。
//!
//! 电量变化用定时器驱动（每分钟更新），不是事件驱动。
//! 被动模式消耗 rate×0.1，主动发言消耗 rate×1.5。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

/// 社交电量状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialBattery {
    /// 当前电量 (0.0-100.0)
    pub level: f32,
    /// 最大电量（人格决定，可配置）
    pub capacity: f32,
    /// 每条回复消耗
    pub drain_rate: f32,
    /// 每分钟休息恢复
    pub recharge_rate: f32,
    /// 被动聆听消耗更少
    pub is_passive_mode: bool,
    /// 是否处于倦怠恢复期
    pub is_burned_out: bool,
    /// 倦怠后恢复更慢
    pub burnout_recovery_mult: f32,
    /// 情绪消耗修正（焦虑时消耗更快）
    pub emotion_drain_modifier: f32,
    /// 上次更新时间
    pub last_update: u64,
    /// 连续活跃分钟数
    pub active_minutes: u32,
    /// 最后一次主动回复时间
    pub last_active_reply: u64,
    /// 倦怠触发次数（用于延长恢复）
    pub burnout_count: u32,
}

impl Default for SocialBattery {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity;
        Self {
            level: h.battery_capacity,
            capacity: h.battery_capacity,
            drain_rate: h.battery_drain_rate,
            recharge_rate: h.battery_recharge_rate,
            is_passive_mode: true,
            is_burned_out: false,
            burnout_recovery_mult: h.burnout_recovery_mult,
            emotion_drain_modifier: 1.0,
            last_update: crate::util::now_secs(),
            active_minutes: 0,
            last_active_reply: 0,
            burnout_count: 0,
        }
    }
}

fn battery_path() -> std::path::PathBuf {
    config::data_dir().join("social_battery.json")
}

/// 加载电量状态
pub fn load() -> SocialBattery {
    let path = battery_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => SocialBattery::default(),
    }
}

/// 保存电量状态
pub fn save(battery: &SocialBattery) {
    let path = battery_path();
    if let Ok(json) = serde_json::to_string_pretty(battery) {
        fs::write(path, json).ok();
    }
}

/// 每分钟更新一次电量（由定时器驱动）
///
/// - 不活跃时自然恢复
/// - 被动模式消耗极少
/// - 倦怠恢复期消耗更慢
/// - 情绪影响消耗速率
pub fn update(battery: &mut SocialBattery) {
    let now = crate::util::now_secs();
    let elapsed_minutes = now.saturating_sub(battery.last_update) as f32 / 60.0;

    if elapsed_minutes < 0.5 {
        return; // 不到30秒不更新
    }

    battery.last_update = now;

    // 计算恢复量
    let effective_recharge = if battery.is_burned_out {
        battery.recharge_rate * battery.burnout_recovery_mult
    } else {
        battery.recharge_rate
    };

    // 被动模式几乎不消耗
    if battery.is_passive_mode {
        battery.level = (battery.level + effective_recharge * elapsed_minutes)
            .min(battery.capacity);
        battery.active_minutes = 0;
    } else {
        // 活跃状态：恢复 - 自然消耗
        let drain = battery.drain_rate * battery.emotion_drain_modifier * 0.1; // 每分钟消耗
        battery.level = (battery.level + (effective_recharge - drain) * elapsed_minutes)
            .clamp(0.0, battery.capacity);
    }

    // 倦怠恢复检查
    if battery.is_burned_out {
        let cfg = config::get();
        if battery.level > cfg.humanity.burnout_threshold * 3.0 {
            battery.is_burned_out = false;
            battery.burnout_count = 0;
            debug!("social_battery: recovered from burnout");
        }
    }

    // 电量过低触发倦怠
    if battery.level <= config::get().humanity.burnout_threshold && !battery.is_burned_out {
        battery.is_burned_out = true;
        battery.burnout_count += 1;
        debug!(
            level = battery.level,
            count = battery.burnout_count,
            "social_battery: burned out"
        );
    }

    debug!(
        level = battery.level,
        burned_out = battery.is_burned_out,
        passive = battery.is_passive_mode,
        "social_battery: updated"
    );
}

/// 记录一次主动回复（消耗电量）
pub fn record_active_reply(battery: &mut SocialBattery) {
    let cfg = config::get();
    let drain_mult = if battery.is_burned_out { 1.5 } else { 1.0 };
    let drain = cfg.humanity.battery_drain_rate * drain_mult * battery.emotion_drain_modifier;

    battery.level = (battery.level - drain).max(0.0);
    battery.is_passive_mode = false;
    battery.active_minutes += 1;
    battery.last_active_reply = crate::util::now_secs();

    debug!(level = battery.level, drain, "social_battery: active reply recorded");
}

/// 记录被动参与（听消息但不回复，消耗极少）
pub fn record_passive_participation(battery: &mut SocialBattery) {
    battery.is_passive_mode = true;
}

/// 设置情绪消耗修正（焦虑/负面情绪加速消耗）
pub fn set_emotion_modifier(battery: &mut SocialBattery, emotion_type: &crate::emotion::EmotionType) {
    battery.emotion_drain_modifier = match emotion_type {
        crate::emotion::EmotionType::Worried => 1.4,
        crate::emotion::EmotionType::Sad => 1.3,
        crate::emotion::EmotionType::Angry => 1.5,
        crate::emotion::EmotionType::Tired => 1.6,
        crate::emotion::EmotionType::Excited => 0.8,
        crate::emotion::EmotionType::Happy => 0.9,
        _ => 1.0,
    };
}

/// 获取电量状态的自然语言描述（用于注入 prompt，非数值）
pub fn get_battery_context(battery: &SocialBattery) -> String {
    let level = battery.level;
    let capacity = battery.capacity;
    let percentage = level / capacity;

    let mut lines = Vec::new();

    if battery.is_burned_out {
        lines.push("- 你现在社交电量严重不足，感到疲惫和倦怠。回复可以非常简短，用表情包代替文字也完全可以。不需要主动找话题。如果实在不想回复，可以说「累了」之类的话".to_string());
    } else if percentage < 0.1 {
        lines.push("- 你现在社交电量很低，感到很累。可以简短回复，偶尔「嗯」「哦」也可以。不想说话的时候可以直接表达".to_string());
    } else if percentage < 0.3 {
        lines.push("- 你现在社交电量偏低，回复可以短一些，不用太热情".to_string());
    } else if percentage < 0.5 {
        lines.push("- 你现在社交电量一般，保持平常的回复状态即可".to_string());
    } else if percentage > 0.8 {
        lines.push("- 你现在精力充沛，可以主动聊天、开玩笑，回复也可以更长更有趣".to_string());
    } else if percentage > 0.6 {
        lines.push("- 你现在社交电量比较充足，可以积极一些".to_string());
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("# 当前社交状态\n{}", lines.join("\n"))
    }
}

/// 获取电量百分比
pub fn level_percentage(battery: &SocialBattery) -> f32 {
    (battery.level / battery.capacity).clamp(0.0, 1.0)
}
