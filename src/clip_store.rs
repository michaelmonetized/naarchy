use crate::services::ClipEntry;
use std::path::PathBuf;

#[derive(Default)]
pub struct ClipStore {
    pub entries: Vec<ClipEntry>,
    path: PathBuf,
    blobs_dir: PathBuf,
}

impl ClipStore {
    /// Load clipboard history from `$XDG_DATA_HOME/naarchy`.
    ///
    /// Missing `clipboard.json` yields an empty ring.
    ///
    /// Returns: a store rooted at the user data dir.
    pub fn load() -> Self {
        Self::open(crate::util::data_dir())
    }

    /// Load (or create) a clipboard ring rooted at `dir`.
    ///
    /// Arguments:
    /// - `dir`: directory holding `clipboard.json` and `blobs/`
    ///
    /// Returns: the store, creating the directory tree as needed.
    pub fn open(dir: PathBuf) -> Self {
        let blobs = dir.join("blobs");
        let _ = std::fs::create_dir_all(&blobs);
        let path = dir.join("clipboard.json");
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<ClipEntry>>(&b).ok())
            .unwrap_or_default();
        Self {
            entries,
            path,
            blobs_dir: blobs,
        }
    }

    pub fn add_raw(
        &mut self,
        mime: &str,
        data: &[u8],
        max_entries: usize,
        max_image: usize,
    ) -> bool {
        use crate::services::ClipKind;
        let is_image = mime.starts_with("image/");
        if is_image && data.len() > max_image {
            return false;
        }
        // dedupe with newest
        let key = crate::util::cache_key(data);
        if let Some(first) = self.entries.first() {
            let first_key = if first.kind == ClipKind::Image {
                std::fs::read(self.blobs_dir.join(&first.data_ref))
                    .map(|b| crate::util::cache_key(&b))
            } else {
                Ok(crate::util::cache_key(first.text.as_bytes()))
            };
            if first_key.map(|k| k == key).unwrap_or(false) {
                return false;
            }
        }

        let (kind, preview, text, data_ref) = if is_image {
            (
                ClipKind::Image,
                "Image".to_string(),
                String::new(),
                format!("clip-{key}.bin"),
            )
        } else {
            let text = String::from_utf8_lossy(data).into_owned();
            let preview: String = text.chars().take(80).collect();
            (ClipKind::Text, preview, text, String::new())
        };

        if kind == ClipKind::Image {
            let _ = std::fs::write(self.blobs_dir.join(&data_ref), data);
        }

        self.entries.insert(
            0,
            ClipEntry {
                id: crate::shelf_store::new_id(),
                kind,
                mime: mime.to_string(),
                preview,
                text,
                data_ref,
                at: now(),
                pinned: false,
            },
        );

        let blobs = self.blobs_dir.clone();
        let drop_img = |e: &crate::services::ClipEntry| {
            if e.kind == crate::services::ClipKind::Image && !e.data_ref.is_empty() {
                let _ = std::fs::remove_file(blobs.join(&e.data_ref));
            }
        };

        let image_cap = 24.min(max_entries);
        let image_count = self
            .entries
            .iter()
            .filter(|e| e.kind == crate::services::ClipKind::Image && !e.pinned)
            .count();
        if image_count > image_cap {
            let overflow = image_count - image_cap;
            let mut removed = 0;
            self.entries.reverse();
            self.entries.retain(|e| {
                if e.kind == crate::services::ClipKind::Image && !e.pinned && removed < overflow {
                    drop_img(e);
                    removed += 1;
                    false
                } else {
                    true
                }
            });
            self.entries.reverse();
        }

        let unpinned_count = self.entries.iter().filter(|e| !e.pinned).count();
        let overflow = unpinned_count.saturating_sub(max_entries);
        if overflow > 0 {
            let mut removed = 0;
            self.entries.reverse();
            self.entries.retain(|e| {
                if !e.pinned && removed < overflow {
                    drop_img(e);
                    removed += 1;
                    false
                } else {
                    true
                }
            });
            self.entries.reverse();
        }
        self.persist();
        true
    }

    pub fn toggle_pin(&mut self, id: &str) -> bool {
        let mut changed = false;
        for e in &mut self.entries {
            if e.id == id {
                e.pinned = !e.pinned;
                changed = true;
            }
        }
        if changed {
            self.entries.sort_by_key(|e| !e.pinned);
            self.persist();
        }
        changed
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(e) = self.entries.iter().find(|e| e.id == id) {
            self.drop_blob(e);
        }
        self.entries.retain(|e| e.id != id);
        self.persist();
    }

    pub fn clear_unpinned(&mut self) {
        for e in self.entries.iter().filter(|e| !e.pinned) {
            self.drop_blob(e);
        }
        self.entries.retain(|e| e.pinned);
        self.persist();
    }

    fn drop_blob(&self, e: &crate::services::ClipEntry) {
        if e.kind == crate::services::ClipKind::Image && !e.data_ref.is_empty() {
            let _ = std::fs::remove_file(self.blobs_dir.join(&e.data_ref));
        }
    }

    pub fn blob_path(&self, r: &str) -> PathBuf {
        self.blobs_dir.join(r)
    }

    fn persist(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(&self.entries) {
            let tmp = self.path.with_extension("tmp");
            if std::fs::write(&tmp, &json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp(prefix: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("{prefix}-{}-{}-{}", std::process::id(), n, nanos));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ring_dedupe_cap_pin() {
        let dir = tmp("naarchy-clip");
        let mut s = ClipStore::open(dir.clone());
        assert!(s.add_raw("text/plain", b"alpha", 3, 64));
        assert!(!s.add_raw("text/plain", b"alpha", 3, 64)); // dedupe newest
        assert!(s.add_raw("text/plain", b"bravo", 3, 64));
        assert!(s.add_raw("text/plain", b"charlie", 3, 64));
        assert_eq!(s.entries.len(), 3);
        let pin_id = s.entries.last().unwrap().id.clone(); // oldest
        s.toggle_pin(&pin_id);
        assert!(s.add_raw("text/plain", b"delta", 3, 64));
        assert!(s.add_raw("text/plain", b"echo", 3, 64));
        assert_eq!(s.entries.len(), 4); // 3 unpinned cap + 1 pin
        assert!(s.entries.iter().any(|e| e.id == pin_id && e.pinned));
        assert!(!s.add_raw("image/png", &[0u8; 128], 3, 64)); // over image cap
        cleanup(&dir);
    }
}
