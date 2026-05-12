//! 活动状态系统
//!
//! 当 bot 表示正在做某事（训练、吃饭、睡觉等）时，维持该状态一段时间。
//! 避免刚说"训练去了"就立刻回"嗯 回来了"的不自然行为。

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

/// 活动类型
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityType {
    /// 训练/运动
    Training,
    /// 吃饭
    Eating,
    /// 睡觉/休息
    Sleeping,
    /// 工作/学习
    Working,
    /// 外出
    Outing,
    /// 洗澡
    Bathing,
    /// 自定义活动
    Custom(String),
}

impl ActivityType {
    /// 活动的默认持续时间（秒）
    pub fn default_duration(&self) -> u64 {
        match self {
            ActivityType::Training => 1800,  // 30 分钟
            ActivityType::Eating => 1200,    // 20 分钟
            ActivityType::Sleeping => 28800, // 8 小时
            ActivityType::Working => 7200,   // 2 小时
            ActivityType::Outing => 3600,    // 1 小时
            ActivityType::Bathing => 1200,   // 20 分钟
            ActivityType::Custom(_) => 1800, // 默认 30 分钟
        }
    }

    /// 活动中的回复风格提示
    pub fn reply_hint(&self) -> &str {
        match self {
            ActivityType::Training => "你正在训练/运动中，回复要简短，表示还在忙",
            ActivityType::Eating => "你正在吃饭，回复要简短，表示在吃饭",
            ActivityType::Sleeping => "你正在睡觉/休息，不应该回复或回复很简短迷糊",
            ActivityType::Working => "你正在工作/学习，回复简短表示在忙",
            ActivityType::Outing => "你正在外面，回复简短",
            ActivityType::Bathing => "你正在洗澡，不应该回复",
            ActivityType::Custom(_) => "你正在忙，回复简短表示在忙",
        }
    }
}

/// 用户活动状态
#[derive(Debug, Clone)]
pub struct ActivityState {
    pub activity: ActivityType,
    pub started_at: u64,
    pub expires_at: u64,
}

/// 活动状态存储 (user_id -> ActivityState)
static ACTIVITY_STATE: Mutex<Option<HashMap<u64, ActivityState>>> = Mutex::new(None);

/// darling_qq 的活动持续时间缩减比例（darling 发消息时活动更快结束）
const DARLING_ACTIVITY_DURATION_RATIO: f64 = 0.3;

/// 活动关键词映射 + 每日计划任务匹配
fn detect_activity(message: &str) -> Option<ActivityType> {
    // 先检查是否匹配每日计划任务
    let plan = crate::schedule::get_today_plan();
    for goal in &plan.goals {
        if !plan.completed.contains(goal) && message.contains(goal.as_str()) {
            return Some(ActivityType::Custom(format!("执行计划：{}", goal)));
        }
    }
    // 训练/运动
    if message.contains("训练") || message.contains("健身") || message.contains("跑步")
        || message.contains("运动") || message.contains("打球") || message.contains("游泳")
    {
        return Some(ActivityType::Training);
    }

    // 吃饭
    if message.contains("吃饭") || message.contains("去吃") || message.contains("干饭")
        || message.contains("外卖到了") || message.contains("做饭")
    {
        return Some(ActivityType::Eating);
    }

    // 睡觉/休息
    if message.contains("睡觉") || message.contains("睡了") || message.contains("晚安")
        || message.contains("休息") || message.contains("困了") || message.contains("去睡")
    {
        return Some(ActivityType::Sleeping);
    }

    // 工作/学习
    if message.contains("上班") || message.contains("开会") || message.contains("加班")
        || message.contains("学习") || message.contains("写代码") || message.contains("上课")
        || message.contains("去忙") || message.contains("忙了")
    {
        return Some(ActivityType::Working);
    }

    // 外出
    if message.contains("出去") || message.contains("出门") || message.contains("走了")
        || message.contains("出去玩") || message.contains("逛街") || message.contains("出去浪")
    {
        return Some(ActivityType::Outing);
    }

    // 洗澡
    if message.contains("洗澡") || message.contains("冲澡") {
        return Some(ActivityType::Bathing);
    }

    None
}

/// 检测 bot 自己的消息是否包含活动声明，并记录
pub fn check_bot_message(user_id: u64, message: &str) {
    if let Some(activity) = detect_activity(message) {
        let now = crate::util::now_secs();
        let duration = activity.default_duration();
        let state = ActivityState {
            activity: activity.clone(),
            started_at: now,
            expires_at: now + duration,
        };

        // 如果匹配每日计划任务，标记为已完成
        if let ActivityType::Custom(ref desc) = activity
            && let Some(task) = desc.strip_prefix("执行计划：") {
                crate::schedule::complete_task(task);
            }

        let mut guard = ACTIVITY_STATE.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        info!(user_id, activity = ?activity, duration, "activity: detected");
        map.insert(user_id, state);
    }
}

/// 检查用户是否有活跃的活动状态
///
/// darling_qq 的活动持续时间会大幅缩短（更容易被"打断"）
pub fn get_active_activity(user_id: u64) -> Option<ActivityState> {
    let guard = ACTIVITY_STATE.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(state) = map.get(&user_id) {
            let now = crate::util::now_secs();
            // darling_qq 的活动持续时间缩短
            let effective_expires = if is_darling(user_id) {
                
                state.started_at
                    + ((state.expires_at - state.started_at) as f64 * DARLING_ACTIVITY_DURATION_RATIO) as u64
            } else {
                state.expires_at
            };
            if now < effective_expires {
                return Some(state.clone());
            }
        }
    None
}

fn is_darling(user_id: u64) -> bool {
    let darling = crate::config::get().darling_qq;
    darling > 0 && user_id == darling
}

/// 清除用户的活动状态（当用户主动对话时）
pub fn clear_activity(user_id: u64) {
    let mut guard = ACTIVITY_STATE.lock().unwrap();
    if let Some(ref mut map) = *guard
        && map.remove(&user_id).is_some() {
            debug!(user_id, "activity: cleared");
        }
}

/// 获取活动状态的 prompt 上下文
pub fn get_activity_context(user_id: u64) -> Option<String> {
    let state = get_active_activity(user_id)?;
    let remaining = state.expires_at.saturating_sub(crate::util::now_secs());
    let remaining_min = remaining / 60;

    let darling_note = if is_darling(user_id) {
        "\n注意：这是你认定的人(Darling)，即使在忙你也愿意去回复他，要温柔一些。"
    } else {
        ""
    };

    Some(format!(
        "# 当前活动状态\n你刚才说了要去做某事（{}），现在还在进行中（还剩约{}分钟）。\n{}{}",
        describe_activity(&state.activity),
        remaining_min,
        state.activity.reply_hint(),
        darling_note
    ))
}

fn describe_activity(activity: &ActivityType) -> &str {
    match activity {
        ActivityType::Training => "训练/运动",
        ActivityType::Eating => "吃饭",
        ActivityType::Sleeping => "睡觉/休息",
        ActivityType::Working => "工作/学习",
        ActivityType::Outing => "外出",
        ActivityType::Bathing => "洗澡",
        ActivityType::Custom(s) => s.as_str(),
    }
}
