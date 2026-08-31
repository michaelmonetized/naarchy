//! Calendar: fetch iCloud/Google ICS feeds, cache them, and parse today's
//! meetings (filtering out noise like birthdays/anniversaries/holidays).

use crate::timefmt;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::services::Event;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct CalEvent {
    pub summary: String,
    pub location: String,
    pub all_day: bool,
    /// Local wall-clock as "HH:MM"; empty for all-day.
    pub time_str: String,
    /// Approx. absolute start (epoch) for ordering + "next" detection.
    pub start_epoch: u64,
}

fn cache_dir() -> PathBuf {
    let mut p = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("naarchy");
    p.push("calendar");
    p
}

/// Download each feed into the cache dir (sanity-checked as ICS).
pub async fn refresh_feeds(feeds: &[String]) {
    for (i, url) in feeds.iter().enumerate() {
        let url = url.clone();
        let url_src = url.clone();
        let path = cache_dir().join(format!("feed-{i}.ics"));
        let fetched = tokio::task::spawn_blocking(move || fetch(&url_src)).await;
        match fetched {
            Ok(Some(body)) if body.starts_with("BEGIN:VCALENDAR") => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if std::fs::write(&path, &body).is_err() {
                    log::warn!("calendar: failed writing cache {}", path.display());
                }
            }
            Ok(_) => log::warn!("calendar: feed {url} returned non-ICS content"),
            Err(e) => log::warn!("calendar: feed {url} failed ({e})"),
        }
    }
}

fn fetch(url: &str) -> Option<String> {
    let resp = ureq::get(url)
        .timeout(Duration::from_secs(20))
        .call()
        .ok()?;
    resp.into_string().ok()
}

/// Background service: refresh feeds every `refresh_min`, then emit
/// `Event::CalendarReload` so the UI re-renders.
pub struct CalendarHandle {
    #[allow(dead_code)]
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub fn spawn(tx: Sender<Event>, feeds: Vec<String>, refresh_min: u64) -> CalendarHandle {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if feeds.is_empty() {
        return CalendarHandle { stop };
    }
    let stop2 = stop.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            loop {
                if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                refresh_feeds(&feeds).await;
                let _ = tx.send(Event::CalendarReload);
                tokio::time::sleep(Duration::from_secs(refresh_min.clamp(1, 1440) * 60)).await;
            }
        });
    });
    CalendarHandle { stop }
}

/// Read every cached feed and return today's meetings, newest source last wins.
pub fn today_from_cache() -> Vec<CalEvent> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = match std::fs::read_dir(cache_dir()) {
        Ok(d) => d.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
        Err(_) => return out,
    };
    entries.sort();
    for p in entries {
        if let Ok(text) = std::fs::read_to_string(&p) {
            out.extend(parse_ics(&text));
        }
    }
    out.sort_by_key(|e| e.start_epoch);
    out
}

/// Parse an ICS calendar and keep only events on the local "today" (skipping
/// birthdays / anniversaries — Google emits those with the BIRTHDAY category,
/// iCloud titles them "...'s Birthday").
pub fn parse_ics(text: &str) -> Vec<CalEvent> {
    let (ty, tm, td) = timefmt::today_parts();
    let now = timefmt::now_epoch() as i64;

    let mut events = Vec::new();
    for block in block_split(text, "BEGIN:VEVENT", "END:VEVENT") {
        let props = ics_props(&block);
        let summary = props
            .get("SUMMARY")
            .map(String::as_str)
            .unwrap_or("(untitled)");
        let cat = props.get("CATEGORIES").cloned().unwrap_or_default();
        let cat_up = cat.to_uppercase();
        if cat_up.contains("BIRTHDAY") || cat_up.contains("ANNIVERSARY") {
            continue;
        }
        let low = summary.to_lowercase();
        if low.contains("birthday") || low.contains("anniversary") || low.contains("holiday") {
            continue;
        }
        let dtstart = match props.get("DTSTART") {
            Some(d) => d,
            None => continue,
        };
        let Some((cy, cm, cd, hhmm, is_utc, all_day)) = parse_dtstart(dtstart) else {
            continue;
        };
        if all_day {
            if (cy, cm, cd) != (ty, tm, td) {
                continue;
            }
            let start = timefmt::days_from_civil(cy, cm, cd) * 86400 - timefmt::local_offset_secs();
            events.push(CalEvent {
                summary: summary.trim().to_string(),
                location: props
                    .get("LOCATION")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default(),
                all_day: true,
                time_str: "All day".into(),
                start_epoch: start as u64,
            });
            continue;
        }
        // Timed event. Resolve its instant to the local timezone.
        let wall_start = timefmt::days_from_civil(cy, cm, cd) * 86400
            + (hhmm.0 as i64 * 3600 + hhmm.1 as i64 * 60)
            - if is_utc {
                0
            } else {
                timefmt::local_offset_secs()
            };
        let t = timefmt_parts(wall_start);
        let (ly, lm, ld, lh, lmn) = t;
        if (ly, lm, ld) != (ty, tm, td) {
            continue;
        }
        if wall_start < now - 3600 {
            // not upcoming anymore; skip
            continue;
        }
        events.push(CalEvent {
            summary: summary.trim().to_string(),
            location: props
                .get("LOCATION")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            all_day: false,
            time_str: format!("{:02}:{:02}", lh.min(23), lmn.min(59)),
            start_epoch: wall_start as u64,
        });
    }
    events.sort_by_key(|e| e.start_epoch);
    events
}

/// Cheap local-time pieces for an epoch (shares libc localtime_r).
fn timefmt_parts(epoch: i64) -> (i32, u32, u32, u32, u32) {
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
        localtime_r(&epoch, &mut c);
    }
    (
        c.tm_year + 1900,
        (c.tm_mon + 1) as u32,
        c.tm_mday as u32,
        c.tm_hour as u32,
        c.tm_min as u32,
    )
}

/// Split text on BEGIN/END markers, returning interior content lines.
fn block_split(text: &str, begin: &str, end: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut inside = false;
    for line in text.lines() {
        let l = line.trim_end_matches('\r');
        if l.eq_ignore_ascii_case(begin) {
            inside = true;
            cur.clear();
            continue;
        }
        if l.eq_ignore_ascii_case(end) {
            inside = false;
            if !cur.trim().is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if inside {
            cur.push_str(l);
            cur.push('\n');
        }
    }
    out
}

/// Parse folded ICS lines into a map of the LAST value per property name.
fn ics_props(block: &str) -> std::collections::HashMap<String, String> {
    let mut props = std::collections::HashMap::new();
    let mut pending: Option<String> = None;
    for raw in block.lines() {
        let line = raw.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(p) = pending.as_mut() {
                p.push_str(line.trim_start());
            }
            continue;
        }
        if let Some(p) = pending.take() {
            if let Some(idx) = p.find(':') {
                let raw_key = &p[..idx];
                // strip parameters like ;TZID=... or ;VALUE=DATE so DTSTART;TZID=... → DTSTART
                let key = raw_key.split(';').next().unwrap().to_uppercase();
                let val = p[idx + 1..].to_string();
                props.insert(key, val);
            }
        }
        pending = Some(line.to_string());
    }
    if let Some(p) = pending {
        if let Some(idx) = p.find(':') {
            let raw_key = &p[..idx];
            let key = raw_key.split(';').next().unwrap().to_uppercase();
            let val = p[idx + 1..].to_string();
            props.insert(key, val);
        }
    }
    props
}

/// Returns (y, m, d, (h, m), is_utc, all_day).
#[allow(clippy::type_complexity)]
fn parse_dtstart(s: &str) -> Option<(i32, u32, u32, (u32, u32), bool, bool)> {
    // s may be just "20260831T100000" (from ics_props) or full "DTSTART;TZID=...:20260831T100000"
    let value = s.rsplit(':').next().unwrap_or(s);
    let (date, time) = match value.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (value, None),
    };
    if date.len() != 8 {
        return None;
    }
    let y: i32 = date[0..4].parse().ok()?;
    let m: u32 = date[4..6].parse().ok()?;
    let d: u32 = date[6..8].parse().ok()?;
    match time {
        None => Some((y, m, d, (0, 0), false, true)),
        Some(t) => {
            let utc = t.ends_with('Z');
            let t = t.trim_end_matches('Z');
            let h: u32 = t.get(0..2)?.parse().ok()?;
            let mi: u32 = t.get(2..4)?.parse().ok()?;
            Some((y, m, d, (h, mi), utc, false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dtstart_variants() {
        assert_eq!(
            parse_dtstart("DTSTART:20260826T120000Z"),
            Some((2026, 8, 26, (12, 0), true, false))
        );
        assert_eq!(
            parse_dtstart("DTSTART;TZID=America/New_York:20260826T090000"),
            Some((2026, 8, 26, (9, 0), false, false))
        );
        assert_eq!(
            parse_dtstart("DTSTART;VALUE=DATE:20260826"),
            Some((2026, 8, 26, (0, 0), false, true))
        );
    }

    #[test]
    fn unfold_and_blocks() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Morning \r\n standup\nEND:VEVENT\n";
        let props = ics_props(block_split(ics, "BEGIN:VEVENT", "END:VEVENT")[0].as_str());
        assert_eq!(props["SUMMARY"], "Morning standup");
    }

    #[test]
    fn filters_birthdays_by_category() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Alex's birthday (30)\nCATEGORIES:BIRTHDAY\nDTSTART;VALUE=DATE:20260827\nEND:VEVENT\nEND:VCALENDAR";
        // Any date regardless — category filter drops it before date handling.
        assert_eq!(parse_ics(text).len(), 0);
    }
}
