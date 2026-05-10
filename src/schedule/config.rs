use serde::{Deserialize, Serialize};
use std::fs;

/// 日程配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// 是否启用日程系统
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 基础作息时间表
    #[serde(default)]
    pub daily: DailySchedule,
    /// 特殊日期/事件（可选）
    #[serde(default)]
    pub events: Vec<ScheduledEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySchedule {
    /// 起床时间 (小时，24h)
    #[serde(default = "default_wake_up")]
    pub wake_up: u32,
    /// 睡觉时间
    #[serde(default = "default_sleep")]
    pub sleep: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEvent {
    /// 日期 (MM-DD 格式)
    pub date: String,
    /// 时间 (HH:MM 格式，可选)
    #[serde(default)]
    pub time: Option<String>,
    /// 事件描述
    pub description: String,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            daily: DailySchedule::default(),
            events: Vec::new(),
        }
    }
}

impl Default for DailySchedule {
    fn default() -> Self {
        Self {
            wake_up: default_wake_up(),
            sleep: default_sleep(),
        }
    }
}

fn default_true() -> bool { true }
fn default_wake_up() -> u32 { 7 }
fn default_sleep() -> u32 { 23 }

pub(crate) fn load_config() -> ScheduleConfig {
    let path = crate::config::data_dir().join("schedule.json");
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => {
            let config = ScheduleConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                fs::write(&path, json).ok();
            }
            config
        }
    }
}
