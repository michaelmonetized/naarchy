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
}

impl WidgetKind {
    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Media => "Media",
            WidgetKind::Timer => "Timer",
            WidgetKind::Clock => "Clock",
        }
    }
    /// Nerd Font glyph used on the widget drawer tiles.
    pub fn glyph(self) -> &'static str {
        match self {
            WidgetKind::Media => "\u{f001}",
            WidgetKind::Timer => "\u{f017}",
            WidgetKind::Clock => "\u{f017}",
        }
    }
    pub fn all() -> [WidgetKind; 3] {
        [WidgetKind::Media, WidgetKind::Timer, WidgetKind::Clock]
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
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for WidgetStore {
    fn default() -> Self {
        Self {
            widgets: vec![WidgetKind::Timer, WidgetKind::Media],
            path: None,
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
        Self::open(Self::path())
    }

    /// Open a layout at an explicit path so tests and previews never change user preferences.
    pub fn open(path: PathBuf) -> Self {
        let mut store = crate::shelf_store::load_state::<serde_json::Value>(&path)
            .map(|value| parse_widgets(&value.to_string()))
            .unwrap_or_default();
        store.path = Some(path);
        store
    }

    pub fn save(&self) -> bool {
        self.path
            .as_ref()
            .is_none_or(|path| crate::shelf_store::save_state(path, self))
    }

    pub fn has(&self, kind: WidgetKind) -> bool {
        self.widgets.contains(&kind)
    }

    pub fn add(&mut self, kind: WidgetKind) -> bool {
        if self.has(kind) {
            return false;
        }
        self.widgets.push(kind);
        if self.save() {
            true
        } else {
            self.widgets.pop();
            false
        }
    }

    pub fn remove(&mut self, kind: WidgetKind) -> bool {
        let Some(index) = self.widgets.iter().position(|widget| *widget == kind) else {
            return false;
        };
        self.widgets.remove(index);
        if self.save() {
            true
        } else {
            self.widgets.insert(index, kind);
            false
        }
    }

    /// Returns true when the widget is now on the Home shelf.
    pub fn toggle(&mut self, kind: WidgetKind) -> bool {
        if self.has(kind) {
            self.remove(kind);
            self.has(kind)
        } else {
            self.add(kind);
            self.has(kind)
        }
    }
}

fn parse_widgets(s: &str) -> WidgetStore {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else {
        return WidgetStore::default();
    };
    let Some(arr) = v.get("widgets").and_then(|w| w.as_array()) else {
        return WidgetStore::default();
    };
    let mut widgets = Vec::new();
    for kind in arr
        .iter()
        .filter_map(|value| value.as_str())
        .filter_map(WidgetKind::from_name)
    {
        if !widgets.contains(&kind) {
            widgets.push(kind);
        }
    }
    WidgetStore {
        widgets,
        path: None,
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
        let ser = serde_json::to_string(&s).unwrap();
        let back: WidgetStore = serde_json::from_str(&ser).unwrap();
        assert_eq!(back.widgets, s.widgets);
    }

    #[test]
    fn empty_layout_and_clock_survive_parsing() {
        assert!(parse_widgets(r#"{"widgets":[]}"#).widgets.is_empty());
        assert_eq!(
            parse_widgets(r#"{"widgets":["Clock","Clock"]}"#).widgets,
            vec![WidgetKind::Clock]
        );
    }

    #[test]
    fn old_json_drops_battery() {
        let s = parse_widgets(r#"{"widgets":["Timer","Media","Battery"]}"#);
        assert_eq!(s.widgets, vec![WidgetKind::Timer, WidgetKind::Media]);
    }
    #[test]
    fn explicit_layout_roundtrips_without_touching_user_config() {
        let dir =
            std::env::temp_dir().join(format!("naarchy-widget-{}", crate::shelf_store::new_id()));
        let path = dir.join("widgets.json");
        let mut store = WidgetStore::open(path.clone());
        store.remove(WidgetKind::Timer);
        store.remove(WidgetKind::Media);
        assert!(WidgetStore::open(path.clone()).widgets.is_empty());
        assert!(store.add(WidgetKind::Clock));
        assert_eq!(WidgetStore::open(path).widgets, vec![WidgetKind::Clock]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
