//! 回神循环与意图堆：她的自卷闹钟
//!
//! 定时器只兑现她的意愿，不产生意愿（方案书 §六）：
//! 意图堆里存的全是回神时她自己留下的"想起"（WakePlan），
//! 加上教养兜底的每日睡前整理。夜间是睡眠不是免打扰——
//! 免打扰时段内所有到期想起静默挂起，晨间首次回神一并处理。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info, warn};

use crate::config;
use crate::util;
use crate::voice::WakeTurn;

/// 意图堆存储路径
const WAKE_FILE: &str = "wake.json";
/// 意图堆容量上限：她不会同时惦记一百件事
const MAX_PLANS: usize = 32;

// ── 模型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeKind {
    /// 走神/想起：可以说话也可以只是想想
    Idle,
    /// 睡前整理：教养兜底的每日回神，只整理不发言
    Digest,
}

/// 一个"想起"：她在某次回神里留给未来的自己
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakePlan {
    pub id: u64,
    pub kind: WakeKind,
    /// 到期时间（unix 秒）
    pub due_at: u64,
    /// 她的原话："睡前把今天过一遍" / "看看群里在聊什么"
    pub reason: String,
    /// 关于谁（条件触发的线索来源）
    pub about_user: Option<u64>,
    /// 发言目标（群）；Digest 为空
    pub target_group: Option<u64>,
    /// 发言目标（私聊用户）
    pub target_user: Option<u64>,
    pub created_at: u64,
}

impl WakePlan {
    pub fn new(kind: WakeKind, due_at: u64, reason: impl Into<String>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = util::now_millis() ^ NEXT_ID.fetch_add(1, Ordering::Relaxed);
        WakePlan {
            id,
            kind,
            due_at,
            reason: reason.into(),
            about_user: None,
            target_group: None,
            target_user: None,
            created_at: util::now_secs(),
        }
    }

    pub fn with_target(mut self, group_id: Option<u64>, user_id: u64) -> Self {
        self.target_group = group_id;
        self.target_user = if group_id.is_none() {
            Some(user_id)
        } else {
            None
        };
        self
    }

    pub fn with_about(mut self, user_id: u64) -> Self {
        self.about_user = Some(user_id);
        self
    }
}

// ── 存储（原子 tmp+rename，中断恢复）────────────────────────────

fn wake_path() -> PathBuf {
    config::data_dir().join("mind").join(WAKE_FILE)
}

static STORE_LOCK: Mutex<()> = Mutex::new(());

fn load_plans() -> Vec<WakePlan> {
    let Ok(content) = fs::read_to_string(wake_path()) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<WakePlan>>(&content) {
        Ok(plans) => plans,
        Err(e) => {
            warn!(error = %e, "wake: 意图堆解析失败，按空处理");
            Vec::new()
        }
    }
}

fn save_plans(plans: &[WakePlan]) {
    if let Some(parent) = wake_path().parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(error = %e, "wake: 创建目录失败");
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(plans) else {
        warn!("wake: 意图堆序列化失败");
        return;
    };
    let tmp = wake_path().with_extension("json.tmp");
    if fs::write(&tmp, json).is_err() {
        warn!("wake: 意图堆写入临时文件失败");
        return;
    }
    if let Err(e) = fs::rename(&tmp, wake_path()) {
        warn!(error = %e, "wake: 意图堆落盘失败");
    }
}

/// 挂起一个想起
pub fn add(plan: WakePlan) {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans = load_plans();
    plans.push(plan);
    // 容量教养：最旧的 Idle 先让位
    while plans.len() > MAX_PLANS {
        let oldest_idle = plans
            .iter()
            .position(|p| p.kind == WakeKind::Idle)
            .unwrap_or(0);
        plans.remove(oldest_idle);
    }
    save_plans(&plans);
}

fn remove_by_id(plans: &mut Vec<WakePlan>, id: u64) {
    plans.retain(|p| p.id != id);
}

/// 到期的想起（旧在前）
pub fn due(now: u64) -> Vec<WakePlan> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans: Vec<WakePlan> = load_plans()
        .into_iter()
        .filter(|p| p.due_at <= now)
        .collect();
    plans.sort_by_key(|p| p.due_at);
    plans
}

/// 她目前惦记的、关于某人的心事（原话列表，供感官包"你惦记的"字段）
pub fn pending_reasons_for(user_id: u64) -> Vec<String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = util::now_secs();
    load_plans()
        .into_iter()
        .filter(|p| {
            p.kind == WakeKind::Idle
                && p.due_at > now
                && (p.about_user == Some(user_id) || p.target_user == Some(user_id))
        })
        .map(|p| p.reason)
        .collect()
}

/// 零容忍清洗：清除与该用户相关的一切心事与待办
pub fn purge_loops_about(uid: u64) {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans = load_plans();
    let before = plans.len();
    plans.retain(|p| p.about_user != Some(uid) && p.target_user != Some(uid));
    if plans.len() != before {
        save_plans(&plans);
        info!(
            uid,
            removed = before - plans.len(),
            "wake: 零容忍清洗相关心事"
        );
    }
}

// ── 夜间门控：她真的睡了 ────────────────────────────────────────

/// 免打扰时段 = 她的睡眠时间（沿用 proactive 配置的免打扰时段）
pub fn is_night() -> bool {
    let hour = util::current_hour_cst() as i32;
    let (start, end) = {
        let cfg = config::get();
        (
            cfg.proactive.quiet_start as i32,
            cfg.proactive.quiet_end as i32,
        )
    };
    if start == end {
        return false;
    }
    if start < end {
        hour >= start && hour < end
    } else {
        // 跨午夜：23 -> 7
        hour >= start || hour < end
    }
}

// ── 睡前整理：教养兜底 ─────────────────────────────────────────

/// 确保存在一个每日睡前整理的想起（今天 23:30，东八区）
pub fn ensure_daily_digest(now: u64) {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans = load_plans();
    let horizon = now + 48 * 3600;
    if plans
        .iter()
        .any(|p| p.kind == WakeKind::Digest && p.due_at <= horizon)
    {
        return;
    }
    // 下一个 23:30（东八区）：取当前本地时刻的天基准再偏移
    let local_now = now + 8 * 3600;
    let day_start = (local_now / 86400) * 86400;
    let today_digest_cst = day_start + 23 * 3600 + 30 * 60;
    let due_at = if today_digest_cst > local_now {
        today_digest_cst - 8 * 3600
    } else {
        today_digest_cst + 86400 - 8 * 3600
    };
    let mut plan = WakePlan::new(WakeKind::Digest, due_at, "睡前把今天过一遍");
    plan.id = util::now_millis() ^ 0xD1_6E_57;
    plans.push(plan);
    save_plans(&plans);
    debug!("wake: 已挂起今日睡前整理");
}

// ── 回神执行 ────────────────────────────────────────────────────

static LAST_CLEANUP_DAY: AtomicU64 = AtomicU64::new(0);

/// 一次回神的产物：走神（可发言）或睡前整理（日记/档案/心事/小结）
#[derive(Debug)]
pub enum WakeProduct {
    Idle(WakeTurn),
    Digest(crate::voice::DigestOutcome),
}

/// 心跳：兑现到期的想起。
///
/// 夜间（免打扰时段）Idle 想起静默留到早上；睡前整理（Digest）是就寝
/// 动作，允许在夜间执行。发言与登记由调用方执行（发送通道在插件层）。
pub fn tick() -> Vec<(WakePlan, WakeProduct)> {
    let now = util::now_secs();

    // 意识流过期清理：每天一次
    let today = now / 86400;
    if LAST_CLEANUP_DAY.load(Ordering::Relaxed) != today {
        super::stream::cleanup(super::stream::KEEP_DAYS);
        LAST_CLEANUP_DAY.store(today, Ordering::Relaxed);
    }

    ensure_daily_digest(now);

    let night = is_night();
    let mut results: Vec<(WakePlan, WakeProduct)> = Vec::new();
    for plan in due(now) {
        if night && plan.kind != WakeKind::Digest {
            continue;
        }
        let input = build_wake_input(&plan);
        let product = match plan.kind {
            WakeKind::Digest => {
                // 睡前整理：今天几乎没有经历就跳过（明天再整理）
                let today_events = super::stream::recent(24 * 3600, 5).len();
                if today_events < 3 {
                    debug!("wake: 今天经历太少，跳过睡前整理");
                    WakeProduct::Digest(crate::voice::DigestOutcome::default())
                } else {
                    WakeProduct::Digest(crate::voice::digest_think(&build_digest_input()))
                }
            }
            WakeKind::Idle => {
                let allow_speak = plan.target_group.is_some() || plan.target_user.unwrap_or(0) > 0;
                WakeProduct::Idle(crate::voice::wake_think(&input, allow_speak))
            }
        };
        info!(plan_id = plan.id, kind = ?plan.kind, "wake: 回神完成");
        {
            let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let mut plans = load_plans();
            remove_by_id(&mut plans, plan.id);
            save_plans(&plans);
        }
        results.push((plan, product));
    }
    results
}

/// 回神的感官输入：最近的经历 + 身体信号 + 她自己留的话
fn build_wake_input(plan: &WakePlan) -> String {
    let mut sections: Vec<String> = Vec::new();

    let recent = super::stream::recent_text(2 * 3600, 40);
    if !recent.is_empty() {
        sections.push(format!("# 你最近的经历\n{recent}"));
    }

    let signals = super::sensation::body_signals();
    if !signals.is_empty() {
        let rendered = signals
            .iter()
            .map(|s| format!("{} {:.1}", s.name, s.level))
            .collect::<Vec<_>>()
            .join("、");
        sections.push(format!("身体：{rendered}"));
    }

    if let Some(uid) = plan.about_user {
        let name = crate::person_info::get_display_name(uid, plan.target_group.unwrap_or(0))
            .unwrap_or_else(|| "那个人".to_string());
        sections.push(format!("关于：{name}"));
    }

    sections.push(format!("你留了话：{}", plan.reason));
    sections.join("\n\n")
}

/// 睡前整理的输入：一整天的经历 + 互动过的人的现有档案
fn build_digest_input() -> String {
    let now = util::now_secs();
    let mut sections: Vec<String> = Vec::new();

    let today = super::stream::recent_text(24 * 3600, 300);
    if !today.is_empty() {
        sections.push(format!(
            "# 你今天（{date}）的经历\n{today}",
            date = util::ts_to_date_str(now)
        ));
    }

    // 今天互动过的人 → 现有档案摘要
    let mut involved: Vec<u64> = Vec::new();
    for event in super::stream::recent(24 * 3600, 300) {
        if let Some(uid) = event.about
            && uid > 0
            && !involved.contains(&uid)
        {
            involved.push(uid);
        }
    }
    let person_lines: Vec<String> = involved
        .iter()
        .map(|&uid| super::persons::get(uid).summary_for_prompt())
        .filter(|s| !s.is_empty())
        .collect();
    if !person_lines.is_empty() {
        sections.push(format!(
            "# 你对今天互动过的人的现有印象\n{}",
            person_lines.join("\n\n")
        ));
    }

    let signals = super::sensation::body_signals();
    if !signals.is_empty() {
        let rendered = signals
            .iter()
            .map(|s| format!("{} {:.1}", s.name, s.level))
            .collect::<Vec<_>>()
            .join("、");
        sections.push(format!("身体：{rendered}"));
    }

    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_plan_serializes_round_trip() {
        let plan = WakePlan::new(WakeKind::Digest, 1_700_000_000, "睡前把今天过一遍");
        let json = serde_json::to_string(&plan).unwrap();
        let back: WakePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, WakeKind::Digest);
        assert_eq!(back.reason, "睡前把今天过一遍");
    }

    #[test]
    fn kind_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&WakeKind::Digest).unwrap(),
            r#""digest""#
        );
        assert_eq!(serde_json::to_string(&WakeKind::Idle).unwrap(), r#""idle""#);
    }

    #[test]
    fn night_gating_wraps_midnight() {
        // 直接验证 23-7 的逻辑分支（不依赖 config：单独复算）
        let in_night = |hour: i32, start: i32, end: i32| {
            if start == end {
                false
            } else if start < end {
                hour >= start && hour < end
            } else {
                hour >= start || hour < end
            }
        };
        assert!(in_night(23, 23, 7));
        assert!(in_night(2, 23, 7));
        assert!(in_night(6, 23, 7));
        assert!(!in_night(7, 23, 7));
        assert!(!in_night(12, 23, 7));
        assert!(!in_night(22, 23, 7));
    }
}
