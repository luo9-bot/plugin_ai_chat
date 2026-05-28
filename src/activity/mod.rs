//! 活动生命周期系统
//!
//! 跟踪 bot 的「日常生活」模拟：
//! - 活动阶段：刚开始 → 进行中 → 快结束 → 已完成
//! - 起床/入睡周期
//! - 最近完成的活动记录（供主动消息使用）

use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

pub use types::ActivityType;

mod types {
    #[derive(Debug, Clone, PartialEq)]
    pub enum ActivityType {
        Training,
        Eating,
        Sleeping,
        Working,
        Outing,
        Bathing,
        Custom(String),
    }

    impl ActivityType {
        pub fn default_duration(&self) -> u64 {
            match self {
                ActivityType::Training => 1800,
                ActivityType::Eating => 1200,
                ActivityType::Sleeping => 28800,
                ActivityType::Working => 7200,
                ActivityType::Outing => 3600,
                ActivityType::Bathing => 1200,
                ActivityType::Custom(_) => 1800,
            }
        }

        pub fn describe(&self) -> &str {
            match self {
                ActivityType::Training => "训练/运动",
                ActivityType::Eating => "吃饭",
                ActivityType::Sleeping => "睡觉/休息",
                ActivityType::Working => "工作/学习",
                ActivityType::Outing => "外出",
                ActivityType::Bathing => "洗澡",
                ActivityType::Custom(s) => s.as_str(),
            }
        }
    }
}

/// 活动阶段
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityPhase {
    JustStarted,
    InProgress,
    NearEnd,
    Completed,
}

impl ActivityPhase {
    pub fn from_progress(progress: f64) -> Self {
        if progress >= 1.0 {
            ActivityPhase::Completed
        } else if progress >= 0.8 {
            ActivityPhase::NearEnd
        } else if progress >= 0.1 {
            ActivityPhase::InProgress
        } else {
            ActivityPhase::JustStarted
        }
    }
}

/// 活动状态（进行中）
#[derive(Debug, Clone)]
pub struct ActivityState {
    pub activity: ActivityType,
    pub started_at: u64,
    pub expires_at: u64,
}

impl ActivityState {
    /// 计算进度 0.0~1.0
    pub fn progress(&self) -> f64 {
        let now = crate::util::now_secs();
        let elapsed = now.saturating_sub(self.started_at);
        let total = self.expires_at.saturating_sub(self.started_at).max(1);
        (elapsed as f64 / total as f64).min(1.0)
    }

    pub fn phase(&self) -> ActivityPhase {
        ActivityPhase::from_progress(self.progress())
    }
}

/// 已完成的活动记录
#[derive(Debug, Clone)]
pub struct CompletedActivity {
    pub activity: ActivityType,
    pub finished_at: u64,
}

/// 待处理的生命事件（用于主动消息触发）
#[derive(Debug, Clone)]
pub enum LifeEvent {
    /// 刚醒来
    WokeUp,
    /// 刚完成某活动
    ActivityCompleted(ActivityType),
    /// 要去睡觉了
    GoingToSleep,
}

// ── 全局状态 ────────────────────────────────────────────────────

/// 当前进行中的活动
static ACTIVITY_STATE: Mutex<Option<HashMap<u64, ActivityState>>> = Mutex::new(None);
/// 最近完成的活动列表
static COMPLETED_ACTIVITIES: Mutex<Vec<CompletedActivity>> = Mutex::new(Vec::new());
/// 今天的日期字符串，用于判断是否需要重新检查起床/入睡
static LAST_WAKE_DATE: Mutex<String> = Mutex::new(String::new());
static LAST_SLEEP_DATE: Mutex<String> = Mutex::new(String::new());

/// 已完成活动的保留时间（秒）
const COMPLETED_TTL: u64 = 7200;

const DARLING_ACTIVITY_DURATION_RATIO: f64 = 0.3;

// ── 公开 API ────────────────────────────────────────────────────

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

        if let ActivityType::Custom(ref desc) = activity
            && let Some(task) = desc.strip_prefix("执行计划：")
        {
            crate::schedule::complete_task(task);
        }

        let mut guard = ACTIVITY_STATE.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        info!(user_id, activity = %activity.describe(), duration, "activity: started");
        map.insert(user_id, state);
    }
}

/// 检查用户是否有活跃的活动状态
pub fn get_active_activity(user_id: u64) -> Option<ActivityState> {
    let guard = ACTIVITY_STATE.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(state) = map.get(&user_id)
    {
        let now = crate::util::now_secs();
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

/// 获取活动状态的 prompt 上下文
pub fn get_activity_context(user_id: u64) -> Option<String> {
    let state = get_active_activity(user_id)?;
    let progress = state.progress();
    let remaining = state.expires_at.saturating_sub(crate::util::now_secs());
    let remaining_min = remaining / 60;

    let phase_hint = match state.phase() {
        ActivityPhase::JustStarted => "你刚说完要做这件事，还不久。",
        ActivityPhase::InProgress => "你还在做这件事。",
        ActivityPhase::NearEnd => "快做完了，正在收尾。",
        ActivityPhase::Completed => "",
    };

    let darling_note = if is_darling(user_id) {
        "\n注意：这是你认定的人(Darling)，即使在忙你也愿意去回复他。"
    } else {
        ""
    };

    Some(format!(
        "# 当前活动状态\n{}（还剩约{}分钟，进度{:.0}%）。\n{}{}",
        state.activity.describe(),
        remaining_min,
        progress * 100.0,
        phase_hint,
        darling_note
    ))
}

/// 检查是否有待处理的生命事件
/// 返回优先级最高的事件（只返回一个，消费后需调用 clear_life_event）
pub fn get_pending_life_event(_user_id: u64) -> Option<LifeEvent> {
    let now = crate::util::now_secs();
    let schedule_config = crate::schedule::config::load_config();

    // 优先级 1: 起床事件（只在今天还没触发过时触发）
    if schedule_config.enabled {
        let today = crate::util::today_str();
        let hour = crate::util::current_hour_cst();
        {
            let last_wake = LAST_WAKE_DATE.lock().unwrap();
            if *last_wake != today && hour >= schedule_config.daily.wake_up && hour < 12 {
                return Some(LifeEvent::WokeUp);
            }
        }
        // 优先级 2: 入睡事件
        {
            let last_sleep = LAST_SLEEP_DATE.lock().unwrap();
            if *last_sleep != today && hour >= schedule_config.daily.sleep {
                return Some(LifeEvent::GoingToSleep);
            }
        }
    }

    // 优先级 3: 刚完成的活动（2 小时内完成的）
    {
        let completed = COMPLETED_ACTIVITIES.lock().unwrap();
        if let Some(last) = completed.last() {
            if now.saturating_sub(last.finished_at) < 600 {
                // 10 分钟内刚完成的
                return Some(LifeEvent::ActivityCompleted(last.activity.clone()));
            }
        }
    }

    None
}

/// 消费一个生命事件（标记为已处理）
pub fn clear_life_event(event: &LifeEvent) {
    match event {
        LifeEvent::WokeUp => {
            let today = crate::util::today_str();
            let mut last = LAST_WAKE_DATE.lock().unwrap();
            *last = today;
            info!("life_event: woke up marked");
        }
        LifeEvent::GoingToSleep => {
            let today = crate::util::today_str();
            let mut last = LAST_SLEEP_DATE.lock().unwrap();
            *last = today;
            info!("life_event: going to sleep marked");
        }
        LifeEvent::ActivityCompleted(_) => {
            // 已完成的活动从列表移除（被消费后就不再触发）
            let mut completed = COMPLETED_ACTIVITIES.lock().unwrap();
            if !completed.is_empty() {
                completed.remove(0);
                info!("life_event: activity completion consumed");
            }
        }
    }
}

/// 从周期循环中调用：检查活动进度，处理阶段转换
pub fn check_activity_progress() {
    let now = crate::util::now_secs();
    let self_qq = crate::config::get().self_qq;
    if self_qq == 0 {
        return;
    }

    let mut guard = ACTIVITY_STATE.lock().unwrap();
    let map = match guard.as_mut() {
        Some(m) => m,
        None => return,
    };

    let mut to_remove = Vec::new();

    for (&uid, state) in map.iter() {
        if now >= state.expires_at {
            // 活动已完成：记录到完成列表
            let completed = CompletedActivity {
                activity: state.activity.clone(),
                finished_at: now,
            };
            let mut completed_list = COMPLETED_ACTIVITIES.lock().unwrap();
            completed_list.push(completed);
            if completed_list.len() > 5 {
                completed_list.remove(0);
            }
            info!(user_id = uid, activity = %state.activity.describe(), "activity: completed");
            to_remove.push(uid);
        } else {
            let progress = state.progress();
            let phase = state.phase();
            if phase == ActivityPhase::NearEnd {
                debug!(user_id = uid, activity = %state.activity.describe(), progress, "activity: near end");
            }
        }
    }

    for uid in to_remove {
        map.remove(&uid);
    }

    // 清理过期的完成记录
    let mut completed_list = COMPLETED_ACTIVITIES.lock().unwrap();
    completed_list.retain(|c| now.saturating_sub(c.finished_at) < COMPLETED_TTL);
}

/// 活动关键词检测
fn detect_activity(message: &str) -> Option<ActivityType> {
    let plan = crate::schedule::get_today_plan();
    for goal in &plan.goals {
        if !plan.completed.contains(goal) && message.contains(goal.as_str()) {
            return Some(ActivityType::Custom(format!("执行计划：{}", goal)));
        }
    }
    if message.contains("训练") || message.contains("健身") || message.contains("跑步")
        || message.contains("运动") || message.contains("打球") || message.contains("游泳")
    {
        return Some(ActivityType::Training);
    }
    if message.contains("吃饭") || message.contains("去吃") || message.contains("干饭")
        || message.contains("外卖到了") || message.contains("做饭")
    {
        return Some(ActivityType::Eating);
    }
    if message.contains("睡觉") || message.contains("睡了") || message.contains("晚安")
        || message.contains("休息") || message.contains("困了") || message.contains("去睡")
    {
        return Some(ActivityType::Sleeping);
    }
    if message.contains("上班") || message.contains("开会") || message.contains("加班")
        || message.contains("学习") || message.contains("写代码") || message.contains("上课")
        || message.contains("去忙") || message.contains("忙了")
    {
        return Some(ActivityType::Working);
    }
    if message.contains("出去") || message.contains("出门") || message.contains("走了")
        || message.contains("出去玩") || message.contains("逛街")
    {
        return Some(ActivityType::Outing);
    }
    if message.contains("洗澡") || message.contains("冲澡") {
        return Some(ActivityType::Bathing);
    }
    None
}

fn is_darling(user_id: u64) -> bool {
    let darling = crate::config::get().darling_qq;
    darling > 0 && user_id == darling
}
