//! 真实世界输入：她能看到的世界不是编出来的
//!
//! 「她在看直播」如果只是一句话，那它永远是假的——说出来没有内容，
//! 事后没有痕迹，被追问就只剩现编。本模块把真实世界（B 站直播间）的
//! 事实接进她的感官：
//!
//! - 开播/下播是**真实状态转变**，转述入流，并作为「感官唤醒」挂进意图堆
//! - 直播中的弹幕是**真实素材**，递给正在看的她（[`crate::activity`]），
//!   她说到这件事时引用的细节是真的，不是编的
//! - 她没在看的时候，弹幕不入流——不在场就听不到，这是诚实
//!
//! 铁律（沿用感官层纪律）：
//! - 零抒情：产出一律是转述 + 引号的机械事实
//! - 网页是不可信输入：弹幕文本过与记忆固化同源的滤壳，命中即丢弃
//! - 感知层不是决策层：开播了要不要去看，由她在回神里自己决定
//!   （意图堆的 reason 是转述的事实，不是替她产生的意愿）

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::{debug, info, warn};

use crate::config;
use crate::util;

/// 世界模型：把一次次巡视沉淀成房间的「记得」与「预期」——熟脸、作息、
/// 与平常的偏差、现场质感。它把世界层从「此刻的快照」拓展成「她住在这里」。
pub(crate) mod model;

/// 一条弹幕（转述前的原始事实）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Danmaku {
    pub uname: String,
    /// 接口给的时间线文本（格式不保证，原样转述）
    pub timeline: String,
    pub text: String,
}

impl Danmaku {
    /// 判重键：同一个人在同一时刻说的同一句话只算一条
    fn key(&self) -> String {
        format!("{}|{}|{}", self.timeline, self.uname, self.text)
    }
}

/// 一个房间的直播状态（机械事实）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RoomState {
    /// 0 下播 / 1 直播 / 2 轮播
    pub live_status: u8,
    pub title: String,
    /// 分区（父分区·子分区）
    pub area: String,
    pub online: u64,
}

impl RoomState {
    pub(crate) fn is_live(&self) -> bool {
        self.live_status == 1
    }

    /// 一行机械描述：《标题》（分区），在线 N 人
    pub(crate) fn brief(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.title.is_empty() {
            parts.push(format!("《{}》", self.title));
        }
        if !self.area.is_empty() {
            parts.push(format!("（{}）", self.area));
        }
        parts.push(format!("在线 {} 人", self.online));
        parts.join("")
    }
}

/// 直播状态的转变（唯一会产生"事件"的地方）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    WentLive,
    EndedLive,
    None,
}

/// 比较前后状态，得出转变（纯函数，可测）
fn transition(prev: Option<&RoomState>, cur: &RoomState) -> Transition {
    let was_live = prev.map(|p| p.is_live()).unwrap_or(false);
    match (was_live, cur.is_live()) {
        (false, true) => Transition::WentLive,
        (true, false) => Transition::EndedLive,
        _ => Transition::None,
    }
}

/// 巡视缓存：房间状态 + 每房最近见过的弹幕键（判重用）
#[derive(Debug, Default, Serialize, Deserialize)]
struct WorldCache {
    #[serde(default)]
    rooms: HashMap<String, RoomState>,
    #[serde(default)]
    seen: HashMap<String, Vec<String>>,
}

/// 每个房间记住的弹幕键上限：只够判"这几分钟见过的"，不做永久日志
const SEEN_CAP: usize = 80;
/// 单轮每房最多转述的弹幕条数：素材要有，但不能把她淹了
const DANMAKU_PER_POLL_CAP: usize = 12;
/// 弹幕正文转述前的截断长度（字符）
const DANMAKU_TEXT_CAP: usize = 60;

static CACHE: Mutex<Option<WorldCache>> = Mutex::new(None);
static POLLING: AtomicBool = AtomicBool::new(false);
static LAST_POLL: AtomicU64 = AtomicU64::new(0);

fn cache_path() -> std::path::PathBuf {
    config::data_dir().join("mind").join("world_live.json")
}

fn load_cache() -> WorldCache {
    let Ok(content) = std::fs::read_to_string(cache_path()) else {
        return WorldCache::default();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "world: 巡视缓存解析失败，按空处理");
        WorldCache::default()
    })
}

fn save_cache(cache: &WorldCache) {
    let Ok(json) = serde_json::to_string_pretty(cache) else {
        warn!("world: 巡视缓存序列化失败");
        return;
    };
    if let Err(e) = util::atomic_write(cache_path(), json) {
        warn!(error = %e, "world: 巡视缓存落盘失败");
    }
}

/// 她关注的直播间的此刻概况（配置 + 上次巡到的状态）
pub(crate) struct LiveBrief {
    pub room_id: u64,
    /// 她怎么称呼这个主播（配置项，接口不给名字就不硬凑）
    pub name: String,
    /// 还没巡到过就是 None
    pub state: Option<RoomState>,
}

fn room_display_name(name: &str, room_id: u64) -> String {
    if name.trim().is_empty() {
        format!("房间{room_id}")
    } else {
        name.trim().to_string()
    }
}

/// 直播间概况（读缓存，纯读不巡）
pub(crate) fn live_brief() -> Vec<LiveBrief> {
    let cfg = config::get();
    let cache_guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache_guard.as_ref();
    cfg.world
        .live_rooms
        .iter()
        .map(|room| LiveBrief {
            room_id: room.room_id,
            name: room_display_name(&room.name, room.room_id),
            state: cache
                .and_then(|c| c.rooms.get(&room.room_id.to_string()).cloned()),
        })
        .collect()
}

// ── 接口 ────────────────────────────────────────────────────────

fn info_url(room_id: u64) -> String {
    format!("https://api.live.bilibili.com/room/v1/Room/get_info?room_id={room_id}")
}

fn danmaku_url(room_id: u64) -> String {
    format!("https://api.live.bilibili.com/xlive/web-room/v1/dM/gethistory?roomid={room_id}")
}

/// 解析房间状态（纯函数，可测）
fn parse_room_info(body: &str) -> Result<RoomState, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("接口返回 code={code}"));
    }
    let data = value.get("data").ok_or("响应缺少 data")?;
    let live_status = data
        .get("live_status")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let parent = data
        .get("parent_area_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let sub = data.get("area_name").and_then(|v| v.as_str()).unwrap_or("");
    let area = match (parent.is_empty(), sub.is_empty()) {
        (true, true) => String::new(),
        (false, true) => parent.to_string(),
        (true, false) => sub.to_string(),
        (false, false) => format!("{parent}·{sub}"),
    };
    let online = data.get("online").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok(RoomState {
        live_status,
        title,
        area,
        online,
    })
}

/// 解析弹幕历史（纯函数，可测）
fn parse_danmaku(body: &str) -> Result<Vec<Danmaku>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("接口返回 code={code}"));
    }
    let Some(items) = value
        .get("data")
        .and_then(|d| d.get("room"))
        .and_then(|r| r.as_array())
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in items {
        // uname 理论上是字符串；字段变形时宁可丢一条也不写错人
        let uname = match item.get("uname") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => continue,
        };
        let timeline = item
            .get("timeline")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text = item
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            continue;
        }
        out.push(Danmaku {
            uname,
            timeline,
            text,
        });
    }
    Ok(out)
}

/// 弹幕 → 转述行；滤壳命中或空文本返回 None（纯函数，可测）
fn render_danmaku_line(d: &Danmaku) -> Option<String> {
    let text: String = d.text.chars().take(DANMAKU_TEXT_CAP).collect();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // 网页是不可信输入：与记忆固化同源的滤壳
    if !crate::anti_injection::check_memory_entry(text).passed {
        crate::mind::security::log_event(0, "world_danmaku", "rejected", text);
        return None;
    }
    let label = format!("弹幕·{} {}", d.uname, d.timeline);
    Some(format!("[{}] “{}”", label.trim(), text))
}

/// 取没见过的弹幕，并把新键记进 `seen`（判重，纯函数，可测）
fn take_unseen(seen: &mut Vec<String>, batch: &[Danmaku], cap: usize) -> Vec<Danmaku> {
    let mut fresh = Vec::new();
    for d in batch.iter().take(DANMAKU_PER_POLL_CAP) {
        let key = d.key();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        fresh.push(d.clone());
    }
    if seen.len() > cap {
        let drop = seen.len() - cap;
        seen.drain(..drop);
    }
    fresh
}

/// 看直播时随行的现场质感（在线轨迹 + 弹幕节奏，全是量出来的数）
pub(crate) fn live_texture(room_id: u64) -> Option<String> {
    model::texture_line(room_id)
}

/// 她关注的直播间的「此刻 + 记得 + 预期」概况（感官事实，零抒情）
///
/// 不在看的时候也递给她：一个活在世界里的存在，对关注的地方是有连续感的——
/// 记得主播的作息、认得常来的面孔、看得见这一场与平常的不同。
pub(crate) fn context_block() -> Option<String> {
    let cfg = config::get();
    if !cfg.world.enabled || cfg.world.live_rooms.is_empty() {
        return None;
    }
    let mut lines = vec!["# 你关注的直播间".to_string()];
    let mut any = false;
    for room in live_brief() {
        let name = &room.name;
        let mut seg: Vec<String> = Vec::new();
        match room.state.as_ref() {
            Some(s) if s.is_live() => {
                seg.push(format!("「{name}」在播 {}", s.brief()));
                if let Some(t) = model::texture_line(room.room_id) {
                    seg.push(t);
                }
                if let Some(a) = model::absence_line(room.room_id) {
                    seg.push(a);
                }
            }
            _ => seg.push(format!("「{name}」这会儿没在播")),
        }
        if let Some(r) = model::rhythm_line(room.room_id) {
            seg.push(r);
        }
        if let Some(f) = model::familiarity_line(room.room_id) {
            seg.push(f);
        }
        lines.push(seg.join("；"));
        any = true;
    }
    any.then(|| lines.join("\n"))
}

// ── 巡视 ────────────────────────────────────────────────────────

/// 周期入口：按 `world.poll_interval_secs` 的节奏巡一圈
///
/// 在维护线程上调用；网络 IO 不进 1ms 主循环。
pub(crate) fn tick() {
    let cfg = config::get();
    if !cfg.world.enabled || cfg.world.live_rooms.is_empty() {
        return;
    }
    let now = util::now_secs();
    let last = LAST_POLL.load(Ordering::Relaxed);
    if now.saturating_sub(last) < cfg.world.poll_interval_secs.max(30) {
        return;
    }
    refresh_now();
}

/// 立刻巡一圈（她的工具路径要"此刻真的在播什么"时用）
pub(crate) fn refresh_now() {
    // 后台正在巡就直接用缓存：同一时刻不发两轮请求
    if POLLING.swap(true, Ordering::AcqRel) {
        return;
    }
    LAST_POLL.store(util::now_secs(), Ordering::Relaxed);
    poll_once();
    POLLING.store(false, Ordering::Release);
}

fn poll_once() {
    let cfg = config::get();

    // 阶段一（锁外）：取真实世界。网络 IO 不占着锁——工具路径还要读缓存
    struct Polled {
        room_id: u64,
        name: String,
        state: RoomState,
        batch: Vec<Danmaku>,
    }
    let mut polled: Vec<Polled> = Vec::new();
    for room in &cfg.world.live_rooms {
        let name = room_display_name(&room.name, room.room_id);
        let Ok(body) = crate::util::get_text(&info_url(room.room_id)) else {
            debug!(room_id = room.room_id, "world: 房间状态取不到，跳过");
            continue;
        };
        let state = match parse_room_info(&body) {
            Ok(state) => state,
            Err(e) => {
                warn!(room_id = room.room_id, error = %e, "world: 房间状态解析失败");
                continue;
            }
        };

        // 弹幕只在真的直播时取（她没在看也会被递进素材，但只有
        // 正在看这个房间的她收得到——见 dispatch_room）
        let mut batch = Vec::new();
        if state.is_live()
            && let Ok(body) = crate::util::get_text(&danmaku_url(room.room_id))
        {
            match parse_danmaku(&body) {
                Ok(items) => batch = items,
                Err(e) => debug!(room_id = room.room_id, error = %e, "world: 弹幕解析失败"),
            }
        }
        polled.push(Polled {
            room_id: room.room_id,
            name,
            state,
            batch,
        });
    }

    // 阶段二（锁内）：与上次巡视做差分——判重与状态转变都在这里定
    struct Pending {
        room_id: u64,
        name: String,
        prev: Option<RoomState>,
        state: RoomState,
        fresh: Vec<Danmaku>,
    }
    let mut pending: Vec<Pending> = Vec::new();
    {
        let mut cache_guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let cache = cache_guard.get_or_insert_with(load_cache);
        for item in polled {
            let key = item.room_id.to_string();
            let fresh = if item.batch.is_empty() {
                Vec::new()
            } else {
                take_unseen(cache.seen.entry(key.clone()).or_default(), &item.batch, SEEN_CAP)
            };
            let prev = cache.rooms.insert(key, item.state.clone());
            pending.push(Pending {
                room_id: item.room_id,
                name: item.name,
                prev,
                state: item.state,
                fresh,
            });
        }
        save_cache(cache);
    }

    // 阶段三（锁外）：落地——入流/唤醒/素材会进心灵与活动层，不在锁内调用
    for item in pending {
        dispatch_room(
            item.room_id,
            &item.name,
            item.prev.as_ref(),
            &item.state,
            &item.fresh,
        );
    }
}

/// 一轮巡视的落地：感官入流 + 感官唤醒 + 素材递给正在看的她
fn dispatch_room(
    room_id: u64,
    name: &str,
    prev: Option<&RoomState>,
    state: &RoomState,
    danmaku: &[Danmaku],
) {
    let now = util::now_secs();
    match transition(prev, state) {
        Transition::WentLive => {
            let line = format!(
                "[直播间·{name} {}] 开播了：{}",
                util::hh_mm(now),
                state.brief()
            );
            push_sensation(line);
            info!(room_id, name, title = %state.title, "world: 开播");
            // 世界模型：记一场，量出「这场与平常的偏差」——晚开/早开是真实事实
            let deviations = model::note_live_start(room_id, &state.title, now);
            for fact in &deviations {
                push_sensation(format!("[直播间·{name}] {fact}"));
            }
            // 感官唤醒：reason 是转述的事实，看不看由她自己决定；
            // 有偏差时一并带上，她一眼看得见「今晚不太一样」在哪
            let mut reason = format!("感官：你关注的主播「{name}」开播了：{}", state.brief());
            for fact in &deviations {
                reason.push_str(&format!("。{fact}"));
            }
            let mut plan = crate::mind::WakePlan::new(crate::mind::WakeKind::Idle, now, reason)
                .with_urgency(crate::mind::Urgency::Soon);
            let announce = config::get().world.announce_group;
            if announce > 0 {
                plan.target_group = Some(announce);
            }
            crate::mind::add_wake_plan(plan);
        }
        Transition::EndedLive => {
            let line = format!(
                "[直播间·{name} {}] 下播了（刚才：{}）",
                util::hh_mm(now),
                state.brief()
            );
            push_sensation(line);
            info!(room_id, name, "world: 下播");
            // 世界模型：结算这一场，量出「这场比平常长/短」等事实
            for fact in model::note_live_end(room_id, now) {
                push_sensation(format!("[直播间·{name}] {fact}"));
            }
            // 她若正在看这个房间，这件事到此为止
            crate::activity::on_live_ended(room_id);
        }
        Transition::None => {}
    }

    // 现场记账（世界模型）：在线采样 + 弹幕节奏 + 熟脸 + 这场出现过的人。
    // 这是房间的客观记录，与她看没看无关（弹幕正文另论——不在场听不到）。
    if state.is_live() {
        let unames: Vec<String> = danmaku.iter().map(|d| d.uname.clone()).collect();
        model::note_live_tick(room_id, state.online, &unames, now);
    }

    // 弹幕素材：递给正在看这个房间的她（她没在看就什么都没有——不在场听不到）
    if !danmaku.is_empty() {
        let lines: Vec<String> = danmaku.iter().filter_map(render_danmaku_line).collect();
        if !lines.is_empty() {
            crate::activity::note_world_facts(room_id, &lines);
        }
    }
}

fn push_sensation(line: String) {
    crate::mind::stream::push(crate::mind::StreamEvent::new(
        crate::mind::StreamKind::Sensation,
        line,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(live: bool) -> RoomState {
        RoomState {
            live_status: if live { 1 } else { 0 },
            title: "深夜电台".into(),
            area: "虚拟主播·聊天".into(),
            online: 12000,
        }
    }

    #[test]
    fn transition_only_on_real_live_changes() {
        assert_eq!(
            transition(Some(&room(false)), &room(true)),
            Transition::WentLive
        );
        assert_eq!(
            transition(Some(&room(true)), &room(false)),
            Transition::EndedLive
        );
        assert_eq!(transition(None, &room(true)), Transition::WentLive);
        // 第一次看到一个下播的房间不是事件：她不知道它"刚刚"下播
        assert_eq!(transition(None, &room(false)), Transition::None);
        assert_eq!(transition(Some(&room(true)), &room(true)), Transition::None);
    }

    #[test]
    fn parses_room_info() {
        let body = r#"{"code":0,"data":{"room_id":1,"live_status":1,"title":" 深夜电台 ",
            "parent_area_name":"虚拟主播","area_name":"聊天","online":12345}}"#;
        let state = parse_room_info(body).expect("应当能解析");
        assert!(state.is_live());
        assert_eq!(state.title, "深夜电台");
        assert_eq!(state.area, "虚拟主播·聊天");
        assert_eq!(state.online, 12345);
        assert!(state.brief().contains("《深夜电台》"));
    }

    #[test]
    fn rejects_error_responses() {
        assert!(parse_room_info(r#"{"code":-352,"message":"-352"}"#).is_err());
        assert!(parse_room_info("404 page not found").is_err());
    }

    #[test]
    fn parses_danmaku_and_skips_broken_rows() {
        let body = r#"{"code":0,"data":{"admin":[],"room":[
            {"uid":1,"uname":"路人甲","timeline":"09-23 22:41","text":"哈哈哈哈"},
            {"uid":2,"uname":"路人乙","timeline":"09-23 22:42","text":"  "},
            {"uid":3,"timeline":"09-23 22:43","text":"没有名字"}
        ]}}"#;
        let items = parse_danmaku(body).expect("应当能解析");
        assert_eq!(items.len(), 1, "空文本与缺 uname 的行都不算弹幕");
        assert_eq!(items[0].uname, "路人甲");
    }

    #[test]
    fn unseen_filter_dedups_across_polls() {
        let batch = vec![
            Danmaku {
                uname: "甲".into(),
                timeline: "22:41".into(),
                text: "一".into(),
            },
            Danmaku {
                uname: "乙".into(),
                timeline: "22:41".into(),
                text: "二".into(),
            },
        ];
        let mut seen = Vec::new();
        let first = take_unseen(&mut seen, &batch, SEEN_CAP);
        assert_eq!(first.len(), 2);
        // 下一轮接口还带着旧弹幕：不该重复递给她
        let second = take_unseen(&mut seen, &batch, SEEN_CAP);
        assert!(second.is_empty());
    }

    #[test]
    fn seen_list_is_capped() {
        let mut seen = Vec::new();
        for round in 0..10 {
            let batch: Vec<Danmaku> = (0..DANMAKU_PER_POLL_CAP)
                .map(|i| Danmaku {
                    uname: format!("路人{round}-{i}"),
                    timeline: "22:41".into(),
                    text: format!("第{round}轮第{i}条"),
                })
                .collect();
            let _ = take_unseen(&mut seen, &batch, SEEN_CAP);
        }
        assert!(seen.len() <= SEEN_CAP);
        assert_eq!(seen.len(), SEEN_CAP, "10 轮 × 12 条会把判重表填满到上限");
    }

    #[test]
    fn danmaku_line_is_transcribed_and_truncated() {
        let d = Danmaku {
            uname: "路人甲".into(),
            timeline: "09-23 22:41".into(),
            text: "字".repeat(DANMAKU_TEXT_CAP + 10),
        };
        let line = render_danmaku_line(&d).expect("应当有转述行");
        assert!(line.starts_with("[弹幕·路人甲 09-23 22:41]"));
        assert!(line.contains("“"));
        let quoted: String = line
            .split('“')
            .nth(1)
            .unwrap_or("")
            .chars()
            .take_while(|c| *c != '”')
            .collect();
        assert_eq!(quoted.chars().count(), DANMAKU_TEXT_CAP);
    }
}
