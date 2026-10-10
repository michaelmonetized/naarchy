//! The plugin wire protocol: newline-delimited JSON.
//!
//! Host → plugin (stdin): one `hello` line, then nothing. When stdin reaches
//! EOF the host has gone away and the plugin must exit.
//!
//! Plugin → host (stdout), one object per line:
//!
//! ```json
//! {"type":"activity","id":"run-1","icon":"","title":"naarchy","detail":"running","started_at":1790000000,"priority":40}
//! {"type":"clear","id":"run-1"}
//! {"type":"clear"}
//! {"type":"log","level":"warn","message":"token expired"}
//! ```
//!
//! `activity` inserts or replaces one live activity by `id`. Naarchy renders
//! the elapsed time from `started_at` itself, so a plugin only writes when
//! something changes. Lines over 16 KiB, unknown types, and malformed JSON
//! are dropped; text is trimmed to what fits on the island.

use serde::{Deserialize, Serialize};

pub const MAX_LINE_BYTES: usize = 16 * 1024;
/// Activities beyond this per plugin are ignored, so one plugin cannot
/// grow the host's memory or crowd out everything else.
pub const MAX_ACTIVITIES_PER_PLUGIN: usize = 8;
const MAX_ID: usize = 128;
const MAX_ICON: usize = 4;
const MAX_TITLE: usize = 48;
const MAX_DETAIL: usize = 96;

#[derive(Debug, Serialize)]
pub struct Hello<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub api: u32,
    pub naarchy: &'static str,
    pub plugin: &'a str,
}

impl<'a> Hello<'a> {
    pub fn new(plugin: &'a str) -> Self {
        Self {
            kind: "hello",
            api: super::manifest::API_VERSION,
            naarchy: env!("CARGO_PKG_VERSION"),
            plugin,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Wire {
    Activity {
        id: String,
        #[serde(default)]
        icon: String,
        title: String,
        #[serde(default)]
        detail: String,
        #[serde(default)]
        started_at: Option<u64>,
        #[serde(default)]
        priority: Option<u8>,
    },
    Clear {
        #[serde(default)]
        id: Option<String>,
    },
    Log {
        #[serde(default)]
        level: String,
        message: String,
    },
}

/// One live activity, already bounded for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub started_at: Option<u64>,
    /// 0-100. 50 and above outranks parked files and music on the island.
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Upsert(Activity),
    Clear(Option<String>),
    Log { level: log::Level, message: String },
}

fn clip(s: &str, max_chars: usize) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let clean = clean.trim();
    if clean.chars().count() <= max_chars {
        clean.to_string()
    } else {
        let mut out: String = clean.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Decode one stdout line. `None` means the line is ignored.
pub fn parse_line(line: &str) -> Option<Message> {
    if line.len() > MAX_LINE_BYTES || line.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<Wire>(line).ok()? {
        Wire::Activity {
            id,
            icon,
            title,
            detail,
            started_at,
            priority,
        } => {
            if id.is_empty() || id.len() > MAX_ID {
                return None;
            }
            let title = clip(&title, MAX_TITLE);
            if title.is_empty() {
                return None;
            }
            Some(Message::Upsert(Activity {
                id,
                icon: clip(&icon, MAX_ICON),
                title,
                detail: clip(&detail, MAX_DETAIL),
                started_at,
                priority: priority.unwrap_or(40).min(100),
            }))
        }
        Wire::Clear { id } => Some(Message::Clear(id.filter(|i| i.len() <= MAX_ID))),
        Wire::Log { level, message } => Some(Message::Log {
            level: match level.as_str() {
                "error" => log::Level::Error,
                "warn" | "warning" => log::Level::Warn,
                "debug" => log::Level::Debug,
                _ => log::Level::Info,
            },
            message: clip(&message, 512),
        }),
    }
}

/// Compact elapsed label for the island: `42s`, `12m`, `1h04m`, `3d`.
pub fn elapsed_label(started_at: u64, now: u64) -> String {
    let secs = now.saturating_sub(started_at);
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60),
        _ => format!("{}d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_round_trip_with_defaults() {
        let m = parse_line(r#"{"type":"activity","id":"a","title":"naarchy"}"#).unwrap();
        assert_eq!(
            m,
            Message::Upsert(Activity {
                id: "a".into(),
                icon: String::new(),
                title: "naarchy".into(),
                detail: String::new(),
                started_at: None,
                priority: 40,
            })
        );
    }

    #[test]
    fn text_is_bounded_and_control_free() {
        let long = "x".repeat(500);
        let line = format!(
            r#"{{"type":"activity","id":"a","icon":"abcdefgh","title":"{long}","detail":"one\ntwo","priority":250}}"#
        );
        let Some(Message::Upsert(a)) = parse_line(&line) else {
            panic!("expected activity");
        };
        assert_eq!(a.title.chars().count(), MAX_TITLE);
        assert!(a.title.ends_with('…'));
        assert_eq!(a.icon.chars().count(), MAX_ICON);
        assert_eq!(a.detail, "one two");
        assert_eq!(a.priority, 100);
    }

    #[test]
    fn junk_is_ignored() {
        for line in [
            "",
            "not json",
            r#"{"type":"exec","cmd":"rm"}"#,
            r#"{"type":"activity","id":"","title":"x"}"#,
            r#"{"type":"activity","id":"a","title":"   "}"#,
        ] {
            assert_eq!(parse_line(line), None, "{line:?}");
        }
        let huge = format!(
            r#"{{"type":"activity","id":"a","title":"{}"}}"#,
            "x".repeat(MAX_LINE_BYTES)
        );
        assert_eq!(parse_line(&huge), None);
    }

    #[test]
    fn clear_one_or_all() {
        assert_eq!(
            parse_line(r#"{"type":"clear","id":"a"}"#),
            Some(Message::Clear(Some("a".into())))
        );
        assert_eq!(
            parse_line(r#"{"type":"clear"}"#),
            Some(Message::Clear(None))
        );
    }

    #[test]
    fn elapsed_labels_stay_short() {
        assert_eq!(elapsed_label(100, 142), "42s");
        assert_eq!(elapsed_label(0, 12 * 60 + 5), "12m");
        assert_eq!(elapsed_label(0, 3600 + 4 * 60), "1h04m");
        assert_eq!(elapsed_label(0, 3 * 86_400 + 5), "3d");
        assert_eq!(elapsed_label(500, 100), "0s");
    }
}
