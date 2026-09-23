//! 活动：她正在做的事，是一个真实占用注意力的过程
//!
//! 旧实现的"活动"是关键词嗅探她说过的话（"休息"→ 睡 8 小时），起一个
//! 固定时长的定时器，最后只写两条日志——没有任何消费方。说出来的活动
//! 没有内容、没有痕迹、没有代价，这就是"模拟感"的来源。
//!
//! 现在的活动是三件真实的事：
//!
//! 1. **由行为开合，不由措辞开合**：她用 `do_activity` 工具真的开始做、
//!    真的停下（表达与回神两条路径共用），不再从她的话里猜
//! 2. **有真实素材**：看直播时，[`crate::world`] 递进来的是真实房间的
//!    标题、在线人数、真实弹幕——她说起这件事时引用的细节是真的；
//!    停下时素材汇成一条经历入流，供回忆与日记引用
//! 3. **有代价**：活动占用她的注意力（[`occupancy`]）——回消息变慢、
//!    话变短、被打断是常态。手上有事的人不会永远秒回
//!
//! 铁律：本模块只记机械事实（何时开始、什么类型、素材原文），
//! 不替她解释"看得开心吗"——那是她的表达，不是状态注解。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tracing::{debug, info};

use crate::config;
use crate::util;

// ── 活动类型 ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityKind {
    /// 看直播（素材来自真实的直播间）
    WatchLive,
    /// 打游戏
    Gaming,
    /// 看书/看视频/看资讯
    Reading,
    /// 训练/运动
    Training,
    /// 吃饭
    Eating,
    /// 工作/学习
    Working,
    /// 外出
    Outing,
    /// 洗澡
    Bathing,
    /// 睡觉/休息
    Sleeping,
    /// 其他（标题说明）
    Other,
}

impl ActivityKind {
    /// 工具参数里的 token（英文短词，模型不容易串味）
    pub(crate) fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "watch_live" | "live" => Some(Self::WatchLive),
            "gaming" | "game" => Some(Self::Gaming),
            "reading" | "read" => Some(Self::Reading),
            "training" | "sport" => Some(Self::Training),
            "eating" | "eat" => Some(Self::Eating),
            "working" | "work" | "study" => Some(Self::Working),
            "outing" | "out" => Some(Self::Outing),
            "bathing" | "bath" | "shower" => Some(Self::Bathing),
            "sleeping" | "sleep" => Some(Self::Sleeping),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub(crate) fn token(self) -> &'static str {
        match self {
            Self::WatchLive => "watch_live",
            Self::Gaming => "gaming",
            Self::Reading => "reading",
            Self::Training => "training",
            Self::Eating => "eating",
            Self::Working => "working",
            Self::Outing => "outing",
            Self::Bathing => "bathing",
            Self::Sleeping => "sleeping",
            Self::Other => "other",
        }
    }

    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::WatchLive => "看直播",
            Self::Gaming => "打游戏",
            Self::Reading => "看书/看视频",
            Self::Training => "训练/运动",
            Self::Eating => "吃饭",
            Self::Working => "工作/学习",
            Self::Outing => "外出",
            Self::Bathing => "洗澡",
            Self::Sleeping => "睡觉/休息",
            Self::Other => "手上的事",
        }
    }

    /// 占用注意力的强度 0.0~1.0：越占用，回消息越慢越短
    pub(crate) fn intensity(self) -> f32 {
        match self {
            Self::WatchLive => 0.55,
            Self::Gaming => 0.8,
            Self::Reading => 0.6,
            Self::Training => 0.7,
            Self::Eating => 0.4,
            Self::Working => 0.85,
            Self::Outing => 0.5,
            Self::Bathing => 0.9,
            Self::Sleeping => 1.0,
            Self::Other => 0.5,
        }
    }

    /// 默认时长（秒）：到点自动收尾；看直播由下播事件提前收尾
    fn default_duration(self) -> u64 {
        match self {
            Self::WatchLive => 7200,
            Self::Gaming => 7200,
            Self::Reading => 3600,
            Self::Training => 1800,
            Self::Eating => 1200,
            Self::Working => 7200,
            Self::Outing => 3600,
            Self::Bathing => 1200,
            Self::Sleeping => 28800,
            Self::Other => 3600,
        }
    }
}

/// 活动阶段（进度是机械事实，感受是她的事）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityPhase {
    JustStarted,
    InProgress,
    NearEnd,
}

impl ActivityPhase {
    fn from_progress(progress: f64) -> Self {
        if progress >= 0.8 {
            Self::NearEnd
        } else if progress >= 0.1 {
            Self::InProgress
        } else {
            Self::JustStarted
        }
    }
}

// ── 会话 ────────────────────────────────────────────────────────

/// 一次正在进行的活动
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActivitySession {
    pub kind: ActivityKind,
    /// 这件事具体是什么（看直播时是直播间的真实标题）
    pub title: String,
    /// 看直播时绑定的真实房间
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<u64>,
    pub started_at: u64,
    pub expires_at: u64,
    /// 真实素材（世界层递进来的转述行，原文保存）
    #[serde(default)]
    pub facts: Vec<String>,
}

/// 会话素材上限：够她引用，不撑爆 prompt
const FACT_CAP: usize = 24;
/// prompt 里带几行素材：带太多就成了逐字转播
const FACTS_IN_PROMPT: usize = 6;

impl ActivitySession {
    fn new(kind: ActivityKind, title: String, room_id: Option<u64>, now: u64) -> Self {
        ActivitySession {
            kind,
            title,
            room_id,
            started_at: now,
            expires_at: now + kind.default_duration(),
            facts: Vec::new(),
        }
    }

    fn elapsed_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.started_at)
    }

    fn progress_at(&self, now: u64) -> f64 {
        let total = self.expires_at.saturating_sub(self.started_at).max(1);
        (self.elapsed_secs(now) as f64 / total as f64).min(1.0)
    }

    fn phase_at(&self, now: u64) -> ActivityPhase {
        ActivityPhase::from_progress(self.progress_at(now))
    }

    /// 一行机械描述：看直播《深夜电台》（七海Nana7mi）
    fn describe_line(&self) -> String {
        if self.title.is_empty() {
            self.kind.describe().to_string()
        } else {
            format!("{}《{}》", self.kind.describe(), self.title)
        }
    }

    fn push_fact(&mut self, line: String) {
        self.facts.push(line);
        if self.facts.len() > FACT_CAP {
            let drop = self.facts.len() - FACT_CAP;
            self.facts.drain(..drop);
        }
    }
}

// ── 状态（当前会话 + 落盘恢复） ─────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct ActivityStore {
    #[serde(default)]
    current: Option<ActivitySession>,
}

static STATE: Mutex<Option<ActivityStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("mind").join("activity.json")
}

fn load_store() -> ActivityStore {
    let Ok(content) = std::fs::read_to_string(store_path()) else {
        return ActivityStore::default();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "activity: 状态解析失败，按空处理");
        ActivityStore::default()
    })
}

fn with_store<R>(f: impl FnOnce(&mut ActivityStore) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.get_or_insert_with(load_store);
    f(store)
}

fn save_store(store: &ActivityStore) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        tracing::warn!("activity: 状态序列化失败");
        return;
    };
    if let Err(e) = util::atomic_write(store_path(), json) {
        tracing::warn!(error = %e, "activity: 状态落盘失败");
    }
}

// ── 开合 ────────────────────────────────────────────────────────

/// 开始一件事（非直播类）。已在做别的就先收尾。
///
/// 返回给她的确认文字（机械事实，不带抒情）。
pub(crate) fn begin(kind: ActivityKind, title: &str) -> String {
    if kind == ActivityKind::WatchLive {
        // 看直播必须绑定真实房间，走专门的入口
        return match begin_watch_live() {
            Ok(text) => text,
            Err(text) => text,
        };
    }
    let now = util::now_secs();
    let title = title.trim().chars().take(30).collect::<String>();
    let session = ActivitySession::new(kind, title, None, now);
    let line = session.describe_line();
    let handover = with_store(|store| {
        let previous = store.current.replace(session);
        save_store(store);
        previous
    });
    let mut out = String::new();
    if let Some(previous) = handover {
        // 手上的事没做完就被放下：照实收尾入流
        let dropped = finish_session(&previous, now);
        out.push_str(&format!("（{dropped}先放下了）"));
    }
    info!(kind = kind.token(), title = %line, "activity: started");
    out.push_str(&format!("开始：{line}。"));
    crate::mind::push_acted(format!("开始{line}"));
    out
}

/// 开始看直播：绑定一个**此刻真的在播**的真实房间
///
/// 没有房间在播就诚实地说没有——"我在看直播"必须是真的。
pub(crate) fn begin_watch_live() -> Result<String, String> {
    // 先巡一圈：她要知道"此刻"的真实状态，而不是上次巡到的旧闻
    crate::world::refresh_now();
    let rooms = crate::world::live_brief();
    if rooms.is_empty() {
        return Err("你想看的直播这会儿都没开。".to_string());
    }
    let Some(picked) = rooms
        .iter()
        .find(|r| r.state.as_ref().is_some_and(|s| s.is_live()))
    else {
        return Err("你关注的房间这会儿都没在播。".to_string());
    };
    let state = picked.state.clone().expect("刚刚判过在播");
    let now = util::now_secs();
    let session =
        ActivitySession::new(ActivityKind::WatchLive, state.title.clone(), Some(picked.room_id), now);
    let line = session.describe_line();
    let handover = with_store(|store| {
        let previous = store.current.replace(session);
        save_store(store);
        previous
    });
    if let Some(previous) = handover {
        finish_session(&previous, now);
    }
    info!(room_id = picked.room_id, title = %state.title, "activity: 开始看直播");
    crate::mind::push_acted(format!("看起了直播：{}（{}）", state.brief(), picked.name));
    Ok(format!("开始看：{line}（{} 的直播间，{}）。", picked.name, state.brief()))
}

/// 收尾手上正在做的事
///
/// 返回给她的一句确认；素材汇成一条经历入流（供回忆/日记引用）。
pub(crate) fn stop() -> Option<String> {
    let now = util::now_secs();
    let finished = with_store(|store| {
        let session = store.current.take();
        save_store(store);
        session
    })?;
    Some(finish_session(&finished, now))
}

/// 把一个会话记成已发生的事：日志 + 一句行动 + 素材入流，返回收尾句
fn finish_session(session: &ActivitySession, now: u64) -> String {
    let line = session.describe_line();
    let minutes = session.elapsed_secs(now).div_ceil(60).max(1);
    info!(kind = session.kind.token(), minutes, "activity: finished");
    crate::mind::push_acted(format!("{line}告一段落"));
    finish_sensation(&line, &session.facts, minutes);
    format!("{line}（{minutes} 分钟）")
}

/// 下播事件：她若正在看这个房间，这件事到此为止
pub(crate) fn on_live_ended(room_id: u64) {
    let watching = with_store(|store| {
        store
            .current
            .as_ref()
            .is_some_and(|s| s.room_id == Some(room_id))
    });
    if watching {
        let _ = stop();
    }
}

/// 收尾时把素材汇成一条经历入流（转述 + 引号的机械事实）
fn finish_sensation(line: &str, facts: &[String], minutes: u64) {
    if minutes > 0 {
        crate::mind::stream::push(crate::mind::StreamEvent::new(
            crate::mind::StreamKind::Sensation,
            format!("[回看] {line}（{minutes} 分钟）"),
        ));
    }
    if facts.is_empty() {
        return;
    }
    let sample: Vec<&str> = facts.iter().rev().take(4).map(String::as_str).collect();
    crate::mind::stream::push(crate::mind::StreamEvent::new(
        crate::mind::StreamKind::Sensation,
        format!("[回看] {line}期间的片段：{}", sample.join(" / ")),
    ));
}

// ── 素材 ────────────────────────────────────────────────────────

/// 世界层递进来的事实（目前是弹幕）
///
/// 只有她正在看**同一个房间**时才算数：不在场就听不到。
pub(crate) fn note_world_facts(room_id: u64, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    with_store(|store| {
        let Some(session) = store.current.as_mut() else {
            return;
        };
        if session.room_id != Some(room_id) {
            return;
        }
        for line in lines {
            session.push_fact(line.clone());
        }
        save_store(store);
    });
}

// ── 呈现与占用 ──────────────────────────────────────────────────

/// prompt 里的"你正在做的事"块（体验呈现，不带行为指令）
pub(crate) fn context_block() -> Option<String> {
    let now = util::now_secs();
    let session = with_store(|store| store.current.clone())?;
    let minutes = session.elapsed_secs(now).div_ceil(60).max(1);
    let mut lines = vec![
        "# 你正在做的事".to_string(),
        format!(
            "{}（{}，已经 {} 分钟）——这件事正占着你的眼睛和手。",
            session.describe_line(),
            session.phase_at(now).label(),
            minutes
        ),
    ];
    if !session.facts.is_empty() {
        let start = session.facts.len().saturating_sub(FACTS_IN_PROMPT);
        let rendered: Vec<&str> = session.facts[start..].iter().map(String::as_str).collect();
        lines.push(format!("刚刚真实发生：\n{}", rendered.join("\n")));
    }
    // 此刻现场：量出来的在线轨迹 + 弹幕节奏——她说起这场时的“热闹/冷清”是有依据的
    if let Some(room_id) = session.room_id
        && let Some(texture) = crate::world::live_texture(room_id)
    {
        lines.push(format!("此刻现场：{texture}"));
    }
    Some(lines.join("\n"))
}

impl ActivityPhase {
    fn label(self) -> &'static str {
        match self {
            Self::JustStarted => "刚开始",
            Self::InProgress => "正投入",
            Self::NearEnd => "快到尾声",
        }
    }
}

/// 注意力占用 0.0~1.0：手上有事的人回消息更慢、话更少
pub(crate) fn occupancy() -> f32 {
    let now = util::now_secs();
    let Some(session) = with_store(|store| store.current.clone()) else {
        return 0.0;
    };
    let phase_factor = match session.phase_at(now) {
        ActivityPhase::JustStarted => 0.8,
        ActivityPhase::InProgress => 1.0,
        // 快结束了反而收心，开始留意周围
        ActivityPhase::NearEnd => 0.7,
    };
    session.kind.intensity() * phase_factor
}

/// 周期检查：到点自动收尾（活动不该永远挂着）
pub(crate) fn check_activity_progress() {
    let now = util::now_secs();
    let expired = with_store(|store| {
        let done = store
            .current
            .as_ref()
            .is_some_and(|s| now >= s.expires_at)
            .then(|| store.current.take());
        if done.is_some() {
            save_store(store);
        }
        done.flatten()
    });
    if let Some(session) = expired {
        debug!(kind = session.kind.token(), "activity: 到点收尾");
        finish_session(&session, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(kind: ActivityKind, now: u64) -> ActivitySession {
        ActivitySession::new(kind, "深夜电台".into(), Some(42), now)
    }

    #[test]
    fn tool_tokens_round_trip() {
        for kind in [
            ActivityKind::WatchLive,
            ActivityKind::Gaming,
            ActivityKind::Reading,
            ActivityKind::Training,
            ActivityKind::Eating,
            ActivityKind::Working,
            ActivityKind::Outing,
            ActivityKind::Bathing,
            ActivityKind::Sleeping,
            ActivityKind::Other,
        ] {
            assert_eq!(ActivityKind::from_token(kind.token()), Some(kind));
        }
        assert_eq!(ActivityKind::from_token("看直播"), None, "只认工具 token");
    }

    #[test]
    fn progress_and_phase_follow_the_clock() {
        let now = 1_000_000;
        let s = session(ActivityKind::WatchLive, now);
        assert_eq!(s.phase_at(now), ActivityPhase::JustStarted);
        // 一半处正投入
        let half = now + (s.expires_at - s.started_at) / 2;
        assert_eq!(s.phase_at(half), ActivityPhase::InProgress);
        // 八成处快到尾声
        let near = now + (s.expires_at - s.started_at) * 4 / 5;
        assert_eq!(s.phase_at(near), ActivityPhase::NearEnd);
    }

    #[test]
    fn facts_are_capped_and_prompt_shows_the_tail() {
        let mut s = session(ActivityKind::WatchLive, 0);
        for i in 0..FACT_CAP + 10 {
            s.push_fact(format!("[弹幕·路人{i}] “话”"));
        }
        assert_eq!(s.facts.len(), FACT_CAP, "素材表有上限");
        assert!(s.facts.last().expect("非空").contains("路人33"));
    }

    #[test]
    fn intensity_says_work_costs_more_attention_than_eating() {
        assert!(ActivityKind::Working.intensity() > ActivityKind::Eating.intensity());
        assert!(ActivityKind::Sleeping.intensity() >= 1.0);
    }

    #[test]
    fn describe_line_carries_the_real_title() {
        let s = session(ActivityKind::WatchLive, 0);
        assert_eq!(s.describe_line(), "看直播《深夜电台》");
        let mut bare = session(ActivityKind::Other, 0);
        bare.title = String::new();
        assert_eq!(bare.describe_line(), "手上的事");
    }
}
