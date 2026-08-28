//! Minimal local-time formatting without external crates (libc localtime_r).

#[derive(Debug, Clone, Copy)]
pub struct Tm {
    pub year: i32,
    pub month: u32, // 1..=12
    pub day: u32,   // 1..=31
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub weekday: u32, // 0=Sun..6=Sat
    pub gmtoff: i64,  // seconds east of UTC
}

fn localtime(secs: u64) -> Tm {
    #[repr(C)]
    struct CTm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const u8,
    }
    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut CTm) -> *mut CTm;
    }
    let t = secs as i64;
    let mut c = CTm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 0,
        tm_mon: 0,
        tm_year: 0,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    unsafe {
        localtime_r(&t, &mut c);
    }
    Tm {
        year: c.tm_year + 1900,
        month: (c.tm_mon + 1) as u32,
        day: c.tm_mday as u32,
        hour: c.tm_hour as u32,
        min: c.tm_min as u32,
        sec: c.tm_sec as u32,
        weekday: (((c.tm_wday % 7) + 7) % 7) as u32,
        gmtoff: c.tm_gmtoff,
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// UTC variant (used by unit tests and anywhere local tz would be flaky).
#[allow(dead_code)]
pub fn strftime_utc(epoch: u64, fmt: &str) -> String {
    let days = (epoch / 86400) as i64;
    let rem = epoch % 86400;
    let (y, m, d) = civil_from_days(days);
    let t = Tm {
        year: y,
        month: m,
        day: d,
        hour: (rem / 3600) as u32,
        min: ((rem % 3600) / 60) as u32,
        sec: (rem % 60) as u32,
        weekday: (((days % 7) + 11) % 7) as u32, // Sun=0
        gmtoff: 0,
    };
    render(fmt, &t)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

fn render(fmt: &str, t: &Tm) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('H') => out.push_str(&format!("{:02}", t.hour)),
            Some('I') => {
                let h = match t.hour % 12 {
                    0 => 12,
                    x => x,
                };
                out.push_str(&format!("{h:02}"));
            }
            Some('M') => out.push_str(&format!("{:02}", t.min)),
            Some('S') => out.push_str(&format!("{:02}", t.sec)),
            Some('p') => out.push_str(if t.hour < 12 { "AM" } else { "PM" }),
            Some('Y') => out.push_str(&format!("{}", t.year)),
            Some('y') => out.push_str(&format!("{:02}", t.year % 100)),
            Some('d') => out.push_str(&format!("{:02}", t.day)),
            Some('e') => out.push_str(&format!("{:2}", t.day)),
            Some('m') => out.push_str(&format!("{:02}", t.month)),
            Some('a') => out.push_str(WDAYS[t.weekday as usize]),
            Some('A') => out.push_str(match t.weekday {
                0 => "Sunday",
                1 => "Monday",
                2 => "Tuesday",
                3 => "Wednesday",
                4 => "Thursday",
                5 => "Friday",
                _ => "Saturday",
            }),
            Some('b') | Some('h') => out.push_str(MONTHS[(t.month - 1) as usize]),
            Some('B') => out.push_str(MONTHS_FULL[(t.month - 1) as usize]),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// strftime subset sufficient for clock configs.
pub fn strftime_local(epoch: u64, fmt: &str) -> String {
    let t = localtime(epoch);
    render(fmt, &t)
}

pub fn today_parts() -> (i32, u32, u32) {
    let t = localtime(now_epoch());
    (t.year, t.month, t.day)
}

pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn month_name(m: u32) -> &'static str {
    MONTHS_FULL[(m.clamp(1, 12) - 1) as usize]
}

/// Seconds east of UTC for the local zone, from libc's tm_gmtoff.
pub fn local_offset_secs() -> i64 {
    let t = localtime(now_epoch());
    t.gmtoff
}

/// Days since 1970-01-01 for a proleptic-Gregorian civil date.
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m as i64) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[allow(dead_code)]
fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[allow(dead_code)]
pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 30,
    }
}

/// Weekday of the 1st of the month; 0=Monday .. 6=Sunday.
#[allow(dead_code)]
pub fn weekday_of_first(y: i32, m: u32) -> u32 {
    // Zeller-ish via days-from-civil (proleptic Gregorian)
    let days = days_from_civil(y, m, 1);
    let wd_sun0 = ((days % 7) + 11) % 7; // epoch day 0 = Thursday(4); shift to Sun=0
                                         // convert Sun=0..6 into Mon=0..6
    ((wd_sun0 + 6) % 7) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_basic() {
        // 2026-08-26 00:00 UTC == 1787702400
        let s = strftime_utc(1787702400, "%Y-%m-%d");
        assert_eq!(s, "2026-08-26");
        let s2 = strftime_utc(1787702400 + 13 * 3600 + 45 * 60, "%H:%M %p");
        assert_eq!(s2, "13:45 PM");
        // Wednesday check via %A
        let s3 = strftime_utc(1787702400, "%a");
        assert_eq!(s3, "Wed");
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2026, 8), 31);
    }

    #[test]
    fn weekday_math() {
        // Aug 1 2026 is a Saturday → Mon-based index 5
        assert_eq!(weekday_of_first(2026, 8), 5);
    }
}
