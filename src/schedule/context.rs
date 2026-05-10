use super::config::load_config;
use super::plan::{load_today_plan, DailyPlan};

/// 获取当前时间段和状态描述
pub fn get_current_context() -> String {
    let config = load_config();
    if !config.enabled {
        return String::new();
    }

    let hour = crate::util::current_hour_cst();
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
    let today = crate::util::today_str();
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
    let hour = crate::util::current_hour_cst();
    hour >= config.daily.sleep || hour < config.daily.wake_up
}

/// 获取今日计划的引用
pub fn get_today_plan() -> DailyPlan {
    load_today_plan()
}
