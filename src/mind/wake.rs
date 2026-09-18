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

/// 紧迫度：决定到期想起的兑现顺序与失败后的重试节奏
///
/// 排序语义 Now < Soon < Later：同一批到期的想起，更紧的先被兑现；
/// 退避语义：更紧的失败后更快回来重试。夜间门控对三种紧迫度一视同仁——
/// 夜间是睡眠不是免打扰。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// 马上想（几分钟内的事）
    Now,
    /// 稍后想（今天之内）
    Soon,
    /// 慢慢想（不急）
    #[default]
    Later,
}

impl Urgency {
    /// 失败重试的基础退避（秒），按 attempts 指数放大
    fn backoff_base_secs(self) -> u64 {
        match self {
            Self::Now => 5 * 60,
            Self::Soon => 30 * 60,
            Self::Later => 2 * 3600,
        }
    }
}

/// 失败退避：第 `attempts` 次失败后等待多久（指数放大，封顶一天）
fn backoff_secs(urgency: Urgency, attempts: u8) -> u64 {
    let base = urgency.backoff_base_secs() as f64;
    (base * 2.0f64.powi(attempts as i32)).clamp(60.0, 24.0 * 3600.0) as u64
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
    /// 紧迫度（旧数据默认 Later）
    #[serde(default)]
    pub urgency: Urgency,
    /// 已失败的回神次数
    #[serde(default)]
    pub attempts: u8,
    /// 放弃前最多失败次数
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u8,
}

fn default_max_attempts() -> u8 {
    3
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
            urgency: Urgency::Later,
            attempts: 0,
            max_attempts: default_max_attempts(),
        }
    }

    pub fn with_about(mut self, user_id: u64) -> Self {
        self.about_user = Some(user_id);
        self
    }

    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
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
    let Ok(json) = serde_json::to_string_pretty(plans) else {
        warn!("wake: 意图堆序列化失败");
        return;
    };
    if let Err(e) = crate::util::atomic_write(wake_path(), json) {
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

/// 到期的想起（紧迫的在前，同级按到期时间）
pub fn due(now: u64) -> Vec<WakePlan> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans: Vec<WakePlan> = load_plans()
        .into_iter()
        .filter(|p| p.due_at <= now)
        .collect();
    plans.sort_by_key(|p| (p.urgency, p.due_at));
    plans
}

/// 全部想起（admin API 用，按到期时间排序）
pub fn all() -> Vec<WakePlan> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans = load_plans();
    plans.sort_by_key(|p| p.due_at);
    plans
}

/// 关闭（移除）一个想起（admin 手动操作）
pub fn close(id: u64) -> bool {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut plans = load_plans();
    let before = plans.len();
    remove_by_id(&mut plans, id);
    let removed = plans.len() != before;
    if removed {
        save_plans(&plans);
    }
    removed
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
///
/// 复用 `circadian::is_quiet_hours()` 的唯一判定，而不是在这里再写一遍
/// 同样的 if/else：两份实现只要有一侧改动，"她睡了没"就会在不同模块里
/// 给出不同答案。
pub fn is_night() -> bool {
    crate::circadian::is_quiet_hours()
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
    // 下一个 23:30（东八区）。日期边界只在 `util` 里定义，这里不再手工
    // 做 `now + 8h` / `- 8h` 的偏移运算。
    let today_digest = util::cst_time_on_same_day(now, 23, 30);
    let due_at = if today_digest > now {
        today_digest
    } else {
        today_digest + 86400
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

        // 回神失败（API 故障等）不丢她的想起：退避后重试，超过上限才放弃
        let failed = matches!(&product, WakeProduct::Idle(turn) if turn.api_failed);
        if failed {
            let attempts = plan.attempts + 1;
            if attempts >= plan.max_attempts {
                warn!(
                    plan_id = plan.id,
                    attempts,
                    reason = %plan.reason,
                    "wake: 回神连续失败，放弃这个想起"
                );
            } else {
                let delay = backoff_secs(plan.urgency, attempts.saturating_sub(1));
                warn!(
                    plan_id = plan.id,
                    attempts, delay, "wake: 回神失败，退避重试"
                );
                let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                let mut plans = load_plans();
                if let Some(p) = plans.iter_mut().find(|p| p.id == plan.id) {
                    p.attempts = attempts;
                    p.due_at = now + delay;
                }
                save_plans(&plans);
                continue;
            }
        } else {
            info!(plan_id = plan.id, kind = ?plan.kind, "wake: 回神完成");
        }
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

    // 她想要的：回神时她看得见自己的愿望（不带 id——这里不是更新的时候）
    if let Some(wishes) = crate::mind::wish::context_block(false) {
        sections.push(wishes);
    }

    // 信息觅食：哪些群攒了没细看的消息——看不看由她自己决定
    let unread = crate::mind::foraging::unread_lines();
    if !unread.is_empty() {
        sections.push(unread.join("\n"));
    }

    // 她留给自己的话在读取侧再滤一次壳（见 sanitize_reasons）
    let reason = sanitize_reasons(std::iter::once(plan.reason.clone()))
        .into_iter()
        .next()
        .unwrap_or_else(|| "（这条想起已被滤掉）".to_string());
    sections.push(format!("你留了话：{reason}"));
    sections.join("\n\n")
}

/// 把一组"她留给自己的话"过滤成可以进 prompt 的文本。
///
/// 这些 reason 由模型生成、落盘、并会在之后每一轮回灌 prompt，
/// 与内心独白/日记属于同一类污染面。写入侧已做滤壳，这里再做一次
/// 读取侧过滤：老数据（滤壳上线前落盘的）同样不能进 prompt。
///
/// `filter_shell_level` 为 "off" 时不过滤，与其它通道保持一致。
pub fn sanitize_reasons(reasons: impl IntoIterator<Item = String>) -> Vec<String> {
    if crate::config::get().conversation.filter_shell_level == "off" {
        return reasons.into_iter().collect();
    }
    reasons
        .into_iter()
        .filter(|reason| {
            let passed = crate::anti_injection::check_memory_entry(reason).passed;
            if !passed {
                super::security::log_event(0, "wake_reason_read", "rejected", reason);
            }
            passed
        })
        .collect()
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

    // 她想要的：带 id——睡前整理时她可以更新进度或收尾
    if let Some(wishes) = crate::mind::wish::context_block(true) {
        sections.push(wishes);
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
    fn old_plans_without_urgency_parse_as_later() {
        // 兼容旧意图堆：缺字段时按"慢慢想"处理
        let legacy = r#"{"id":1,"kind":"idle","due_at":100,"reason":"看看","about_user":null,"target_group":null,"target_user":null,"created_at":50}"#;
        let plan: WakePlan = serde_json::from_str(legacy).unwrap();
        assert_eq!(plan.urgency, Urgency::Later);
        assert_eq!(plan.attempts, 0);
        assert_eq!(plan.max_attempts, 3);
    }

    #[test]
    fn urgency_orders_now_before_later() {
        let mut plans = [
            WakePlan::new(WakeKind::Idle, 100, "慢慢想的事"),
            WakePlan::new(WakeKind::Idle, 50, "马上想的事").with_urgency(Urgency::Now),
        ];
        plans.sort_by_key(|p| (p.urgency, p.due_at));
        assert_eq!(plans[0].urgency, Urgency::Now);
        assert_eq!(plans[0].due_at, 50);
    }

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let base = Urgency::Soon.backoff_base_secs();
        // 首次重试用基础退避，之后指数放大
        assert_eq!(backoff_secs(Urgency::Soon, 0), base);
        assert_eq!(backoff_secs(Urgency::Soon, 1), base * 2);
        // 封顶一天，且不短于一分钟
        assert_eq!(backoff_secs(Urgency::Later, 20), 24 * 3600);
        assert!(backoff_secs(Urgency::Now, 0) >= 60);
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
