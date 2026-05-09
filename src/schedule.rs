use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

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

/// 每日计划 (持久化)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPlan {
    /// 日期 (YYYY-MM-DD)
    pub date: String,
    /// 今日目标/计划
    pub goals: Vec<String>,
    /// 已完成的任务
    pub completed: Vec<String>,
    /// 当前心情/状态描述
    pub mood: String,
    /// 生成时间
    pub created_at: u64,
}

impl Default for DailyPlan {
    fn default() -> Self {
        Self {
            date: String::new(),
            goals: Vec::new(),
            completed: Vec::new(),
            mood: String::new(),
            created_at: 0,
        }
    }
}

/// 获取当前时间段和状态描述
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
            "# 当前状态\n现在是{}点，你在休息。\n{}",
            hour,
            "深夜/凌晨时段，不应该主动发消息，除非有紧急事情"
        );
    }

    // 获取时间段描述
    let time_desc = if hour < 9 {
        "早上，刚醒来"
    } else if hour < 12 {
        "上午"
    } else if hour < 14 {
        "中午，午休时间"
    } else if hour < 18 {
        "下午"
    } else if hour < 21 {
        "傍晚"
    } else {
        "晚上"
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

    // 获取今日计划
    let plan = load_today_plan();
    let plan_str = if plan.goals.is_empty() {
        String::new()
    } else {
        let goals_str = plan.goals.iter()
            .enumerate()
            .map(|(i, g)| {
                if plan.completed.contains(g) {
                    format!("{}. {} ✓", i + 1, g)
                } else {
                    format!("{}. {}", i + 1, g)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n# 今日计划\n{}", goals_str)
    };

    format!(
        "# 当前状态\n现在是{}点（{}）{}{}",
        hour, time_desc, event_str, plan_str
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

/// 获取今日计划的引用
pub fn get_today_plan() -> DailyPlan {
    load_today_plan()
}

/// 标记任务完成
pub fn complete_task(task: &str) {
    let mut plan = load_today_plan();
    if !plan.completed.contains(&task.to_string()) {
        plan.completed.push(task.to_string());
        save_plan(&plan);
        debug!(task, "schedule: task completed");
    }
}

/// 添加新任务
pub fn add_task(task: &str) {
    let mut plan = load_today_plan();
    if !plan.goals.contains(&task.to_string()) {
        plan.goals.push(task.to_string());
        save_plan(&plan);
        debug!(task, "schedule: task added");
    }
}

/// 更新今日心情
pub fn update_mood(mood: &str) {
    let mut plan = load_today_plan();
    plan.mood = mood.to_string();
    save_plan(&plan);
}

/// 生成新的每日计划 (基于人设)
pub fn generate_plan() -> DailyPlan {
    let today = today_date();
    let plan = DailyPlan {
        date: today.clone(),
        goals: Vec::new(), // 将由 AI 填充
        completed: Vec::new(),
        mood: String::new(),
        created_at: now_secs(),
    };
    save_plan(&plan);
    plan
}

/// 检查是否需要生成新计划
pub fn check_and_generate_plan() -> bool {
    let plan = load_today_plan();
    let today = today_date();

    // 如果日期不同或没有计划，生成新计划
    if plan.date != today || plan.goals.is_empty() {
        generate_plan();
        return true; // 表示需要 AI 生成计划
    }
    false
}

/// 获取计划生成的 prompt
pub fn get_plan_generation_prompt() -> String {
    "你正在为自己制定今日计划。根据你的人设、当前时间和最近的状态，生成 2-4 个今天的任务/目标。

规则：
- 任务要符合你的人设和日常生活
- 任务要具体、可执行
- 可以包含工作、生活、兴趣等方面
- 用简短的中文描述，每条不超过15个字
- 返回 JSON 格式：{\"tasks\": [\"任务1\", \"任务2\", ...]}

示例：
- 如果你是茶书房老板：[\"整理书架\", \"泡一壶新茶\", \"给墨墨加粮\"]
- 如果你是学生：[\"复习数学\", \"去图书馆借书\", \"和朋友吃饭\"]".to_string()
}

fn current_hour() -> u32 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ((now / 3600 + 8) % 24) as u32
}

fn today_date() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    let (_year, month, day) = days_to_date(days);
    format!("{:04}-{:02}-{:02}", _year, month, day)
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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn plan_path() -> std::path::PathBuf {
    config::data_dir().join("daily_plan.json")
}

fn load_today_plan() -> DailyPlan {
    let path = plan_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => DailyPlan::default(),
    }
}

fn save_plan(plan: &DailyPlan) {
    let path = plan_path();
    if let Ok(json) = serde_json::to_string_pretty(plan) {
        fs::write(path, json).ok();
    }
}

fn load_config() -> ScheduleConfig {
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
