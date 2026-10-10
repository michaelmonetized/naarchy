//! Plugin packages: small programs in any language that put live activities
//! on the island.
//!
//! A package is a directory `~/.config/naarchy/plugins/<name>/` holding a
//! `plugin.toml` manifest and an executable. Naarchy starts each enabled
//! package as a child process, says hello on stdin, and reads JSON lines from
//! stdout (see [`protocol`]). Plugins never run through a shell, never get
//! Naarchy's IPC socket, and can only show bounded text on the island.
//! Installing a package is the trust decision: it runs as your user.

pub mod cli;
pub mod host;
pub mod manifest;
pub mod protocol;

use protocol::Activity;
use std::path::{Path, PathBuf};

/// A package directory containing this file is skipped at startup.
pub const DISABLED_MARKER: &str = ".disabled";

pub fn plugins_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("naarchy/plugins")
}

/// One directory entry under the plugins directory.
pub struct Found {
    pub dir_name: String,
    pub enabled: bool,
    pub package: Result<manifest::Package, String>,
}

/// Every package directory, sorted by name. Hidden entries are ignored.
pub fn discover(root: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let dir_name = entry.file_name().to_str()?.to_string();
            if dir_name.starts_with('.') {
                return None;
            }
            let dir = entry.path();
            let package = manifest::load(&dir).and_then(|p| {
                if p.manifest.name == dir_name {
                    Ok(p)
                } else {
                    Err(format!(
                        "directory is {dir_name} but the manifest names {}",
                        p.manifest.name
                    ))
                }
            });
            Some(Found {
                enabled: !dir.join(DISABLED_MARKER).exists(),
                dir_name,
                package,
            })
        })
        .collect();
    found.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    found
}

/// Packages the daemon should start; problems are logged, never fatal.
pub fn runnable(root: &Path) -> Vec<manifest::Package> {
    discover(root)
        .into_iter()
        .filter_map(|f| match f.package {
            Ok(p) if f.enabled => Some(p),
            Ok(_) => None,
            Err(error) => {
                log::warn!("plugin {} skipped: {error}", f.dir_name);
                None
            }
        })
        .collect()
}

/// Live activities from every plugin. Main-thread state behind `Shared`.
#[derive(Debug, Default)]
pub struct Board {
    items: Vec<(String, Activity)>,
}

/// What the island shows for plugins right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headline {
    pub icon: String,
    pub text: String,
    pub priority: u8,
    /// Other activities not shown.
    pub more: usize,
}

impl Board {
    pub fn apply(&mut self, update: host::Update) {
        match update {
            host::Update::Upsert { plugin, activity } => {
                if let Some(slot) = self
                    .items
                    .iter_mut()
                    .find(|(p, a)| *p == plugin && a.id == activity.id)
                {
                    slot.1 = activity;
                } else if self.items.iter().filter(|(p, _)| *p == plugin).count()
                    < protocol::MAX_ACTIVITIES_PER_PLUGIN
                {
                    self.items.push((plugin, activity));
                }
            }
            host::Update::Clear { plugin, id: None } => self.items.retain(|(p, _)| *p != plugin),
            host::Update::Clear {
                plugin,
                id: Some(id),
            } => self.items.retain(|(p, a)| !(*p == plugin && a.id == id)),
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Highest priority wins; ties go to the longest-running activity, then
    /// a stable plugin/id order so the island does not flicker between them.
    pub fn headline(&self, now: u64) -> Option<Headline> {
        let best = self.items.iter().max_by(|(pa, a), (pb, b)| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| {
                    b.started_at
                        .unwrap_or(u64::MAX)
                        .cmp(&a.started_at.unwrap_or(u64::MAX))
                })
                .then_with(|| pb.cmp(pa))
                .then_with(|| b.id.cmp(&a.id))
        })?;
        let a = &best.1;
        let mut parts = vec![a.title.clone()];
        if !a.detail.is_empty() {
            parts.push(a.detail.clone());
        }
        if let Some(start) = a.started_at {
            parts.push(protocol::elapsed_label(start, now));
        }
        Some(Headline {
            icon: if a.icon.is_empty() {
                "\u{f12e}".into() // nf-fa-puzzle_piece
            } else {
                a.icon.clone()
            },
            text: parts.join(" · "),
            priority: a.priority,
            more: self.items.len() - 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::host::Update;
    use super::*;

    fn act(id: &str, priority: u8, started_at: Option<u64>) -> Activity {
        Activity {
            id: id.into(),
            icon: String::new(),
            title: format!("t-{id}"),
            detail: "running".into(),
            started_at,
            priority,
        }
    }

    fn up(plugin: &str, a: Activity) -> Update {
        Update::Upsert {
            plugin: plugin.into(),
            activity: a,
        }
    }

    #[test]
    fn headline_prefers_priority_then_oldest() {
        let mut board = Board::default();
        assert_eq!(board.headline(0), None);
        board.apply(up("p", act("new", 40, Some(900))));
        board.apply(up("p", act("old", 40, Some(100))));
        let h = board.headline(1000).unwrap();
        assert_eq!(h.text, "t-old · running · 15m");
        assert_eq!(h.more, 1);
        board.apply(up("q", act("urgent", 60, None)));
        assert_eq!(board.headline(1000).unwrap().text, "t-urgent · running");
    }

    #[test]
    fn upsert_replaces_and_caps_per_plugin() {
        let mut board = Board::default();
        for i in 0..20 {
            board.apply(up("p", act(&i.to_string(), 40, None)));
        }
        board.apply(up("p", act("0", 40, None)));
        assert_eq!(board.items.len(), protocol::MAX_ACTIVITIES_PER_PLUGIN);
        board.apply(up("other", act("x", 40, None)));
        assert_eq!(board.items.len(), protocol::MAX_ACTIVITIES_PER_PLUGIN + 1);
    }

    #[test]
    fn clears_are_scoped_to_their_plugin() {
        let mut board = Board::default();
        board.apply(up("p", act("a", 40, None)));
        board.apply(up("q", act("a", 40, None)));
        board.apply(Update::Clear {
            plugin: "p".into(),
            id: Some("a".into()),
        });
        assert_eq!(board.items.len(), 1);
        board.apply(Update::Clear {
            plugin: "q".into(),
            id: None,
        });
        assert!(board.is_empty());
    }

    #[test]
    fn discovery_reports_bad_packages_and_respects_disable() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("naarchy-disc-{}", crate::shelf_store::new_id()));
        let good = root.join("good");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        for d in [&root, &good, &root.join("broken")] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        std::fs::write(
            good.join("plugin.toml"),
            "name = \"good\"\nversion = \"1\"\napi = 1\nexec = \"run\"\n",
        )
        .unwrap();
        std::fs::write(good.join("run"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(good.join("run"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = discover(&root);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].dir_name, "broken");
        assert!(found[0].package.is_err());
        assert!(found[1].package.is_ok());
        assert_eq!(runnable(&root).len(), 1);
        std::fs::write(good.join(DISABLED_MARKER), "").unwrap();
        assert!(runnable(&root).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
