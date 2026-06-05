pub mod activity;
pub mod admin;
pub mod tracking;
pub mod ai;
pub mod anti_injection;
pub mod archive;
pub mod blocklist;
pub mod config;
pub mod conversation;
pub mod conversation_end;
pub mod crisis;
pub mod crypto;
pub mod util;
#[cfg(feature = "plugin")]
pub mod cron;
pub mod emoji;
pub mod emotion;
pub mod sticker;
pub mod learner;
pub mod memory;
pub mod mental_state;
pub mod person_info;
pub mod personality;
pub mod planner;
pub mod prompt;
pub mod proactive;
pub mod quota;
pub mod reply_effect;
pub mod replyer;
pub mod runtime;
pub mod schedule;
pub mod self_memory;
pub mod circadian;
pub mod social_battery;
#[cfg(feature = "plugin")]
pub mod sender;
pub mod state;
pub mod timing_gate;
pub mod typo;
pub mod vision;
pub mod working_memory;

// ── 测试模式下的 stub ────────────────────────────────────────
#[cfg(not(feature = "plugin"))]
mod cron {
    pub fn handle_cron_in_reply(reply: &str, _group_id: u64) -> String { reply.to_string() }
    pub fn handle_task_event(_json: &str) {}
}
#[cfg(not(feature = "plugin"))]
mod sender {
    pub fn send_msg(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn send_with_typing(_group_id: u64, _user_id: u64, _text: &str) {}
    pub fn send_at_msg(_group_id: u64, _user_id: u64, _text: &str) {}
}

#[cfg(feature = "plugin")]
use luo9_sdk::bus::Bus;
#[cfg(feature = "plugin")]
use luo9_sdk::payload::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;
use tracing::{debug, info};

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
    pub(crate) user_msgs: Vec<(u64, String, Vec<u64>)>,
}

pub(crate) static MESSAGE_QUEUE: OnceLock<MessageQueue> = OnceLock::new();

fn init_message_queue() {
    let (tx, rx) = mpsc::channel::<ProcessingTask>();
    MESSAGE_QUEUE.set(MessageQueue { tx }).ok();

    thread::spawn(move || {
        while let Ok(task) = rx.recv() {
            conversation::batch::process_group_batch(task.group_id, &task.user_msgs);
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
        processing_users().lock().unwrap().remove(&(self.group_id, self.user_id));
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
    let mut s = shared_state().write().unwrap();
    f(&mut s)
}

pub(crate) fn read_shared_state<F, R>(f: F) -> R
where
    F: FnOnce(&state::SharedState) -> R,
{
    let s = shared_state().read().unwrap();
    f(&s)
}

thread_local! {
    static STATE: RefCell<state::State> = RefCell::new(state::State::new());
}

pub(crate) fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut state::State) -> R,
{
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// 上次主动消息检查时间
static LAST_PROACTIVE_CHECK: AtomicU64 = AtomicU64::new(0);
/// 上次记忆审查时间
static LAST_MEMORY_REVIEW: AtomicU64 = AtomicU64::new(0);
/// 上次自我反思时间
static LAST_SELF_REFLECTION: AtomicU64 = AtomicU64::new(0);

// ── 插件入口 ────────────────────────────────────────────────────

/// 轻量读取 config.yaml 中的 log 配置 (在完整 config::init 之前调用)
fn read_log_config(log_dir: &std::path::Path) -> Option<config::LogConfig> {
    // log_dir = data/plugin_ai_chat/logs, config = data/plugin_ai_chat/config.yaml
    let config_path = log_dir.parent()?.join("config.yaml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    #[derive(serde::Deserialize)]
    struct Partial { log: Option<config::LogConfig> }
    serde_yaml::from_str::<Partial>(&content).ok()?.log
}

#[cfg(feature = "plugin")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_main() {
    // 初始化 tracing subscriber：同时输出到控制台和日志文件
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    use tracing_appender::rolling;
    use time::macros::format_description;

    let log_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("data").join("plugin_ai_chat").join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    let file_appender = rolling::daily(&log_dir, "ai_chat.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    // 保留 guard 防止 non_blocking writer 被提前 drop
    FILE_GUARD.set(_guard).ok();

    // 从配置读取日志级别 (config.yaml 可能在 init 之前)
    let log_config = read_log_config(&log_dir);
    let log_level = log_config.as_ref().map(|c| c.level.as_str()).unwrap_or("info");
    let log_enabled = log_config.as_ref().map(|c| c.enabled).unwrap_or(true);

    // 禁用日志时用 error 级别，实际上不输出任何内容
    let effective_level = if log_enabled { log_level } else { "error" };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("plugin_ai_chat={},warn", effective_level)));

    // 使用东八区（北京时间）格式: 2026-05-03 14:30:45
    let timer = fmt::time::LocalTime::new(
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]")
    );

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

    // 初始化错别字生成器（加载字频和拼音字典）
    typo::init(config::data_dir());

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

    // ── 同步 config.blacklist 到运行时黑名单 ──
    {
        let cfg = config::get();
        with_state(|s| {
            for &uid in &cfg.blacklist {
                s.add_blacklist(uid);
            }
        });
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

            // 自动开启对话
            with_state(|s| { s.active.insert(user_id); });
            info!(user_id, "auto_start: 活跃用户私聊");
        }
    }

    // ── 默认启动群聊 ──
    // 根据配置自动开启指定群聊
    {
        let cfg = config::get();
        for &group_id in &cfg.auto_start_groups {
            with_state(|s| { s.active_groups.insert(group_id); });
            info!(group_id, "auto_start: 活跃群聊");
        }
    }

    // 初始化 ECC 密钥对 (在注册之前)
    crypto::init();

    // 注册到远程注册表 (后台线程，不阻塞启动)
    thread::spawn(crate::self_memory::register_to_registry);

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
    LAST_SELF_REFLECTION.store(now, Ordering::Relaxed);

    let msg_sub = Bus::topic("luo9_message").subscribe().unwrap();
    let task_sub = Bus::topic("luo9_task").subscribe().unwrap();
    let ver_sub = Bus::topic("luo9_version").subscribe().unwrap();
    let msg_topic = Bus::topic("luo9_message");
    let task_topic = Bus::topic("luo9_task");
    let ver_topic = Bus::topic("luo9_version");

    loop {
        if let Some(json) = msg_topic.pop(msg_sub)
            && let Some(BusPayload::Message(msg)) = BusPayload::parse(&json) {
                match msg.message_type {
                    MsgType::Group => {
                        conversation::handle_group_msg(msg.group_id.unwrap_or(0), msg.user_id, &msg.message);
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

        // 每60秒检查一次主动消息和情绪衰减
        check_periodic();

        // 每5秒同步活跃对话状态到共享内存（供管理线程读取）
        {
            static LAST_SYNC: AtomicU64 = AtomicU64::new(0);
            let now = util::now_secs();
            if now.saturating_sub(LAST_SYNC.load(Ordering::Relaxed)) >= 5 {
                LAST_SYNC.store(now, Ordering::Relaxed);
                sync_active_to_shared();
            }
        }

        // ── 版本查询 ──
        if let Some(json) = ver_topic.pop(ver_sub)
            && luo9_sdk::version::is_version_query(&json) {
                luo9_sdk::version::reply_version(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            }

        thread::sleep(Duration::from_millis(1));
    }
}

// ── 周期性检查 ──────────────────────────────────────────────────

fn check_periodic() {
    let now = util::now_secs();
    let last = LAST_PROACTIVE_CHECK.load(Ordering::Relaxed);
    let interval = config::get().proactive.check_interval;
    if now.saturating_sub(last) < interval {
        return;
    }
    LAST_PROACTIVE_CHECK.store(now, Ordering::Relaxed);
    debug!("periodic: starting check cycle");

    // 社交电量更新
    if config::get().humanity.social_battery_enabled {
        let mut battery = social_battery::load();
        social_battery::update(&mut battery);
        social_battery::save(&battery);
    }

    // 主动消息动机更新
    proactive::motivation::update_motivations();

    // 情绪衰减 + 主动消息检查 (分步获取锁，释放后再调用 proactive/emotion)
    let mut all_users: Vec<(u64, u64)> = Vec::new();

    // 私聊活跃用户 (thread_local State)
    with_state(|s| {
        for &uid in &s.active {
            all_users.push((uid, 0u64));
        }
    });

    // 群聊用户: 从 SharedState 获取 contexts，从 State 获取 active_groups
    let active_groups: std::collections::HashSet<u64> = with_state(|s| s.active_groups.clone());
    read_shared_state(|s| {
        for (&(gid, uid), ctx) in &s.contexts {
            if gid > 0 && active_groups.contains(&gid) && !ctx.history.is_empty() {
                all_users.push((uid, gid));
            }
        }
    });

    // 也包含当前有活跃批次的用户 (thread_local State)
    with_state(|s| {
        for &(gid, uid) in s.batches.keys() {
            if gid > 0 {
                all_users.push((uid, gid));
            }
        }
    });

    all_users.sort_unstable();
    all_users.dedup();
    debug!(count = all_users.len(), "proactive: checking users");

    // 锁已释放，安全调用 proactive/emotion
    // 所有用户都走 per-user 检查，每个用户有独立的 last_sent/ignore_count 控制
    for (user_id, group_id) in &all_users {
        emotion::decay(*user_id);
        proactive::check_proactive_messages(*user_id, *group_id);
    }

    // 定期记忆审查 (每小时一次)
    let last_review = LAST_MEMORY_REVIEW.load(Ordering::Relaxed);
    if now.saturating_sub(last_review) >= 3600 {
        LAST_MEMORY_REVIEW.store(now, Ordering::Relaxed);
        memory::ai_review_all();
    }

    // 兴趣分衰减 (每天一次)
    static LAST_INTEREST_DECAY: AtomicU64 = AtomicU64::new(0);
    if now.saturating_sub(LAST_INTEREST_DECAY.load(Ordering::Relaxed)) >= 86400 {
        LAST_INTEREST_DECAY.store(now, Ordering::Relaxed);
        quota::decay_all_interest();
        debug!("interest: decayed all scores");
    }

    // 每日遗忘扫描
    memory::unpredictability::run_forgetting_scan();

    // 工作记忆清理 (每次周期检查都执行，轻量级)
    let expire_hours = config::get().memory.working_memory_expire_hours;
    working_memory::cleanup(expire_hours * 3600);

    // SharedState 清理不活跃条目（释放内存）
    {
        let inactive_groups: std::collections::HashSet<u64> = with_state(|s| s.active_groups.clone());
        with_shared_state(|s| s.cleanup_inactive(&inactive_groups));
    }

    // 刷新挂起的 embedding 批量写入
    memory::flush_pending_embeddings();

    // 检查活动进度（完成的活动会记录，供生命事件路径触发）
    activity::check_activity_progress();

    // 心理状态衰减 (担忧 + 要考量)
    let ms_cfg = &config::get().mental_state;
    mental_state::decay_concerns(ms_cfg.concern_decay_rate);
    mental_state::decay_deliberations(ms_cfg.deliberation_decay_rate);

    // 每日计划检查 (每天早上生成新计划)
    if schedule::check_and_generate_plan() {
        do_daily_plan_generation();
    }

    // 周计划检查 (每周一自动生成)
    if schedule::check_and_generate_weekly_plan() {
        do_weekly_plan_generation();
    }

    // 月计划检查 (每月1号自动生成)
    if schedule::check_and_generate_monthly_plan() {
        do_monthly_plan_generation();
    }

    // 计划推动：检查今天有没有该做的任务
    let pushes = schedule::check_plan_push();
    for push in &pushes {
        debug!(content = %push, "schedule: plan push");
        // 把推动内容加入自我记忆，主动消息会读取并自然带出来
        self_memory::add(push, self_memory::ThoughtCategory::Plan);
    }

    // 对话后反思: 对话结束一段时间后回顾刚结束的对话
    let post_delay = config::get().self_reflection.post_conversation_delay_secs;
    let idle_groups = read_shared_state(|s| s.get_idle_groups(now, post_delay));
    for group_id in idle_groups {
        with_shared_state(|s| { s.reflected_groups.insert(group_id); });
        with_state(|s| { s.last_review_times.insert(group_id, now); });
        do_post_conversation_reflection(group_id);
    }

    // 长时间对话的定期审查：对话还在继续，但距离上次审查已经很久
    let review_interval = config::get().self_reflection.interval;
    let conv_times = read_shared_state(|s| s.last_conversation_times.clone());
    let active_review_groups = with_state(|s| {
        state::get_groups_needing_review(&conv_times, &s.last_review_times, now, review_interval, post_delay)
    });
    for group_id in active_review_groups {
        with_state(|s| { s.last_review_times.insert(group_id, now); });
        do_post_conversation_reflection(group_id);
    }

    // 内心独白：生成和衰减
    if config::get().humanity.inner_thought_enabled {
        if let Some(thought) = self_memory::inner_thought::try_generate() {
            debug!(content = %thought.content, "inner_thought: new thought generated");
            // 有行动潜力的想法加入自我记忆，可能触发主动消息
            if thought.action_potential > 0.5 {
                self_memory::add(
                    &thought.content,
                    self_memory::ThoughtCategory::Feeling,
                );
            }
        }
        self_memory::inner_thought::decay_thoughts();
    }

    // 定时空闲反思 (从配置读取间隔)
    let reflect_interval = config::get().self_reflection.interval;
    let last_reflect = LAST_SELF_REFLECTION.load(Ordering::Relaxed);
    if now.saturating_sub(last_reflect) >= reflect_interval {
        LAST_SELF_REFLECTION.store(now, Ordering::Relaxed);
        do_self_reflection();
    }
}

/// 执行自我反思：收集最近对话上下文，调用 AI 生成内心想法
fn do_self_reflection() {
    // 收集群组列表 (thread_local) 和私聊上下文 (shared)
    let group_ids: Vec<u64> = with_state(|s| {
        s.active_groups.iter().filter(|&&gid| gid > 0).copied().collect()
    });

    let recent_context = read_shared_state(|s| {
        let mut context_parts = Vec::new();
        for (&(gid, uid), ctx) in &s.contexts {
            if gid == 0 && !ctx.history.is_empty() {
                let recent: Vec<String> = ctx.history.iter()
                    .rev()
                    .take(4)
                    .map(|(role, content)| format!("[{}] {}", role, content))
                    .collect();
                if !recent.is_empty() {
                    context_parts.push(format!("用户{}的私聊:\n{}", uid, recent.join("\n")));
                }
            }
        }
        context_parts.join("\n\n")
    });

    // 构建群组画像：每个群的最近消息，让 AI 理解每个群是干什么的
    let group_profiles: Vec<self_memory::GroupProfile> = group_ids.iter().map(|&gid| {
        let entries = working_memory::get_recent(gid, 7200, 20);
        let recent_messages = if entries.is_empty() {
            "(最近没有消息)".to_string()
        } else {
            let lines: Vec<String> = entries.iter().map(|e| {
                let name = person_info::get_display_name(e.user_id, gid)
                    .unwrap_or_else(|| "群友".to_string());
                format!("[{}] {}", name, e.content)
            }).collect();
            lines.join("\n")
        };
        self_memory::GroupProfile { group_id: gid, recent_messages }
    }).collect();

    let (count, share) = self_memory::reflect(&recent_context, &group_profiles);

    // 如果反思产生了想分享的想法，主动发送 (只发到激活的群)
    // 但要检查对话是否仍然活跃，避免发送过时的内容
    if let Some((content, group_id)) = share {
        let is_active = with_state(|s| s.active_groups.contains(&group_id));
        if !is_active {
            debug!(group_id, "self_reflect: skipping share to inactive group");
            return;
        }

        // 检查该群最近是否有活跃对话（5分钟内有消息）
        let recent_entries = working_memory::get_recent(group_id, 300, 5);
        if recent_entries.is_empty() {
            debug!(group_id, "self_reflect: skipping share, no recent conversation");
            return;
        }

        debug!(group_id, content, "self_reflect: sharing thought");
        sender::safe_send_quiet(group_id, 0, &content);
    }

    debug!(count, "self_reflect completed");
}

/// 生成每日计划
fn do_daily_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() {
        return;
    }

    // 构建上下文
    let mut context = format!(
        "{}\n\n{}",
        user_prompt,
        schedule::get_plan_generation_prompt()
    );

    // 添加最近的自我记忆
    let self_mem = self_memory::get_context(5);
    if !self_mem.is_empty() {
        context = format!("{}\n\n# 最近的想法\n{}", context, self_mem);
    }

    // 调用 AI 生成计划
    match ai::analyze_with_tools(
        &context,
        "根据你的人设，为自己制定今天的计划。",
        &[ai::daily_plan_tool()],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => {
            if let Some(tasks) = parsed.get("tasks").and_then(|t| t.as_array()) {
                for task in tasks {
                    if let Some(task_str) = task.as_str() {
                        schedule::add_task(task_str);
                    }
                }
                debug!(count = tasks.len(), "daily plan generated");
            }
        }
        Err(e) => {
            debug!(error = %e, "daily plan generation failed");
        }
    }
}

/// 生成周计划
fn do_weekly_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() { return; }

    let context = format!(
        "{}\n\n{}\n\n# 最近的想法\n{}",
        user_prompt,
        crate::prompt::PromptManager::get().raw("weekly_plan"),
        self_memory::get_context(5),
    );

    match ai::analyze_with_tools(
        &context,
        "制定本周计划",
        &[ai::weekly_plan_tool()],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => {
            if let Some(goals) = parsed.get("goals").and_then(|g| g.as_array()) {
                let mut plan = schedule::load_weekly_plan();
                for goal in goals {
                    if let (Some(content), Some(target_day)) = (
                        goal.get("content").and_then(|c| c.as_str()),
                        goal.get("target_day").and_then(|d| d.as_str()),
                    ) {
                        plan.goals.push(schedule::WeeklyGoal {
                            content: content.to_string(),
                            target_day: target_day.to_string(),
                            completed: false,
                            completed_at: 0,
                        });
                    }
                }
                schedule::save_weekly_plan(&plan);
                debug!(count = plan.goals.len(), "weekly plan generated");
            }
        }
        Err(e) => debug!(error = %e, "weekly plan generation failed"),
    }
}

/// 生成月计划
fn do_monthly_plan_generation() {
    let user_prompt = config::prompt();
    if user_prompt.is_empty() { return; }

    let context = format!(
        "{}\n\n{}\n\n# 最近的想法\n{}",
        user_prompt,
        crate::prompt::PromptManager::get().raw("monthly_plan"),
        self_memory::get_context(5),
    );

    match ai::analyze_with_tools(
        &context,
        "制定本月计划",
        &[ai::monthly_plan_tool()],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => {
            if let Some(goals) = parsed.get("goals").and_then(|g| g.as_array()) {
                let mut plan = schedule::load_monthly_plan();
                for goal in goals {
                    if let Some(content) = goal.as_str() {
                        plan.goals.push(schedule::MonthlyGoal {
                            content: content.to_string(),
                            completed: false,
                            completed_at: 0,
                        });
                    }
                }
                schedule::save_monthly_plan(&plan);
                debug!(count = plan.goals.len(), "monthly plan generated");
            }
        }
        Err(e) => debug!(error = %e, "monthly plan generation failed"),
    }
}

/// 对话后反思：回顾刚结束的群对话 + 审查新消息（已读+未读）
fn do_post_conversation_reflection(group_id: u64) {
    // 获取上次审查到的时间戳，只处理之后的新消息
    let last_reviewed = with_shared_state(|s| {
        *s.last_reviewed_timestamps.get(&group_id).unwrap_or(&0)
    });

    // 取该群的新工作记忆 (上次审查之后的消息)
    let entries = working_memory::get_since(group_id, last_reviewed, 50);
    if entries.is_empty() {
        return;
    }

    let self_qq = config::get().self_qq;

    // 全部消息都展示，但用 [bot] 和 [user_id:XXX] 清晰区分谁说了什么
    let recent_context: Vec<String> = entries.iter().map(|e| {
        let is_self = self_qq > 0 && e.user_id == self_qq;
        let who = if is_self { "bot".to_string() } else {
            person_info::get_display_name(e.user_id, group_id)
                .unwrap_or_else(|| "群友".to_string())
        };
        let tag = if e.bot_replied { "[已回复]" } else { "[未回复]" };
        format!("[{}]{} {}", who, tag, e.content)
    }).collect();
    let context_text = recent_context.join("\n");

    // 记录最新消息的时间戳，下次只处理更新的
    let max_timestamp = entries.iter().map(|e| e.timestamp).max().unwrap_or(0);
    with_shared_state(|s| { s.last_reviewed_timestamps.insert(group_id, max_timestamp); });

    // 检查内容是否与上次反思时足够相似，避免对同一话题反复思考
    let normalized = util::normalize_for_compare(&context_text);
    let should_reflect = with_shared_state(|s| {
        if let Some(prev) = s.last_reflected_content.get(&group_id) {
            util::content_overlap(prev, &normalized) < 0.5
        } else {
            true
        }
    });

    if should_reflect {
        // 1. 自我反思
        let group_profiles = vec![self_memory::GroupProfile {
            group_id,
            recent_messages: context_text.clone(),
        }];
        let (count, _share) = self_memory::reflect(&context_text, &group_profiles);
        debug!(group_id, count, "post_conversation_reflect completed");

        with_shared_state(|s| { s.last_reflected_content.insert(group_id, normalized); });
    } else {
        debug!(group_id, "post_conversation_reflect skipped (similar content)");
    }

    // 2. 审查对话消息 (已读+未读，像人翻聊天记录一样)
    review_conversation_messages(group_id, &context_text);

    // 3. 从对话中生成担忧和要考量
    mental_state::generate_from_conversation(group_id, &context_text);
}



/// 审查对话消息，只提取有关的记忆
fn review_conversation_messages(group_id: u64, messages_text: &str) {
    let mut context_parts = Vec::new();
    let user_prompt = config::prompt();
    if !user_prompt.is_empty() {
        context_parts.push(format!("# 你的身份\n{}", user_prompt));
    }
    let personality = personality::get_prompt_context();
    if !personality.is_empty() { context_parts.push(personality); }
    let mem = memory::get_context(0, group_id);
    if !mem.is_empty() { context_parts.push(mem); }

    let full_context = format!("{}\n\n# 对话记录\n{}", context_parts.join("\n\n"), messages_text);

    match ai::analyze_with_tools(
    crate::prompt::PromptManager::get().raw("review_conversation"),
    &full_context,
    &[ai::review_conversation_tool()],
    Some(serde_json::json!("auto"))
    ) {
        Ok(parsed) => {
            if let Some(relevant) = parsed.get("relevant").and_then(|r| r.as_array()) {
                for item in relevant {
                    let user_id = item.get("user_id").and_then(|u| u.as_u64()).unwrap_or(0);
                    let memory_content = item.get("memory").and_then(|m| m.as_str()).unwrap_or("");
                    let importance_str = item.get("importance").and_then(|i| i.as_str()).unwrap_or("normal");
                    if memory_content.is_empty() || user_id == 0 { continue; }
                    let importance = match importance_str {
                        "permanent" => memory::Importance::Permanent,
                        "important" => memory::Importance::Important,
                        _ => memory::Importance::Normal,
                    };
                    memory::add(user_id, group_id, memory_content, importance);
                }
                debug!(group_id, count = relevant.len(), "review_conversation: memories extracted");
            }

            if let Some(emotion_obj) = parsed.get("emotion") {
                let state = emotion_obj.get("state").and_then(|s| s.as_str()).unwrap_or("neutral");
                let intensity = emotion_obj.get("intensity").and_then(|i| i.as_f64()).unwrap_or(0.3) as f32;
                emotion::update_from_analysis(0, state, intensity);
            }
        }
        Err(e) => {
            debug!(error = %e, "review_conversation: AI error");
        }
    }
}

// ── 对话管理 API（供 admin.rs 调用） ──────────────────────────

/// 获取所有活跃群聊 ID（从共享内存读取，管理线程可用）
pub fn get_active_groups() -> Vec<u64> {
    read_shared_state(|s| s.active_groups.iter().copied().collect())
}

/// 获取所有活跃私聊用户 ID（从共享内存读取，管理线程可用）
pub fn get_active_users() -> Vec<u64> {
    read_shared_state(|s| s.active_users.iter().copied().collect())
}

/// 开启/关闭群聊，返回是否改变了状态
pub fn toggle_group_chat(group_id: u64, enable: bool) -> bool {
    let changed = if enable {
        let already = with_state(|s| s.active_groups.contains(&group_id));
        if already { return false; }
        with_state(|s| { s.active_groups.insert(group_id); });
        true
    } else {
        let active = with_state(|s| s.active_groups.contains(&group_id));
        if !active { return false; }
        with_state(|s| { s.active_groups.remove(&group_id); });
        true
    };
    sync_active_to_shared();
    changed
}

/// 开启/关闭私聊，返回是否改变了状态
pub fn toggle_private_chat(user_id: u64, enable: bool) -> bool {
    let changed = if enable {
        let already = with_state(|s| s.active.contains(&user_id));
        if already { return false; }
        with_state(|s| { s.active.insert(user_id); });
        true
    } else {
        let active = with_state(|s| s.active.contains(&user_id));
        if !active { return false; }
        with_state(|s| {
            s.active.remove(&user_id);
            s.batches.remove(&(0, user_id));
        });
        true
    };
    sync_active_to_shared();
    changed
}

/// 同步活跃对话状态到 SharedState（供管理线程读取）
fn sync_active_to_shared() {
    let (groups, users) = with_state(|s| {
        (s.active_groups.clone(), s.active.clone())
    });
    with_shared_state(|s| s.sync_active(&groups, &users));
}

// ── 消息处理 ────────────────────────────────────────────────────

/// 检查用户是否是管理员
pub(crate) fn is_admin(user_id: u64) -> bool {
    let admin = config::get().admin_qq;
    admin == 0 || admin == user_id
}

// ── 命令处理 re-exports（供 admin 模块调用） ─────────────────────
pub use conversation::handle_admin_command;
pub use conversation::handle_control_command;
pub use conversation::handle_personality_command;
pub use conversation::handle_proactive_command;
