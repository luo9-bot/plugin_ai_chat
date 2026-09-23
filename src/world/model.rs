//! 世界模型：她对真实世界的「记得」与「预期」
//!
//! 世界层的另一半是**连续性**。巡视（[`super`]）只告诉她「此刻」发生了什么，
//! 但一个活在世界里的存在，对世界是有积累、也有预期的：她认得常来的面孔、
//! 记得主播的作息、会在「比平常晚了两小时还没开播」时心里咯噔一下。这些
//! 不是台词，是**从真实历史里量出来的事实**——本模块把一次次巡视沉淀成
//! 房间的持久模型，再把「与平常的偏差」作为机械事实递给她。
//!
//! 纪律（沿用世界层铁律）：
//! - 零抒情：产出一律是**数值比较**（几点开播、播多久、在线多少、弹幕几条），
//!   不写「意外」「惊喜」——惊不惊、喜不喜是她的诠释，不是状态注解
//! - 事实是从真实历史聚合出来的（开播时刻、时长、在线峰值、谁来过），
//!   永不臆造「她大概喜欢」
//! - 惊讶是**预测误差**：先有「平常」这个基线，才有「这场不太一样」这个事实
//! - 本模块只写机械事实与世界记忆，**永不写她的内心**（那是回神路径的专属）
//!
//! 由此拓展的「真实人格性」是**连续且会被意外打断的**：她活在一个有自己
//! 规律的世界里，规律被打破时她看得见——这正是「真的住在这里」而非
//! 「照着台词演」的分野。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::warn;

use crate::config;
use crate::util;

// ── 常量 ────────────────────────────────────────────────────────

/// 记住多少场直播的历史（作息/平常的样本量）
const KEEP_SESSIONS: usize = 30;
/// 记住多少个常见面孔
const KEEP_REGULARS: usize = 300;
/// 一场里「出现过的人」名单上限（判缺席够用即可）
const APPEARED_CAP: usize = 200;
/// 在线人数 / 弹幕节奏的采样上限（现场质感是最近的事，不做永久日志）
const TRACE_CAP: usize = 40;
/// 弹幕节奏的估算窗口（秒）
const PACE_WINDOW_SECS: u64 = 120;
/// 见过多少次才算「常来的」
const REGULAR_SEEN: u64 = 5;
/// 开播时刻偏离平常多少小时才值得递给她（太小就是日常浮动）
const START_DEVIATION_HOURS: f64 = 1.0;
/// 时长偏离平常多少秒才值得递给她
const DUR_DEVIATION_SECS: u64 = 1800;
/// 在线峰值相对平常的比值超出这个区间才递给她
const PEAK_HI_RATIO: f64 = 1.4;
const PEAK_LO_RATIO: f64 = 0.6;

// ── 模型 ────────────────────────────────────────────────────────

/// 一个常见面孔（客观：这房间里常见的人，按弹幕出现次数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Regular {
    /// 出现过的弹幕条数
    pub seen: u64,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// 一场已经过去的直播（作息/平常的样本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionRecord {
    pub started_at: u64,
    /// None = 那场没等到下播（她走开或进程重启错过了下播）
    pub ended_at: Option<u64>,
    pub title: String,
    pub peak_online: u64,
    pub danmaku: u64,
}

/// 一场进行中的直播的累积（现场质感 + 待结算）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct LiveAccum {
    pub started_at: u64,
    pub title: String,
    /// 开场时的在线（0 = 还没量到）
    pub first_online: u64,
    pub peak_online: u64,
    /// 递到她面前的弹幕累计条数
    pub danmaku: u64,
    /// 在线人数采样（旧在前）：估趋势/峰值
    #[serde(default)]
    pub online_trace: Vec<(u64, u64)>,
    /// 弹幕节奏采样（旧在前）：(时间, 该轮条数)
    #[serde(default)]
    pub bursts: Vec<(u64, u64)>,
    /// 这场出现过的人（去重，判缺席用）
    #[serde(default)]
    pub appeared: Vec<String>,
}

/// 一个房间的世界记忆
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct RoomModel {
    #[serde(default)]
    pub regulars: HashMap<String, Regular>,
    /// 场次历史，旧在前
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    /// 进行中的场次（没在播时为 None）
    #[serde(default)]
    pub live: Option<LiveAccum>,
}

/// 整个世界模型（按房间，键是 room_id 的字符串形态）
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct WorldModel {
    #[serde(default)]
    pub rooms: HashMap<String, RoomModel>,
}

// ── 纯推导（可测） ─────────────────────────────────────────────

/// 时间戳 → 一天中的第几小时（含小数，东八区）
fn hod(secs: u64) -> f64 {
    util::secs_of_day_cst(secs) as f64 / 3600.0
}

/// 小时数（含小数）→ "HH:MM"
fn hod_to_hhmm(h: f64) -> String {
    let total = ((h * 60.0).round() as i64).rem_euclid(24 * 60);
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// 时长（秒）→ "X 小时 Y 分" / "X 分"
fn fmt_dur(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 && m > 0 {
        format!("{h} 小时 {m} 分")
    } else if h > 0 {
        format!("{h} 小时")
    } else {
        format!("{m} 分")
    }
}

/// 带符号的环形小时差 `cur - base`，落在 (-12, 12]（跨午夜不翻车）
fn hour_diff(cur: f64, base: f64) -> f64 {
    let mut d = cur - base;
    while d > 12.0 {
        d -= 24.0;
    }
    while d <= -12.0 {
        d += 24.0;
    }
    d
}

/// 开播时刻的「平常」：(中位数, 下四分位, 上四分位)
fn start_window(hours: &[f64]) -> Option<(f64, f64, f64)> {
    if hours.is_empty() {
        return None;
    }
    let mut h = hours.to_vec();
    h.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let quantile = |p: f64| -> f64 {
        let pos = p * (h.len() as f64 - 1.0);
        let lo = pos.floor() as usize;
        let hi = pos.ceil() as usize;
        let frac = pos - lo as f64;
        h[lo] + (h[hi] - h[lo]) * frac
    };
    Some((quantile(0.5), quantile(0.25), quantile(0.75)))
}

/// 一组数的中位数
fn median_u64(vals: &[u64]) -> Option<u64> {
    if vals.is_empty() {
        return None;
    }
    let mut v = vals.to_vec();
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// 开播时刻的预测误差：这场比平常晚/早了多少（少于 3 场历史、或在浮动范围内不报）
fn start_deviation(cur_hod: f64, hist_hours: &[f64]) -> Option<String> {
    if hist_hours.len() < 3 {
        return None;
    }
    let (med, _, _) = start_window(hist_hours)?;
    let d = hour_diff(cur_hod, med);
    if d.abs() < START_DEVIATION_HOURS {
        return None;
    }
    let dir = if d > 0.0 { "晚" } else { "早" };
    Some(format!(
        "今天 {} 开播；平常（最近 {} 场）约 {}，比平常{}了约 {} 小时",
        hod_to_hhmm(cur_hod),
        hist_hours.len(),
        hod_to_hhmm(med),
        dir,
        d.abs().round() as i64
    ))
}

/// 播出时长的预测误差：这场比平常长/短了多少
fn duration_deviation(dur: u64, hist_durs: &[u64]) -> Option<String> {
    if hist_durs.len() < 3 {
        return None;
    }
    let med = median_u64(hist_durs)?;
    let diff = dur as i64 - med as i64;
    if diff.unsigned_abs() < DUR_DEVIATION_SECS {
        return None;
    }
    let dir = if diff > 0 { "长" } else { "短" };
    Some(format!(
        "这场播了 {}；平常约 {}，比平常{}了约 {}",
        fmt_dur(dur),
        fmt_dur(med),
        dir,
        fmt_dur(diff.unsigned_abs())
    ))
}

/// 在线峰值的预测误差（纯数值比较，不加情绪词）
fn peak_deviation(peak: u64, hist_peaks: &[u64]) -> Option<String> {
    if peak == 0 {
        return None;
    }
    let med = median_u64(hist_peaks)?;
    if med == 0 {
        return None;
    }
    let ratio = peak as f64 / med as f64;
    if ratio > PEAK_HI_RATIO {
        Some(format!("这场在线峰值 {peak}；平常约 {med}，多了约 {}%", ((ratio - 1.0) * 100.0).round() as i64))
    } else if ratio < PEAK_LO_RATIO {
        Some(format!("这场在线峰值 {peak}；平常约 {med}，少了约 {}%", ((1.0 - ratio) * 100.0).round() as i64))
    } else {
        None
    }
}

/// 取最活跃的前 n 个常见面孔（见过至少 `REGULAR_SEEN` 次才算「常来的」）
fn pick_regulars(regulars: &HashMap<String, Regular>, n: usize) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = regulars
        .iter()
        .filter(|(_, r)| r.seen >= REGULAR_SEEN)
        .map(|(u, r)| (u.clone(), r.seen))
        .collect();
    v.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
    v.truncate(n);
    v
}

// ── 房间推导（机械事实文本） ────────────────────────────────────

impl RoomModel {
    fn start_history_hours(&self) -> Vec<f64> {
        self.sessions.iter().map(|s| hod(s.started_at)).collect()
    }

    fn duration_history(&self) -> Vec<u64> {
        self.sessions
            .iter()
            .filter_map(|s| s.ended_at.map(|e| e.saturating_sub(s.started_at)))
            .filter(|d| *d > 0)
            .collect()
    }

    fn peak_history(&self) -> Vec<u64> {
        self.sessions
            .iter()
            .map(|s| s.peak_online)
            .filter(|p| *p > 0)
            .collect()
    }

    /// 开播时刻的预测误差（在把这场算进历史**之前**调用）
    pub(crate) fn start_deviation_fact(&self, now: u64) -> Option<String> {
        start_deviation(hod(now), &self.start_history_hours())
    }

    /// 这个房间的「平常」：约几点开播、一般播多久
    pub(crate) fn rhythm_line(&self) -> Option<String> {
        let hours = self.start_history_hours();
        if hours.len() < 3 {
            return None;
        }
        let (med, lo, hi) = start_window(&hours)?;
        let mut s = format!(
            "平常约 {} 开播（{}~{} 点）",
            hod_to_hhmm(med),
            lo.floor() as i64,
            hi.ceil() as i64
        );
        if let Some(d) = median_u64(&self.duration_history()) {
            s += &format!("，一般播 {}", fmt_dur(d));
        }
        Some(s)
    }

    /// 这房间常见的人（客观：按弹幕出现次数）
    pub(crate) fn familiarity_line(&self) -> Option<String> {
        let top = pick_regulars(&self.regulars, 3);
        if top.is_empty() {
            return None;
        }
        let rendered: Vec<String> = top
            .into_iter()
            .map(|(u, n)| format!("「{u}」{n} 次"))
            .collect();
        Some(format!("常见的人：{}", rendered.join("、")))
    }

    /// 此刻现场质感：在线人数轨迹 + 最近的弹幕节奏（全是量出来的数）
    pub(crate) fn texture_line(&self, now: u64) -> Option<String> {
        let live = self.live.as_ref()?;
        let cur = live
            .online_trace
            .last()
            .map(|(_, o)| *o)
            .unwrap_or(live.first_online);
        let mut online = format!("在线 {cur}");
        if live.first_online > 0 {
            online += &format!("（开播时 {}）", live.first_online);
        }
        if live.peak_online > 0 {
            online += &format!("，峰值 {}", live.peak_online);
        }
        let recent: u64 = live
            .bursts
            .iter()
            .filter(|(t, _)| now.saturating_sub(*t) <= PACE_WINDOW_SECS)
            .map(|(_, c)| *c)
            .sum();
        let mut parts = vec![online];
        if recent > 0 {
            parts.push(format!("最近两分钟约 {recent} 条弹幕"));
        }
        Some(parts.join("；"))
    }

    /// 常来的、这场到现在还没见着的人（缺席是真实信号，不是情绪）
    pub(crate) fn absence_line(&self) -> Option<String> {
        let live = self.live.as_ref()?;
        // 已经有些聊天了，缺席才有意义（刚开播没人说话是常态）
        if live.danmaku < 3 {
            return None;
        }
        let mut top: Vec<(String, u64)> = self
            .regulars
            .iter()
            .filter(|(_, r)| r.seen >= REGULAR_SEEN)
            .map(|(u, r)| (u.clone(), r.seen))
            .collect();
        top.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
        let missing: Vec<String> = top
            .iter()
            .take(5)
            .filter(|(u, _)| !live.appeared.contains(u))
            .map(|(u, _)| u.clone())
            .collect();
        if missing.is_empty() {
            return None;
        }
        let rendered: Vec<String> = missing.iter().map(|m| format!("「{m}」")).collect();
        Some(format!("常来的还没见着：{}", rendered.join("、")))
    }
}

// ── 结算 ────────────────────────────────────────────────────────

/// 把一场直播记进历史（可测）：先按「平常」算偏差事实，再入史
fn settle(room: &mut RoomModel, live: LiveAccum, ended_at: Option<u64>) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(end) = ended_at {
        let dur = end.saturating_sub(live.started_at);
        if let Some(f) = duration_deviation(dur, &room.duration_history()) {
            facts.push(f);
        }
        if let Some(f) = peak_deviation(live.peak_online, &room.peak_history()) {
            facts.push(f);
        }
    }
    room.sessions.push(SessionRecord {
        started_at: live.started_at,
        ended_at,
        title: live.title,
        peak_online: live.peak_online,
        danmaku: live.danmaku,
    });
    while room.sessions.len() > KEEP_SESSIONS {
        room.sessions.remove(0);
    }
    facts
}

/// 常见面孔表的容量教养：先淘汰最少出现的
fn trim_regulars(regulars: &mut HashMap<String, Regular>) {
    if regulars.len() <= KEEP_REGULARS {
        return;
    }
    let mut v: Vec<(String, u64)> = regulars
        .iter()
        .map(|(u, r)| (u.clone(), r.seen))
        .collect();
    v.sort_by_key(|(_, s)| *s);
    let drop = regulars.len() - KEEP_REGULARS;
    for (u, _) in v.into_iter().take(drop) {
        regulars.remove(&u);
    }
}

// ── 持久化 ──────────────────────────────────────────────────────

static MODEL: Mutex<Option<WorldModel>> = Mutex::new(None);

fn model_path() -> std::path::PathBuf {
    config::data_dir().join("mind").join("world_model.json")
}

fn load_model() -> WorldModel {
    let Ok(content) = std::fs::read_to_string(model_path()) else {
        return WorldModel::default();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "world/model: 世界模型解析失败，按空处理");
        WorldModel::default()
    })
}

fn save_model(model: &WorldModel) {
    let Ok(json) = serde_json::to_string_pretty(model) else {
        warn!("world/model: 世界模型序列化失败");
        return;
    };
    if let Err(e) = util::atomic_write(model_path(), json) {
        warn!(error = %e, "world/model: 世界模型落盘失败");
    }
}

fn with_model<R>(f: impl FnOnce(&mut WorldModel) -> R) -> R {
    let mut guard = MODEL.lock().unwrap_or_else(|e| e.into_inner());
    let model = guard.get_or_insert_with(load_model);
    f(model)
}

// ── 对外：世界层的落地点调用 ───────────────────────────────────

/// 开播：开一场新的累积，返回「与平常的偏差」事实（可能是空）
pub(crate) fn note_live_start(room_id: u64, title: &str, now: u64) -> Vec<String> {
    with_model(|m| {
        let room = m.rooms.entry(room_id.to_string()).or_default();
        // 上一场没等到下播就被顶掉：照实按「没等到下播」结算
        if let Some(prev) = room.live.take() {
            let _ = settle(room, prev, None);
        }
        let facts = room.start_deviation_fact(now).into_iter().collect();
        room.live = Some(LiveAccum {
            started_at: now,
            title: title.to_string(),
            ..Default::default()
        });
        save_model(m);
        facts
    })
}

/// 一轮巡视的现场记账：在线采样 + 弹幕节奏 + 常见面孔 + 这场出现过的人
pub(crate) fn note_live_tick(room_id: u64, online: u64, unames: &[String], now: u64) {
    with_model(|m| {
        let room = m.rooms.entry(room_id.to_string()).or_default();
        for u in unames {
            let e = room.regulars.entry(u.clone()).or_insert_with(|| Regular {
                seen: 0,
                first_seen: now,
                last_seen: now,
            });
            e.seen += 1;
            e.last_seen = now;
        }
        trim_regulars(&mut room.regulars);

        let live = room.live.get_or_insert_with(|| LiveAccum {
            started_at: now,
            ..Default::default()
        });
        if live.first_online == 0 && online > 0 {
            live.first_online = online;
        }
        live.peak_online = live.peak_online.max(online);
        live.danmaku += unames.len() as u64;
        live.online_trace.push((now, online));
        if live.online_trace.len() > TRACE_CAP {
            let drop = live.online_trace.len() - TRACE_CAP;
            live.online_trace.drain(..drop);
        }
        if !unames.is_empty() {
            live.bursts.push((now, unames.len() as u64));
            if live.bursts.len() > TRACE_CAP {
                let drop = live.bursts.len() - TRACE_CAP;
                live.bursts.drain(..drop);
            }
        }
        for u in unames {
            if !live.appeared.contains(u) {
                live.appeared.push(u.clone());
            }
        }
        if live.appeared.len() > APPEARED_CAP {
            let drop = live.appeared.len() - APPEARED_CAP;
            live.appeared.drain(..drop);
        }
        save_model(m);
    });
}

/// 下播：结算这一场，返回「与平常的偏差」事实（可能是空）
pub(crate) fn note_live_end(room_id: u64, now: u64) -> Vec<String> {
    with_model(|m| {
        let room = m.rooms.entry(room_id.to_string()).or_default();
        let Some(live) = room.live.take() else {
            return Vec::new();
        };
        let facts = settle(room, live, Some(now));
        save_model(m);
        facts
    })
}

// ── 对外：呈现（读，不落盘） ────────────────────────────────────

/// 此刻现场质感（看直播时给她随行的机械事实）
pub(crate) fn texture_line(room_id: u64) -> Option<String> {
    let now = util::now_secs();
    with_model(|m| {
        m.rooms
            .get(&room_id.to_string())
            .and_then(|r| r.texture_line(now))
    })
}

/// 常来的、这场还没见着的人
pub(crate) fn absence_line(room_id: u64) -> Option<String> {
    with_model(|m| {
        m.rooms
            .get(&room_id.to_string())
            .and_then(|r| r.absence_line())
    })
}

/// 这房间的「平常」（作息）
pub(crate) fn rhythm_line(room_id: u64) -> Option<String> {
    with_model(|m| {
        m.rooms
            .get(&room_id.to_string())
            .and_then(|r| r.rhythm_line())
    })
}

/// 这房间常见的人（熟脸）
pub(crate) fn familiarity_line(room_id: u64) -> Option<String> {
    with_model(|m| {
        m.rooms
            .get(&room_id.to_string())
            .and_then(|r| r.familiarity_line())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regular(seen: u64) -> Regular {
        Regular {
            seen,
            first_seen: 0,
            last_seen: 0,
        }
    }

    fn session(start_hod: f64, dur_h: f64) -> SessionRecord {
        // 用一个固定基准日把「当天第几小时」换算回时间戳（东八区 2024-01-01 00:00 = 1704038400）
        let day_start = 1_704_038_400i64 - 8 * 3600; // 东八区当天 00:00 的绝对秒
        let started = day_start + (start_hod * 3600.0) as i64;
        SessionRecord {
            started_at: started as u64,
            ended_at: Some((started + (dur_h * 3600.0) as i64) as u64),
            title: "深夜电台".into(),
            peak_online: 1000,
            danmaku: 10,
        }
    }

    #[test]
    fn hour_diff_wraps_around_midnight() {
        assert!((hour_diff(1.0, 23.0) - 2.0).abs() < 1e-6, "1 点比 23 点晚 2 小时");
        assert!((hour_diff(23.0, 1.0) + 2.0).abs() < 1e-6, "23 点比 1 点早 2 小时");
        assert!((hour_diff(12.0, 12.0)).abs() < 1e-6);
    }

    #[test]
    fn start_window_gives_median_and_spread() {
        let hours = vec![21.0, 21.5, 22.0, 22.5, 23.0];
        let (med, lo, hi) = start_window(&hours).expect("非空");
        assert!((med - 22.0).abs() < 1e-6);
        assert!(lo <= med && med <= hi);
    }

    #[test]
    fn start_deviation_needs_history_and_a_real_gap() {
        // 样本太少 → 不报（没有「平常」可言）
        assert!(start_deviation(23.0, &[21.0, 22.0]).is_none());
        // 平常 21~22 点，这场 23 点开 → 晚了约 1~2 小时
        let fact = start_deviation(23.5, &[21.0, 21.5, 22.0, 22.0, 21.5]).expect("应当报偏差");
        assert!(fact.contains("晚"), "{fact}");
        // 与平常相当 → 不报
        assert!(start_deviation(21.5, &[21.0, 21.5, 22.0, 22.0, 21.5]).is_none());
    }

    #[test]
    fn duration_deviation_compares_to_median() {
        let hist = vec![7200, 7500, 7000, 7300, 7100]; // 平常约 2 小时
        let short = duration_deviation(1800, &hist).expect("明显短应报");
        assert!(short.contains("短"), "{short}");
        let ok = duration_deviation(7200, &hist);
        assert!(ok.is_none(), "差不多长不报");
        assert!(duration_deviation(1800, &[600, 700]).is_none(), "样本不足不报");
    }

    #[test]
    fn peak_deviation_only_on_big_swings() {
        let hist = vec![1000, 1100, 900, 1050, 1000];
        assert!(peak_deviation(2000, &hist).expect("翻倍应报").contains("多"));
        assert!(peak_deviation(300, &hist).expect("暴跌应报").contains("少"));
        assert!(peak_deviation(1000, &hist).is_none(), "平常水平不报");
        assert!(peak_deviation(0, &hist).is_none(), "没量到在线不报");
    }

    #[test]
    fn pick_regulars_ranks_by_seen_and_filters_oneoffs() {
        let mut regulars = HashMap::new();
        regulars.insert("甲".to_string(), regular(12));
        regulars.insert("乙".to_string(), regular(8));
        regulars.insert("路人".to_string(), regular(1)); // 一次性，不算常来的
        let top = pick_regulars(&regulars, 3);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "甲");
        assert_eq!(top[1].0, "乙");
    }

    #[test]
    fn familiarity_line_lists_the_top_faces() {
        let mut room = RoomModel::default();
        room.regulars.insert("甲".to_string(), regular(12));
        room.regulars.insert("乙".to_string(), regular(8));
        let line = room.familiarity_line().expect("有常来的");
        assert!(line.contains("「甲」12 次"));
        assert!(line.contains("「乙」8 次"));
    }

    #[test]
    fn texture_line_reports_online_and_pace() {
        let mut room = RoomModel::default();
        let now = 1_704_038_400u64;
        room.live = Some(LiveAccum {
            started_at: now - 600,
            title: "深夜电台".into(),
            first_online: 800,
            peak_online: 1200,
            danmaku: 10,
            online_trace: vec![(now - 600, 800), (now, 1000)],
            bursts: vec![(now - 30, 4), (now - 10, 2)],
            appeared: vec!["甲".into()],
        });
        let line = room.texture_line(now).expect("在播应有质感");
        assert!(line.contains("在线 1000"), "{line}");
        assert!(line.contains("开播时 800"), "{line}");
        assert!(line.contains("峰值 1200"), "{line}");
        assert!(line.contains("约 6 条弹幕"), "{line}");
    }

    #[test]
    fn absence_line_flags_missing_regulars_once_chat_is_live() {
        let mut room = RoomModel::default();
        room.regulars.insert("甲".to_string(), regular(12));
        room.regulars.insert("乙".to_string(), regular(8));
        room.live = Some(LiveAccum {
            danmaku: 10,
            appeared: vec!["乙".into()],
            ..Default::default()
        });
        let line = room.absence_line().expect("甲缺席应报");
        assert!(line.contains("「甲」"), "{line}");
        assert!(!line.contains("「乙」"), "乙在场不该报缺席");
        // 刚开播没人说话 → 不报缺席
        room.live.as_mut().unwrap().danmaku = 0;
        assert!(room.absence_line().is_none());
    }

    #[test]
    fn settle_records_session_and_reports_deviation_before_push() {
        let mut room = RoomModel::default();
        // 三场平常约 2 小时
        for _ in 0..3 {
            room.sessions.push(session(21.0, 2.0));
        }
        let live = LiveAccum {
            started_at: room.sessions[0].started_at, // 任意基准
            title: "深夜电台".into(),
            peak_online: 1000,
            danmaku: 5,
            ..Default::default()
        };
        // 播 20 分钟 → 明显短
        let facts = settle(&mut room, live.clone(), Some(live.started_at + 1200));
        assert!(facts.iter().any(|f| f.contains("短")), "{facts:?}");
        assert_eq!(room.sessions.len(), 4, "结算后入史");
    }

    #[test]
    fn sessions_are_capped() {
        let mut room = RoomModel::default();
        for i in 0..KEEP_SESSIONS + 5 {
            room.sessions.push(session(20.0 + (i % 3) as f64, 2.0));
        }
        while room.sessions.len() > KEEP_SESSIONS {
            room.sessions.remove(0);
        }
        assert_eq!(room.sessions.len(), KEEP_SESSIONS);
    }

    #[test]
    fn trim_regulars_evicts_the_coldest() {
        let mut regulars = HashMap::new();
        for i in 0..KEEP_REGULARS + 10 {
            regulars.insert(format!("u{i}"), regular(i as u64));
        }
        trim_regulars(&mut regulars);
        assert_eq!(regulars.len(), KEEP_REGULARS);
        assert!(regulars.contains_key("u10"), "最活跃的应当留下");
        assert!(!regulars.contains_key("u0"), "最少见的应当淘汰");
    }
}
