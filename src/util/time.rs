//! 时间与日历边界
//!
//! 整个项目的时区与"日/小时/段"边界**只在这里定义一次**。
//!
//! 之所以强调这一点：这些边界曾经散落在五处（`quota::current_segment_start`
//! 按绝对 epoch 对齐、`current_max_replies` 按东八区小时查表、
//! `mind/stream.rs` 自己拼日期文件名、`mind/wake.rs` 用 `now + 8h` 手工偏移、
//! `util::ts_to_date_str` 再实现一遍），于是"同一个 5 分钟段"可以跨过整点，
//! 段内的配额上限会在中途变化而计数不重置。
//!
//! 这里的边界函数都是**纯函数**（传入时间戳、返回边界值），因此可以
//! 直接对跨零点、跨整点这类边界做单元测试，不需要注入时钟。

use std::time::SystemTime;

/// 东八区偏移（秒）——时区只在这一处出现
const CST_OFFSET_SECS: i64 = 8 * 3600;
/// 一天的秒数
const SECS_PER_DAY: i64 = 86_400;

/// 当前 Unix 时间戳（秒）
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 当前 Unix 时间戳（毫秒）
pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── 东八区投影（纯函数） ────────────────────────────────────────

/// 时间戳 → 东八区"日序号"（自 epoch 起的天数）
pub(crate) fn day_index_cst(secs: u64) -> u64 {
    ((secs as i64 + CST_OFFSET_SECS).div_euclid(SECS_PER_DAY)) as u64
}

/// 时间戳 → 东八区当天已过的秒数
pub(crate) fn secs_of_day_cst(secs: u64) -> u64 {
    (secs as i64 + CST_OFFSET_SECS).rem_euclid(SECS_PER_DAY) as u64
}

/// 时间戳 → 东八区小时 (0-23)
pub(crate) fn hour_cst_at(secs: u64) -> u32 {
    (secs_of_day_cst(secs) / 3600) as u32
}

/// 时间戳 → 所在配额段的起始时间戳（秒）
///
/// 段边界对齐到**东八区当天零点**，而不是绝对 epoch。绝对对齐会让
/// 5 分钟段跨过整点（13:58–14:02）：段内的小时配额上限中途变化，
/// 而该段的计数不重置，于是"这一小时还剩多少条"两个量互相矛盾。
/// 对齐到当天零点后，只要段长能整除一小时，一个段必然落在同一个小时里。
pub(crate) fn segment_start_cst(secs: u64, segment_secs: u64) -> u64 {
    if segment_secs == 0 {
        return secs;
    }
    let of_day = secs_of_day_cst(secs);
    secs - of_day + (of_day / segment_secs) * segment_secs
}

/// 判断某个小时是否落在 [start, end) 内，支持跨午夜（如 23 → 7）
///
/// `start == end` 视为"没有免打扰时段"，而不是"整天都免打扰"——
/// 后者会让一个没配置过的时段静默关掉全部主动行为。
pub(crate) fn hour_in_window(hour: u32, start: u32, end: u32) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    }
}

/// 当前 UTC+8 小时数 (0-23)
pub(crate) fn current_hour_cst() -> u32 {
    hour_cst_at(now_secs())
}

// ── 日期换算 ────────────────────────────────────────────────────

/// 某个时间戳所在的那个东八区日期里，HH:MM 对应的 unix 时间戳
///
/// 用于"今天 23:30"这类以本地日历表达的调度时刻，替代手写
/// `now + 8*3600` → 取整 → `- 8*3600` 的偏移运算：那种写法一旦有一处
/// 忘了减回去，就会把本地时间当 UTC 用。
pub(crate) fn cst_time_on_same_day(secs: u64, hour: u32, minute: u32) -> u64 {
    let day_start = secs - secs_of_day_cst(secs);
    day_start + hour as u64 * 3600 + minute as u64 * 60
}

/// 判断闰年
pub(crate) fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// 从 epoch 天数计算 (年, 月, 日)，UTC+8
pub(crate) fn epoch_days_to_ymd(mut days: u64) -> (u64, u32, u32) {
    let mut y = 1970u64;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        y += 1;
    }
    let leap = is_leap_year(y);
    let md = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for &dim in &md {
        if days < dim {
            return (y, month, days as u32 + 1);
        }
        days -= dim;
        month += 1;
    }
    (y, 12, 31)
}

/// 时间戳 → (年, 月, 日)，UTC+8
pub(crate) fn ts_to_ymd_cst(secs: u64) -> (u64, u32, u32) {
    epoch_days_to_ymd(day_index_cst(secs))
}

/// 时间戳 → 日期字符串 "YYYY-MM-DD"（UTC+8）
pub(crate) fn ts_to_date_str(secs: u64) -> String {
    let (y, m, d) = ts_to_ymd_cst(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 当前 UTC+8 日期字符串 "YYYY-MM-DD"
pub(crate) fn today_str() -> String {
    ts_to_date_str(now_secs())
}

/// 当前 UTC+8 格式化时间 "HH:MM:SS (YYYY年M月D日)"
pub(crate) fn now_formatted_cst() -> String {
    let now = now_secs();
    let of_day = secs_of_day_cst(now);
    let (year, month, day) = ts_to_ymd_cst(now);
    format!(
        "{:02}:{:02}:{:02} ({}年{}月{}日)",
        of_day / 3600,
        (of_day % 3600) / 60,
        of_day % 60,
        year,
        month,
        day
    )
}

/// 时间戳 → "HH:MM"（东八区）
pub(crate) fn hh_mm(secs: u64) -> String {
    let of_day = secs_of_day_cst(secs);
    format!("{:02}:{:02}", of_day / 3600, (of_day % 3600) / 60)
}

/// 时间戳 → 月份字符串 "YYYY-MM"（UTC+8）
pub(crate) fn ts_to_month_str(secs: u64) -> String {
    let (y, m, _) = ts_to_ymd_cst(secs);
    format!("{y:04}-{m:02}")
}

/// 时间戳 → UTC+8 星期几（1=周一, 7=周日）
pub(crate) fn weekday_cst_at(secs: u64) -> u32 {
    // 1970-01-01 是**周四**，因此日序号 0 对应 4。
    // 这里曾经写成 `((days + 3) % 7)` 再把 0 特判成 7——那会把周四算成周三，
    // 于是 `monday_of_week_str()` 得到的是"周一的前一天"，周计划的边界
    // 与 `current_weekday_eng()` 报给模型的星期都错了一天。
    ((day_index_cst(secs) + 3) % 7) as u32 + 1
}

/// 本周周一的日期字符串 "YYYY-MM-DD"（UTC+8）
///
/// 直接在"epoch 天数"上做减法，不经过时间戳：用 `now_secs() - offset*86400`
/// 会把当天的时分秒一起带进结果，跨零点时容易算错一天。
pub(crate) fn monday_of_week_str() -> String {
    monday_of_week_str_at(now_secs())
}

/// 某时间戳所在那一周的周一日期字符串（UTC+8）
pub(crate) fn monday_of_week_str_at(secs: u64) -> String {
    let days = day_index_cst(secs);
    let offset = (weekday_cst_at(secs) - 1) as u64;
    let (y, m, d) = epoch_days_to_ymd(days.saturating_sub(offset));
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-01-01 00:00:00 UTC+8 == 2025-12-31 16:00:00 UTC
    const CST_NEW_YEAR_2026: u64 = 1_767_196_800;

    #[test]
    fn cst_day_boundary_is_exact() {
        // 东八区零点前 1 秒仍属于前一天
        assert_eq!(ts_to_date_str(CST_NEW_YEAR_2026 - 1), "2025-12-31");
        assert_eq!(ts_to_date_str(CST_NEW_YEAR_2026), "2026-01-01");
        assert_eq!(hour_cst_at(CST_NEW_YEAR_2026 - 1), 23);
        assert_eq!(hour_cst_at(CST_NEW_YEAR_2026), 0);
        // 日序号必须随之 +1，不能因为取模而回退
        assert_eq!(
            day_index_cst(CST_NEW_YEAR_2026),
            day_index_cst(CST_NEW_YEAR_2026 - 1) + 1
        );
    }

    #[test]
    fn hour_is_stable_across_a_multiple_of_seven_days() {
        // 86400 秒后小时数不变（不会出现 %86400 的符号问题）
        for hour in 0..24u64 {
            let ts = CST_NEW_YEAR_2026 + hour * 3600;
            assert_eq!(hour_cst_at(ts), hour as u32);
            assert_eq!(hour_cst_at(ts + 7 * 86400), hour as u32);
        }
    }

    /// 段边界必须落在同一个小时内——这是对齐到当天零点换来的性质
    #[test]
    fn quota_segment_never_straddles_an_hour() {
        const SEGMENT: u64 = 5 * 60;
        // 遍历跨整点前后的每一秒
        for offset in 0..(2 * 3600) {
            let ts = CST_NEW_YEAR_2026 + 13 * 3600 + offset; // 13:00 起两小时
            let start = segment_start_cst(ts, SEGMENT);
            assert_eq!(
                hour_cst_at(start),
                hour_cst_at(ts),
                "段起点落在另一个小时：ts={ts} start={start}"
            );
            assert!(start <= ts && ts - start < SEGMENT, "段起点不在本段内");
        }
    }

    #[test]
    fn zero_length_segment_does_not_divide_by_zero() {
        let ts = CST_NEW_YEAR_2026 + 123;
        assert_eq!(segment_start_cst(ts, 0), ts);
    }

    #[test]
    fn hour_window_handles_midnight_wrap_and_empty_window() {
        // 普通区间 9..18
        assert!(hour_in_window(9, 9, 18));
        assert!(hour_in_window(17, 9, 18));
        assert!(!hour_in_window(18, 9, 18));
        // 跨午夜 23..7
        assert!(hour_in_window(23, 23, 7));
        assert!(hour_in_window(0, 23, 7));
        assert!(hour_in_window(6, 23, 7));
        assert!(!hour_in_window(7, 23, 7));
        assert!(!hour_in_window(12, 23, 7));
        // 空区间：一天都不算免打扰（而不是整天都算）
        for hour in 0..24 {
            assert!(!hour_in_window(hour, 8, 8), "空区间不该匹配任何小时");
        }
    }

    #[test]
    fn weekday_matches_known_dates() {
        // 2026-01-01 是周四
        assert_eq!(weekday_cst_at(CST_NEW_YEAR_2026), 4);
        // 2026-01-05 是周一
        assert_eq!(weekday_cst_at(CST_NEW_YEAR_2026 + 4 * 86400), 1);
        // 整个一周都必须按 1..7 循环（周四起连续 7 天，周日之后回到周一）
        for day in 0..7u64 {
            let expected = ((4 + day as u32 - 1) % 7) + 1;
            assert_eq!(
                weekday_cst_at(CST_NEW_YEAR_2026 + day * 86400),
                expected,
                "第 {day} 天的星期不对"
            );
        }
    }

    /// 周计划的边界靠这个函数：2026-01-01（周四）所在周的周一是 2025-12-29。
    /// 星期算错一天会让周计划整周错位。
    #[test]
    fn monday_of_week_is_correct_across_a_year_boundary() {
        assert_eq!(monday_of_week_str_at(CST_NEW_YEAR_2026), "2025-12-29");
        // 周一当天返回自己
        let monday = CST_NEW_YEAR_2026 - 3 * 86400; // 2025-12-29
        assert_eq!(monday_of_week_str_at(monday), "2025-12-29");
        // 周日仍属于同一周
        assert_eq!(monday_of_week_str_at(monday + 6 * 86400), "2025-12-29");
        // 下一个周一是新的一周
        assert_eq!(monday_of_week_str_at(monday + 7 * 86400), "2026-01-05");
    }

    /// 睡前整理（今天 23:30）靠这个函数定位：它必须钉住**本地墙钟**，
    /// 而不是把本地时间当 UTC 用。
    #[test]
    fn cst_time_on_same_day_pins_local_wall_clock() {
        const DIGEST_CST: u64 = CST_NEW_YEAR_2026 + 23 * 3600 + 30 * 60;

        // 当天上午请求 23:30 → 落到当天
        let morning = CST_NEW_YEAR_2026 + 9 * 3600;
        assert_eq!(cst_time_on_same_day(morning, 23, 30), DIGEST_CST);

        // 当天 23:00 请求 23:30 → 仍是当天
        let late_evening = CST_NEW_YEAR_2026 + 23 * 3600;
        assert_eq!(cst_time_on_same_day(late_evening, 23, 30), DIGEST_CST);

        // 刚过零点请求 23:30 → 还是**当天**的 23:30（调用方自行决定是否 +86400）
        let after_midnight = CST_NEW_YEAR_2026 + 600;
        assert_eq!(cst_time_on_same_day(after_midnight, 23, 30), DIGEST_CST);

        // 前一天的 23:00 请求 23:30 → 前一天的时刻，不能跳到今天
        let yesterday = CST_NEW_YEAR_2026 - 3600;
        assert_eq!(cst_time_on_same_day(yesterday, 23, 30), DIGEST_CST - 86400);
    }
}
