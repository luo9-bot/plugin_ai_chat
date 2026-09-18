pub mod activity;
pub mod admin;
pub mod ai;
pub mod anti_injection;
pub mod archive;
pub mod blocklist;
pub mod circadian;
pub mod config;
pub mod conversation;
pub mod conversation_end;
pub mod crisis;
#[cfg(feature = "plugin")]
pub mod cron;
pub mod emoji;
pub mod emotion;
pub mod learner;
pub mod memory;
pub mod mind;
pub mod person_info;
pub mod personal_tasks;
pub mod prompt;
pub mod quota;
pub mod reply_effect;
pub mod runtime;
pub mod schedule;
#[cfg(feature = "plugin")]
pub mod sender;
pub mod social_battery;
pub mod state;
pub mod sticker;
pub mod tracking;
pub mod util;
pub mod vision;
pub mod voice;
pub mod working_memory;

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
            .lock()
            .unwrap()
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
            with_state(|s| {
                s.active.insert(user_id);
            });
            info!(user_id, "auto_start: 活跃用户私聊");
        }
    }

    // ── 默认启动群聊 ──
    // 根据配置自动开启指定群聊
    {
        let cfg = config::get();
        for &group_id in &cfg.auto_start_groups {
            with_state(|s| {
                s.active_groups.insert(group_id);
            });
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

    let msg_sub = Bus::topic("luo9_message").subscribe().unwrap();
    let task_sub = Bus::topic("luo9_task").subscribe().unwrap();
    let ver_sub = Bus::topic("luo9_version").subscribe().unwrap();
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
            && luo9_sdk::version::is_version_query(&json)
        {
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

    // 社会世界模型周期维护：注意力全表衰减 + 线程生命周期 + 落盘
    let active_groups: Vec<u64> = with_state(|s| s.active_groups.iter().copied().collect());
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

    // 情绪衰减（轻量，主循环执行）。
    // 主动消息不再由规则触发器驱动：她主动不主动，由她自己的想起（意图堆）决定。
    let mut known_users: Vec<u64> = Vec::new();
    with_state(|s| {
        for &uid in &s.active {
            known_users.push(uid);
        }
    });
    read_shared_state(|s| {
        for (&(gid, uid), ctx) in &s.contexts {
            if gid > 0 && uid == config::get().self_qq {
                continue;
            }
            if !ctx.history.is_empty() && !known_users.contains(&uid) {
                known_users.push(uid);
            }
        }
    });
    for uid in &known_users {
        emotion::decay(*uid);
    }

    // 定期记忆审查 (每小时一次，移到后台线程避免阻塞主循环)
    let last_review = LAST_MEMORY_REVIEW.load(Ordering::Relaxed);
    if now.saturating_sub(last_review) >= 3600 {
        LAST_MEMORY_REVIEW.store(now, Ordering::Relaxed);
        std::thread::spawn(|| {
            memory::ai_review_all();
        });
    }

    // 每日遗忘扫描
    memory::unpredictability::run_forgetting_scan();

    // 工作记忆清理 (每次周期检查都执行，轻量级)
    let expire_hours = config::get().memory.working_memory_expire_hours;
    working_memory::cleanup(expire_hours * 3600);

    // SharedState 清理不活跃条目（释放内存）
    {
        let inactive_groups: std::collections::HashSet<u64> =
            with_state(|s| s.active_groups.clone());
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
        if already {
            return false;
        }
        with_state(|s| {
            s.active_groups.insert(group_id);
        });
        true
    } else {
        let active = with_state(|s| s.active_groups.contains(&group_id));
        if !active {
            return false;
        }
        with_state(|s| {
            s.active_groups.remove(&group_id);
        });
        true
    };
    sync_active_to_shared();
    changed
}

/// 开启/关闭私聊，返回是否改变了状态
pub fn toggle_private_chat(user_id: u64, enable: bool) -> bool {
    let changed = if enable {
        let already = with_state(|s| s.active.contains(&user_id));
        if already {
            return false;
        }
        with_state(|s| {
            s.active.insert(user_id);
        });
        true
    } else {
        let active = with_state(|s| s.active.contains(&user_id));
        if !active {
            return false;
        }
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
    let (groups, users) = with_state(|s| (s.active_groups.clone(), s.active.clone()));
    with_shared_state(|s| s.sync_active(&groups, &users));
}

// ── 消息处理 ────────────────────────────────────────────────────

/// 检查用户是否是管理员
pub(crate) fn is_admin(user_id: u64) -> bool {
    let admin = config::get().admin_qq;
    admin == 0 || admin == user_id
}
