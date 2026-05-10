use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

use crate::config;

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

fn plan_path() -> std::path::PathBuf {
    config::data_dir().join("daily_plan.json")
}

pub(crate) fn load_today_plan() -> DailyPlan {
    let path = plan_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => DailyPlan::default(),
    }
}

pub(crate) fn save_plan(plan: &DailyPlan) {
    let path = plan_path();
    if let Ok(json) = serde_json::to_string_pretty(plan) {
        fs::write(path, json).ok();
    }
}

/// 生成新的每日计划 (基于人设)
pub fn generate_plan() -> DailyPlan {
    let today = crate::util::today_str();
    let plan = DailyPlan {
        date: today.clone(),
        goals: Vec::new(), // 将由 AI 填充
        completed: Vec::new(),
        mood: String::new(),
        created_at: crate::util::now_secs(),
    };
    save_plan(&plan);
    plan
}

/// 检查是否需要生成新计划
pub fn check_and_generate_plan() -> bool {
    let plan = load_today_plan();
    let today = crate::util::today_str();

    // 如果日期不同或没有计划，生成新计划
    if plan.date != today || plan.goals.is_empty() {
        generate_plan();
        return true; // 表示需要 AI 生成计划
    }
    false
}

/// 获取计划生成的 prompt
pub fn get_plan_generation_prompt() -> String {
    crate::prompt::PromptManager::get().raw("daily_plan").to_string()
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
