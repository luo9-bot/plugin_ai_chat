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
    /// 上午活动描述
    #[serde(default = "default_morning")]
    pub morning: String,
    /// 中午活动描述
    #[serde(default = "default_noon")]
    pub noon: String,
    /// 下午活动描述
    #[serde(default = "default_afternoon")]
    pub afternoon: String,
    /// 傍晚活动描述
    #[serde(default = "default_evening")]
    pub evening: String,
    /// 晚上活动描述
    #[serde(default = "default_night")]
    pub night: String,
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
    /// 是否已触发
    #[serde(skip)]
    pub triggered: bool,
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
            morning: default_morning(),
            noon: default_noon(),
            afternoon: default_afternoon(),
            evening: default_evening(),
            night: default_night(),
        }
    }
}

fn default_true() -> bool { true }
fn default_wake_up() -> u32 { 7 }
fn default_sleep() -> u32 { 23 }
fn default_morning() -> String { "刚起床，开店准备中".into() }
fn default_noon() -> String { "午休时间".into() }
fn default_afternoon() -> String { "正常营业中".into() }
fn default_evening() -> String { "收拾店里，准备关门".into() }
fn default_night() -> String { "休息时间".into() }
    
/// 获取当前时间段描述
pub fn get_current_context() -> String {
    let config = load_config();
    if !config.enabled {
        return String::new();
    }

    let hour = current_hour();
    let daily = &config.daily;

    // 检查是否在睡觉时间
    if hour >= daily.sleep || hour < daily.wake_up {
        return format!(
            "# 当前状态\n现在是{}点，{}。\n{}",
            hour,
            if hour >= daily.sleep { "深夜" } else { "凌晨" },
            "你在休息，不应该主动发消息，除非有紧急事情"
        );
    }

    // 根据时间段返回状态
    let (time_desc, activity) = if hour < 12 {
        ("上午", &daily.morning)
    } else if hour < 14 {
        ("中午", &daily.noon)
    } else if hour < 18 {
        ("下午", &daily.afternoon)
    } else if hour < 21 {
        ("傍晚", &daily.evening)
    } else {
        ("晚上", &daily.night)
    };

    // 检查今天的特殊事件
    let today = today_date();
    let mut event_str = String::new();
    for event in &config.events {
        if event.date == today {
            event_str = format!("\n今天有安排：{}", event.description);
            break;
        }
    }

    format!(
        "# 当前状态\n现在是{}点（{}），你正在：{}{}",
        hour, time_desc, activity, event_str
    )
}

/// 检查当前是否应该保持安静
pub fn is_quiet_time() -> bool {
    let config = load_config();
    if !config.enabled {
        return false;
    }
    let hour = current_hour();
    hour >= config.daily.sleep || hour < config.daily.wake_up
}

fn current_hour() -> u32 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 简单计算小时（UTC+8）
    ((now / 3600 + 8) % 24) as u32
}

fn today_date() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    // 简化的日期计算（从1970-01-01开始）
    let (_year, month, day) = days_to_date(days);
    format!("{:02}-{:02}", month, day)
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let leap = is_leap_year(y);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    for (i, &days_in_month) in month_days.iter().enumerate() {
        if remaining < days_in_month {
            return (y, i as u64 + 1, remaining + 1);
        }
        remaining -= days_in_month;
    }

    (y, 12, 31)
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn load_config() -> ScheduleConfig {
    let path = crate::config::data_dir().join("schedule.json");
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => {
            // 首次运行生成默认配置
            let config = ScheduleConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                fs::write(&path, json).ok();
            }
            config
        }
    }
}
