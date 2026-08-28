//! Persisted set of widgets shown on the Home shelf. Defaults: Timer + Media
//! so both stay reachable without extra setup; more can be dragged in from the
//! widget drawer.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetKind {
    Media,
    Timer,
    Clock,
    Battery,
}

impl WidgetKind {
    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Media => "Media",
            WidgetKind::Timer => "Timer",
            WidgetKind::Clock => "Clock",
            WidgetKind::Battery => "Battery",
        }
    }
    /// Nerd Font glyph used on the widget drawer tiles.
    pub fn glyph(self) -> &'static str {
        match self {
            WidgetKind::Media => "\u{f001}",
            WidgetKind::Timer => "\u{f017}",
            WidgetKind::Clock => "\u{f017}",
            WidgetKind::Battery => "\u{f240}",
        }
    }
    pub fn all() -> [WidgetKind; 4] {
        [
            WidgetKind::Media,
            WidgetKind::Timer,
            WidgetKind::Clock,
            WidgetKind::Battery,
        ]
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetStore {
    pub widgets: Vec<WidgetKind>,
}

impl Default for WidgetStore {
    fn default() -> Self {
        Self {
            widgets: vec![WidgetKind::Timer, WidgetKind::Media],
        }
    }
}

impl WidgetStore {
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("naarchy")
            .join("widgets.json")
    }

    pub fn load() -> Self {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }

    pub fn has(&self, kind: WidgetKind) -> bool {
        self.widgets.contains(&kind)
    }

    pub fn add(&mut self, kind: WidgetKind) -> bool {
        if self.has(kind) {
            return false;
        }
        self.widgets.push(kind);
        self.save();
        true
    }

    pub fn remove(&mut self, kind: WidgetKind) -> bool {
        let n = self.widgets.len();
        self.widgets.retain(|k| *k != kind);
        if self.widgets.len() != n {
            self.save();
            true
        } else {
            false
        }
    }

    /// Returns true when the widget is now on the Home shelf.
    pub fn toggle(&mut self, kind: WidgetKind) -> bool {
        if self.has(kind) {
            self.remove(kind);
            false
        } else {
            self.add(kind);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_timer_and_media() {
        let s = WidgetStore::default();
        assert!(s.has(WidgetKind::Timer));
        assert!(s.has(WidgetKind::Media));
    }

    #[test]
    fn roundtrip_and_add_is_idempotent() {
        let mut s = WidgetStore::default();
        assert!(s.add(WidgetKind::Clock));
        assert!(!s.add(WidgetKind::Clock));
        assert!(s.add(WidgetKind::Battery));
        let ser = serde_json::to_string(&s).unwrap();
        let back: WidgetStore = serde_json::from_str(&ser).unwrap();
        assert_eq!(back.widgets, s.widgets);
    }
}
