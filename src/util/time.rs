//! 时间工具函数

use std::time::SystemTime;

/// 当前 Unix 时间戳（秒）
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 当前 Unix 时间戳（毫秒）
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 当前 UTC+8 小时数 (0-23)
pub fn current_hour_cst() -> u32 {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 + 8 * 3600;
    ((secs % 86400) / 3600) as u32
}

/// 判断闰年
pub fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// 从 epoch 天数计算 (年, 月, 日)，UTC+8
pub fn epoch_days_to_ymd(mut days: u64) -> (u64, u32, u32) {
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
    let md = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
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

/// 当前 UTC+8 的 (年, 月, 日)
pub fn today_ymd() -> (u64, u32, u32) {
    let secs = now_secs() as i64 + 8 * 3600;
    let days = (secs / 86400) as u64;
    epoch_days_to_ymd(days)
}

/// 当前 UTC+8 日期字符串 "YYYY-MM-DD"
pub fn today_str() -> String {
    let (y, m, d) = today_ymd();
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 当前 UTC+8 日期字符串 "MM-DD"
pub fn current_date_mm_dd() -> String {
    let (_, m, d) = today_ymd();
    format!("{:02}-{:02}", m, d)
}

/// 当前 UTC+8 年份
pub fn current_year() -> u32 {
    today_ymd().0 as u32
}

/// 当前 UTC+8 格式化时间 "HH:MM:SS (YYYY年M月D日)"
pub fn now_formatted_cst() -> String {
    let secs = now_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    let (year, month, day) = epoch_days_to_ymd(days as u64);
    format!("{:02}:{:02}:{:02} ({}年{}月{}日)", hour, minute, second, year, month, day)
}

/// 时间戳 → 日期字符串 "YYYY-MM-DD"
pub fn ts_to_date_str(secs: u64) -> String {
    let local_secs = secs as i64 + 8 * 3600;
    let days = (local_secs / 86400) as u64;
    let (y, m, d) = epoch_days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 时间戳 → 月份字符串 "YYYY-MM"
pub fn ts_to_month_str(secs: u64) -> String {
    let local_secs = secs as i64 + 8 * 3600;
    let days = (local_secs / 86400) as u64;
    let (y, m, _) = epoch_days_to_ymd(days);
    format!("{:04}-{:02}", y, m)
}

/// 当前 UTC+8 星期几（1=周一, 7=周日）
pub fn current_weekday_cst() -> u32 {
    let secs = now_secs() as i64 + 8 * 3600;
    let days = (secs / 86400) as u64;
    // 1970-01-01 是周四
    let weekday = ((days + 3) % 7) as u32;
    if weekday == 0 { 7 } else { weekday }
}

/// 当前 UTC+8 英文星期几
pub fn current_weekday_eng() -> String {
    let names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    let idx = (current_weekday_cst() - 1) as usize;
    names[idx].to_string()
}
