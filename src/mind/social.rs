//! L4 Social World Model：群聊作为一个持续社会环境的世界模型（方案书 v3 §三）
//!
//! 她在群里生活，不是处理消息。本模块为她持续维护一份社会感知：
//! - 谁在热聊、谁走了（参与者注意力，指数衰减）
//! - 现在有几条线在聊、哪条热哪条凉（话题线程，实词 + 时间连续性分割）
//! - 谁和谁熟（PairBond，一来一往累积）
//! - 有没有人在等她（未回应的提问）
//! - 她开口后有没有人接（被忽略感——连续人格的原料）
//!
//! 实现纪律（§1.1 第四命题）：
//! - 世界模型是**感知层**，不是决策层——只被两种方式消费：
//!   门控（低分不唤醒，"这轮没注意到"）与感官注入（[`context_block`]）。
//! - 零 LLM、零网络；纯逻辑与 IO 严格分离，单测全部走纯函数。
//! - 消息入口在 1ms 主循环上：**observe 纯内存更新，不碰磁盘**；
//!   状态由周期 tick（60s）统一落盘，读写经全局锁保证一致。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config;
use crate::mind::recall::{STOPWORDS, topic_overlap};
use crate::util;

// ── 常量（命名常量不进 config，有需要再提升）────────────────────

/// 并行话题上限
const MAX_THREADS: usize = 5;
/// 线程内保留的转述条数
const THREAD_MAX_MESSAGES: usize = 6;
/// 15 分钟无消息 → 强度减半的时间常数
const THREAD_IDLE_SECS: f32 = 900.0;
/// 1 小时无消息 → 关闭移除
const THREAD_CLOSE_SECS: u64 = 3600;
/// 归入既有线程所需的实词命中数
const MATCH_THRESHOLD: usize = 2;
/// 时间连续性：距线程上一条消息不超过该时长时，命中 1 个实词即可归入
/// （真人对话的连续性首先是时间性的：紧跟的回复几乎必然属于当前这条线）
const RECENT_JOIN_SECS: u64 = 90;
/// 短回复判定阈值：实词数低于它的消息只做时间挂靠，永不新建线程
const SHORT_CONTENT_CHARS: usize = 2;
/// 注意力时间常数（秒）：20 分钟衰减到 ~37%
const ATTENTION_TAU: f32 = 1200.0;
/// PairBond 上限
const PAIR_BOND_MAX: f32 = 1.0;
/// 参与者超过该时长无消息即从状态中淡出
const PARTICIPANT_TTL_SECS: u64 = 24 * 3600;

/// 注意力信号权重
const SIGNAL_PLAIN_MESSAGE: f32 = 0.15;
const SIGNAL_NAMED: f32 = 0.30;
const SIGNAL_AT: f32 = 0.70;
const SIGNAL_QUESTION_TO_BOT: f32 = 0.50;
const SIGNAL_ANSWERED_BY_PEER: f32 = 0.40;

/// 话题归入时线程强度的增量
const INTENSITY_STEP: f32 = 0.25;
/// 新建线程的初始强度
const INTENSITY_SPAWN: f32 = 0.3;
/// 感官渲染的"活跃"阈值：全部线程低于此值视为无事发生
const CONTEXT_MIN_INTENSITY: f32 = 0.15;

// ── SpeakScore 权重（可调）──────────────────────────────────────

const W_TOPIC_RELEVANCE: f32 = 0.30;
const W_ATTENTION_MAX: f32 = 0.20;
const W_UNANSWERED: f32 = 0.15;
const W_INTIMACY: f32 = 0.15;
const W_FRESHNESS: f32 = 0.10;
const W_FATIGUE: f32 = 0.15;
const W_SOCIAL_RISK: f32 = 0.20;
const W_RECENT_REPLY: f32 = 0.15;

/// 低于此分不唤醒（她这轮没注意到）
pub const SPEAK_GATE: f32 = 0.30;

/// 话题相关度的饱和分母：命中数达到它即记满分
const RELEVANCE_SATURATION: f32 = 6.0;
/// 线程新鲜窗口：最热线程该时长内有消息视为"正热"
const FRESH_WINDOW_SECS: u64 = 300;
/// 社交风险窗口：她该时长内说过话，再插话有风险
const SOCIAL_RISK_WINDOW_SECS: u64 = 600;
/// 连续发言惩罚窗口
const RECENT_REPLY_WINDOW_SECS: u64 = 180;
/// 她发言后对方继续说话算"被回应"的窗口
const ANSWERED_WINDOW_SECS: u64 = 300;
/// PairBond 相邻一问一答的窗口
const ADJACENT_REPLY_SECS: u64 = 60;
/// 被忽略感的观察窗口：她开口后该时长内没人接就算挂在那里
const IGNORED_WINDOW_SECS: u64 = 600;

// ── 模型 ───────────────────────────────────────────────────────

/// 一个群的完整社会状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialState {
    /// 参与者注意力表
    #[serde(default)]
    pub participants: HashMap<u64, Participant>,
    /// 活跃话题线程（最多 MAX_THREADS 个）
    #[serde(default)]
    pub topics: Vec<TopicThread>,
    /// 说话人熟悉度（无向；外层 key = 较小 uid，内层 key = 较大 uid）
    #[serde(default)]
    pub bonds: HashMap<u64, HashMap<u64, f32>>,
    /// 线程 id 分配器
    #[serde(default)]
    pub next_thread_id: usize,
    /// 上次整表衰减时间（unix 秒）
    #[serde(default)]
    pub updated_at: u64,
    /// 她最近一次开口的踪迹（被忽略感的锚点）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_bot_speech: Option<BotSpeechTrace>,
    /// 自她上次开口后没人接她话的消息数
    #[serde(default)]
    pub ignored_streak: u32,
}

/// 群聊参与者的注意力状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Participant {
    /// 0.0~1.0，指数衰减
    #[serde(default)]
    pub attention: f32,
    /// 最近一次发言（unix 秒）
    #[serde(default)]
    pub last_seen: u64,
    /// 最近一次发言所在线程 id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_topic: Option<usize>,
    #[serde(default)]
    pub msg_count: u64,
}

/// 一个话题线程：一条并行展开的对话线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicThread {
    pub id: usize,
    /// 话题标题：消息中的实词片段（≤10 字，无 LLM，只用于展示与匹配）
    pub title: String,
    pub participants: Vec<u64>,
    /// 0~1，消息密度滑窗值
    pub intensity: f32,
    /// 最近活跃（unix 秒）
    pub last_active: u64,
    /// 线程内最近的转述（旧在上，≤THREAD_MAX_MESSAGES 条）
    pub transcript: Vec<TranscriptLine>,
    /// 未回应的提问
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unanswered: Option<Question>,
    /// 线程内最近一条消息的发言人（PairBond 相邻判定用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_speaker: Option<u64>,
    /// 线程内最近一条消息的时间（unix 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_msg_at: Option<u64>,
}

/// 线程转述的一行：谁说了什么。
/// 结构化而非裸字符串：拉黑清洗可按 speaker 精确过滤，
/// 她自己的发言（is_bot）在线程里一眼可见——"我刚说过什么"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptLine {
    pub speaker: u64,
    pub name: String,
    pub text: String,
    /// 她自己说的话
    #[serde(default)]
    pub is_bot: bool,
}

/// 未回应的提问：有人在等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub from: u64,
    /// 原文（≤60 字，保留 @ 码以判断是否指向她）
    pub text: String,
    pub at: u64,
}

/// 她最近一次开口的踪迹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotSpeechTrace {
    pub at: u64,
    /// 当时挂进去的线程（None = 当时群里没有活跃线）
    pub thread_id: Option<usize>,
}

// ── 内存态（主循环零 IO）───────────────────────────────────────

static STORE_LOCK: Mutex<()> = Mutex::new(());
static STATES: Mutex<Option<HashMap<u64, SocialState>>> = Mutex::new(None);

fn social_path(group_id: u64) -> PathBuf {
    config::data_dir()
        .join("mind")
        .join("social")
        .join(format!("{group_id}.json"))
}

/// 从磁盘读一份状态（缺失/损坏按空处理）
fn load_state_from_disk(group_id: u64) -> SocialState {
    let Ok(content) = fs::read_to_string(social_path(group_id)) else {
        return SocialState::default();
    };
    match serde_json::from_str::<SocialState>(&content) {
        Ok(state) => state,
        Err(e) => {
            warn!(group_id, error = %e, "social: 状态解析失败，按空处理");
            SocialState::default()
        }
    }
}

/// 落盘一份状态（原子 tmp+rename，中断恢复）
fn save_state_to_disk(group_id: u64, state: &SocialState) {
    let path = social_path(group_id);
    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(group_id, error = %e, "social: 创建目录失败");
        return;
    }
    let Ok(json) = serde_json::to_string(state) else {
        warn!(group_id, "social: 状态序列化失败");
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_err() {
        warn!(group_id, "social: 写入临时文件失败");
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        warn!(group_id, error = %e, "social: 状态落盘失败");
    }
}

/// 拿到某群的内存状态句柄：无则从磁盘惰性加载
fn with_state<R>(group_id: u64, f: impl FnOnce(&mut SocialState) -> R) -> R {
    let _lock = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut states = STATES.lock().unwrap_or_else(|e| e.into_inner());
    let map = states.get_or_insert_with(HashMap::new);
    let state = map
        .entry(group_id)
        .or_insert_with(|| load_state_from_disk(group_id));
    f(state)
}

// ── 文本判定（纯函数）──────────────────────────────────────────

/// 消息中可归线的实词数（虚词/标点/空白不算——"哈哈哈"是零）
fn content_char_count(text: &str) -> usize {
    text.chars()
        .filter(|c| c.is_alphanumeric() && !STOPWORDS.contains(*c))
        .count()
}

/// 话题标题：取前 20 字，删停用字/标点/空白，截前 10 字；全虚词则取原文前 6 字
fn extract_title(text: &str) -> String {
    let head: String = text.chars().take(20).collect();
    let stripped: String = head
        .chars()
        .filter(|c| c.is_alphanumeric() && !STOPWORDS.contains(*c))
        .collect();
    let title: String = stripped.chars().take(10).collect();
    if title.is_empty() {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .take(6)
            .collect()
    } else {
        title
    }
}

/// 提问检测：含"？/?"、"X不X"正反问（你来不来/玩不玩）、固定疑问结构、
/// 或以疑问词结尾
fn is_question(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.contains('？') || trimmed.contains('?') {
        return true;
    }
    if ["有没有", "是不是", "能不能", "要不要"]
        .iter()
        .any(|p| trimmed.contains(p))
    {
        return true;
    }
    // "X不X" 正反问：来不来、玩不玩、好不好（是/要/能已列于上面固定结构）
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.windows(3).any(|w| w[1] == '不' && w[0] == w[2]) {
        return true;
    }
    trimmed.ends_with("怎么")
        || trimmed.ends_with("多少")
        || trimmed
            .chars()
            .last()
            .is_some_and(|c| "吗呢谁几".contains(c))
}

/// 消息是否 @ 她
fn ats_bot(text: &str, self_qq: u64) -> bool {
    self_qq > 0 && text.contains(&format!("[CQ:at,qq={self_qq}]"))
}

/// 消息是否点了她的名字
fn names_bot(text: &str, bot_name: &str) -> bool {
    !bot_name.is_empty() && text.contains(bot_name)
}

/// 提问是否在等她（@她或点了她的名字）
fn question_targets_bot(q: &Question, self_qq: u64, bot_name: &str) -> bool {
    ats_bot(&q.text, self_qq) || names_bot(&q.text, bot_name)
}

/// 转述文本上限：线程里只需要最近的话
fn clamp_text(text: &str) -> String {
    text.chars().take(60).collect()
}

/// bond 的无向 key 归一化：(小 uid, 大 uid)
fn bond_key(a: u64, b: u64) -> (u64, u64) {
    if a <= b { (a, b) } else { (b, a) }
}

fn bump_bond(bonds: &mut HashMap<u64, HashMap<u64, f32>>, a: u64, b: u64, delta: f32) {
    let (outer, inner) = bond_key(a, b);
    let entry = bonds.entry(outer).or_default();
    let value = entry.entry(inner).or_default();
    *value = (*value + delta).min(PAIR_BOND_MAX);
}

/// 判读一对接话人之间的关系档位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondLevel {
    /// ≥0.5 熟络
    Close,
    /// ≥0.25 认识
    Known,
    /// 陌生
    Stranger,
}

pub fn bond_level(familiarity: f32) -> BondLevel {
    if familiarity >= 0.5 {
        BondLevel::Close
    } else if familiarity >= 0.25 {
        BondLevel::Known
    } else {
        BondLevel::Stranger
    }
}

// ── 观察（核心逻辑，纯函数）────────────────────────────────────

/// 观察上下文：由调用方提供环境取值，保持核心逻辑可测
pub struct ObserveCtx<'a> {
    pub self_qq: u64,
    pub bot_name: &'a str,
    /// 发言者显示名（None → uid）
    pub display_name: Option<String>,
    /// 她 N 秒内是否回过这个人的话（被回应信号）
    pub is_follow_up: Box<dyn Fn(u64, u64) -> bool + 'a>,
}

/// 处理一条群消息对社会状态的影响。
///
/// 顺序：注意力 → 线程匹配归入/新建 → 提问检测 → PairBond → 被忽略感。
pub fn observe_event(state: &mut SocialState, ctx: &ObserveCtx, user_id: u64, text: &str, ts: u64) {
    if user_id == 0 || text.trim().is_empty() {
        return;
    }
    let self_qq = ctx.self_qq;
    let bot_name = ctx.bot_name;

    // ── 1. 参与者注意力：先衰减再叠加信号 ──
    let mut signal = SIGNAL_PLAIN_MESSAGE;
    if names_bot(text, bot_name) {
        signal += SIGNAL_NAMED;
    }
    if ats_bot(text, self_qq) {
        signal += SIGNAL_AT;
    }
    // 之前发出、还没人接、且在等她的提问 → 她被点名了
    let waiting_on_her = state
        .topics
        .iter()
        .filter_map(|t| t.unanswered.as_ref())
        .any(|q| question_targets_bot(q, self_qq, bot_name));
    if waiting_on_her {
        signal += SIGNAL_QUESTION_TO_BOT;
    }
    // 她刚回过话，对方跟上来继续说——这是被回应
    if (ctx.is_follow_up)(user_id, ANSWERED_WINDOW_SECS) {
        signal += SIGNAL_ANSWERED_BY_PEER;
    }
    let p = state.participants.entry(user_id).or_default();
    p.attention = decay_value(p.attention, ts.saturating_sub(p.last_seen));
    p.attention = (p.attention + signal).clamp(0.0, 1.0);
    p.last_seen = ts;
    p.msg_count += 1;

    // 纯噪声（哈哈哈、纯表情、[图片]）：是社交出席，但不携带话题信息。
    // 只更新注意力与被忽略感，不进线程模型——避免垃圾线程碎片化。
    let text_core = text.replace("[图片]", "");
    let core_chars = content_char_count(&text_core);
    let called_her = names_bot(text, bot_name) || ats_bot(text, self_qq);
    if core_chars == 0 && !called_her && !is_question(&text_core) {
        note_engagement(state, user_id, None, ctx, text, ts);
        return;
    }

    // ── 2. 线程归属：实词命中 + 时间连续性 ──
    // ≥2 实词命中随时可归入；≥1 命中且时间紧跟（≤RECENT_JOIN_SECS）也算——
    // 紧跟的回复几乎必然属于当前这条线，这正是"回复 20 秒前旧话题"的解药。
    let best = state
        .topics
        .iter()
        .rev()
        .map(|t| {
            let corpus = format!(
                "{} {}",
                t.title,
                t.transcript
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            (t.id, t.last_msg_at, topic_overlap(&corpus, &text_core))
        })
        .filter(|&(_, last_at, hits)| {
            let contiguous = last_at.is_some_and(|at| ts.saturating_sub(at) <= RECENT_JOIN_SECS);
            hits >= MATCH_THRESHOLD || (hits >= 1 && contiguous)
        })
        .max_by_key(|&(_, _, hits)| hits)
        .map(|(id, _, _)| id);

    // 短回复（实词不足，如"来/嗯/好"）：紧跟某条线时挂靠为对它的参与，
    // 不转述、不开新线；无线可依时整个忽略——单字不该成为话题。
    let short_reply = best.is_none() && core_chars < SHORT_CONTENT_CHARS;
    let attach_target = if short_reply {
        state
            .topics
            .iter()
            .filter(|t| {
                t.last_msg_at
                    .is_some_and(|at| ts.saturating_sub(at) <= RECENT_JOIN_SECS)
            })
            .max_by_key(|t| t.last_msg_at.unwrap_or(0))
            .map(|t| t.id)
    } else {
        None
    };

    let joined = best
        .map(|id| (id, true))
        .or(attach_target.map(|id| (id, false)));

    let name = ctx
        .display_name
        .clone()
        .unwrap_or_else(|| user_id.to_string());

    let touched: Option<usize> = match joined {
        Some((id, push_line)) => {
            // ── 3. 归入/挂靠既有线程 ──
            let Some(thread) = state.topics.iter_mut().find(|t| t.id == id) else {
                // 理论不可达：id 来自同一列表的迭代；防御性跳过
                return;
            };
            // 上一条留下提问且本条发言人不同 → 这是回应，有人等的事解决了
            if thread
                .unanswered
                .as_ref()
                .is_some_and(|q| q.from != user_id)
            {
                thread.unanswered = None;
            }
            // PairBond：相邻一问一答 +0.05，同线程共同出现 +0.01
            let adjacent = thread.last_speaker.is_some_and(|prev| prev != user_id)
                && thread
                    .last_msg_at
                    .is_some_and(|at| ts.saturating_sub(at) <= ADJACENT_REPLY_SECS);
            let mates: Vec<u64> = thread
                .participants
                .iter()
                .copied()
                .filter(|&m| m != user_id)
                .collect();
            for mate in mates {
                let bond = if adjacent && thread.last_speaker == Some(mate) {
                    0.05
                } else {
                    0.01
                };
                bump_bond(&mut state.bonds, mate, user_id, bond);
            }
            if push_line {
                push_transcript(
                    &mut thread.transcript,
                    TranscriptLine {
                        speaker: user_id,
                        name,
                        text: clamp_text(&text_core),
                        is_bot: false,
                    },
                );
            }
            if !thread.participants.contains(&user_id) {
                thread.participants.push(user_id);
            }
            thread.intensity = (thread.intensity + INTENSITY_STEP).min(1.0);
            thread.last_active = ts;
            thread.last_speaker = Some(user_id);
            thread.last_msg_at = Some(ts);
            note_engagement(state, user_id, Some(id), ctx, text, ts);
            Some(id)
        }
        None if short_reply => {
            // 短回复但无线可依：只算出席
            note_engagement(state, user_id, None, ctx, text, ts);
            None
        }
        None => {
            // ── 4. 新建线程；超出上限时移除最久不活跃的 ──
            if state.topics.len() >= MAX_THREADS
                && let Some(victim) = state
                    .topics
                    .iter()
                    .min_by_key(|t| t.last_active)
                    .map(|t| t.id)
            {
                state.topics.retain(|t| t.id != victim);
            }
            let id = state.next_thread_id;
            state.next_thread_id += 1;
            state.topics.push(TopicThread {
                id,
                title: extract_title(&text_core),
                participants: vec![user_id],
                intensity: INTENSITY_SPAWN,
                last_active: ts,
                transcript: vec![TranscriptLine {
                    speaker: user_id,
                    name,
                    text: clamp_text(&text_core),
                    is_bot: false,
                }],
                unanswered: None,
                last_speaker: Some(user_id),
                last_msg_at: Some(ts),
            });
            note_engagement(state, user_id, Some(id), ctx, text, ts);
            Some(id)
        }
    };
    if let Some(id) = touched {
        set_last_topic(state, user_id, id);
    }

    // ── 5. 提问检测：本条是否留下一个"有人在等"的提问 ──
    if is_question(&text_core)
        && let Some(thread) = state
            .topics
            .iter_mut()
            .find(|t| t.last_speaker == Some(user_id) && t.last_msg_at == Some(ts))
    {
        thread.unanswered = Some(Question {
            from: user_id,
            text: clamp_text(&text_core),
            at: ts,
        });
    }
}

/// 记录"这条消息是否接了她的话"——被忽略感在这里生长或消解
fn note_engagement(
    state: &mut SocialState,
    user_id: u64,
    joined_thread: Option<usize>,
    ctx: &ObserveCtx,
    text: &str,
    ts: u64,
) {
    let Some(trace) = &state.last_bot_speech else {
        return;
    };
    let in_window = ts.saturating_sub(trace.at) <= IGNORED_WINDOW_SECS;
    let engaged = called_her(ctx, text)
        || (ctx.is_follow_up)(user_id, IGNORED_WINDOW_SECS)
        || matches!((trace.thread_id, joined_thread), (Some(t), Some(j)) if t == j);
    if engaged {
        state.last_bot_speech = None;
        state.ignored_streak = 0;
    } else if in_window {
        state.ignored_streak = state.ignored_streak.saturating_add(1);
    }
}

fn called_her(ctx: &ObserveCtx, text: &str) -> bool {
    names_bot(text, ctx.bot_name) || ats_bot(text, ctx.self_qq)
}

fn set_last_topic(state: &mut SocialState, user_id: u64, id: usize) {
    if let Some(p) = state.participants.get_mut(&user_id) {
        p.last_topic = Some(id);
    }
}

fn push_transcript(transcript: &mut Vec<TranscriptLine>, line: TranscriptLine) {
    transcript.push(line);
    if transcript.len() > THREAD_MAX_MESSAGES {
        let drop = transcript.len() - THREAD_MAX_MESSAGES;
        transcript.drain(..drop);
    }
}

// ── 衰减与生命周期（纯函数）────────────────────────────────────

/// 指数衰减：value × exp(-Δt / τ)
fn decay_value(value: f32, elapsed_secs: u64) -> f32 {
    let dt = elapsed_secs as f32;
    value * (-dt / ATTENTION_TAU).exp()
}

/// 周期维护：全表注意力衰减 + 线程强度衰减/关闭 + 参与者淡出
pub fn evolve(state: &mut SocialState, now: u64) {
    let elapsed = now.saturating_sub(state.updated_at);
    for p in state.participants.values_mut() {
        p.attention = decay_value(p.attention, elapsed);
    }
    state
        .participants
        .retain(|_, p| now.saturating_sub(p.last_seen) < PARTICIPANT_TTL_SECS);

    state.topics.retain(|t| {
        let since = now.saturating_sub(t.last_active);
        since <= THREAD_CLOSE_SECS && (t.intensity > 0.08 || since < THREAD_IDLE_SECS as u64)
    });
    for t in &mut state.topics {
        let since = now.saturating_sub(t.last_active);
        t.intensity *= (-(since as f32) / THREAD_IDLE_SECS).exp();
    }

    state.updated_at = now;
}

// ── SpeakScore（纯函数）────────────────────────────────────────

/// 门控打分的输入（全部来自 SocialState + 既有状态系统，零 LLM）
pub struct SpeakScoreInput<'a> {
    /// 批次逐条消息 (说话人, 文本)——相关度逐条计算取最大，
    /// 避免跨话题拼接稀释命中
    pub utterances: &'a [(u64, &'a str)],
    /// 她的社交电量比例（0~1；未启用电量时传 1.0）
    pub battery_ratio: f32,
    /// 她与主要发言人的亲密度（0~1）
    pub intimacy: f32,
    /// 她 N 秒内是否在本群说过话（由 SharedState 提供）
    pub spoke_within: Box<dyn Fn(u64) -> bool + 'a>,
}

// ── 感官注入 ───────────────────────────────────────────────────

/// 强度档位词：很热 / 正聊 / 凉了
fn intensity_word(intensity: f32) -> &'static str {
    if intensity >= 0.6 {
        "很热"
    } else if intensity >= 0.3 {
        "正聊"
    } else {
        "凉了"
    }
}

/// 渲染"群里的势"感官块（感知语气，无指令；无活跃线程且无悬案时 None）。
///
/// `bot_spoke_ago`：她本群最近一次发言距今的秒数（None = 很久没说）。
pub fn render_context_block(
    state: &SocialState,
    now: u64,
    self_qq: u64,
    bot_spoke_ago: Option<u64>,
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();

    let mut topics: Vec<&TopicThread> = state
        .topics
        .iter()
        .filter(|t| t.intensity >= CONTEXT_MIN_INTENSITY)
        .collect();
    topics.sort_by(|a, b| b.intensity.total_cmp(&a.intensity));

    for (i, t) in topics.iter().enumerate() {
        let names: Vec<String> = t
            .participants
            .iter()
            .filter(|&&uid| uid != self_qq)
            .map(|&uid| participant_name(state, uid, self_qq))
            .collect();
        let mut line = if i == 0 {
            format!("正在聊：「{}」（{}）", t.title, intensity_word(t.intensity))
        } else {
            format!(
                "另一条线：「{}」（{}）",
                t.title,
                intensity_word(t.intensity)
            )
        };
        if !names.is_empty() {
            line.push_str(&format!("——{} 在聊", names.join("、")));
        }
        if let Some(last) = t.transcript.last() {
            let who = if last.is_bot {
                "你".to_string()
            } else {
                last.name.clone()
            };
            line.push_str(&format!("；最近：{who}“{}”", last.text));
        }
        lines.push(line);

        // 未回应的提问单独成行："有人在等"
        if let Some(q) = &t.unanswered {
            let asker = participant_name(state, q.from, self_qq);
            lines.push(format!("有人在等：{asker}问“{}”还没人接", q.text));
        }
    }

    // 被忽略感：连续人格的原料——递给她，不替她决定怎么办
    if state.ignored_streak >= 1
        && state
            .last_bot_speech
            .as_ref()
            .is_some_and(|t| now.saturating_sub(t.at) <= IGNORED_WINDOW_SECS)
    {
        if state.ignored_streak >= 3 {
            lines.push("你已经好几次开口没人接了".to_string());
        } else {
            lines.push("你刚说的话好像还没人接".to_string());
        }
    }

    if lines.is_empty() {
        return None;
    }

    // 熟络关系（感官口径："他们常一来一往"，不做判断）
    for (a, b, familiarity) in notable_bonds(state) {
        let (na, nb) = (
            participant_name(state, a, self_qq),
            participant_name(state, b, self_qq),
        );
        let word = match bond_level(familiarity) {
            BondLevel::Close => "很熟",
            BondLevel::Known => "认识",
            BondLevel::Stranger => continue,
        };
        lines.push(format!("{na}和{nb}{word}（他们常一来一往）"));
    }

    // 她自己的发言间隔提示：数字转成定性提醒，不报系统口吻的精确值
    if let Some(ago) = bot_spoke_ago
        && ago < SOCIAL_RISK_WINDOW_SECS
    {
        let mins = (ago / 60).max(1);
        lines.push(format!("你 {mins} 分钟前刚说过话，再插话要谨慎"));
    }

    Some(format!(
        "# 群里的势（你的社会感知，供参考）\n{}",
        lines.join("\n")
    ))
}

fn participant_name(state: &SocialState, uid: u64, self_qq: u64) -> String {
    if uid == self_qq {
        return "你".to_string();
    }
    // 转述里出现过的名字优先，否则退回 uid
    state
        .topics
        .iter()
        .flat_map(|t| t.transcript.iter())
        .find(|l| l.speaker == uid && !l.name.is_empty())
        .map(|l| l.name.clone())
        .unwrap_or_else(|| uid.to_string())
}

/// 达到"值得提起"档位的 bond 对（取前三）
fn notable_bonds(state: &SocialState) -> Vec<(u64, u64, f32)> {
    let mut out: Vec<(u64, u64, f32)> = state
        .bonds
        .iter()
        .flat_map(|(&a, inner)| inner.iter().map(move |(&b, &v)| (a, b, v)))
        .filter(|&(a, b, _)| a != b)
        .filter(|&(_, _, v)| v >= 0.25)
        .collect();
    out.sort_by(|x, y| y.2.total_cmp(&x.2));
    out.into_iter().take(3).collect()
}

// ── 她的发言入模 ───────────────────────────────────────────────

/// 把她刚说的话挂进最热的活跃线程——
/// 她下一眼就能在线程里看见自己刚说过什么（P1 复读的解药之一），
/// 并以此为锚点开始观察"有没有人接她的话"。
/// 群里没有活跃线程时不强行建线（那段话由意识流承担）。
pub fn record_bot_speech_in(state: &mut SocialState, text: &str, bot_name: &str, now: u64) {
    let clipped = clamp_text(text);
    if clipped.is_empty() {
        return;
    }
    state.ignored_streak = 0;
    let hottest = state
        .topics
        .iter_mut()
        .filter(|t| now.saturating_sub(t.last_active) <= THREAD_CLOSE_SECS)
        .max_by(|a, b| a.intensity.total_cmp(&b.intensity));
    let thread_id = hottest.map(|t| {
        // 她开口即是该线仍在延续；线上悬着的提问她已经看见了
        t.intensity = (t.intensity + INTENSITY_STEP).min(1.0);
        t.last_active = now;
        t.last_msg_at = Some(now);
        // 她的话不算"一问一答"的锚——她的发言不该制造群友之间的熟悉度
        t.last_speaker = None;
        t.unanswered = None;
        push_transcript(
            &mut t.transcript,
            TranscriptLine {
                speaker: 0,
                name: bot_name.to_string(),
                text: clipped,
                is_bot: true,
            },
        );
        t.id
    });
    state.last_bot_speech = Some(BotSpeechTrace { at: now, thread_id });
}

// ── 清洗 ───────────────────────────────────────────────────────

/// 被拉黑用户的一切社会痕迹：participants、线程参与者、转述行、bonds
pub fn purge_user_in(state: &mut SocialState, user_id: u64) {
    state.participants.remove(&user_id);
    for t in &mut state.topics {
        t.participants.retain(|&uid| uid != user_id);
        t.transcript.retain(|l| l.speaker != user_id);
        if t.unanswered.as_ref().is_some_and(|q| q.from == user_id) {
            t.unanswered = None;
        }
        if t.last_speaker == Some(user_id) {
            t.last_speaker = None;
        }
    }
    state
        .topics
        .retain(|t| !t.participants.is_empty() || !t.transcript.is_empty());
    // 涉及他的 bond：以其为外层 key 的整条删除；作为内层 key 的逐个删除
    state.bonds.remove(&user_id);
    for inner_map in state.bonds.values_mut() {
        inner_map.remove(&user_id);
    }
    state.bonds.retain(|_, inner| !inner.is_empty());
}

// ── IO 包装（入口调用，observe 不落盘）─────────────────────────

/// 消息入口调用：观察一条群消息（纯内存更新，主循环零 IO）
pub fn observe_message(group_id: u64, user_id: u64, text: &str, ts: u64) {
    let cfg = config::get();
    let display_name = crate::person_info::get_display_name(user_id, group_id);
    let ctx = ObserveCtx {
        self_qq: cfg.self_qq,
        bot_name: &cfg.bot_name,
        display_name,
        is_follow_up: Box::new(move |uid, window| {
            crate::read_shared_state(|s| s.is_in_follow_up(group_id, uid, window))
        }),
    };
    with_state(group_id, |state| {
        observe_event(state, &ctx, user_id, text, ts);
        state.updated_at = util::now_secs();
    });
    debug!(group_id, user_id, "social: observed");
}

/// 周期维护：全表衰减 + 线程生命周期 + 落盘（check_periodic 对活跃群逐群调用）
pub fn tick(group_id: u64) {
    let snapshot = with_state(group_id, |state| {
        let had_content =
            !state.topics.is_empty() || !state.participants.is_empty() || state.next_thread_id > 0;
        if had_content {
            evolve(state, util::now_secs());
        }
        had_content.then(|| state.clone())
    });
    if let Some(state) = snapshot {
        save_state_to_disk(group_id, &state);
    }
}

/// 她刚说的话挂进最热线程（回复落地时调用，handler 线程，落盘代价可接受）
pub fn record_bot_speech(group_id: u64, text: &str) {
    let cfg = config::get();
    let snapshot = with_state(group_id, |state| {
        record_bot_speech_in(state, text, &cfg.bot_name, util::now_secs());
        state.updated_at = util::now_secs();
        state.clone()
    });
    save_state_to_disk(group_id, &snapshot);
}

/// 渲染"群里的势"感官块（voice::speak_group 注入用；无事发生时 None）
pub fn context_block(group_id: u64) -> Option<String> {
    let cfg = config::get();
    let spoke_ago = crate::read_shared_state(|s| s.last_reply_ago(group_id));
    with_state(group_id, |state| {
        render_context_block(state, util::now_secs(), cfg.self_qq, spoke_ago)
    })
}

/// 门控打分（speak_and_deliver_group 调用；forced 路径不会进来）
pub fn speak_score(group_id: u64, utterances: &[(u64, &str)], primary: u64) -> f32 {
    let cfg = config::get();
    let battery_ratio = if cfg.humanity.social_battery_enabled {
        crate::social_battery::level_percentage(&crate::social_battery::load())
    } else {
        1.0
    };
    let intimacy = crate::person_info::relationship::get_relationship(primary).intimacy;
    let input = SpeakScoreInput {
        utterances,
        battery_ratio,
        intimacy,
        spoke_within: Box::new(move |window| {
            crate::read_shared_state(|s| s.is_in_follow_up(group_id, 0, window))
        }),
    };
    let (bot_name, self_qq) = (cfg.bot_name.clone(), cfg.self_qq);
    let now = util::now_secs();
    with_state(group_id, |state| {
        speak_score_with(state, now, &input, &bot_name, self_qq)
    })
}

/// admin API：读取一份社会状态快照（供 WebUI 展示"群里的势"）
pub fn state_for_admin(group_id: u64) -> SocialState {
    with_state(group_id, |state| state.clone())
}

// speak_score_of 里 unanswered 判定需要 bot_name/self_qq：
// 把它们并入打分上下文而不是塞进 Input（Input 只承载她的内在状态）
/// 计算这批消息对她的"开口势"（speak_score ∈ 约 [-0.5, 1.0]）。
///
/// 正项：话题相关、在场注意力、有人在等她、亲密度、线程正热。
/// 负项：疲惫、刚说过话（社交风险 / 连续发言）。
/// unanswered 判定需要 bot_name/self_qq 环境值，故它们作独立参数传入，
/// [`SpeakScoreInput`] 只承载她的内在状态。
fn speak_score_with(
    state: &SocialState,
    now: u64,
    input: &SpeakScoreInput,
    bot_name: &str,
    self_qq: u64,
) -> f32 {
    let hottest = state
        .topics
        .iter()
        .max_by(|a, b| a.intensity.total_cmp(&b.intensity));

    let topic_relevance = hottest
        .map(|t| {
            let corpus = format!(
                "{} {}",
                t.title,
                t.transcript
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            input
                .utterances
                .iter()
                .map(|(_, text)| topic_overlap(&corpus, text))
                .max()
                .unwrap_or(0) as f32
                / RELEVANCE_SATURATION
        })
        .unwrap_or(0.0)
        .min(1.0);

    let attention_max = input
        .utterances
        .iter()
        .map(|(uid, _)| uid)
        .filter_map(|uid| state.participants.get(uid))
        .map(|p| p.attention)
        .fold(0.0_f32, f32::max);

    let unanswered_bonus = if state.topics.iter().any(|t| {
        t.unanswered
            .as_ref()
            .is_some_and(|q| question_targets_bot(q, self_qq, bot_name))
    }) {
        1.0
    } else {
        0.0
    };

    let thread_freshness = hottest
        .map(|t| now.saturating_sub(t.last_active) <= FRESH_WINDOW_SECS)
        .unwrap_or(false) as i32 as f32;

    let fatigue = (1.0 - input.battery_ratio).clamp(0.0, 1.0);
    let social_risk = (input.spoke_within)(SOCIAL_RISK_WINDOW_SECS) as i32 as f32;
    let recent_reply = (input.spoke_within)(RECENT_REPLY_WINDOW_SECS) as i32 as f32;

    W_TOPIC_RELEVANCE * topic_relevance
        + W_ATTENTION_MAX * attention_max
        + W_UNANSWERED * unanswered_bonus
        + W_INTIMACY * input.intimacy
        + W_FRESHNESS * thread_freshness
        - W_FATIGUE * fatigue
        - W_SOCIAL_RISK * social_risk
        - W_RECENT_REPLY * recent_reply
}

// ── 测试 ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>() -> ObserveCtx<'a> {
        ObserveCtx {
            self_qq: 999,
            bot_name: "洛玖",
            display_name: None,
            is_follow_up: Box::new(|_, _| false),
        }
    }

    #[test]
    fn new_message_opens_thread_with_title() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚有人打游戏吗", 1_000);
        assert_eq!(s.topics.len(), 1);
        let t = &s.topics[0];
        assert_eq!(t.participants, vec![100]);
        assert!((t.intensity - INTENSITY_SPAWN).abs() < 1e-6);
        assert!(t.unanswered.is_some(), "疑问句应留下未回应提问");
        assert!(t.title.contains("打游戏"), "标题应剔除虚词：{}", t.title);
    }

    #[test]
    fn related_message_joins_existing_thread() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚有人打游戏吗", 1_000);
        observe_event(&mut s, &c, 200, "我玩 打瓦的话叫上我", 1_050);
        assert_eq!(s.topics.len(), 1, "紧跟且实词命中应归入同一线程");
        assert_eq!(s.topics[0].participants.len(), 2);
        assert_eq!(s.topics[0].transcript.len(), 2);
        assert!(s.topics[0].unanswered.is_none(), "一问一答后提问应消解");
        let (a, b) = bond_key(100, 200);
        let familiarity = s.bonds[&a][&b];
        assert!(familiarity >= 0.05, "相邻应 +0.05，实际 {familiarity}");
    }

    #[test]
    fn contiguous_single_hit_joins_but_stale_doesnt() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打瓦去不去", 1_000);
        // 只命中"打"一个实词，但时间紧跟 → 归入
        observe_event(&mut s, &c, 200, "打完这把就睡", 1_030);
        assert_eq!(s.topics.len(), 1, "时间连续时单命中即归入");
        // 同样单命中，但时间已断 → 新线程
        observe_event(&mut s, &c, 300, "打电话给妈妈", 1_030 + 600);
        assert_eq!(s.topics.len(), 2, "时间断开时单命中不足以归入");
    }

    #[test]
    fn short_replies_attach_temporally() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打瓦去不去", 1_000);
        // 单字短回复：挂靠当前线，不开新线、不转述
        observe_event(&mut s, &c, 200, "来", 1_050);
        assert_eq!(s.topics.len(), 1);
        assert!(s.topics[0].participants.contains(&200));
        assert_eq!(s.topics[0].transcript.len(), 1, "短回复不进转述");
        // 短回复但已无线紧跟：整个忽略，不生成"来"这种垃圾线程
        observe_event(&mut s, &c, 300, "来", 1_050 + 600);
        assert_eq!(s.topics.len(), 1);
    }

    #[test]
    fn unrelated_message_opens_parallel_thread() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚有人打游戏吗", 1_000);
        observe_event(&mut s, &c, 300, "今天真热啊", 1_600);
        assert_eq!(s.topics.len(), 2, "时间断开的无关消息应开并行线程");
        assert_ne!(s.topics[0].id, s.topics[1].id);
    }

    #[test]
    fn pure_noise_never_spawns_threads() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        observe_event(&mut s, &c, 400, "哈哈哈哈哈哈", 1_100);
        observe_event(&mut s, &c, 500, "[图片]", 1_200);
        assert_eq!(s.topics.len(), 1, "纯笑声/图片不应开线程");
        // 但出席被记录
        assert!(s.participants.contains_key(&400));
    }

    #[test]
    fn thread_cap_evicts_oldest() {
        let mut s = SocialState::default();
        let c = ctx();
        let msgs = [
            "今晚打游戏啊",
            "明天天气怎么样",
            "午饭吃什么好",
            "这部电影真不错",
            "昨晚球赛看了吗",
            "新专辑发布了",
        ];
        for (i, m) in msgs.iter().enumerate() {
            observe_event(&mut s, &c, 100 + i as u64, m, 1_000 + i as u64 * 600);
        }
        assert_eq!(s.topics.len(), MAX_THREADS);
        assert!(!s.topics.iter().any(|t| t.title.contains("打游戏")));
    }

    #[test]
    fn attention_decays_and_accumulates() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "在吗", 1_000);
        let first = s.participants[&100].attention;
        assert!(first >= SIGNAL_PLAIN_MESSAGE);

        // 20 分钟衰减到 ~37%，再来一条普通消息
        observe_event(&mut s, &c, 100, "嗯", 1_000 + 1_200);
        let after = s.participants[&100].attention;
        let expected = decay_value(first, 1_200) + SIGNAL_PLAIN_MESSAGE;
        assert!((after - expected).abs() < 1e-4, "{after} vs {expected}");
    }

    #[test]
    fn at_message_signals_strongly() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "[CQ:at,qq=999] 你怎么看", 1_000);
        assert!(s.participants[&100].attention >= SIGNAL_PLAIN_MESSAGE + SIGNAL_AT);
    }

    #[test]
    fn answered_signal_fires_on_follow_up() {
        let mut s = SocialState::default();
        let mut c = ObserveCtx {
            self_qq: 999,
            bot_name: "洛玖",
            display_name: None,
            is_follow_up: Box::new(|_, _| true),
        };
        observe_event(&mut s, &c, 100, "哈哈哈", 1_000);
        let base = SIGNAL_PLAIN_MESSAGE + SIGNAL_ANSWERED_BY_PEER;
        assert!((s.participants[&100].attention - base).abs() < 1e-6);
        c.is_follow_up = Box::new(|_, _| false);
        observe_event(&mut s, &c, 200, "哈哈", 1_100);
        assert!((s.participants[&200].attention - SIGNAL_PLAIN_MESSAGE).abs() < 1e-6);
    }

    #[test]
    fn question_waiting_on_her_boosts_attention() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "[CQ:at,qq=999] 你来不来", 1_000);
        let before = s.participants[&100].attention;
        observe_event(&mut s, &c, 200, "路过", 1_100);
        assert!(
            s.participants[&200].attention >= SIGNAL_PLAIN_MESSAGE + SIGNAL_QUESTION_TO_BOT - 1e-6
        );
        assert!(before >= SIGNAL_PLAIN_MESSAGE + SIGNAL_AT);
    }

    #[test]
    fn decay_closes_stale_threads() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        evolve(&mut s, 1_000 + THREAD_CLOSE_SECS + 1);
        assert!(s.topics.is_empty());
    }

    #[test]
    fn decay_fades_participants() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "在吗", 1_000);
        evolve(&mut s, 1_000 + PARTICIPANT_TTL_SECS + 10);
        assert!(s.participants.is_empty(), "超过 TTL 的参与者应淡出");
    }

    #[test]
    fn question_detection_covers_forms() {
        assert!(is_question("你来吗"));
        assert!(is_question("几点了？"));
        assert!(is_question("有没有人去看展"));
        assert!(is_question("这是不是坏了"));
        assert!(is_question("然后呢"));
        assert!(is_question("要不要一起"));
        assert!(is_question("你来不来"));
        assert!(is_question("到底玩不玩"));
        assert!(!is_question("哈哈哈"));
        assert!(!is_question("我回来了"));
        assert!(!is_question("好的"));
        assert!(!is_question("差不多了"));
    }

    #[test]
    fn title_extraction_strips_fillers() {
        assert_eq!(extract_title("你们是不是机器人啊"), "机器人");
        assert_eq!(extract_title("今晚打瓦？"), "今晚打瓦");
        assert!(!extract_title("的了吗呢").is_empty());
    }

    #[test]
    fn bond_accumulates_and_clamps() {
        let mut bonds = HashMap::new();
        for _ in 0..30 {
            bump_bond(&mut bonds, 7, 3, 0.05);
        }
        let (a, b) = bond_key(3, 7);
        assert_eq!(bonds[&a][&b], PAIR_BOND_MAX);
        assert_eq!(bond_level(0.6), BondLevel::Close);
        assert_eq!(bond_level(0.3), BondLevel::Known);
        assert_eq!(bond_level(0.1), BondLevel::Stranger);
    }

    #[test]
    fn speak_score_rewards_waiting_question_and_penalizes_chattiness() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "[CQ:at,qq=999] 你到底玩不玩", 1_000);

        let input = |spoke: bool| SpeakScoreInput {
            utterances: &[(100, "随便说点什么")],
            battery_ratio: 1.0,
            intimacy: 0.0,
            spoke_within: Box::new(move |_| spoke),
        };
        let waiting = speak_score_with(&s, 1_100, &input(false), "洛玖", 999);
        assert!(
            waiting >= W_UNANSWERED - 1e-6,
            "在等她的提问应抬分：{waiting}"
        );

        let penalized = speak_score_with(&s, 1_100, &input(true), "洛玖", 999);
        assert!(
            penalized < waiting - W_SOCIAL_RISK - W_RECENT_REPLY + 1e-6,
            "刚说过话应明显压分：{penalized} vs {waiting}"
        );
    }

    #[test]
    fn context_block_renders_threads_and_waits() {
        let mut s = SocialState::default();
        let c = ObserveCtx {
            self_qq: 999,
            bot_name: "洛玖",
            display_name: Some("土豆".to_string()),
            is_follow_up: Box::new(|_, _| false),
        };
        observe_event(&mut s, &c, 100, "今晚有人打游戏吗", 1_000);
        observe_event(&mut s, &c, 200, "我玩", 1_050);
        observe_event(&mut s, &c, 100, "那打瓦？", 1_100);

        let block = render_context_block(&s, 1_200, 999, None).expect("有活跃线程应有感官块");
        assert!(block.contains("群里的势"));
        assert!(block.contains("很热") || block.contains("正聊"));
        assert!(block.contains("土豆"));
        assert!(block.contains("还没人接"), "悬着的提问应被看见");
    }

    #[test]
    fn context_block_warns_when_ignored() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        record_bot_speech_in(&mut s, "带我一个", "洛玖", 1_100);
        // 之后三条消息都开了别的线、没人叫她
        observe_event(&mut s, &c, 200, "今天天气真好", 1_200);
        observe_event(&mut s, &c, 300, "中午吃什么啊", 1_300);
        let block = render_context_block(&s, 1_400, 999, None).expect("被忽略感应有感官行");
        assert!(block.contains("没人接"));
    }

    #[test]
    fn context_block_none_when_all_cold() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        for t in &mut s.topics {
            t.intensity = 0.1;
        }
        assert!(render_context_block(&s, 1_100, 999, None).is_none());
    }

    #[test]
    fn bot_speech_enters_thread_and_starts_watch() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "你到底玩不玩", 1_000);
        record_bot_speech_in(&mut s, "来 来 来", "洛玖", 1_100);
        let t = &s.topics[0];
        let last = t.transcript.last().expect("她的发言应入线");
        assert!(last.is_bot);
        assert_eq!(last.name, "洛玖");
        assert!(t.unanswered.is_none(), "她开了口，等待提示不再成立");
        assert!(s.last_bot_speech.is_some(), "开口后开始观察有没有人接");
    }

    #[test]
    fn engaging_message_clears_ignored_feeling() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        record_bot_speech_in(&mut s, "带我一个", "洛玖", 1_100);
        observe_event(&mut s, &c, 200, "我也来", 1_150);
        assert_eq!(s.ignored_streak, 0, "有人接了她的话");
        assert!(s.last_bot_speech.is_none());
    }

    #[test]
    fn purge_removes_every_trace() {
        let mut s = SocialState::default();
        let c = ctx();
        observe_event(&mut s, &c, 100, "今晚打游戏啊", 1_000);
        observe_event(&mut s, &c, 666, "我玩 我玩", 1_050);
        bump_bond(&mut s.bonds, 100, 666, 0.5);
        bump_bond(&mut s.bonds, 666, 200, 0.4);
        purge_user_in(&mut s, 666);
        assert!(!s.participants.contains_key(&666));
        assert!(s.topics.iter().all(|t| !t.participants.contains(&666)));
        assert!(
            s.topics
                .iter()
                .all(|t| t.transcript.iter().all(|l| l.speaker != 666))
        );
        assert!(
            s.bonds
                .iter()
                .all(|(&outer, inner)| outer != 666 && !inner.contains_key(&666))
        );
    }

    #[test]
    fn clamp_text_limits_length() {
        let long = "啊".repeat(200);
        assert_eq!(clamp_text(&long).chars().count(), 60);
    }
}
