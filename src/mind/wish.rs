//! 愿望系统：她想要什么
//!
//! 计划系统回答"今天该做什么"，个人任务回答"答应的事怎么推进"，
//! 愿望系统回答"她自己想要什么"。
//! - 目标（goal）：带层级（父目标）、进度与期限的长期愿望
//! - 想法（idea）：新冒出来的念头，带兴奋度，有自己的生命周期
//!
//! 两者都由她自己产生（睡前整理时的 `come_up_goal` / `come_up_idea`），
//! 不是系统安排。定时器只负责在她想要的期限到来时提醒她——
//! 提醒以"想起"（意图堆）的形式出现，说不提起由她自己决定。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, info, warn};

use crate::config;
use crate::util;

/// 活跃目标上限：她不会同时追着一百件事
const MAX_ACTIVE_GOALS: usize = 8;
/// 想法数量上限：念头太多就不再是她的心事，而是噪音
const MAX_IDEAS: usize = 24;
/// 已完成目标的留存上限：给她的自我认识留证据，但不无限堆积
const MAX_FINISHED_GOALS: usize = 10;
/// 期限提醒后多少秒内不再重复提醒
const NOTIFY_COOLDOWN_SECS: u64 = 24 * 3600;

// ── 模型 ────────────────────────────────────────────────────────

/// 目标状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalStatus {
    /// 正在放在心上
    Active,
    /// 实现了
    Achieved,
    /// 放下了（不是失败，是不再想要）
    Given,
}

impl GoalStatus {
    pub(crate) fn is_active(self) -> bool {
        self == Self::Active
    }

    fn label(self) -> &'static str {
        match self {
            Self::Active => "放在心上",
            Self::Achieved => "实现了",
            Self::Given => "放下了",
        }
    }
}

/// 想法状态机：new → developing → realized / dropped
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IdeaStatus {
    /// 刚冒出来
    New,
    /// 正在琢磨
    Developing,
    /// 变成了行动/实现了
    Realized,
    /// 没了下文
    Dropped,
}

impl IdeaStatus {
    pub(crate) fn is_alive(self) -> bool {
        matches!(self, Self::New | Self::Developing)
    }
}

/// 愿望来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WishSource {
    /// 睡前整理时的反思
    Reflection,
    /// 聊天中冒出来的
    Conversation,
    /// 梦里/走神时想到的
    Dream,
}

/// 一个长期愿望：她想要的东西
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WishGoal {
    pub id: u64,
    pub text: String,
    /// 父目标（"学会做甜点"下面挂着"学会做提拉米苏"）
    pub parent_id: Option<u64>,
    /// 优先级 1~10
    pub priority: u8,
    /// 进度 0~100
    pub progress: u8,
    /// 期限（unix 秒，可选）
    pub deadline_secs: Option<u64>,
    pub status: GoalStatus,
    pub source: WishSource,
    pub created_at: u64,
    pub updated_at: u64,
    /// 上次因期限被提醒的时间（防止反复提醒同一件事）
    pub last_notified_at: Option<u64>,
}

/// 一个念头：她想试一试的主意
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WishIdea {
    pub id: u64,
    pub text: String,
    /// 兴奋度 1~10：越兴奋越容易被想起
    pub excitement: u8,
    pub status: IdeaStatus,
    pub source: WishSource,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WishStore {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    goals: Vec<WishGoal>,
    #[serde(default)]
    ideas: Vec<WishIdea>,
}

// ── 存储（Mutex + 原子 tmp+rename，中断恢复） ───────────────────

static STORE_LOCK: Mutex<()> = Mutex::new(());

fn wish_path() -> PathBuf {
    config::data_dir().join("mind").join("wish.json")
}

fn load_store() -> WishStore {
    let Ok(content) = fs::read_to_string(wish_path()) else {
        return WishStore::default();
    };
    match serde_json::from_str(&content) {
        Ok(store) => store,
        Err(e) => {
            warn!(error = %e, "wish: 愿望库解析失败，按空处理");
            WishStore::default()
        }
    }
}

fn save_store(store: &WishStore) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        warn!("wish: 愿望库序列化失败");
        return;
    };
    if let Err(e) = crate::util::atomic_write(wish_path(), json) {
        warn!(error = %e, "wish: 愿望库落盘失败");
    }
}

fn next_id(store: &mut WishStore) -> u64 {
    store.next_id += 1;
    store.next_id
}

/// 文本校验：非空、去首尾、不超过 80 字
fn clean_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 80 {
        return None;
    }
    Some(trimmed.to_string())
}

/// 归一化文本（去标点留实字，小写）——用于"同一个念头"的判重
fn normalized(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .collect::<String>()
        .to_lowercase()
}

/// 判断两个愿望文本是不是同一个念头（一方包含另一方）
fn same_wish(left: &str, right: &str) -> bool {
    let left = normalized(left);
    let right = normalized(right);
    left.len() >= 4 && right.len() >= 4 && (left.contains(&right) || right.contains(&left))
}

// ── 目标 ────────────────────────────────────────────────────────

/// 新增目标；与现有活跃目标重复时把旧目标抬回台面（优先级取高者）
pub(crate) fn add_goal(
    text: &str,
    parent_id: Option<u64>,
    priority: u8,
    deadline_secs: Option<u64>,
    source: WishSource,
) -> Option<WishGoal> {
    let text = clean_text(text)?;
    let priority = priority.clamp(1, 10);
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_store();

    // 父目标必须存在且活着
    if let Some(pid) = parent_id
        && !store
            .goals
            .iter()
            .any(|g| g.id == pid && g.status.is_active())
    {
        debug!(parent_id = pid, "wish: 父目标不存在或已结束，忽略");
        return None;
    }

    if let Some(existing) = store
        .goals
        .iter_mut()
        .find(|g| g.status.is_active() && same_wish(&g.text, &text))
    {
        existing.updated_at = util::now_secs();
        existing.priority = existing.priority.max(priority);
        if deadline_secs.is_some() {
            existing.deadline_secs = deadline_secs;
        }
        let result = existing.clone();
        save_store(&store);
        debug!(goal_id = result.id, "wish: 目标再次确认");
        return Some(result);
    }

    let active_count = store.goals.iter().filter(|g| g.status.is_active()).count();
    if active_count >= MAX_ACTIVE_GOALS {
        debug!(active_count, "wish: 活跃目标已达上限");
        return None;
    }

    let now = util::now_secs();
    let goal = WishGoal {
        id: next_id(&mut store),
        text,
        parent_id,
        priority,
        progress: 0,
        deadline_secs,
        status: GoalStatus::Active,
        source,
        created_at: now,
        updated_at: now,
        last_notified_at: None,
    };
    store.goals.push(goal.clone());
    save_store(&store);
    info!(goal_id = goal.id, text = %goal.text, "wish: 新目标");
    Some(goal)
}

/// 更新目标进度（0~100）
pub(crate) fn set_goal_progress(goal_id: u64, progress: u8) -> bool {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_store();
    let Some(goal) = store.goals.iter_mut().find(|g| g.id == goal_id) else {
        return false;
    };
    if !goal.status.is_active() {
        return false;
    }
    goal.progress = progress.min(100);
    goal.updated_at = util::now_secs();
    let done = goal.progress >= 100;
    if done {
        goal.status = GoalStatus::Achieved;
    }
    let text = goal.text.clone();
    retain_finished_goals(&mut store);
    save_store(&store);
    if done {
        info!(goal_id, text = %text, "wish: 目标实现");
    }
    done
}

/// 目标收尾：实现了，或者放下了
pub(crate) fn close_goal(goal_id: u64, achieved: bool) -> bool {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_store();
    let Some(goal) = store.goals.iter_mut().find(|g| g.id == goal_id) else {
        return false;
    };
    if !goal.status.is_active() {
        return false;
    }
    goal.status = if achieved {
        GoalStatus::Achieved
    } else {
        GoalStatus::Given
    };
    goal.progress = if achieved { 100 } else { goal.progress };
    goal.updated_at = util::now_secs();
    let text = goal.text.clone();
    retain_finished_goals(&mut store);
    save_store(&store);
    info!(goal_id, text = %text, achieved, "wish: 目标收尾");
    true
}

/// 目标是否到期该被提醒：有期限、过期、且最近没提醒过
fn goal_due(goal: &WishGoal, now: u64) -> bool {
    goal.status.is_active()
        && goal.deadline_secs.is_some_and(|d| d <= now)
        && goal
            .last_notified_at
            .is_none_or(|t| now.saturating_sub(t) >= NOTIFY_COOLDOWN_SECS)
}

/// 把到期目标的期限变成她的"想起"（意图堆），并标记已提醒
///
/// 定时器只兑现不产生意愿：这里提醒的是她自己立下的期限。
pub(crate) fn sync_due_to_wake(now: u64) {
    let due: Vec<WishGoal> = {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut store = load_store();
        let due: Vec<WishGoal> = store
            .goals
            .iter()
            .filter(|g| goal_due(g, now))
            .cloned()
            .collect();
        if due.is_empty() {
            return;
        }
        for goal in store.goals.iter_mut().filter(|g| goal_due(g, now)) {
            goal.last_notified_at = Some(now);
        }
        save_store(&store);
        due
    };

    for goal in due {
        let reason = format!("你想起的愿望：{}（期限到了）", goal.text);
        let plan = crate::mind::WakePlan::new(crate::mind::WakeKind::Idle, now, reason)
            .with_urgency(crate::mind::Urgency::Soon);
        crate::mind::add_wake_plan(plan);
        info!(goal_id = goal.id, "wish: 期限到期，挂起想起");
    }
}

fn retain_finished_goals(store: &mut WishStore) {
    let finished: Vec<usize> = store
        .goals
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.status.is_active())
        .map(|(i, _)| i)
        .collect();
    let overflow = finished.len().saturating_sub(MAX_FINISHED_GOALS);
    for index in finished.into_iter().take(overflow) {
        store.goals.remove(index);
    }
}

// ── 想法 ────────────────────────────────────────────────────────

/// 新增念头；与现有活着念头重复时只提升兴奋度
pub(crate) fn add_idea(text: &str, excitement: u8, source: WishSource) -> Option<WishIdea> {
    let text = clean_text(text)?;
    let excitement = excitement.clamp(1, 10);
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_store();

    if let Some(existing) = store
        .ideas
        .iter_mut()
        .find(|i| i.status.is_alive() && same_wish(&i.text, &text))
    {
        existing.updated_at = util::now_secs();
        existing.excitement = existing.excitement.max(excitement);
        let result = existing.clone();
        save_store(&store);
        debug!(idea_id = result.id, "wish: 念头再次冒出来");
        return Some(result);
    }

    let alive = store.ideas.iter().filter(|i| i.status.is_alive()).count();
    if alive >= MAX_IDEAS {
        // 心里太挤了：让最不兴奋的念头先退场
        if let Some(oldest) = store
            .ideas
            .iter()
            .enumerate()
            .filter(|(_, i)| i.status.is_alive())
            .min_by_key(|(_, i)| (i.excitement, i.updated_at))
            .map(|(idx, _)| idx)
        {
            store.ideas.remove(oldest);
        }
    }

    let now = util::now_secs();
    let idea = WishIdea {
        id: next_id(&mut store),
        text,
        excitement,
        status: IdeaStatus::New,
        source,
        created_at: now,
        updated_at: now,
    };
    store.ideas.push(idea.clone());
    save_store(&store);
    debug!(idea_id = idea.id, text = %idea.text, "wish: 新念头");
    Some(idea)
}

// ── 上下文呈现 ──────────────────────────────────────────────────

/// 目标的一行描述（进度/期限是机械事实，解释交给她的表达）
fn goal_line(goal: &WishGoal, with_id: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if with_id {
        parts.push(format!("#{}", goal.id));
    }
    parts.push(goal.text.clone());
    parts.push(format!("进度{}%", goal.progress));
    if let Some(deadline) = goal.deadline_secs {
        let days_left = deadline as i64 - util::now_secs() as i64;
        let time_desc = if days_left <= 0 {
            "期限已到".to_string()
        } else {
            format!("期限还有{}天", days_left / 86400)
        };
        parts.push(time_desc);
    }
    format!("- {}（{}）", parts.join(" "), goal.status.label())
}

/// 想法的一行描述
fn idea_line(idea: &WishIdea, with_id: bool) -> String {
    let id_prefix = if with_id {
        format!("#{} ", idea.id)
    } else {
        String::new()
    };
    format!("- {id_prefix}{}（兴奋度 {}）", idea.text, idea.excitement)
}

/// 渲染"你想要的"上下文块：活跃目标 + 最兴奋的念头
///
/// `with_id` 供睡前整理使用——她要能引用 id 来更新进度。
pub(crate) fn context_block(with_id: bool) -> Option<String> {
    if !config::get().humanity.wish_enabled {
        return None;
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = load_store();

    let mut goals: Vec<&WishGoal> = store
        .goals
        .iter()
        .filter(|g| g.status.is_active())
        .collect();
    if goals.is_empty() {
        return None;
    }
    // 期限近的、优先级高的排前面
    goals.sort_by_key(|g| {
        (
            g.deadline_secs.unwrap_or(u64::MAX),
            std::cmp::Reverse(g.priority),
        )
    });

    let mut lines: Vec<String> = vec!["你想要的：".to_string()];
    for goal in goals.iter().take(3) {
        lines.push(goal_line(goal, with_id));
    }

    let mut ideas: Vec<&WishIdea> = store.ideas.iter().filter(|i| i.status.is_alive()).collect();
    ideas.sort_by_key(|i| std::cmp::Reverse((i.excitement, i.updated_at)));
    for idea in ideas.iter().take(3) {
        lines.push(idea_line(idea, with_id));
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unfinished_ideas_are_alive() {
        assert!(IdeaStatus::New.is_alive());
        assert!(IdeaStatus::Developing.is_alive());
        assert!(!IdeaStatus::Realized.is_alive());
        assert!(!IdeaStatus::Dropped.is_alive());
    }

    #[test]
    fn same_wish_ignores_punctuation() {
        assert!(same_wish("想学会做提拉米苏！", "想学会做提拉米苏"));
        assert!(same_wish("去一次海边旅行", "海边旅行"));
        assert!(!same_wish("学会做提拉米苏", "整理书架"));
    }

    #[test]
    fn clean_text_rejects_empty_and_overlong() {
        assert!(clean_text("  ").is_none());
        assert!(clean_text("想去极光").is_some());
        let long = "想".repeat(81);
        assert!(clean_text(&long).is_none());
    }

    #[test]
    fn goal_due_requires_deadline_and_cooldown() {
        let now = 1_000_000_u64;
        let make = |deadline: Option<u64>, notified: Option<u64>| WishGoal {
            id: 1,
            text: "测试".into(),
            parent_id: None,
            priority: 5,
            progress: 0,
            deadline_secs: deadline,
            status: GoalStatus::Active,
            source: WishSource::Reflection,
            created_at: 0,
            updated_at: 0,
            last_notified_at: notified,
        };
        // 无期限不提醒
        assert!(!goal_due(&make(None, None), now));
        // 期限已到 + 未提醒 → 提醒
        assert!(goal_due(&make(Some(now - 1), None), now));
        // 期限未到不提醒
        assert!(!goal_due(&make(Some(now + 100), None), now));
        // 刚提醒过不再提醒
        assert!(!goal_due(&make(Some(now - 1), Some(now - 60)), now));
        // 提醒已过冷却 → 再提醒
        assert!(goal_due(
            &make(Some(now - 1), Some(now - NOTIFY_COOLDOWN_SECS)),
            now
        ));
    }

    #[test]
    fn context_block_renders_progress_and_deadline() {
        let goal = WishGoal {
            id: 3,
            text: "学会做提拉米苏".into(),
            parent_id: None,
            priority: 6,
            progress: 40,
            deadline_secs: Some(util::now_secs() + 3 * 86400),
            status: GoalStatus::Active,
            source: WishSource::Reflection,
            created_at: 0,
            updated_at: 0,
            last_notified_at: None,
        };
        let line = goal_line(&goal, false);
        assert!(line.contains("学会做提拉米苏"));
        assert!(line.contains("进度40%"));
        assert!(line.contains("期限还有3天"));
        let with_id = goal_line(&goal, true);
        assert!(with_id.contains("#3"));
    }

    #[test]
    fn idea_line_carries_excitement() {
        let idea = WishIdea {
            id: 7,
            text: "去看一次极光".into(),
            excitement: 8,
            status: IdeaStatus::New,
            source: WishSource::Dream,
            created_at: 0,
            updated_at: 0,
        };
        let line = idea_line(&idea, false);
        assert!(line.contains("去看一次极光"));
        assert!(line.contains("兴奋度 8"));
    }
}
