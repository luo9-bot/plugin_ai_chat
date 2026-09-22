//! 计划的统一领域模型与存储
//!
//! 这里只有**一种**计划条目：[`PlanItem`]。日、周、月三种跨度是同一个模型
//! 的一个字段，而不是三套并行的结构。
//!
//! 之前是三套各活各的：日计划生成完就沉淀（不进任何推进系统、界面上也看
//! 不到），周/月计划能被推动但从不落成可推进的事项，而"完成"的唯一自动
//! 入口是把她自己说出的话与计划文本做**字面整句包含**匹配——三天日志里
//! 该路径 0 次命中，所以永远不会有任何勾选。
//!
//! 现在她通过带 id 的工具自己判断并落笔（见 [`set_status`]）：
//! 判断交给她，歧义由 id 消除，不再靠猜文本。

use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tracing::{debug, info};

use crate::config;

static PLAN_LOCK: Mutex<()> = Mutex::new(());

/// 计划跨度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Timeframe {
    /// 今日
    Day,
    /// 本周
    Week,
    /// 本月
    Month,
}

impl Timeframe {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Day => "今日",
            Self::Week => "本周",
            Self::Month => "本月",
        }
    }

    /// 全部跨度，按"越近越先说"排序
    pub(crate) const ALL: [Timeframe; 3] = [Timeframe::Day, Timeframe::Week, Timeframe::Month];

    /// 稳定前缀，用于让她用 id 指认时不产生歧义
    pub(crate) fn id_prefix(self) -> &'static str {
        match self {
            Self::Day => "d",
            Self::Week => "w",
            Self::Month => "m",
        }
    }
}

/// 一条计划事项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PlanItem {
    /// 稳定短 id（如 `d3` / `w1` / `m2`）——她引用它来落笔
    pub id: String,
    pub content: String,
    pub timeframe: Timeframe,
    pub completed: bool,
    #[serde(default)]
    pub completed_at: u64,
    /// 她留下的进展（她自己的话，最新的在后）
    #[serde(default)]
    pub progress: Vec<String>,
    /// 完成时她留的一句话（"做完了" / "这次先算了"）
    #[serde(default)]
    pub completion_note: String,
    /// 周计划专用：安排在哪一天（"Monday"…），其它跨度为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_day: Option<String>,
    #[serde(default)]
    pub created_at: u64,
}

/// 每条计划保留的进展行数上限
const MAX_PROGRESS_LINES: usize = 8;
/// 单条内容长度上限（prompt 里明说每条不超过 15 字，这里放宽兜底）
const MAX_CONTENT_CHARS: usize = 60;

impl PlanItem {
    /// 是否还值得推进（未完成）
    pub(crate) fn is_open(&self) -> bool {
        !self.completed
    }

    /// 一行用于 prompt 的渲染
    pub(crate) fn render_line(&self) -> String {
        let mark = if self.completed { "✓" } else { "·" };
        let note = if self.completion_note.is_empty() {
            String::new()
        } else {
            format!("（{}）", self.completion_note)
        };
        let progress = self
            .progress
            .last()
            .map(|p| format!("；进展：{p}"))
            .unwrap_or_default();
        format!(
            "{mark} [{}] {}{note}{progress}",
            self.id,
            self.content.trim()
        )
    }
}

/// 一个跨度的计划（落盘单元）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Plan {
    /// 周期标识：日报日期 / 周一日期 / 年月
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub items: Vec<PlanItem>,
    #[serde(default)]
    pub created_at: u64,
    /// 周反思摘要（周计划专用）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reflection: String,
}

// ── 文件路径 ────────────────────────────────────────────────────

fn path_of(timeframe: Timeframe) -> std::path::PathBuf {
    let name = match timeframe {
        Timeframe::Day => "daily_plan.json",
        Timeframe::Week => "weekly_plan.json",
        Timeframe::Month => "monthly_plan.json",
    };
    config::data_dir().join(name)
}

fn load(timeframe: Timeframe) -> Plan {
    fs::read_to_string(path_of(timeframe))
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn save(timeframe: Timeframe, plan: &Plan) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(plan)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    crate::util::atomic_write(path_of(timeframe), json)
}

/// 当前周期标识
fn current_period(timeframe: Timeframe) -> String {
    match timeframe {
        Timeframe::Day => crate::util::today_str(),
        Timeframe::Week => crate::util::monday_of_week_str(),
        Timeframe::Month => crate::util::ts_to_month_str(crate::util::now_secs()),
    }
}

// ── 生成 ────────────────────────────────────────────────────────

/// 生成失败后的重试间隔
///
/// 空计划会被判定为"需要生成"，但生成可能失败或返回空。若不设间隔，
/// 每次周期检查都会再调一次模型——检查是每分钟跑的，那就是一分钟一次
/// AI 调用。给一个间隔，既保留重试，又不至于把调用打爆。
const GENERATION_RETRY_SECS: u64 = 30 * 60;

/// 若当前周期还没有内容则返回"需要 AI 生成"
///
/// 三种情况需要生成：
/// - 周期对不上（跨天/跨周/跨月了）：开一份新的空计划
/// - 条目为空且距上次尝试已过 [`GENERATION_RETRY_SECS`]：上次没生成出内容，重试
///
/// 条目为空但刚试过 → 不重试，等间隔到。
pub(crate) fn ensure_plan(timeframe: Timeframe) -> bool {
    let plan = load(timeframe);
    let period = current_period(timeframe);
    if plan.period == period && !plan.items.is_empty() {
        return false;
    }

    let now = crate::util::now_secs();
    if plan.period == period {
        // 同一周期内、条目为空：属于"上次生成没出内容"，按间隔重试
        if now.saturating_sub(plan.created_at) < GENERATION_RETRY_SECS {
            return false;
        }
        let retry = Plan {
            period,
            items: Vec::new(),
            created_at: now,
            reflection: plan.reflection,
        };
        if let Err(error) = save(timeframe, &retry) {
            tracing::warn!(%error, "schedule: retry state could not be saved");
            return false;
        }
        debug!(
            timeframe = timeframe.label(),
            "schedule: 上次生成没出内容，重试"
        );
        return true;
    }

    // 跨周期：开一份新的
    if let Err(error) = save(
        timeframe,
        &Plan {
            period,
            items: Vec::new(),
            created_at: now,
            reflection: String::new(),
        },
    ) {
        tracing::warn!(%error, "schedule: period state could not be saved");
        return false;
    }
    debug!(
        timeframe = timeframe.label(),
        "schedule: 新周期，等待生成计划"
    );
    true
}

/// 用生成结果替换当前周期的计划条目
///
/// 生成是整体替换而不是追加：跨周/跨月时旧条目已在 `ensure_plan` 里清空，
/// 同日重跑也不该把同一批目标堆两份。
pub(crate) fn replace_items(timeframe: Timeframe, generated: Vec<GeneratedItem>) {
    let mut plan = load(timeframe);
    if plan.period != current_period(timeframe) {
        // 生成期间跨了周期：以当前周期为准重开
        plan = Plan {
            period: current_period(timeframe),
            created_at: crate::util::now_secs(),
            ..Default::default()
        };
    }
    let now = crate::util::now_secs();
    plan.items = generated
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let content = item.content.trim();
            if content.is_empty() || content.chars().count() > MAX_CONTENT_CHARS {
                return None;
            }
            Some(PlanItem {
                id: format!("{}{}", timeframe.id_prefix(), index + 1),
                content: content.to_string(),
                timeframe,
                completed: false,
                completed_at: 0,
                progress: Vec::new(),
                completion_note: String::new(),
                target_day: item.target_day,
                created_at: now,
            })
        })
        .collect();
    let count = plan.items.len();
    if let Err(error) = save(timeframe, &plan) {
        tracing::warn!(%error, "schedule: generated plan could not be saved");
        return;
    }
    info!(timeframe = timeframe.label(), count, "schedule: 计划已生成");
}

/// AI 生成结果里的一条（还没有 id）
#[derive(Debug, Clone)]
pub(crate) struct GeneratedItem {
    pub content: String,
    pub target_day: Option<String>,
}

// ── 读取 ────────────────────────────────────────────────────────

/// 某个跨度的计划（admin 展示用）
pub(crate) fn plan_of(timeframe: Timeframe) -> Plan {
    load(timeframe)
}

/// 当前周期仍未完成的条目
pub(crate) fn open_items(timeframe: Timeframe) -> Vec<PlanItem> {
    load(timeframe)
        .items
        .into_iter()
        .filter(PlanItem::is_open)
        .collect()
}

/// 跨全部跨度的未完成条目，供她一眼看全并指认
///
/// 排序：今日 → 本周 → 本月，各自保持生成顺序。
pub(crate) fn open_items_all() -> Vec<PlanItem> {
    Timeframe::ALL.into_iter().flat_map(open_items).collect()
}

/// 渲染成给她看的一行行文本；没有未完成事项时返回 None
///
/// 只给未完成的：已勾掉的再列一遍会诱导她重复决定"要不要做"。
/// 上限 `max` 条，避免计划变长后把 prompt 撑爆。
pub(crate) fn render_open_items(max: usize) -> Option<String> {
    let items = open_items_all();
    if items.is_empty() {
        return None;
    }
    let lines: Vec<String> = items
        .iter()
        .take(max)
        .map(|item| format!("{} {}", item.timeframe.label(), item.render_line()))
        .collect();
    Some(lines.join("\n"))
}

// ── 她自己的落笔 ────────────────────────────────────────────────

/// 状态变更的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SetStatusOutcome {
    /// 落笔成功
    Applied {
        id: String,
        content: String,
        completed: bool,
    },
    /// 找不到这个 id
    UnknownId,
    PersistenceFailed(String),
}

/// 写回一条计划的状态（她通过工具调用这里）
///
/// `completed = Some(true)` 勾选完成，`Some(false)` 取消勾选，
/// `None` 只推进展不动完成状态。
pub(crate) fn set_status(
    item_id: &str,
    completed: Option<bool>,
    note: &str,
    progress: &str,
) -> SetStatusOutcome {
    let _guard = PLAN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let wanted = item_id.trim().to_ascii_lowercase();
    for timeframe in Timeframe::ALL {
        let mut plan = load(timeframe);
        if plan.period != current_period(timeframe) { continue; }
        let Some(index) = plan.items.iter().position(|item| item.id == wanted) else {
            continue;
        };
        let now = crate::util::now_secs();
        {
            let item = &mut plan.items[index];
            if let Some(done) = completed {
                item.completed = done;
                item.completed_at = if done { if item.completed_at == 0 { now } else { item.completed_at } } else { 0 };
                if done && !note.trim().is_empty() {
                    item.completion_note = note.trim().to_string();
                }
                if !done {
                    item.completion_note.clear();
                }
            }
            if !progress.trim().is_empty() {
                item.progress.push(progress.trim().to_string());
                let excess = item.progress.len().saturating_sub(MAX_PROGRESS_LINES);
                item.progress.drain(..excess);
            }
        }
        let item = plan.items[index].clone();
        if let Err(error) = save(timeframe, &plan) {
            return SetStatusOutcome::PersistenceFailed(error.to_string());
        }
        if let Some(done) = completed {
            record_activity_log(&item, done);
        }
        info!(
            id = %item.id,
            timeframe = timeframe.label(),
            completed = item.completed,
            content = %item.content,
            "schedule: 她自己改了计划状态"
        );
        return SetStatusOutcome::Applied {
            id: item.id,
            content: item.content,
            completed: item.completed,
        };
    }
    SetStatusOutcome::UnknownId
}

/// 她给自己加一条计划（当日）
pub(crate) fn add_own_item(content: &str) -> Option<PlanItem> {
    let content = content.trim();
    if content.is_empty() || content.chars().count() > MAX_CONTENT_CHARS {
        return None;
    }
    let mut plan = load(Timeframe::Day);
    if plan.period != current_period(Timeframe::Day) {
        plan = Plan {
            period: current_period(Timeframe::Day),
            created_at: crate::util::now_secs(),
            ..Default::default()
        };
    }
    if plan
        .items
        .iter()
        .any(|item| item.is_open() && item.content == content)
    {
        return None;
    }
    let next = plan.items.len() + 1;
    let item = PlanItem {
        id: format!("{}{}", Timeframe::Day.id_prefix(), next),
        content: content.to_string(),
        timeframe: Timeframe::Day,
        completed: false,
        completed_at: 0,
        progress: Vec::new(),
        completion_note: String::new(),
        target_day: None,
        created_at: crate::util::now_secs(),
    };
    plan.items.push(item.clone());
    if let Err(error) = save(Timeframe::Day, &plan) {
        tracing::warn!(%error, "schedule: new item could not be saved");
        return None;
    }
    info!(id = %item.id, content, "schedule: 她给自己加了一件事");
    Some(item)
}

// ── 推动历史（admin 展示） ──────────────────────────────────────

fn push_history_path() -> std::path::PathBuf {
    config::data_dir().join("push_history.json")
}

/// 记一笔计划状态变更，供管理页回看
pub(crate) fn record_activity_log(item: &PlanItem, completed: bool) {
    let path = push_history_path();
    let mut history: Vec<serde_json::Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    history.push(serde_json::json!({
        "time": crate::util::now_secs(),
        "kind": format!("{}计划{}", item.timeframe.label(), if completed { "完成" } else { "取消完成" }),
        "content": item.content,
    }));
    if history.len() > 200 {
        history.drain(0..history.len() - 200);
    }
    if let Ok(json) = serde_json::to_string_pretty(&history)
        && let Err(error) = crate::util::atomic_write(path, json.as_bytes())
    {
        tracing::warn!(error = %error, "状态写盘失败");
    }
}

/// 推动历史（admin 读取）
pub(crate) fn push_history() -> Vec<serde_json::Value> {
    fs::read_to_string(push_history_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::MutexExt;

    /// 落盘路径的测试共用同一个 `data_dir`（进程内只有一个），必须串行。
    ///
    /// 这两个测试都会调 `config::init()` 并写真实的计划文件；并发跑会互相
    /// 覆盖，表现为偶发失败（实测约每十几次一次）。这不是实现的问题，
    /// 但偶发红灯会让人不再相信红灯，所以在这里显式串行。
    static DISK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── 纯函数部分：不碰文件系统 ──────────────────────────────

    fn item(id: &str, content: &str, timeframe: Timeframe) -> PlanItem {
        PlanItem {
            id: id.to_string(),
            content: content.to_string(),
            timeframe,
            completed: false,
            completed_at: 0,
            progress: Vec::new(),
            completion_note: String::new(),
            target_day: None,
            created_at: 0,
        }
    }

    #[test]
    fn render_line_shows_id_and_progress() {
        let mut it = item("d2", "整理书架", Timeframe::Day);
        it.progress.push("先收拾了上层".to_string());
        let line = it.render_line();
        assert!(line.contains("[d2]"), "要带 id 供她指认：{line}");
        assert!(line.contains("整理书架"));
        assert!(line.contains("先收拾了上层"));
    }

    #[test]
    fn completed_item_renders_a_check() {
        let mut it = item("w1", "给豆发消息", Timeframe::Week);
        it.completed = true;
        it.completion_note = "发过了".to_string();
        let line = it.render_line();
        assert!(line.starts_with('✓'), "{line}");
        assert!(line.contains("发过了"));
    }

    #[test]
    fn timeframes_have_distinct_id_prefixes() {
        // 前缀区分跨度，避免她指认 d1 时歧义到 w1
        assert_eq!(Timeframe::Day.id_prefix(), "d");
        assert_eq!(Timeframe::Week.id_prefix(), "w");
        assert_eq!(Timeframe::Month.id_prefix(), "m");
    }

    #[test]
    fn open_items_excludes_completed() {
        let mut plan = Plan {
            period: "2026-01-01".to_string(),
            ..Default::default()
        };
        plan.items.push(item("d1", "做完的", Timeframe::Day));
        plan.items.push(item("d2", "没做完的", Timeframe::Day));
        plan.items[0].completed = true;
        let open: Vec<&PlanItem> = plan.items.iter().filter(|i| i.is_open()).collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, "d2");
    }

    #[test]
    fn missing_optional_fields_still_load() {
        // 落盘的旧文件里没有 progress / completion_note / target_day，
        // 反序列化不能因此整份失败
        let raw = r#"{
            "period": "2026-05-28",
            "items": [
                {"id": "d1", "content": "整理书架", "timeframe": "day", "completed": false}
            ]
        }"#;
        let plan: Plan = serde_json::from_str(raw).expect("缺省字段应当有默认值");
        assert_eq!(plan.items.len(), 1);
        assert!(plan.items[0].progress.is_empty());
        assert!(plan.items[0].completion_note.is_empty());
        assert!(plan.items[0].target_day.is_none());
        assert!(plan.reflection.is_empty());
    }

    // ── 落盘路径：一个顺序场景 ────────────────────────────────
    //
    // 存储层写的是同一个 data_dir（生产里本来就只有一份计划），
    // 所以这些步骤必须在同一个测试里顺序执行：拆成多个 #[test]
    // 会并发写同一批文件，互相覆盖——那是测试的问题，不是实现的。
    //
    // 这条场景正是原来坏掉的地方：勾选从来不会发生，也不会存活。

    /// 把某个跨度重置成一份确定的计划
    fn seed(timeframe: Timeframe, contents: &[&str]) {
        replace_items(
            timeframe,
            contents
                .iter()
                .map(|c| GeneratedItem {
                    content: (*c).to_string(),
                    target_day: None,
                })
                .collect(),
        );
    }

    #[test]
    fn plan_lifecycle_end_to_end() {
        let _serial = DISK_LOCK.lock_recover();
        crate::config::init();

        // ── 生成：替换而不是追加，空/超长条目被丢掉 ──
        seed(Timeframe::Day, &["第一版"]);
        seed(
            Timeframe::Day,
            &[
                "整理书架",
                "   ",
                &"长".repeat(MAX_CONTENT_CHARS + 1),
                "泡一壶新茶",
            ],
        );
        let day_items = plan_of(Timeframe::Day).items;
        assert_eq!(day_items.len(), 2, "重复生成不该堆旧目标，空/超长要丢掉");
        assert_eq!(day_items[0].content, "整理书架");
        assert_eq!(day_items[0].id, "d1");

        // ── 生成时机：当前周期有条目就不重开，跨周期才重开 ──
        assert!(!ensure_plan(Timeframe::Day), "当前周期且有条目，不该重开");
        // 生成失败（条目为空）后短期内不重试，避免每分钟调一次模型
        replace_items(Timeframe::Day, Vec::new());
        assert!(
            !ensure_plan(Timeframe::Day),
            "刚试过生成且为空，应当等重试间隔而不是立刻再调模型"
        );
        seed(Timeframe::Day, &["整理书架", "泡一壶新茶"]);

        // ── 勾选：跨调用存活，并且离开未完成清单 ──
        let target = open_items(Timeframe::Day)[0].id.clone();
        assert!(matches!(
            set_status(&target, Some(true), "收拾完了", ""),
            SetStatusOutcome::Applied {
                completed: true,
                ..
            }
        ));
        let after = open_items(Timeframe::Day);
        assert_eq!(after.len(), 1, "勾掉的那条不该还留在未完成里");
        assert!(after.iter().all(|i| i.id != target));

        let done = plan_of(Timeframe::Day)
            .items
            .into_iter()
            .find(|i| i.id == target)
            .expect("完成记录必须留在计划文件里");
        assert!(done.completed);
        assert_eq!(done.completion_note, "收拾完了");
        assert!(done.completed_at > 0);

        // ── 取消勾选：回到未完成，且清掉完成痕迹 ──
        assert!(matches!(
            set_status(&target, Some(false), "", ""),
            SetStatusOutcome::Applied {
                completed: false,
                ..
            }
        ));
        assert!(
            open_items(Timeframe::Day).iter().any(|i| i.id == target),
            "取消勾选后应回到未完成"
        );
        let back = plan_of(Timeframe::Day)
            .items
            .into_iter()
            .find(|i| i.id == target)
            .unwrap();
        assert_eq!(back.completed_at, 0, "取消勾选要清掉完成时间");
        assert!(back.completion_note.is_empty(), "取消勾选要清掉完成备注");

        // ── 记进展：不动完成状态 ──
        set_status(&target, None, "", "练了半小时音阶");
        let progressed = plan_of(Timeframe::Day)
            .items
            .into_iter()
            .find(|i| i.id == target)
            .unwrap();
        assert!(!progressed.completed, "记进展不该顺手把事勾掉");
        assert_eq!(progressed.progress, vec!["练了半小时音阶".to_string()]);

        // ── 未知 id 要被报出来，不能静默当作成功 ──
        assert_eq!(
            set_status("d99", Some(true), "", ""),
            SetStatusOutcome::UnknownId
        );
        assert_eq!(
            set_status("", Some(true), "", ""),
            SetStatusOutcome::UnknownId
        );

        // ── 她自己加事：落在今日，重复/空白/超长都拒绝 ──
        let added = add_own_item("临时想起来的事").expect("应当能加进今日清单");
        assert_eq!(added.timeframe, Timeframe::Day);
        assert!(!added.completed);
        assert!(add_own_item("临时想起来的事").is_none(), "重复内容不再加");
        assert!(add_own_item("   ").is_none());
        assert!(add_own_item(&"长".repeat(MAX_CONTENT_CHARS + 1)).is_none());

        // ── 跨跨度：三个跨度各有独立编号 ──
        seed(Timeframe::Week, &["本周的事"]);
        seed(Timeframe::Month, &["本月的事"]);
        assert_eq!(open_items(Timeframe::Week)[0].id, "w1");
        assert_eq!(open_items(Timeframe::Month)[0].id, "m1");

        // ── 递给她看的清单：覆盖三个跨度、带标签、受上限约束 ──
        let rendered = render_open_items(20).expect("有未完成事项就该渲染出清单");
        for needle in ["[d", "[w1]", "[m1]", "今日", "本周", "本月"] {
            assert!(rendered.contains(needle), "清单缺少 {needle}：\n{rendered}");
        }
        assert_eq!(
            render_open_items(1).unwrap().lines().count(),
            1,
            "上限要生效"
        );

        // ── 全部做完时不该再催她 ──
        for tf in Timeframe::ALL {
            for id in open_items(tf).into_iter().map(|i| i.id) {
                set_status(&id, Some(true), "", "");
            }
        }
        assert!(
            render_open_items(20).is_none(),
            "没有未完成事项时不该渲染出清单"
        );
    }

    #[test]
    fn stale_period_is_discarded_on_regeneration() {
        let _serial = DISK_LOCK.lock_recover();
        crate::config::init();
        seed(Timeframe::Week, &["上周的事"]);
        let mut stale = Plan {
            period: "1999-01-04".to_string(),
            ..plan_of(Timeframe::Week)
        };
        stale.items = plan_of(Timeframe::Week).items;
        save(Timeframe::Week, &stale).expect("测试计划应能写盘");

        assert!(ensure_plan(Timeframe::Week), "跨周期需要重新生成");
        assert!(
            plan_of(Timeframe::Week).items.is_empty(),
            "跨周期后不该留着上一周期的条目"
        );
        assert_eq!(
            plan_of(Timeframe::Week).period,
            current_period(Timeframe::Week)
        );
    }
}
