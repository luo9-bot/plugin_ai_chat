use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::debug;

use crate::config;

// ── 周计划 ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyGoal {
    pub content: String,
    pub target_day: String,    // "Monday" | "Tuesday" | ...
    pub completed: bool,
    #[serde(default)]
    pub completed_at: u64,     // Unix timestamp when completed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyPlan {
    pub week_start: String,    // YYYY-MM-DD (周一)
    pub goals: Vec<WeeklyGoal>,
    pub created_at: u64,
    pub week_reflection: String, // 周反思摘要
}

impl Default for WeeklyPlan {
    fn default() -> Self {
        Self {
            week_start: String::new(),
            goals: Vec::new(),
            created_at: 0,
            week_reflection: String::new(),
        }
    }
}

// ── 月计划 ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyGoal {
    pub content: String,
    pub completed: bool,
    #[serde(default)]
    pub completed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyPlan {
    pub month: String,         // YYYY-MM
    pub goals: Vec<MonthlyGoal>,
    pub created_at: u64,
}

impl Default for MonthlyPlan {
    fn default() -> Self {
        Self {
            month: String::new(),
            goals: Vec::new(),
            created_at: 0,
        }
    }
}

// ── 今日推动状态 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushState {
    /// 今天已经推动过的任务列表（防止重复推动）
    pub pushed_today: Vec<String>,
    /// 当天日期
    pub date: String,
}

// ── 文件路径 ────────────────────────────────────────────────────

fn weekly_path() -> std::path::PathBuf {
    config::data_dir().join("weekly_plan.json")
}

fn monthly_path() -> std::path::PathBuf {
    config::data_dir().join("monthly_plan.json")
}

fn push_state_path() -> std::path::PathBuf {
    config::data_dir().join("plan_push_state.json")
}

fn push_history_path() -> std::path::PathBuf {
    config::data_dir().join("push_history.json")
}

/// 记录推动日志
pub fn record_push_log(kind: &str, content: &str) {
    let path = push_history_path();
    let mut history: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    history.push(serde_json::json!({
        "time": crate::util::now_secs(),
        "kind": kind,
        "content": content,
    }));
    // 只保留最近 200 条
    if history.len() > 200 {
        history.drain(0..history.len() - 200);
    }
    if let Ok(json) = serde_json::to_string_pretty(&history) {
        std::fs::write(path, json).ok();
    }
}

// ── 周计划 CRUD ─────────────────────────────────────────────────

fn get_monday_of_week() -> String {
    // 简单实现：取今天的日期，计算周一是哪天
    // 用时间戳方式计算
    let now = crate::util::now_secs();
    // 一天 86400 秒，用今天的秒数反推周一
    let weekday = crate::util::current_weekday_cst();
    let offset = (weekday - 1) as u64; // Monday=1, offset=0; Sunday=7, offset=6
    let monday_ts = now - offset * 86400;
    crate::util::ts_to_date_str(monday_ts)
}

pub fn load_weekly_plan() -> WeeklyPlan {
    let path = weekly_path();
    match fs::read_to_string(&path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => WeeklyPlan::default(),
    }
}

pub fn save_weekly_plan(plan: &WeeklyPlan) {
    let path = weekly_path();
    if let Ok(json) = serde_json::to_string_pretty(plan) {
        fs::write(path, json).ok();
    }
}

/// 检查是否需要生成新周计划
pub fn check_and_generate_weekly_plan() -> bool {
    let plan = load_weekly_plan();
    let monday = get_monday_of_week();
    if plan.week_start != monday || plan.goals.is_empty() {
        debug!("schedule: generating new weekly plan");
        save_weekly_plan(&WeeklyPlan {
            week_start: monday,
            goals: Vec::new(),
            created_at: crate::util::now_secs(),
            week_reflection: String::new(),
        });
        return true;
    }
    false
}

/// 获取今天的周计划任务
pub fn get_today_weekly_goals() -> Vec<String> {
    let plan = load_weekly_plan();
    let today_eng = crate::util::current_weekday_eng();
    plan.goals
        .iter()
        .filter(|g| g.target_day == today_eng && !g.completed)
        .map(|g| g.content.clone())
        .collect()
}

pub fn complete_weekly_goal(content: &str) {
    let mut plan = load_weekly_plan();
    let now = crate::util::now_secs();
    for goal in &mut plan.goals {
        if goal.content == content && !goal.completed {
            goal.completed = true;
            goal.completed_at = now;
            save_weekly_plan(&plan);
            record_push_log("周计划", content);
            debug!(content, "schedule: weekly goal completed");
            break;
        }
    }
}

pub fn update_week_reflection(reflection: &str) {
    let mut plan = load_weekly_plan();
    plan.week_reflection = reflection.to_string();
    save_weekly_plan(&plan);
}

// ── 月计划 CRUD ─────────────────────────────────────────────────

fn get_current_month() -> String {
    let now = crate::util::now_secs();
    crate::util::ts_to_month_str(now)
}

pub fn load_monthly_plan() -> MonthlyPlan {
    let path = monthly_path();
    match fs::read_to_string(&path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => MonthlyPlan::default(),
    }
}

pub fn save_monthly_plan(plan: &MonthlyPlan) {
    let path = monthly_path();
    if let Ok(json) = serde_json::to_string_pretty(plan) {
        fs::write(path, json).ok();
    }
}

pub fn check_and_generate_monthly_plan() -> bool {
    let plan = load_monthly_plan();
    let month = get_current_month();
    if plan.month != month || plan.goals.is_empty() {
        debug!("schedule: generating new monthly plan");
        save_monthly_plan(&MonthlyPlan {
            month,
            goals: Vec::new(),
            created_at: crate::util::now_secs(),
        });
        return true;
    }
    false
}

pub fn complete_monthly_goal(content: &str) {
    let mut plan = load_monthly_plan();
    let now = crate::util::now_secs();
    for goal in &mut plan.goals {
        if goal.content == content && !goal.completed {
            goal.completed = true;
            goal.completed_at = now;
            save_monthly_plan(&plan);
            record_push_log("月计划", content);
            debug!(content, "schedule: monthly goal completed");
            break;
        }
    }
}

// ── 推动系统 ────────────────────────────────────────────────────

/// 检查今天有没有需要推动的任务，返回应该推动的内容
pub fn check_plan_push() -> Vec<String> {
    let today = crate::util::today_str();

    // 读取推动状态
    let state_path = push_state_path();
    let mut state: PushState = match fs::read_to_string(&state_path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => PushState::default(),
    };

    // 如果日期变了，重置推动记录
    if state.date != today {
        state.date = today.clone();
        state.pushed_today.clear();
    }

    let mut to_push = Vec::new();

    // 1. 周计划任务：今天应该做的，且还没推动过的
    let weekly_goals = get_today_weekly_goals();
    for goal in &weekly_goals {
        if !state.pushed_today.contains(goal) {
            to_push.push(format!("周计划：{}", goal));
            state.pushed_today.push(goal.clone());
        }
    }

    // 2. 月计划任务：每天推动一次当月目标（随机选一个未完成的）
    let monthly = load_monthly_plan();
    let unfinished_monthly: Vec<&str> = monthly.goals
        .iter()
        .filter(|g| !g.completed)
        .map(|g| g.content.as_str())
        .collect();
    if !unfinished_monthly.is_empty() && !state.pushed_today.contains(&format!("月:{}", unfinished_monthly[0])) {
        let pick = unfinished_monthly[0];
        to_push.push(format!("月计划：{}", pick));
        state.pushed_today.push(format!("月:{}", pick));
    }

    // 保存推动状态
    if !to_push.is_empty() {
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            fs::write(&state_path, json).ok();
        }
        // 记录到历史
        for p in &to_push {
            let kind = if p.starts_with("周计划") { "周计划" } else { "月计划" };
            let content = p.trim_start_matches("周计划：").trim_start_matches("月计划：");
            record_push_log(kind, content);
        }
    }

    to_push
}

/// 加载今日推动状态
pub fn load_push_state() -> PushState {
    let state_path = push_state_path();
    match fs::read_to_string(&state_path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => PushState::default(),
    }
}

/// 获取用于上下文展示的周/月计划文本
pub fn get_plan_context() -> String {
    let mut parts = Vec::new();

    let weekly = load_weekly_plan();
    if !weekly.goals.is_empty() {
        let goals_str: Vec<String> = weekly.goals.iter().map(|g| {
            let status = if g.completed { "✓" } else { "○" };
            format!("  {}[{}] {}", status, g.target_day, g.content)
        }).collect();
        parts.push(format!("# 本周计划\n{}", goals_str.join("\n")));
    }

    let monthly = load_monthly_plan();
    if !monthly.goals.is_empty() {
        let goals_str: Vec<String> = monthly.goals.iter().map(|g| {
            let status = if g.completed { "✓" } else { "○" };
            format!("  {} {}", status, g.content)
        }).collect();
        parts.push(format!("# 本月目标\n{}", goals_str.join("\n")));
    }

    parts.join("\n\n")
}
