//! 活动生命周期系统
//!
//! 跟踪 bot 的「日常生活」模拟：
//! - 活动阶段：刚开始 → 进行中 → 快结束 → 已完成
//! - 起床/入睡周期
//! - 最近完成的活动记录（供主动消息使用）

use crate::util::MutexExt;
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

pub(crate) use types::ActivityType;

mod types {
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) enum ActivityType {
        Training,
        Eating,
        Sleeping,
        Working,
        Outing,
        Bathing,
    }

    impl ActivityType {
        pub(crate) fn default_duration(&self) -> u64 {
            match self {
                ActivityType::Training => 1800,
                ActivityType::Eating => 1200,
                ActivityType::Sleeping => 28800,
                ActivityType::Working => 7200,
                ActivityType::Outing => 3600,
                ActivityType::Bathing => 1200,
            }
        }

        pub(crate) fn describe(&self) -> &str {
            match self {
                ActivityType::Training => "训练/运动",
                ActivityType::Eating => "吃饭",
                ActivityType::Sleeping => "睡觉/休息",
                ActivityType::Working => "工作/学习",
                ActivityType::Outing => "外出",
                ActivityType::Bathing => "洗澡",
            }
        }
    }
}

/// 活动阶段
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ActivityPhase {
    JustStarted,
    InProgress,
    NearEnd,
    Completed,
}

impl ActivityPhase {
    pub(crate) fn from_progress(progress: f64) -> Self {
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
pub(crate) struct ActivityState {
    pub activity: ActivityType,
    pub started_at: u64,
    pub expires_at: u64,
}

impl ActivityState {
    /// 计算进度 0.0~1.0
    pub(crate) fn progress(&self) -> f64 {
        let now = crate::util::now_secs();
        let elapsed = now.saturating_sub(self.started_at);
        let total = self.expires_at.saturating_sub(self.started_at).max(1);
        (elapsed as f64 / total as f64).min(1.0)
    }

    pub(crate) fn phase(&self) -> ActivityPhase {
        ActivityPhase::from_progress(self.progress())
    }
}

/// 已完成的活动记录
///
/// 只保留完成时刻：`activity` 字段原本也存了一份，但没有任何读取点，
/// 而活动类型已经进了日志（见下方 `info!`）。
#[derive(Debug, Clone)]
pub(crate) struct CompletedActivity {
    pub finished_at: u64,
}

/// 当前进行中的活动
static ACTIVITY_STATE: Mutex<Option<HashMap<u64, ActivityState>>> = Mutex::new(None);
/// 最近完成的活动列表
static COMPLETED_ACTIVITIES: Mutex<Vec<CompletedActivity>> = Mutex::new(Vec::new());
/// 已完成活动的保留时间（秒）
const COMPLETED_TTL: u64 = 7200;

/// 检测 bot 自己的消息是否包含活动声明，并记录
///
/// 这里**不**再顺带判定"她是不是完成了某条计划"。早先的实现把她说的话
/// 与计划文本做整句字面包含匹配来判定完成，三天日志里 0 次命中——
/// 判断该由她自己在工具的帮助下做（见 `schedule::set_status`），
/// 而不是靠猜文本。
pub(crate) fn check_bot_message(user_id: u64, message: &str) {
    if let Some(activity) = detect_activity(message) {
        let now = crate::util::now_secs();
        let duration = activity.default_duration();
        let state = ActivityState {
            activity: activity.clone(),
            started_at: now,
            expires_at: now + duration,
        };

        let mut guard = ACTIVITY_STATE.lock_recover();
        let map = guard.get_or_insert_with(HashMap::new);
        info!(user_id, activity = %activity.describe(), duration, "activity: started");
        map.insert(user_id, state);
    }
}

/// 从周期循环中调用：检查活动进度，处理阶段转换
pub(crate) fn check_activity_progress() {
    let now = crate::util::now_secs();
    let self_qq = crate::config::get().self_qq;
    if self_qq == 0 {
        return;
    }

    let mut guard = ACTIVITY_STATE.lock_recover();
    let map = match guard.as_mut() {
        Some(m) => m,
        None => return,
    };

    let mut to_remove = Vec::new();

    for (&uid, state) in map.iter() {
        if now >= state.expires_at {
            // 活动已完成：记录到完成列表
            let completed = CompletedActivity { finished_at: now };
            let mut completed_list = COMPLETED_ACTIVITIES.lock_recover();
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
    let mut completed_list = COMPLETED_ACTIVITIES.lock_recover();
    completed_list.retain(|c| now.saturating_sub(c.finished_at) < COMPLETED_TTL);
}

/// 活动关键词检测
///
/// 只认"她在做什么"这类通用活动，不再把计划文本拿来做匹配：
/// 计划是否完成由她自己用 `finish_plan` 落笔。
fn detect_activity(message: &str) -> Option<ActivityType> {
    if message.contains("训练")
        || message.contains("健身")
        || message.contains("跑步")
        || message.contains("运动")
        || message.contains("打球")
        || message.contains("游泳")
    {
        return Some(ActivityType::Training);
    }
    if message.contains("吃饭")
        || message.contains("去吃")
        || message.contains("干饭")
        || message.contains("外卖到了")
        || message.contains("做饭")
    {
        return Some(ActivityType::Eating);
    }
    if message.contains("睡觉")
        || message.contains("睡了")
        || message.contains("晚安")
        || message.contains("休息")
        || message.contains("困了")
        || message.contains("去睡")
    {
        return Some(ActivityType::Sleeping);
    }
    if message.contains("上班")
        || message.contains("开会")
        || message.contains("加班")
        || message.contains("学习")
        || message.contains("写代码")
        || message.contains("上课")
        || message.contains("去忙")
        || message.contains("忙了")
    {
        return Some(ActivityType::Working);
    }
    if message.contains("出去")
        || message.contains("出门")
        || message.contains("走了")
        || message.contains("出去玩")
        || message.contains("逛街")
    {
        return Some(ActivityType::Outing);
    }
    if message.contains("洗澡") || message.contains("冲澡") {
        return Some(ActivityType::Bathing);
    }
    None
}
