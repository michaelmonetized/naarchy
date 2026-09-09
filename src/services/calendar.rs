//! Calendar: fetch iCloud/Google ICS feeds, cache them, and parse today's
//! meetings (filtering out noise like birthdays/anniversaries/holidays).

use super::encode_uri_component;
use crate::timefmt;
use gtk4::glib::{DateTime, TimeZone};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::services::{Event, EventTx};

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
    /// Virtual meeting join URL (Meet/Zoom/Teams) extracted from DESCRIPTION/LOCATION/URL
    pub join_url: Option<String>,
    /// Human label for join_url: "Meet" | "Zoom" | "Teams"
    pub join_kind: Option<String>,
    /// Driving directions URL for physical addresses (Google Maps)
    pub directions_url: Option<String>,
    /// "Leave 09:23 (18 min)" computed from current location — filled async
    pub leave_label: Option<String>,
}

fn cache_dir() -> PathBuf {
    crate::util::cache_dir().join("calendar")
}

const FEED_LIMIT: usize = 8 * 1024 * 1024;

fn feed_path(url: &str) -> PathBuf {
    cache_dir().join(format!("{}.ics", crate::util::cache_key(url.as_bytes())))
}

/// Download with bounded parallelism; publish complete feeds with atomic rename.
pub async fn refresh_feeds(feeds: &[String]) {
    for group in feeds.chunks(4) {
        let mut tasks = tokio::task::JoinSet::new();
        for url in group {
            let url = url.clone();
            tasks.spawn_blocking(move || {
                let path = feed_path(&url);
                let Some(body) = fetch(&url) else {
                    // Calendar URLs often contain private feed tokens. Never log them.
                    log::warn!("calendar: feed refresh failed; keeping last cached copy");
                    return;
                };
                if std::fs::create_dir_all(cache_dir()).is_err() {
                    return;
                }
                let pending = path.with_extension("pending");
                if std::fs::write(&pending, body).is_ok() {
                    let _ = std::fs::rename(&pending, &path);
                }
                let _ = std::fs::remove_file(pending);
            });
        }
        while tasks.join_next().await.is_some() {}
    }
}

fn fetch(url: &str) -> Option<String> {
    let url = url
        .strip_prefix("webcal://")
        .map(|u| format!("https://{u}"))
        .unwrap_or_else(|| url.to_string());
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return None;
    }
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(15))
        .call()
        .ok()?;
    let bytes = super::read_limited(resp.into_reader(), FEED_LIMIT).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    (text.starts_with("BEGIN:VCALENDAR") && text.trim_end().ends_with("END:VCALENDAR"))
        .then(|| text.to_string())
}

/// Background service: refresh feeds every `refresh_min`, then emit
/// `Event::CalendarReload` so the UI re-renders.
pub async fn run(tx: EventTx, feeds: Vec<String>, refresh_min: u64) {
    if feeds.is_empty() {
        return;
    }
    // Remove caches for calendars no longer configured, including old index-based names.
    // Otherwise deleting/reordering a feed could keep showing its private events forever.
    let keep: std::collections::HashSet<_> = feeds.iter().map(|url| feed_path(url)).collect();
    if let Ok(entries) = std::fs::read_dir(cache_dir()) {
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            if path.extension().is_some_and(|ext| ext == "ics") && !keep.contains(&path) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    tx.send(Event::CalendarReload);
    loop {
        refresh_feeds(&feeds).await;
        tx.send(Event::CalendarReload);
        for _ in 0..refresh_min.clamp(1, 1440) {
            tokio::time::sleep(Duration::from_secs(60)).await;
            // Re-evaluate local dates and expired meetings without another download.
            tx.send(Event::CalendarReload);
        }
    }
}

/// Read every cached feed and return today's meetings, newest source last wins.
pub fn today_from_cache() -> Vec<CalEvent> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = match std::fs::read_dir(cache_dir()) {
        Ok(d) => d
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "ics"))
            .collect(),
        Err(_) => return out,
    };
    entries.sort();
    for p in entries {
        if let Ok(file) = std::fs::File::open(&p) {
            if let Some(text) = super::read_limited(file, FEED_LIMIT)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
            {
                out.extend(parse_ics(&text));
            }
        }
    }
    out.sort_by_key(|e| e.start_epoch);
    out
}

/// Enrich physical-location events with "Leave HH:MM (N min)" via current location.
/// Blocking — call from a background thread after travel estimates are enabled.
pub fn enrich_with_travel(mut events: Vec<CalEvent>) -> Vec<CalEvent> {
    if !events
        .iter()
        .any(|ev| ev.directions_url.is_some() && !ev.all_day)
    {
        return events;
    }
    let Some(cur) = crate::services::location::current_coords() else {
        return events;
    };
    for ev in events.iter_mut() {
        if ev.all_day || ev.directions_url.is_none() || ev.location.is_empty() {
            continue;
        }
        // avoid hammering Nominatim/OSRM for far-future events (only today, already filtered)
        if let Some(dest) = crate::services::location::geocode(&ev.location) {
            if let Some(label) =
                crate::services::location::leave_label_for(ev.start_epoch, cur, dest)
            {
                ev.leave_label = Some(label);
            }
        }
    }
    events
}

/// Parse an ICS calendar and keep only events on the local "today" (skipping
/// birthdays / anniversaries — Google emits those with the BIRTHDAY category,
/// iCloud titles them "...'s Birthday").
pub fn parse_ics(text: &str) -> Vec<CalEvent> {
    parse_ics_at(text, timefmt::now_epoch() as i64)
}

type Properties = HashMap<String, String>;

struct CalendarRecord {
    props: Properties,
    recurrence: String,
    uid: String,
    recurrence_id: Option<i64>,
}

fn cancelled(props: &Properties) -> bool {
    props
        .get("STATUS")
        .is_some_and(|v| v.eq_ignore_ascii_case("CANCELLED"))
}

fn revision(props: &Properties) -> (u64, &str) {
    (
        props
            .get("SEQUENCE")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        props
            .get("LAST-MODIFIED")
            .or_else(|| props.get("DTSTAMP"))
            .map(String::as_str)
            .unwrap_or_default(),
    )
}

fn parse_ics_at(text: &str, now: i64) -> Vec<CalEvent> {
    let Ok(today) = DateTime::from_unix_local(now) else {
        return vec![];
    };
    let Ok(midnight) =
        DateTime::from_local(today.year(), today.month(), today.day_of_month(), 0, 0, 0.0)
    else {
        return vec![];
    };
    let Ok(tomorrow) = midnight.add_days(1) else {
        return vec![];
    };
    let (begin, end) = (midnight.to_unix(), tomorrow.to_unix());
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut records: BTreeMap<(String, Option<i64>), CalendarRecord> = BTreeMap::new();
    for (index, block) in block_split(text, "BEGIN:VEVENT", "END:VEVENT")
        .into_iter()
        .take(10_000)
        .enumerate()
    {
        let props = ics_props(&block);
        let uid = props
            .get("UID")
            .cloned()
            .unwrap_or_else(|| format!("anonymous-{index}"));
        let recurrence_id = event_datetime(&props, "RECURRENCE-ID").map(|(date, _)| date.to_unix());
        let recurrence = unfold_ics(&block)
            .lines()
            .filter(|line| {
                matches!(
                    line.split([';', ':'])
                        .next()
                        .unwrap_or_default()
                        .to_uppercase()
                        .as_str(),
                    "DTSTART" | "RRULE" | "RDATE" | "EXDATE"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let key = (uid.clone(), recurrence_id);
        if records
            .get(&key)
            .is_some_and(|old| revision(&old.props) > revision(&props))
        {
            continue;
        }
        records.insert(
            key,
            CalendarRecord {
                props,
                recurrence,
                uid,
                recurrence_id,
            },
        );
    }
    let replaced: HashSet<_> = records
        .values()
        .filter_map(|r| r.recurrence_id.map(|id| (r.uid.as_str(), id)))
        .collect();
    let cancelled_series: HashSet<_> = records
        .values()
        .filter(|r| r.recurrence_id.is_none() && cancelled(&r.props))
        .map(|r| r.uid.as_str())
        .collect();
    let mut events = Vec::new();
    for record in records.values() {
        if Instant::now() >= deadline {
            log::warn!("calendar: recurrence expansion reached processing limit");
            break;
        }
        if cancelled(&record.props) || cancelled_series.contains(record.uid.as_str()) {
            continue;
        }
        // Detached instances inherit unchanged details from their recurring master.
        let mut props = if record.recurrence_id.is_some() {
            records
                .get(&(record.uid.clone(), None))
                .map(|r| r.props.clone())
                .unwrap_or_default()
        } else {
            Properties::new()
        };
        props.extend(record.props.clone());
        let Some((start, all_day)) = event_start(&props) else {
            continue;
        };
        let recurring = record.recurrence_id.is_none()
            && (props.contains_key("RRULE") || props.contains_key("RDATE"));
        let starts = if recurring {
            recurrence_dates(&record.recurrence, begin, end, deadline)
        } else {
            vec![start.to_unix()]
        };
        for start in starts {
            if record.recurrence_id.is_none() && replaced.contains(&(record.uid.as_str(), start)) {
                continue;
            }
            if let Some(event) = event_for_occurrence(&props, start, all_day, begin, end, now) {
                events.push(event);
            }
        }
    }
    events.sort_by_key(|e| e.start_epoch);
    events
}

/// The iterator's internal limit plus outer count/time limits bound hostile or
/// accidentally huge recurrences, including infinitely repeating rules.
fn recurrence_dates(text: &str, begin: i64, end: i64, deadline: Instant) -> Vec<i64> {
    let Ok(set) = text.parse::<rrule::RRuleSet>() else {
        log::debug!("calendar: invalid recurrence set skipped");
        return vec![];
    };
    let start = *set.get_dt_start();
    let set = set.rdate(start).limit();
    let mut dates = Vec::new();
    for (index, date) in set.into_iter().take(100_000).enumerate() {
        if index % 64 == 0 && Instant::now() >= deadline {
            break;
        }
        let epoch = date.timestamp();
        if epoch >= end {
            break;
        }
        if epoch >= begin {
            if dates.last() != Some(&epoch) {
                dates.push(epoch);
            }
            if dates.len() >= 256 {
                break;
            }
        }
    }
    dates
}

fn event_for_occurrence(
    props: &Properties,
    start: i64,
    all_day: bool,
    begin: i64,
    end: i64,
    now: i64,
) -> Option<CalEvent> {
    let summary = props
        .get("SUMMARY")
        .map(String::as_str)
        .unwrap_or("(untitled)")
        .trim();
    let category = props
        .get("CATEGORIES")
        .map(|v| v.to_uppercase())
        .unwrap_or_default();
    if category.contains("BIRTHDAY") || category.contains("ANNIVERSARY") {
        return None;
    }
    let title = summary.to_lowercase();
    if title.contains("birthday") || title.contains("anniversary") || title.contains("holiday") {
        return None;
    }
    let original_start = event_start(props)?.0.to_unix();
    let duration = event_datetime(props, "DTEND")
        .map(|(date, _)| (date.to_unix() - original_start).max(0))
        .unwrap_or(0);
    let finish = start.saturating_add(duration);
    if start >= end || (start < begin && finish <= begin) {
        return None;
    }
    if !all_day && start < now - 3600 && finish <= now {
        return None;
    }
    let local = DateTime::from_unix_local(start).ok()?;
    let location = props
        .get("LOCATION")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let (join_url, join_kind) = extract_join_url(props);
    let directions_url = is_physical_address(&location).then(|| directions_url_for(&location));
    Some(CalEvent {
        summary: summary.to_string(),
        location,
        all_day,
        time_str: if all_day {
            "All day".into()
        } else {
            format!("{:02}:{:02}", local.hour(), local.minute())
        },
        start_epoch: start.max(0) as u64,
        join_url,
        join_kind,
        directions_url,
        leave_label: None,
    })
}

/// Resolve TZID/UTC/floating dates with GLib's timezone database, including DST.
fn event_start(props: &std::collections::HashMap<String, String>) -> Option<(DateTime, bool)> {
    event_datetime(props, "DTSTART")
}

fn event_datetime(props: &Properties, property: &str) -> Option<(DateTime, bool)> {
    let (year, month, day, (hour, minute), utc, all_day) = parse_dtstart(props.get(property)?)?;
    let timezone = if utc {
        TimeZone::utc()
    } else if let Some(tzid) = props.get(&format!("{property};TZID")) {
        {
            let id = tzid.trim_matches('"');
            let zone = TimeZone::new(Some(id));
            if zone.identifier().as_str() != id {
                return None;
            }
            zone
        }
    } else {
        TimeZone::local()
    };
    let start = DateTime::new(
        &timezone,
        year,
        month as i32,
        day as i32,
        hour as i32,
        minute as i32,
        props
            .get(property)
            .and_then(|v| v.split_once('T'))
            .and_then(|(_, t)| t.get(4..6))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            .min(59) as f64,
    )
    .ok()?;
    Some((start, all_day))
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
    let unfolded = unfold_ics(block);
    for line in unfolded.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let mut parameters = name.split(';');
        let key = parameters.next().unwrap_or_default().to_uppercase();
        for parameter in parameters {
            if let Some((name, value)) = parameter.split_once('=') {
                if name.eq_ignore_ascii_case("TZID") {
                    props.insert(format!("{key};TZID"), value.to_string());
                }
            }
        }
        props.insert(key, unescape_ics(value));
    }
    props
}

fn unfold_ics(block: &str) -> String {
    let mut unfolded = String::new();
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            // RFC 5545 removes exactly one folding character, preserving content spaces.
            unfolded.push_str(&line[1..]);
        } else {
            unfolded.push('\n');
            unfolded.push_str(line);
        }
    }
    // Alarm components have their own SUMMARY/DTSTART. They must not replace
    // the surrounding event's fields or leak into the recurrence rule set.
    let mut depth = 0_u32;
    unfolded
        .lines()
        .filter(|line| {
            if line.starts_with("BEGIN:") {
                depth += 1;
                return false;
            }
            if line.starts_with("END:") {
                depth = depth.saturating_sub(1);
                return false;
            }
            depth == 0
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape_ics(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(c) => out.push(c),
            None => out.push('\\'),
        }
    }
    out
}

fn extract_join_url(
    props: &std::collections::HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    // scan all prop values for a virtual meeting URL, prefer Meet > Zoom > Teams
    let mut candidates: Vec<(String, String)> = Vec::new();
    for v in props.values() {
        for url in find_urls(v) {
            if let Some(kind) = classify_join_url(&url) {
                candidates.push((url, kind));
            }
        }
    }
    // also check raw block values that might be in X- props with different casing
    if candidates.is_empty() {
        return (None, None);
    }
    // prefer Meet, then Zoom, then Teams
    let order = |k: &str| match k {
        "Meet" => 0,
        "Zoom" => 1,
        "Teams" => 2,
        _ => 3,
    };
    candidates.sort_by_key(|(_, k)| order(k));
    let (url, kind) = candidates.into_iter().next().unwrap();
    (Some(url), Some(kind))
}

fn find_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = text;
    while let Some(start) = [rest.find("https://"), rest.find("http://")]
        .into_iter()
        .flatten()
        .min()
    {
        rest = &rest[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '>' | '<' | ')' | ']'))
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ',', ';', '!']);
        if url.len() > 10 {
            urls.push(url.to_string());
        }
        rest = &rest[end..];
    }
    urls
}

fn classify_join_url(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.to_ascii_lowercase();
    let matches = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if host == "meet.google.com" {
        Some("Meet".into())
    } else if matches("zoom.us") || matches("zoom.com") {
        Some("Zoom".into())
    } else if matches("teams.microsoft.com")
        || matches("teams.live.com")
        || matches("teams.cloud.microsoft")
    {
        Some("Teams".into())
    } else {
        None
    }
}

fn is_physical_address(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    // if it looks like a URL, not physical
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return false;
    }
    if classify_join_url(t).is_some() {
        return false;
    }
    // must not be all URL-like, and contain some address heuristics
    // Google Calendar often puts address with commas and numbers
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_comma = t.contains(',');
    let has_letter = t.chars().any(|c| c.is_alphabetic());
    let len_ok = t.len() > 8;
    // if it has a comma and digit and letter, likely address; or long with comma
    (has_digit && has_letter && len_ok) || (has_comma && len_ok && has_letter)
}

fn directions_url_for(address: &str) -> String {
    // use Google Maps directions with destination; origin defaults to current location
    let enc = encode_uri_component(address);
    format!(
        "https://www.google.com/maps/dir/?api=1&destination={}&travelmode=driving",
        enc
    )
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
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y: i32 = date[0..4].parse().ok()?;
    let m: u32 = date[4..6].parse().ok()?;
    let d: u32 = date[6..8].parse().ok()?;
    if !(1..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || d == 0
        || d > timefmt::days_in_month(y, m)
    {
        return None;
    }
    match time {
        None => Some((y, m, d, (0, 0), false, true)),
        Some(t) => {
            let utc = t.ends_with('Z');
            let t = t.trim_end_matches('Z');
            if !matches!(t.len(), 4 | 6) || !t.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let h: u32 = t.get(0..2)?.parse().ok()?;
            let mi: u32 = t.get(2..4)?.parse().ok()?;
            if h > 23 || mi > 59 || (t.len() == 6 && t[4..6].parse::<u32>().ok()? > 60) {
                return None;
            }
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

    #[test]
    fn rejects_malformed_dates_without_panicking() {
        for value in [
            "éééé",
            "20260230",
            "20261301",
            "20260101T256000",
            "20260101T1200garbage",
        ] {
            assert!(parse_dtstart(value).is_none(), "{value}");
        }
        assert!(parse_dtstart("20240229T123000Z").is_some());
    }

    #[test]
    fn honors_calendar_timezone_and_daylight_saving() {
        let summer = event_start(&ics_props("DTSTART;TZID=America/New_York:20260701T090000"))
            .unwrap()
            .0;
        let winter = event_start(&ics_props("DTSTART;TZID=America/New_York:20260101T090000"))
            .unwrap()
            .0;
        assert_eq!(summer.to_utc().unwrap().hour(), 13);
        assert_eq!(winter.to_utc().unwrap().hour(), 14);
        assert!(event_start(&ics_props("DTSTART;TZID=Invalid/Zone:20260701T090000")).is_none());
    }

    #[test]
    fn unfolds_exactly_one_space_and_decodes_escaped_text() {
        let props =
            ics_props("SUMMARY:Plan\\, review\\; ship\n  today\nDESCRIPTION:Line one\\nLine two");
        assert_eq!(props["SUMMARY"], "Plan, review; ship today");
        assert_eq!(props["DESCRIPTION"], "Line one\nLine two");
    }

    #[test]
    fn meeting_links_require_real_provider_hosts() {
        assert_eq!(
            classify_join_url("https://company.zoom.us/j/123").as_deref(),
            Some("Zoom")
        );
        assert!(classify_join_url("https://zoom.us.attacker.test/j/123").is_none());
        assert!(classify_join_url("https://attacker.test/?url=meet.google.com").is_none());
        assert_eq!(
            find_urls("http://first.test https://meet.google.com/abc。 next").len(),
            2
        );
        assert_eq!(
            find_urls("https://meet.google.com/abc\u{2003}after"),
            vec!["https://meet.google.com/abc"]
        );
    }

    fn epoch(value: &str) -> i64 {
        DateTime::from_iso8601(value, None).unwrap().to_unix()
    }

    #[test]
    fn recurring_meetings_follow_dst_and_excluded_dates() {
        let dates = recurrence_dates(
            "DTSTART;TZID=America/New_York:20261025T090000\nRRULE:FREQ=WEEKLY;COUNT=4\nEXDATE;TZID=America/New_York:20261108T090000",
            epoch("2026-10-25T00:00:00Z"), epoch("2026-11-20T00:00:00Z"),
            Instant::now() + Duration::from_secs(1),
        );
        assert_eq!(
            dates,
            vec![
                epoch("2026-10-25T13:00:00Z"),
                epoch("2026-11-01T14:00:00Z"),
                epoch("2026-11-15T14:00:00Z")
            ]
        );
    }

    #[test]
    fn detached_edits_and_cancellations_replace_original_instances() {
        let calendar = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:standup\nSUMMARY:Standup\nLOCATION:Room 2\nDTSTART:20260901T150000Z\nRRULE:FREQ=DAILY;COUNT=10\nEND:VEVENT\nBEGIN:VEVENT\nUID:standup\nRECURRENCE-ID:20260905T150000Z\nDTSTART:20260905T170000Z\nSUMMARY:Delayed standup\nSEQUENCE:2\nEND:VEVENT\nEND:VCALENDAR";
        let now = epoch("2026-09-05T12:00:00Z");
        let events = parse_ics_at(calendar, now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Delayed standup");
        assert_eq!(events[0].location, "Room 2");
        assert_eq!(events[0].start_epoch, epoch("2026-09-05T17:00:00Z") as u64);
        let cancelled = calendar.replace("SEQUENCE:2", "SEQUENCE:2\nSTATUS:CANCELLED");
        assert!(parse_ics_at(&cancelled, now).is_empty());
    }

    #[test]
    fn all_day_recurrence_and_duplicate_revisions_are_supported() {
        let calendar = "BEGIN:VEVENT\nUID:planning\nSUMMARY:Old\nDTSTART;VALUE=DATE:20260901\nRRULE:FREQ=DAILY;COUNT=6\nSEQUENCE:1\nEND:VEVENT\nBEGIN:VEVENT\nUID:planning\nSUMMARY:Planning\nDTSTART;VALUE=DATE:20260901\nRRULE:FREQ=DAILY;COUNT=6\nSEQUENCE:2\nEND:VEVENT";
        let now = DateTime::from_local(2026, 9, 5, 12, 0, 0.0)
            .unwrap()
            .to_unix();
        let events = parse_ics_at(calendar, now);
        assert_eq!(events.len(), 1);
        assert!(events[0].all_day);
        assert_eq!(events[0].summary, "Planning");
    }

    #[test]
    fn recurrence_results_and_processing_are_bounded() {
        let begin = epoch("2026-09-05T00:00:00Z");
        let text = "DTSTART:20260905T000000Z\nRRULE:FREQ=SECONDLY";
        let dates = recurrence_dates(
            text,
            begin,
            begin + 86400,
            Instant::now() + Duration::from_secs(1),
        );
        assert_eq!(dates.len(), 256);
        assert!(recurrence_dates(text, begin, begin + 86400, Instant::now()).is_empty());
    }

    #[test]
    fn embedded_alarm_cannot_override_event_details() {
        let props = ics_props("SUMMARY:Actual meeting\nDTSTART:20260905T120037Z\nBEGIN:VALARM\nSUMMARY:Alarm\nDTSTART:20260905T110000Z\nEND:VALARM");
        assert_eq!(props["SUMMARY"], "Actual meeting");
        assert_eq!(event_start(&props).unwrap().0.second(), 37);
    }
}
