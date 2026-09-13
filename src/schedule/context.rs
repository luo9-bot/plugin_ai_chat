use super::plan::{DailyPlan, load_today_plan};

/// 获取今日计划的引用
pub fn get_today_plan() -> DailyPlan {
    load_today_plan()
}
