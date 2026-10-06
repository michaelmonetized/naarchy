//! Snapshot startup evidence before Naarchy creates its default configuration.
//! There was no first-run marker before this feature; existing config/data counts
//! as a prior run. CLI queries do not consume the welcome.

use std::io;
use std::path::{Path, PathBuf};

const MARKER: &str = "first-run.json";

pub struct FirstRun {
    marker: PathBuf,
    date: (i32, u32, u32),
    eligible: bool,
}

impl FirstRun {
    pub fn inspect(config: &Path, data: &Path, date: (i32, u32, u32)) -> io::Result<Self> {
        let marker = data.join(MARKER);
        // An unreadable/corrupt marker still means this was already attempted.
        let marked = marker.try_exists()?;
        let existing_data = match std::fs::read_dir(data) {
            Ok(mut entries) => entries.next().transpose()?.is_some(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        Ok(Self {
            marker,
            date,
            eligible: !marked && !config.try_exists()? && !existing_data,
        })
    }

    /// Commit before the first frame. Dismissal, shutdown and a crash must never
    /// replay the takeover. A persistence failure suppresses the animation.
    /// The IPC server's exclusive lock serializes daemon startups.
    pub fn consume(self) -> io::Result<bool> {
        if self.marker.try_exists()? {
            return Ok(false);
        }
        let welcome = self.eligible && self.date.1 == 10 && self.date.2 == 10;
        let record = serde_json::json!({
            "version": 1,
            "first_start_local": format!("{:04}-{:02}-{:02}", self.date.0, self.date.1, self.date.2),
            "welcome_claimed": welcome,
        });
        crate::util::atomic_write_private(&self.marker, record.to_string().as_bytes())?;
        Ok(welcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "naarchy-first-run-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn inspect(&self, date: (i32, u32, u32)) -> FirstRun {
            FirstRun::inspect(&self.0.join("config.toml"), &self.0.join("data"), date).unwrap()
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn first_start_on_oct10_is_once_even_if_interrupted_before_drawing() {
        let s = Sandbox::new();
        assert!(s.inspect((2026, 10, 10)).consume().unwrap());
        // No completion callback: simulate dismissal, a crash, or immediate quit.
        assert!(!s.inspect((2026, 10, 10)).consume().unwrap());
        assert!(!s.inspect((2027, 10, 10)).consume().unwrap());
        use std::os::unix::fs::PermissionsExt;
        let marker = s.0.join("data").join(MARKER);
        assert_eq!(
            std::fs::metadata(marker).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn off_date_first_start_permanently_skips() {
        for date in [(2026, 10, 6), (2026, 10, 9), (2026, 10, 11), (2026, 1, 10)] {
            let s = Sandbox::new();
            assert!(!s.inspect(date).consume().unwrap());
            assert!(!s.inspect((2026, 10, 10)).consume().unwrap());
        }
    }

    #[test]
    fn startup_snapshot_survives_default_config_creation() {
        let s = Sandbox::new();
        let first = s.inspect((2026, 10, 10));
        crate::config::Config::save_default_if_missing(&s.0.join("config.toml"));
        assert!(first.consume().unwrap());
    }

    #[test]
    fn legacy_evidence_skips_upgrades() {
        for config in [true, false] {
            let s = Sandbox::new();
            if config {
                std::fs::write(s.0.join("config.toml"), "").unwrap();
            } else {
                std::fs::create_dir(s.0.join("data")).unwrap();
                std::fs::write(s.0.join("data/shelf.json"), "[]").unwrap();
            }
            assert!(!s.inspect((2026, 10, 10)).consume().unwrap());
        }
    }

    #[test]
    fn corrupt_marker_is_preserved_and_never_replayed() {
        let s = Sandbox::new();
        std::fs::create_dir(s.0.join("data")).unwrap();
        let marker = s.0.join("data").join(MARKER);
        std::fs::write(&marker, "interrupted old state").unwrap();
        assert!(!s.inspect((2026, 10, 10)).consume().unwrap());
        assert_eq!(
            std::fs::read_to_string(marker).unwrap(),
            "interrupted old state"
        );
    }

    #[test]
    fn persistence_failure_cannot_authorize_a_welcome() {
        let s = Sandbox::new();
        let first = s.inspect((2026, 10, 10));
        std::fs::write(s.0.join("data"), "blocked directory").unwrap();
        assert!(first.consume().is_err());
    }
}
