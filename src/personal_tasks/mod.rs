//! 个人任务系统：把计划和社交承诺转化为可推进的执行状态。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::debug;

const MAX_ACTIVE_TASKS: usize = 30;
const MAX_FINISHED_TASKS: usize = 20;
const FOLLOW_UP_DELAY_SECS: u64 = 4 * 3600;
const MAX_FOLLOW_UPS: u8 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    WaitingForPerson,
    Completed,
    Snoozed,
    Abandoned,
}

impl TaskStatus {
    fn is_active(self) -> bool {
        matches!(
            self,
            Self::Pending | Self::InProgress | Self::WaitingForPerson | Self::Snoozed
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "待开始",
            Self::InProgress => "进行中",
            Self::WaitingForPerson => "等对方",
            Self::Completed => "已完成",
            Self::Snoozed => "暂缓",
            Self::Abandoned => "已放弃",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalTask {
    pub id: u64,
    pub title: String,
    pub source: String,
    pub status: TaskStatus,
    pub next_action: String,
    pub priority: u8,
    pub associated_user: u64,
    pub associated_group: u64,
    pub review_at: u64,
    pub last_pushed_at: u64,
    pub follow_up_count: u8,
    pub blocker: String,
    pub progress: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskStore {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    tasks: Vec<PersonalTask>,
}

fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("personal_tasks.json")
}

fn load() -> TaskStore {
    fs::read_to_string(store_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save(store: &TaskStore) {
    if let Ok(content) = serde_json::to_string_pretty(store) {
        fs::write(store_path(), content).ok();
    }
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(ch))
        .collect::<String>()
        .to_lowercase()
}

fn titles_similar(left: &str, right: &str) -> bool {
    let left = normalized(left);
    let right = normalized(right);
    left.len() >= 4 && right.len() >= 4 && (left.contains(&right) || right.contains(&left))
}

/// 新增任务；相同未完成事项只补充进展，避免把同一个念头拆成多项任务。
pub fn add_or_reinforce(
    title: &str,
    source: &str,
    next_action: &str,
    associated_user: u64,
    associated_group: u64,
) -> Option<PersonalTask> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 80 {
        return None;
    }

    let now = crate::util::now_secs();
    let mut store = load();
    if let Some(task) = store.tasks.iter_mut().find(|task| {
        task.status.is_active()
            && task.associated_user == associated_user
            && task.associated_group == associated_group
            && titles_similar(&task.title, title)
    }) {
        task.updated_at = now;
        if !next_action.trim().is_empty() {
            task.next_action = next_action.trim().to_string();
        }
        task.progress.push(format!("再次确认：{}", title));
        task.progress.truncate(8);
        let result = task.clone();
        save(&store);
        return Some(result);
    }

    let active_count = store
        .tasks
        .iter()
        .filter(|task| task.status.is_active())
        .count();
    if active_count >= MAX_ACTIVE_TASKS {
        debug!(title, "personal_tasks: active task limit reached");
        return None;
    }

    store.next_id += 1;
    let task = PersonalTask {
        id: store.next_id,
        title: title.to_string(),
        source: source.to_string(),
        status: if associated_user > 0 {
            TaskStatus::InProgress
        } else {
            TaskStatus::Pending
        },
        next_action: next_action.trim().to_string(),
        priority: 1,
        associated_user,
        associated_group,
        review_at: now,
        last_pushed_at: 0,
        follow_up_count: 0,
        blocker: String::new(),
        progress: vec![format!("创建：{}", title)],
        created_at: now,
        updated_at: now,
    };
    store.tasks.push(task.clone());
    save(&store);
    debug!(task_id = task.id, title, "personal_tasks: created");
    Some(task)
}

/// 收到关联对象的新消息。等待任务被真正的外部事件推进，而不是自动猜测完成。
pub fn note_user_message(user_id: u64, group_id: u64, message: &str) {
    let mut store = load();
    let now = crate::util::now_secs();
    let mut changed = false;
    for task in &mut store.tasks {
        if task.status == TaskStatus::WaitingForPerson
            && task.associated_user == user_id
            && task.associated_group == group_id
        {
            task.status = TaskStatus::InProgress;
            task.review_at = now;
            task.updated_at = now;
            task.blocker.clear();
            task.progress.push(format!(
                "对方回复：{}",
                message.chars().take(40).collect::<String>()
            ));
            task.progress.truncate(8);
            changed = true;
        }
    }
    if changed {
        save(&store);
        debug!(
            user_id,
            group_id, "personal_tasks: waiting task resumed by reply"
        );
    }
}

/// 到期任务不会凭空完成：等待超过重试上限则搁置，其余任务回到待决定状态。
pub fn review_due_tasks() {
    let mut store = load();
    let now = crate::util::now_secs();
    let mut changed = false;
    for task in &mut store.tasks {
        if !task.status.is_active() || task.review_at > now {
            continue;
        }
        match task.status {
            TaskStatus::WaitingForPerson if task.follow_up_count >= MAX_FOLLOW_UPS => {
                task.status = TaskStatus::Snoozed;
                task.blocker = "已跟进两次，先别催了".to_string();
                task.review_at = now + 24 * 3600;
                task.progress.push("暂缓：等待对方回应".to_string());
                changed = true;
            }
            TaskStatus::WaitingForPerson => {
                task.status = TaskStatus::InProgress;
                task.next_action = "决定是否自然地跟进一次".to_string();
                task.progress.push("等待超时：需要决定下一步".to_string());
                changed = true;
            }
            TaskStatus::Snoozed => {
                task.status = TaskStatus::Pending;
                task.progress.push("重新考虑：是否继续处理".to_string());
                changed = true;
            }
            _ => {}
        }
        if changed {
            task.updated_at = now;
            task.progress.truncate(8);
        }
    }
    if changed {
        retain_finished(&mut store);
        save(&store);
    }
}

/// 返回当前会话可自然跟进的一项社交任务。
pub fn due_social_follow_up(user_id: u64, group_id: u64) -> Option<PersonalTask> {
    let now = crate::util::now_secs();
    load()
        .tasks
        .into_iter()
        .filter(|task| {
            task.status == TaskStatus::InProgress
                && task.associated_user == user_id
                && task.associated_group == group_id
                && task.review_at <= now
                && task.follow_up_count < MAX_FOLLOW_UPS
                && !task.next_action.is_empty()
        })
        .max_by_key(|task| (task.priority, std::cmp::Reverse(task.updated_at)))
}

/// 只有发送成功后才转入等待状态，避免生成失败也消耗一次跟进机会。
pub fn mark_follow_up_sent(task_id: u64) {
    let mut store = load();
    let now = crate::util::now_secs();
    if let Some(task) = store.tasks.iter_mut().find(|task| task.id == task_id) {
        if task.status != TaskStatus::InProgress || task.follow_up_count >= MAX_FOLLOW_UPS {
            return;
        }
        task.status = TaskStatus::WaitingForPerson;
        task.follow_up_count += 1;
        task.last_pushed_at = now;
        task.review_at = now + FOLLOW_UP_DELAY_SECS;
        task.progress.push("已自然跟进，等待回应".to_string());
        task.progress.truncate(8);
        task.updated_at = now;
        save(&store);
    }
}

fn mark_waiting_for_person(task_id: u64) {
    let mut store = load();
    let now = crate::util::now_secs();
    if let Some(task) = store.tasks.iter_mut().find(|task| task.id == task_id) {
        if task.associated_user == 0 {
            return;
        }
        task.status = TaskStatus::WaitingForPerson;
        task.review_at = now + FOLLOW_UP_DELAY_SECS;
        task.blocker = "等对方回应".to_string();
        task.updated_at = now;
        task.progress.push("已约定，等待对方回应".to_string());
        task.progress.truncate(8);
        save(&store);
    }
}

/// 将匹配的执行任务标记完成，并保留完成记录供后续反思使用。
pub fn complete_by_title(title: &str, progress: &str) -> bool {
    let mut store = load();
    let now = crate::util::now_secs();
    let Some(task) = store
        .tasks
        .iter_mut()
        .find(|task| task.status.is_active() && titles_similar(&task.title, title))
    else {
        return false;
    };

    task.status = TaskStatus::Completed;
    task.next_action.clear();
    task.blocker.clear();
    task.review_at = 0;
    task.updated_at = now;
    task.progress.push(format!("完成：{}", progress));
    task.progress.truncate(8);
    retain_finished(&mut store);
    save(&store);
    true
}

pub fn get_context(max_count: usize) -> String {
    let now = crate::util::now_secs();
    let mut tasks: Vec<PersonalTask> = load()
        .tasks
        .into_iter()
        .filter(|task| {
            task.status.is_active() && !(task.status == TaskStatus::Snoozed && task.review_at > now)
        })
        .collect();
    tasks.sort_by_key(|task| std::cmp::Reverse(task.updated_at));
    let lines: Vec<String> = tasks
        .iter()
        .take(max_count)
        .map(|task| {
            let progress = task
                .progress
                .last()
                .map(String::as_str)
                .unwrap_or("尚未开始");
            format!(
                "- #{} {}（{}）：{}；下一步：{}",
                task.id,
                task.title,
                task.status.label(),
                progress,
                task.next_action
            )
        })
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        format!("# 正在推进的事\n{}", lines.join("\n"))
    }
}

/// 从一次真实对话中提取 bot 的明确事项。模型只能给出候选，本模块负责绑定对象和存储。
pub fn extract_from_conversation(user_id: u64, group_id: u64, user_message: &str, bot_reply: &str) {
    let context = format!(
        "# 当前任务\n{}\n\n# 对方刚说\n{}\n\n# 你刚回复\n{}",
        get_context(10),
        user_message,
        bot_reply,
    );
    let result = crate::ai::analyze_with_tools(
        crate::prompt::PromptManager::get().raw("task_progress"),
        &context,
        &[crate::ai::task_progress_tool()],
        Some(serde_json::json!("auto")),
    );
    let Ok(parsed) = result else {
        return;
    };
    let Some(tasks) = parsed.get("tasks").and_then(|value| value.as_array()) else {
        return;
    };
    for candidate in tasks.iter().take(2) {
        let title = candidate.get("title").and_then(|value| value.as_str()).unwrap_or("");
        let next_action = candidate.get("next_action").and_then(|value| value.as_str()).unwrap_or("");
        let waiting = candidate
            .get("waiting_for_person")
            .and_then(crate::ai::parse_bool)
            .unwrap_or(false);
        if title.is_empty() || next_action.is_empty() {
            continue;
        }
        let task_user = if waiting { user_id } else { 0 };
        let task_group = if waiting { group_id } else { 0 };
        if let Some(task) = add_or_reinforce(title, "conversation", next_action, task_user, task_group) {
            if waiting {
                mark_waiting_for_person(task.id);
            }
        }
    }
}

fn retain_finished(store: &mut TaskStore) {
    let mut finished = store
        .tasks
        .iter()
        .filter(|task| !task.status.is_active())
        .count();
    while finished > MAX_FINISHED_TASKS {
        if let Some(index) = store.tasks.iter().position(|task| !task.status.is_active()) {
            store.tasks.remove(index);
            finished -= 1;
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_titles_are_merged() {
        assert!(titles_similar("明天约小王见面", "约小王见面"));
        assert!(!titles_similar("去吃饭", "整理书架"));
    }

    #[test]
    fn only_due_in_progress_social_task_can_follow_up() {
        let task = PersonalTask {
            id: 1,
            title: "确认明天见面".to_string(),
            source: "test".to_string(),
            status: TaskStatus::InProgress,
            next_action: "问问对方时间".to_string(),
            priority: 1,
            associated_user: 2,
            associated_group: 3,
            review_at: 1,
            last_pushed_at: 0,
            follow_up_count: 0,
            blocker: String::new(),
            progress: Vec::new(),
            created_at: 1,
            updated_at: 1,
        };
        assert!(task.status == TaskStatus::InProgress && task.review_at <= crate::util::now_secs());
        assert!(task.follow_up_count < MAX_FOLLOW_UPS);
    }
}
