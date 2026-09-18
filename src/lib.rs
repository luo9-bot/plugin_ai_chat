//! ai_chat —— QQ 聊天机器人插件
//!
//! ## 生产代码不得 panic
//!
//! `AGENTS.md` §硬性要求 5 要求生产代码不使用可能由外部输入、文件、网络或
//! 数据库状态触发的 `unwrap`/`expect`/`panic`。这条规则曾经只是纪律，本仓
//! 有过 143 处；现在由编译器执行：
//!
//! 插件入口是 `extern "C"`，一次 panic 不会刷新日志，而在 1ms 轮询模型下
//! 它会让**整条消息路径消失**。测试代码不在此列——那里的 `unwrap` 就是断言。
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub(crate) mod activity;
pub(crate) mod admin;
pub(crate) mod ai;
pub(crate) mod anti_injection;
pub(crate) mod archive;
pub(crate) mod circadian;
pub(crate) mod config;
pub(crate) mod conversation;
pub(crate) mod conversation_end;
pub(crate) mod crisis;
#[cfg(feature = "plugin")]
pub(crate) mod cron;
pub(crate) mod db;
pub(crate) mod emoji;
pub(crate) mod emotion;
pub(crate) mod learner;
pub(crate) mod memory;
pub(crate) mod mind;
pub(crate) mod person_info;
pub(crate) mod personal_tasks;
pub(crate) mod prompt;
pub(crate) mod quota;
pub(crate) mod reply_effect;
pub(crate) mod runtime;
pub(crate) mod schedule;
#[cfg(feature = "plugin")]
pub(crate) mod sender;
pub(crate) mod social_battery;
pub(crate) mod state;
pub(crate) mod sticker;
pub(crate) mod tracking;
pub(crate) mod util;
pub(crate) mod vision;
pub(crate) mod voice;
pub(crate) mod working_memory;

// ── 测试模式下的 stub ────────────────────────────────────────
#[cfg(not(feature = "plugin"))]
mod cron {
    pub fn handle_cron_in_reply(reply: &str, _group_id: u64) -> String {
        reply.to_string()
    }
    pub fn handle_task_event(_json: &str) {}
}
#[cfg(not(feature = "plugin"))]
mod sender {
    pub fn send_msg(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn send_with_typing(_group_id: u64, _user_id: u64, _text: &str, _incoming: &str) {}
    pub fn send_at_msg(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn safe_send(_group_id: u64, _user_id: u64, _reply: &str, _incoming: &str) -> bool {
        true
    }
    pub fn safe_send_quiet(_group_id: u64, _user_id: u64, _reply: &str) -> bool {
        true
    }
}

use crate::util::{MutexExt, RwLockReadExt, RwLockWriteExt};
#[cfg(feature = "plugin")]
use luo9_sdk::bus::Bus;
#[cfg(feature = "plugin")]
use luo9_sdk::payload::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// 正在处理中的用户集合 (group_id, user_id)，防止同一用户的消息被并发处理
static PROCESSING_USERS: OnceLock<Mutex<HashSet<(u64, u64)>>> = OnceLock::new();

pub(crate) fn processing_users() -> &'static Mutex<HashSet<(u64, u64)>> {
    PROCESSING_USERS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 消息处理队列：替代 thread::spawn，串行化处理避免并发混乱
pub(crate) struct MessageQueue {
    pub(crate) tx: mpsc::Sender<ProcessingTask>,
}

pub(crate) struct ProcessingTask {
    pub(crate) group_id: u64,
    pub(crate) user_msgs: Vec<conversation::handler::GroupBatch>,
}

pub(crate) static MESSAGE_QUEUE: OnceLock<MessageQueue> = OnceLock::new();

fn init_message_queue() {
    let (tx, rx) = mpsc::channel::<ProcessingTask>();
    MESSAGE_QUEUE.set(MessageQueue { tx }).ok();

    thread::spawn(move || {
        while let Ok(task) = rx.recv() {
            conversation::handler::process_group_batch(task.group_id, &task.user_msgs);
        }
    });
}

/// RAII guard: 确保在作用域结束时移除用户的处理中标记
pub(crate) struct ProcessingGuard {
    pub(crate) group_id: u64,
    pub(crate) user_id: u64,
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        processing_users()
            .lock_recover()
            .remove(&(self.group_id, self.user_id));
    }
}

/// 日志文件 non-blocking writer 的 guard，必须保持存活
static FILE_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// 跨线程共享状态 (对话历史、回复时间、bot消息等)
static SHARED_STATE: OnceLock<RwLock<state::SharedState>> = OnceLock::new();

fn shared_state() -> &'static RwLock<state::SharedState> {
    SHARED_STATE.get_or_init(|| RwLock::new(state::SharedState::new()))
}

pub(crate) fn with_shared_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut state::SharedState) -> R,
{
    let mut s = shared_state().write_recover();
    f(&mut s)
}

pub(crate) fn read_shared_state<F, R>(f: F) -> R
where
    F: FnOnce(&state::SharedState) -> R,
{
    let s = shared_state().read_recover();
    f(&s)
}

/// 门禁状态（谁在对话、谁被拉黑）。
///
/// 进程级共享：admin 线程、消息队列线程、主循环线程看到的必须是同一份。
/// 读多写少，因此用 `RwLock`；锁中毒只意味着某次写入中断，内存里的集合
/// 仍是自洽的，所以取回内部值继续用，而不是 panic（主循环不能因它停摆）。
static GATE: OnceLock<RwLock<state::GateState>> = OnceLock::new();

fn gate() -> &'static RwLock<state::GateState> {
    GATE.get_or_init(|| RwLock::new(state::GateState::load()))
}

pub(crate) fn gate_read<F, R>(f: F) -> R
where
    F: FnOnce(&state::GateState) -> R,
{
    let guard = gate().read_recover();
    f(&guard)
}

pub(crate) fn gate_write<F, R>(f: F) -> R
where
    F: FnOnce(&mut state::GateState) -> R,
{
    let mut guard = gate().write_recover();
    f(&mut guard)
}

/// 运行时拉黑/解禁的唯一入口
///
/// 三步顺序是刻意的：先改内存（权威、立即可见），再写状态库（持久），
/// 最后留审计。状态库写入失败只留痕，不阻断内存改动——拉黑一个正在发
/// 注入的人是安全操作，不该因为磁盘问题而失败。
pub(crate) fn set_blacklisted(actor: db::Actor, user_id: u64, blocked: bool) {
    if !gate_write(|g| g.set_blacklisted(user_id, blocked)) {
        return;
    }
    let db = db::db();
    let persisted = if blocked {
        db.add_blocked(user_id, "")
    } else {
        db.remove_blocked(user_id)
    };
    if let Err(error) = persisted {
        warn!(%error, user_id, "state: 黑名单写库失败（内存已生效）");
    }
    if let Err(error) = db.record_audit(
        actor,
        if blocked {
            "blocklist_add"
        } else {
            "blocklist_remove"
        },
        &format!("user={user_id}"),
    ) {
        warn!(%error, "state: 审计写入失败");
    }
}

/// 运行时黑名单快照（供 admin 与管理命令读取）
pub fn get_blacklist() -> Vec<u64> {
    gate_read(|g| g.blacklisted().collect())
}

thread_local! {
    /// 批次缓冲属于主循环线程：后台线程不得触碰（见 `state::BatchBuffer`）
    static BATCHES: RefCell<state::BatchBuffer> = RefCell::new(state::BatchBuffer::default());
}

pub(crate) fn batches<F, R>(f: F) -> R
where
    F: FnOnce(&mut state::BatchBuffer) -> R,
{
    BATCHES.with(|b| f(&mut b.borrow_mut()))
}

/// 上次主动消息检查时间
static LAST_PROACTIVE_CHECK: AtomicU64 = AtomicU64::new(0);
/// 上次记忆审查时间
static LAST_MEMORY_REVIEW: AtomicU64 = AtomicU64::new(0);

// ── 插件入口 ────────────────────────────────────────────────────

/// 轻量读取 config.yaml 中的 log 配置 (在完整 config::init 之前调用)
fn read_log_config(log_dir: &std::path::Path) -> Option<config::LogConfig> {
    // log_dir = data/plugin_ai_chat/logs, config = data/plugin_ai_chat/config.yaml
    let config_path = log_dir.parent()?.join("config.yaml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    #[derive(serde::Deserialize)]
    struct Partial {
        log: Option<config::LogConfig>,
    }
    serde_yaml::from_str::<Partial>(&content).ok()?.log
}

#[cfg(feature = "plugin")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    // 初始化 tracing subscriber：同时输出到控制台和日志文件
    use time::macros::format_description;
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let log_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("data")
        .join("plugin_ai_chat")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    let file_appender = rolling::daily(&log_dir, "ai_chat.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    // 保留 guard 防止 non_blocking writer 被提前 drop
    FILE_GUARD.set(_guard).ok();

    // 从配置读取日志级别 (config.yaml 可能在 init 之前)
    let log_config = read_log_config(&log_dir);
    let log_level = log_config
        .as_ref()
        .map(|c| c.level.as_str())
        .unwrap_or("info");
    let log_enabled = log_config.as_ref().map(|c| c.enabled).unwrap_or(true);

    // 禁用日志时用 error 级别，实际上不输出任何内容
    let effective_level = if log_enabled { log_level } else { "error" };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("plugin_ai_chat={},warn", effective_level)));

    // 使用东八区（北京时间）格式: 2026-05-03 14:30:45
    let timer = fmt::time::LocalTime::new(format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second]"
    ));

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(false)
        .with_timer(timer.clone());

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_timer(timer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    config::init();
    debug!(model = %config::get().model, "plugin loaded");

    // 初始化 PromptManager（加载所有 .prompt 模板文件）
    prompt::PromptManager::init(config::data_dir());

    // 初始化记忆系统（JSON 存储）
    memory::init();

    // 初始化知识图谱
    memory::graph::init();

    // 初始化内置表情包（NeSticker）
    sticker::init_ne_stickers();

    // 初始化防注入模块
    anti_injection::init();

    // 初始化消息处理队列（串行化处理，避免并发混乱）
    init_message_queue();

    // 初始化配额系统
    quota::init();

    // 状态库（激活/黑名单/审计的唯一权威）。必须在门禁状态之前：
    // `gate()` 会从它载入快照。
    if let Err(error) = db::init() {
        error!(%error, "db: 状态库初始化失败，本次运行的激活与黑名单不会持久化");
    }
    // 旧 blocklist.json / emotion.json 只导入一次：不导入会让先前被拉黑的人
    // 静默恢复发言，也会让所有用户的情绪状态一起归零
    if let Err(error) = db::migrate_legacy_blocklist() {
        error!(%error, "db: 旧黑名单迁移失败");
    }
    if let Err(error) = db::migrate_legacy_emotion() {
        error!(%error, "db: 旧情绪状态迁移失败");
    }
    if let Err(error) = db::migrate_legacy_working_memory() {
        error!(%error, "db: 旧工作记忆迁移失败");
    }
    if let Err(error) = db::migrate_legacy_api_usage() {
        error!(%error, "db: 旧用量记录迁移失败");
    }
    if let Err(error) = db::migrate_legacy_cognitive_state() {
        error!(%error, "db: 旧认知状态迁移失败");
    }
    if let Err(error) = db::migrate_legacy_quota() {
        error!(%error, "db: 旧配额记录迁移失败");
    }
    if let Err(error) = db::migrate_legacy_per_user_files() {
        error!(%error, "db: 旧人物/关系档案迁移失败");
    }

    // 门禁状态提前初始化：不能让后台线程在配置未就绪时惰性创建它
    let _ = gate();

    // ── 同步 config.blacklist 到运行时黑名单 ──
    // 逐个走唯一入口（内存 + 状态库 + 审计）：配置里的名单也是状态的一部分，
    // 跳过写库会让"配置里拉黑了但运行时没有"重新出现
    {
        let cfg = config::get();
        for &uid in &cfg.blacklist {
            set_blacklisted(db::Actor::Boot, uid, true);
        }
    }

    // ── 默认启动对话用户 ──
    // 根据配置自动开启指定用户的私聊
    {
        let cfg = config::get();
        let whitelist = &cfg.whitelist;
        let blacklist = &cfg.blacklist;

        for &user_id in &cfg.auto_start_users {
            // 检查白名单/黑名单
            if !whitelist.is_empty() && !whitelist.contains(&user_id) {
                debug!(user_id, "auto_start: skipped (not in whitelist)");
                continue;
            }
            if !blacklist.is_empty() && blacklist.contains(&user_id) {
                debug!(user_id, "auto_start: skipped (in blacklist)");
                continue;
            }

            // 自动开启对话（走唯一入口：内存 + 状态库 + 审计）
            toggle_private_chat(db::Actor::Boot, user_id, true);
            info!(user_id, "auto_start: 活跃用户私聊");
        }
    }

    // ── 默认启动群聊 ──
    // 根据配置自动开启指定群聊
    {
        let cfg = config::get();
        for &group_id in &cfg.auto_start_groups {
            toggle_group_chat(db::Actor::Boot, group_id, true);
            info!(group_id, "auto_start: 活跃群聊");
        }
    }

    // 启动管理后台 (后台线程)
    if !config::get().admin.token.is_empty() {
        thread::spawn(admin::start_server);
    }

    // 表情包维护线程（定期清理 + steal_emoji + do_replace）
    thread::spawn(|| {
        use std::time::Duration;
        loop {
            thread::sleep(Duration::from_secs(3600)); // 每小时执行
            sticker::maintenance();
            let cfg = crate::config::get();
            if cfg.sticker.steal_emoji {
                sticker::steal_emoji_scan();
            }
            if cfg.sticker.do_replace {
                sticker::do_replace_eviction(cfg.sticker.max_reg_num);
            }
        }
    });

    // 初始化定时器，避免启动时立即触发
    let now = util::now_secs();
    LAST_PROACTIVE_CHECK.store(now, Ordering::Relaxed);
    LAST_MEMORY_REVIEW.store(now, Ordering::Relaxed);

    // 订阅失败不再 panic：原先三处 `unwrap()` 意味着任何一次失败都让
    // **整个循环从未开始**，而外部只看到日志里一条 panic——既不知道
    // 哪些能力可用，也无法重试（见 .docs/counter-world-design.md §1.2）。
    // 现在失败是一个具名的可观测状态：逐主题记录，然后空转。
    let (msg_sub, task_sub, ver_sub) = match (
        Bus::topic("luo9_message").subscribe(),
        Bus::topic("luo9_task").subscribe(),
        Bus::topic("luo9_version").subscribe(),
    ) {
        (Ok(msg), Ok(task), Ok(ver)) => (msg, task, ver),
        (msg, task, ver) => {
            error!(
                message_topic = msg.is_ok(),
                task_topic = task.is_ok(),
                version_topic = ver.is_ok(),
                "plugin: 消息总线订阅失败，业务能力不可用"
            );
            idle_loop();
        }
    };
    let msg_topic = Bus::topic("luo9_message");
    let task_topic = Bus::topic("luo9_task");
    let ver_topic = Bus::topic("luo9_version");

    loop {
        if let Some(json) = msg_topic.pop(msg_sub)
            && let Some(BusPayload::Message(msg)) = BusPayload::parse(&json)
        {
            match msg.message_type {
                MsgType::Group => {
                    conversation::handle_group_msg(
                        msg.group_id.unwrap_or(0),
                        msg.user_id,
                        &msg.message,
                    );
                }
                MsgType::Private => {
                    conversation::handle_private_msg(msg.user_id, &msg.message);
                }
                _ => {}
            }
        }

        if let Some(json) = task_topic.pop(task_sub) {
            cron::handle_task_event(&json);
        }

        conversation::batch::process_expired_batches();

        // 周期性维护：这里只做判定与投递，维护本身在后台线程
        schedule_periodic_maintenance();

        // ── 版本查询 ──
        if let Some(json) = ver_topic.pop(ver_sub)
            && luo9_sdk::version::is_version_query(&json)
        {
            luo9_sdk::version::reply_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        }

        thread::sleep(Duration::from_millis(1));
    }
}

/// 订阅失败时的兜底：保持线程存活，并周期性记录状态
///
/// 不 panic 是为了让"插件活着但能力不可用"成为一个**可观测状态**，
/// 而不是一条 panic 之后的静默。心跳存在是为了让运维能看出
/// "还在空转"与"已经死掉"的区别。
fn idle_loop() -> ! {
    warn!("plugin: 进入空转——消息总线不可用，业务能力全部关闭");
    loop {
        thread::sleep(Duration::from_secs(60));
        warn!("plugin: 仍处于空转（消息总线不可用）");
    }
}

// ── 周期性检查 ──────────────────────────────────────────────────

/// 一次周期维护是否还在跑
///
/// 维护包含整文件读写与模型调用，可能比周期本身还长。没有这个开关时，
/// 下一轮 tick 会在上一轮还没结束时再起一个线程，负载翻倍叠加。
static MAINTENANCE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 释放单飞标志的 RAII 守卫
///
/// 用 `Drop` 而不是在调用点末尾复位：维护线程一旦 panic，末尾那行永远不会
/// 执行，标志会永久为真——周期维护从此静默停摆，而这件事没有任何外部症状。
struct MaintenanceGuard;

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        MAINTENANCE_ACTIVE.store(false, Ordering::Release);
    }
}

/// 周期维护的调度器：**只做判定与投递，不做事**
///
/// 1ms 轮询循环是全进程唯一的驱动器，它的循环体必须是常数时间的
/// （见 `.docs/counter-world-design.md` §1）。而周期维护里有整文件读写
/// （emotion.json 每用户、social 每群、working_memory 全群共用一份）和
/// 最多 3 次同步模型调用（日/周/月计划生成），任何一项都不能留在循环体上。
fn schedule_periodic_maintenance() {
    let now = util::now_secs();
    let last = LAST_PROACTIVE_CHECK.load(Ordering::Relaxed);
    let interval = config::get().proactive.check_interval;
    if now.saturating_sub(last) < interval {
        return;
    }
    LAST_PROACTIVE_CHECK.store(now, Ordering::Relaxed);

    // 上一轮还没跑完就跳过本轮：宁可少一次维护，也不叠加负载
    if MAINTENANCE_ACTIVE.swap(true, Ordering::AcqRel) {
        debug!("periodic: 上一轮维护尚未结束，跳过本轮");
        return;
    }
    thread::spawn(|| {
        let _guard = MaintenanceGuard;
        run_periodic_maintenance();
    });
}

/// 周期维护的实际内容，运行在后台线程上
fn run_periodic_maintenance() {
    let now = util::now_secs();
    debug!("periodic: starting check cycle");

    // 社交电量更新
    if config::get().humanity.social_battery_enabled {
        let mut battery = social_battery::load();
        social_battery::update(&mut battery);
        social_battery::save(&battery);
    }

    // 社会世界模型周期维护：注意力全表衰减 + 线程生命周期 + 落盘
    let active_groups: Vec<u64> = gate_read(|g| g.active_groups().collect());
    for group_id in active_groups {
        mind::social::tick(group_id);
    }

    // 回神：兑现她自己留下的想起 + 睡前整理（后台线程）
    std::thread::spawn(|| {
        for (plan, product) in mind::wake_tick() {
            match product {
                mind::wake::WakeProduct::Idle(turn) => {
                    let cfg = config::get();
                    let filter_on = cfg.conversation.filter_shell_level != "off";
                    for thought in &turn.inner {
                        if !filter_on || anti_injection::check_inner_output(thought).passed {
                            mind::push_inner(thought.clone());
                        } else {
                            mind::security::log_event(0, "inner_dialog", "rejected", thought);
                        }
                    }
                    if let voice::WakeAction::Speak { text, reply_to } = &turn.action {
                        let payload = match reply_to {
                            Some(mid) => format!("[CQ:reply,id={mid}]{text}"),
                            None => text.clone(),
                        };
                        match (plan.target_group, plan.target_user) {
                            (Some(gid), _) => {
                                sender::safe_send_quiet(gid, 0, &payload);
                                mind::push_acted(text.clone());
                            }
                            (_, Some(uid)) => {
                                sender::safe_send_quiet(0, uid, &payload);
                                mind::push_acted(text.clone());
                            }
                            _ => debug!("wake: 回神想说但无目标，只留在心里"),
                        }
                    }
                    if let Some((in_secs, reason)) = turn.wake {
                        // 与内心/日记同级滤壳：这条 reason 会落盘并反复回灌 prompt
                        if !anti_injection::check_memory_entry(&reason).passed {
                            mind::security::log_event(0, "wake_plan", "rejected", &reason);
                        } else {
                            let due_at = util::now_secs() + in_secs.max(60);
                            let mut follow = mind::WakePlan::new(plan.kind, due_at, reason);
                            follow.target_group = plan.target_group;
                            follow.target_user = plan.target_user;
                            mind::add_wake_plan(follow);
                        }
                    }
                }
                mind::wake::WakeProduct::Digest(outcome) => {
                    let cfg = config::get();
                    let filter_on = cfg.conversation.filter_shell_level != "off";
                    let shell_check = |content: &str, gate: &str| {
                        !filter_on || anti_injection::check_memory_entry(content).passed || {
                            mind::security::log_event(0, gate, "rejected", content);
                            false
                        }
                    };
                    // 内心活动入流（内对话滤壳）
                    for thought in &outcome.inner {
                        if shell_check(thought, "inner_dialog") {
                            mind::push_inner(thought.clone());
                        }
                    }
                    // 日记落库（沉淀滤壳）
                    let date = util::ts_to_date_str(util::now_secs());
                    let entries: Vec<mind::diary::DiaryEntry> = outcome
                        .diary
                        .iter()
                        .filter(|d| shell_check(&d.content, "consolidation"))
                        .map(|d| mind::diary::DiaryEntry {
                            id: String::new(),
                            date: date.clone(),
                            content: d.content.clone(),
                            feeling: d.feeling.clone(),
                            about: d.about,
                        })
                        .collect();
                    mind::diary::add(entries);
                    // 自我认识（她亲笔；沉淀滤壳 + 证据自动引用今日日记）
                    for belief in &outcome.beliefs {
                        if shell_check(belief, "persona") {
                            mind::self_model::add_belief(mind::self_model::Belief {
                                belief: belief.clone(),
                                since: util::now_secs(),
                                evidence: Some(format!("diary:{date}")),
                            });
                        }
                    }
                    // 档案修订（她亲笔，只带新内容；逐字段过沉淀滤壳）
                    for upd in &outcome.persons {
                        let mut file = mind::persons::get(upd.user_id);
                        let apply = |slot: &mut String, value: &Option<String>| {
                            if let Some(v) = value
                                && shell_check(v, "consolidation")
                            {
                                *slot = v.clone();
                            }
                        };
                        apply(&mut file.impression, &upd.impression);
                        apply(&mut file.my_feeling, &upd.my_feeling);
                        apply(&mut file.mode, &upd.mode);
                        apply(&mut file.address, &upd.address);
                        if let Some(v) = &upd.want_to_say_add
                            && shell_check(v, "consolidation")
                        {
                            file.want_to_say.push(v.clone());
                        }
                        file.updated_at = util::now_secs();
                        mind::persons::save(upd.user_id, &file);
                    }
                    // 心事 → 意图堆（滤壳）
                    for loop_ in &outcome.loops {
                        if !shell_check(&loop_.content, "consolidation") {
                            continue;
                        }
                        let due_at =
                            util::now_secs() + loop_.in_secs.unwrap_or(24 * 3600).max(3600);
                        let mut p =
                            mind::WakePlan::new(mind::WakeKind::Idle, due_at, &loop_.content);
                        if let Some(uid) = loop_.about_user {
                            p = p.with_about(uid);
                        }
                        mind::add_wake_plan(p);
                    }
                    // 愿望：新目标 / 新念头 / 目标推进（滤壳；都是她自己的事，不关联用户）
                    if cfg.humanity.wish_enabled {
                        for goal in &outcome.goal_drafts {
                            if !shell_check(&goal.text, "consolidation") {
                                continue;
                            }
                            let deadline = goal
                                .deadline_in_secs
                                .map(|s| util::now_secs() + s.max(3600));
                            mind::wish::add_goal(
                                &goal.text,
                                goal.parent_id,
                                goal.priority.unwrap_or(5),
                                deadline,
                                mind::wish::WishSource::Reflection,
                            );
                        }
                        for idea in &outcome.idea_drafts {
                            if !shell_check(&idea.text, "consolidation") {
                                continue;
                            }
                            mind::wish::add_idea(
                                &idea.text,
                                idea.excitement.unwrap_or(5),
                                mind::wish::WishSource::Reflection,
                            );
                        }
                        for upd in &outcome.wish_updates {
                            if let Some(achieved) = upd.achieved {
                                mind::wish::close_goal(upd.goal_id, achieved);
                            } else if let Some(progress) = upd.progress {
                                mind::wish::set_goal_progress(upd.goal_id, progress);
                            }
                        }
                    }
                    // 给明天的她的小结（滤壳）
                    if let Some(summary) = &outcome.compress
                        && shell_check(summary, "consolidation")
                    {
                        mind::push_digested(summary.clone());
                    }
                }
            }
        }
    });

    // 推进到期事项：等待超时不会被虚构为完成，只转为需要决定下一步。
    personal_tasks::review_due_tasks();

    // 愿望期限：到期目标的期限变成她自己的"想起"（意图堆）
    if config::get().humanity.wish_enabled {
        mind::wish::sync_due_to_wake(now);
    }

    // 信息觅食：未读计数周期落盘（记账本身纯内存零 IO）
    if config::get().humanity.foraging_enabled {
        mind::foraging::flush();
    }

    // 情绪衰减（一次载入、一次落盘）。
    //
    // 主动消息不再由规则触发器驱动：她主动不主动，由她自己的想起（意图堆）决定。
    // 这里刻意收集成一批再推进：`emotion.json` 是"每用户一个条目"的单一文件，
    // 逐用户调用会让每次维护产生 3N 次文件操作（N = 已知用户数）。
    let known_users: Vec<u64> = {
        let mut users: Vec<u64> = gate_read(|g| g.active_users().collect());
        read_shared_state(|s| {
            for (&(gid, uid), ctx) in &s.contexts {
                if gid > 0 && uid == config::get().self_qq {
                    continue;
                }
                if !ctx.history.is_empty() && !users.contains(&uid) {
                    users.push(uid);
                }
            }
        });
        users
    };
    emotion::decay_many(&known_users);

    // 定期记忆审查 (每小时一次，移到后台线程避免阻塞主循环)
    let last_review = LAST_MEMORY_REVIEW.load(Ordering::Relaxed);
    if now.saturating_sub(last_review) >= 3600 {
        LAST_MEMORY_REVIEW.store(now, Ordering::Relaxed);
        std::thread::spawn(|| {
            memory::ai_review_all();
        });

        // Turn 契约的影子观测：每小时把最近 24 小时的分布写进日志，
        // 并裁掉 7 天前的记录。这是"严格 Turn 会失败多少"的可见出口，
        // 没有它这份观测就只是躺在库里没人看。
        let report = ai::shadow::report(24 * 3600);
        if report.rounds > 0 {
            info!(summary = %report.summary_line(), "turn_shadow: 24h 观测");
        }
        ai::shadow::prune(7 * 24 * 3600);
    }

    // 每日遗忘扫描
    memory::unpredictability::run_forgetting_scan();

    // 工作记忆清理 (每次周期检查都执行，轻量级)
    let expire_hours = config::get().memory.working_memory_expire_hours;
    working_memory::cleanup(expire_hours * 3600);

    // SharedState 清理不活跃条目（释放内存）
    {
        let inactive_groups: std::collections::HashSet<u64> =
            gate_read(|g| g.active_groups().collect());
        with_shared_state(|s| s.cleanup_inactive(&inactive_groups));
    }

    // 刷新挂起的 embedding 批量写入
    memory::flush_pending_embeddings();

    // 检查活动进度（完成的活动会记录，供生命事件路径触发）
    activity::check_activity_progress();

    // 计划：跨周期就开一份新的，需要时让 AI 生成内容
    //
    // 三个阶段各自独立判断，因为周期长度不同（日/周/月）。生成完的条目
    // 由 schedule 统一存储，她随后会以带编号的清单形式看到它们，
    // 并且自己用 finish_plan 勾选——不再有任何文本匹配式的"完成检测"。
    if schedule::ensure_plan(schedule::Timeframe::Day) {
        do_daily_plan_generation();
    }
    if schedule::ensure_plan(schedule::Timeframe::Week) {
        do_weekly_plan_generation();
    }
    if schedule::ensure_plan(schedule::Timeframe::Month) {
        do_monthly_plan_generation();
    }
}

/// 计划生成的三条路径共用：调 AI、取条目数组
///
/// 三种计划用的工具 schema 形状不同（`tasks`/`goals`，字符串或带字段的对象），
/// 但"调一次模型、拿到条目数组"这一步是一样的，集中在这里。
fn generate_plan_entries(
    context: &str,
    instruction: &str,
    tool: ai::Tool,
    timeframe: schedule::Timeframe,
) -> Vec<schedule::GeneratedItem> {
    let parsed = match ai::analyze_with_tools(
        context,
        instruction,
        &[tool],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => parsed,
        Err(e) => {
            debug!(error = %e, timeframe = timeframe.label(), "schedule: 计划生成失败");
            return Vec::new();
        }
    };

    // 兼容三种形状：字符串数组、带 content 的对象数组、单层对象
    let array = ["tasks", "goals"]
        .iter()
        .find_map(|key| parsed.get(*key).and_then(|v| v.as_array()).cloned())
        .unwrap_or_default();

    let items: Vec<schedule::GeneratedItem> = array
        .iter()
        .filter_map(|entry| {
            if let Some(text) = entry.as_str() {
                return Some(schedule::GeneratedItem {
                    content: text.to_string(),
                    target_day: None,
                });
            }
            let content = entry.get("content")?.as_str()?.to_string();
            Some(schedule::GeneratedItem {
                content,
                target_day: entry
                    .get("target_day")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
            })
        })
        .collect();

    if items.is_empty() {
        debug!(
            timeframe = timeframe.label(),
            "schedule: 生成结果为空，保持空计划"
        );
    }
    items
}

/// 生成每日计划：`{"tasks": ["…"]}`
fn do_daily_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() {
        return;
    }
    let context = format!(
        "{}\n\n{}",
        user_prompt,
        crate::prompt::PromptManager::get().raw("daily_plan")
    );
    let items = generate_plan_entries(
        &context,
        "根据你的人设，为自己制定今天的计划。",
        ai::daily_plan_tool(),
        schedule::Timeframe::Day,
    );
    schedule::replace_items(schedule::Timeframe::Day, items);
}

/// 生成周计划：`{"goals": [{"content", "target_day"}]}`
fn do_weekly_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() {
        return;
    }
    let context = format!(
        "{}\n\n{}",
        user_prompt,
        crate::prompt::PromptManager::get().raw("weekly_plan")
    );
    let items = generate_plan_entries(
        &context,
        "制定本周计划",
        ai::weekly_plan_tool(),
        schedule::Timeframe::Week,
    );
    schedule::replace_items(schedule::Timeframe::Week, items);
}

/// 生成月计划：`{"goals": ["…"]}`
fn do_monthly_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() {
        return;
    }
    let context = format!(
        "{}\n\n{}",
        user_prompt,
        crate::prompt::PromptManager::get().raw("monthly_plan")
    );
    let items = generate_plan_entries(
        &context,
        "制定本月计划",
        ai::monthly_plan_tool(),
        schedule::Timeframe::Month,
    );
    schedule::replace_items(schedule::Timeframe::Month, items);
}

// ── 对话管理 API（供 admin.rs 与消息队列线程调用） ─────────────
//
// 这些入口直接读/写进程级门禁状态，因此后台的改动对主循环**立即生效**。
// 早先它们读的是每 5 秒同步一次的 `SharedState` 快照、写的是 admin 线程
// 自己的 `thread_local` 副本，结果是"对话开关功能从未生效过"。

/// 获取所有活跃群聊 ID
pub fn get_active_groups() -> Vec<u64> {
    gate_read(|g| g.active_groups().collect())
}

/// 获取所有活跃私聊用户 ID
pub fn get_active_users() -> Vec<u64> {
    gate_read(|g| g.active_users().collect())
}

/// 开启/关闭群聊，返回是否改变了状态
///
/// 与 [`set_blacklisted`] 同样的三步：内存 → 状态库 → 审计。
/// 先前的实现只改内存，于是**激活状态从不落盘**：后台开的对话重启即失。
pub fn toggle_group_chat(actor: db::Actor, group_id: u64, enable: bool) -> bool {
    apply_activation(actor, db::Scope::Group, group_id, enable)
}

/// 开启/关闭私聊，返回是否改变了状态
///
/// 关闭时不去动批次缓冲：那是主循环线程独有的状态，而缓冲到期后
/// 仍会在入口处被门禁拦下（`handle_private_msg` 检查是否活跃）。
pub fn toggle_private_chat(actor: db::Actor, user_id: u64, enable: bool) -> bool {
    apply_activation(actor, db::Scope::Private, user_id, enable)
}

/// 两个作用域共用的写路径
fn apply_activation(actor: db::Actor, scope: db::Scope, id: u64, enable: bool) -> bool {
    let changed = gate_write(|g| match scope {
        db::Scope::Group => g.set_group_active(id, enable),
        db::Scope::Private => g.set_private_active(id, enable),
    });
    if !changed {
        return false;
    }

    let db = db::db();
    if let Err(error) = db.set_activation(scope, id, enable) {
        warn!(%error, id, scope = scope.as_str(), "state: 激活状态写库失败（内存已生效）");
    }
    if let Err(error) = db.record_audit(
        actor,
        "set_activation",
        &format!("scope={} id={id} enabled={enable}", scope.as_str()),
    ) {
        warn!(%error, "state: 审计写入失败");
    }
    true
}

// ── 消息处理 ────────────────────────────────────────────────────

/// 检查用户是否是管理员
pub(crate) fn is_admin(user_id: u64) -> bool {
    let admin = config::get().admin_qq;
    admin == 0 || admin == user_id
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 后台线程改门禁、主循环线程读到的必须是同一份状态。
    ///
    /// 这是 F1.2 的回归测试：状态曾经放在 `thread_local`，于是 admin 线程
    /// 只改到自己的副本——"从网页开启/关闭一个对话"从未对消息处理生效。
    #[test]
    fn gate_changes_are_visible_across_threads() {
        const GROUP_ID: u64 = 990_001;

        let changed_on_admin_thread =
            std::thread::spawn(move || toggle_group_chat(db::Actor::Admin, GROUP_ID, true))
                .join()
                .expect("admin 线程不应 panic");

        assert!(changed_on_admin_thread, "首次开启应报告状态已改变");
        assert!(
            gate_read(|g| g.is_group_active(GROUP_ID)),
            "主循环线程必须看到后台线程写入的门禁状态"
        );
        assert!(get_active_groups().contains(&GROUP_ID));

        // 幂等：重复开启不再报告改变，且状态仍然一致
        assert!(!toggle_group_chat(db::Actor::Admin, GROUP_ID, true));
        assert!(toggle_group_chat(db::Actor::Admin, GROUP_ID, false));
        assert!(!gate_read(|g| g.is_group_active(GROUP_ID)));
        assert!(!toggle_group_chat(db::Actor::Admin, GROUP_ID, false));
    }

    /// 私聊开关同理，且不依赖批次缓冲（批次只属于主循环线程）
    #[test]
    fn private_toggle_is_visible_across_threads() {
        const USER_ID: u64 = 990_002;

        assert!(
            std::thread::spawn(move || toggle_private_chat(db::Actor::Admin, USER_ID, true))
                .join()
                .expect("admin 线程不应 panic")
        );
        assert!(gate_read(|g| g.is_private_active(USER_ID)));
        assert!(get_active_users().contains(&USER_ID));
        assert!(toggle_private_chat(db::Actor::Admin, USER_ID, false));
        assert!(!gate_read(|g| g.is_private_active(USER_ID)));
    }

    /// 维护线程即使 panic，单飞标志也必须释放。
    ///
    /// 否则周期维护会永久停摆：`MAINTENANCE_ACTIVE` 永远为真，后续每轮
    /// 都被当成"上一轮还没结束"跳过，而外部看不出任何异常。
    #[test]
    fn maintenance_flag_is_released_even_on_panic() {
        MAINTENANCE_ACTIVE.store(true, Ordering::Release);

        let outcome = std::panic::catch_unwind(|| {
            let _guard = MaintenanceGuard;
            panic!("模拟维护线程 panic");
        });

        assert!(outcome.is_err(), "这里应该捕获到 panic");
        assert!(
            !MAINTENANCE_ACTIVE.load(Ordering::Acquire),
            "标志未释放：周期维护将永久停止"
        );
    }

    /// 激活状态必须**落盘**：重启（从状态库重新载入）之后仍然有效。
    ///
    /// 这是 F2.4/F2.7 的回归测试：先前激活只存在于内存，`toggle_*` 连写库
    /// 都没有，于是"从后台开启一个对话"在重启后消失，而配置里的
    /// `auto_start_*` 会把它再打开一次——缺陷被这层巧合掩盖着。
    #[test]
    fn activation_survives_a_restart() {
        const GROUP_ID: u64 = 990_101;
        const USER_ID: u64 = 990_102;

        // 测试共用同一个 data 目录（进程内只有一份），状态库里可能残留
        // 上一次运行的数据，因此先把本用例自己的条目清干净再开始断言
        toggle_group_chat(db::Actor::Admin, GROUP_ID, false);
        toggle_private_chat(db::Actor::Admin, USER_ID, false);

        assert!(toggle_group_chat(db::Actor::Admin, GROUP_ID, true));
        assert!(toggle_private_chat(db::Actor::Admin, USER_ID, true));

        // 模拟重启：从状态库重新载入门禁快照
        let reloaded = state::GateState::load();
        assert!(reloaded.is_group_active(GROUP_ID), "群聊激活状态没有落盘");
        assert!(reloaded.is_private_active(USER_ID), "私聊激活状态没有落盘");

        // 关闭同样要落盘，否则"关掉的对话重启后又回来了"
        assert!(toggle_group_chat(db::Actor::Admin, GROUP_ID, false));
        assert!(
            !state::GateState::load().is_group_active(GROUP_ID),
            "关闭状态没有落盘"
        );

        // 收拾干净，避免影响其它用例
        toggle_private_chat(db::Actor::Admin, USER_ID, false);
    }

    /// 拉黑同样要落盘，且留下审计
    #[test]
    fn blocklist_is_persisted_and_audited() {
        const USER_ID: u64 = 990_103;

        // 同 activation 测试：先清掉可能残留的本用例条目
        set_blacklisted(db::Actor::Admin, USER_ID, false);

        set_blacklisted(db::Actor::Admin, USER_ID, true);
        assert!(state::GateState::load().is_blacklisted(USER_ID));

        let audit = db::db().recent_audit(50).expect("审计必须可读");
        assert!(
            audit.iter().any(|entry| entry.command == "blocklist_add"
                && entry.detail.contains(&USER_ID.to_string())),
            "拉黑必须留下审计记录"
        );

        set_blacklisted(db::Actor::Admin, USER_ID, false);
        assert!(!state::GateState::load().is_blacklisted(USER_ID));
    }
}
