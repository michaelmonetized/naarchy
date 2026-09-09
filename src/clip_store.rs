use crate::services::{ClipEntry, ClipKind};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Default)]
pub struct ClipStore {
    /// Clipboard order is always newest first, independently of pinning.
    pub entries: Vec<ClipEntry>,
    path: PathBuf,
    blobs_dir: PathBuf,
}

impl ClipStore {
    pub fn load() -> Self {
        Self::open(crate::util::data_dir())
    }

    /// Open private clipboard history. Invalid blob paths are never followed.
    pub fn open(dir: PathBuf) -> Self {
        let path = dir.join("clipboard.json");
        let blobs_dir = dir.join("blobs");
        let mut entries: Vec<ClipEntry> = crate::shelf_store::load_state(&path).unwrap_or_default();
        entries.retain(|entry| {
            entry.kind != ClipKind::Image
                || (crate::util::valid_blob_ref(&entry.data_ref)
                    && blobs_dir.join(&entry.data_ref).is_file())
        });
        // Older releases reordered entries when pinning. Restore history order.
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.at));
        for entry in &entries {
            if entry.kind == ClipKind::Image {
                crate::shelf_store::make_private(&blobs_dir.join(&entry.data_ref));
            }
        }
        Self {
            entries,
            path,
            blobs_dir,
        }
    }

    pub fn add_raw(
        &mut self,
        mime: &str,
        data: &[u8],
        max_entries: usize,
        max_image: usize,
    ) -> bool {
        let is_image = mime.starts_with("image/");
        if data.is_empty() || max_entries == 0 || (is_image && data.len() > max_image) {
            return false;
        }
        let data_ref = if is_image {
            format!("clip-{}.bin", crate::util::cache_key(data))
        } else {
            String::new()
        };
        let text = if is_image {
            String::new()
        } else {
            String::from_utf8_lossy(data).into_owned()
        };
        let duplicate = self.entries.iter().position(|entry| {
            entry.mime == mime
                && if is_image {
                    entry.kind == ClipKind::Image && entry.data_ref == data_ref
                } else {
                    entry.kind == ClipKind::Text && entry.text == text
                }
        });
        if duplicate == Some(0) {
            return false;
        }
        let new_blob = (is_image
            && duplicate.is_none()
            && !self.entries.iter().any(|entry| entry.data_ref == data_ref))
        .then(|| self.blobs_dir.join(&data_ref));
        let mut next = self.entries.clone();
        let entry = if let Some(index) = duplicate {
            let mut entry = next.remove(index);
            entry.at = crate::util::now_unix();
            entry
        } else {
            if is_image {
                if let Err(err) =
                    crate::util::atomic_write_private(&self.blobs_dir.join(&data_ref), data)
                {
                    log::warn!("could not save clipboard image: {err}");
                    return false;
                }
            }
            ClipEntry {
                id: crate::shelf_store::new_id(),
                kind: if is_image {
                    ClipKind::Image
                } else {
                    ClipKind::Text
                },
                mime: mime.into(),
                preview: if is_image {
                    "Image".into()
                } else {
                    text.chars().take(80).collect()
                },
                text,
                data_ref,
                at: crate::util::now_unix(),
                pinned: false,
            }
        };
        next.insert(0, entry);
        // One forward pass retains the newest entries and all pins.
        let (mut count, mut images) = (0, 0);
        next.retain(|entry| {
            if entry.pinned {
                return true;
            }
            if count >= max_entries {
                return false;
            }
            if entry.kind == ClipKind::Image {
                if images >= 24.min(max_entries) {
                    return false;
                }
                images += 1;
            }
            count += 1;
            true
        });
        let saved = self.commit(next);
        if !saved {
            if let Some(path) = new_blob {
                let _ = std::fs::remove_file(path);
            }
        }
        saved
    }

    pub fn toggle_pin(&mut self, id: &str) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let mut next = self.entries.clone();
        next[index].pinned = !next[index].pinned;
        self.commit(next)
    }

    pub fn remove(&mut self, id: &str) {
        if !self.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        self.commit(
            self.entries
                .iter()
                .filter(|entry| entry.id != id)
                .cloned()
                .collect(),
        );
    }

    pub fn clear_unpinned(&mut self) {
        if self.entries.iter().all(|entry| entry.pinned) {
            return;
        }
        self.commit(
            self.entries
                .iter()
                .filter(|entry| entry.pinned)
                .cloned()
                .collect(),
        );
    }

    pub fn blob_path(&self, reference: &str) -> PathBuf {
        if crate::util::valid_blob_ref(reference) {
            self.blobs_dir.join(reference)
        } else {
            self.blobs_dir.join(".invalid-blob-reference")
        }
    }

    fn commit(&mut self, next: Vec<ClipEntry>) -> bool {
        if !crate::shelf_store::save_state(&self.path, &next) {
            return false;
        }
        let retained: HashSet<&str> = next
            .iter()
            .filter(|entry| entry.kind == ClipKind::Image)
            .map(|entry| entry.data_ref.as_str())
            .collect();
        // Delete only after the new index is durable, and only when no retained
        // entry references the blob (older histories may contain duplicates).
        for entry in &self.entries {
            if entry.kind == ClipKind::Image
                && !retained.contains(entry.data_ref.as_str())
                && crate::util::valid_blob_ref(&entry.data_ref)
            {
                let _ = std::fs::remove_file(self.blobs_dir.join(&entry.data_ref));
            }
        }
        self.entries = next;
        true
    }
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
    #[test]
    fn recopy_promotes_existing_item_and_pin_does_not_change_latest() {
        let dir = tmp("naarchy-clip-recopy");
        let mut store = ClipStore::open(dir.clone());
        store.add_raw("text/plain", b"first", 3, 64);
        let first = store.entries[0].id.clone();
        store.add_raw("text/plain", b"second", 3, 64);
        store.toggle_pin(&first);
        assert_eq!(store.entries[0].text, "second");
        assert!(store.add_raw("text/plain", b"first", 3, 64));
        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.entries[0].id, first);
        assert!(store.entries[0].pinned);
        cleanup(&dir);
    }

    #[test]
    fn removing_one_legacy_image_reference_preserves_the_other() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("naarchy-clip-shared");
        let mut store = ClipStore::open(dir.clone());
        assert!(store.add_raw("image/png", b"image", 3, 64));
        let path = store.blob_path(&store.entries[0].data_ref);
        let mut duplicate = store.entries[0].clone();
        duplicate.id = crate::shelf_store::new_id();
        duplicate.pinned = true;
        store.entries.push(duplicate);
        let removed = store.entries[0].id.clone();
        store.remove(&removed);
        assert_eq!(std::fs::read(&path).unwrap(), b"image");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let retained = store.entries[0].id.clone();
        store.remove(&retained);
        assert!(!path.exists());
        cleanup(&dir);
    }

    #[test]
    fn failed_index_write_does_not_accept_clipboard_change() {
        let dir = tmp("naarchy-clip-failure");
        let mut store = ClipStore::open(dir.clone());
        std::fs::create_dir(dir.join("clipboard.json")).unwrap();
        assert!(!store.add_raw("text/plain", b"unsaved", 3, 64));
        assert!(store.entries.is_empty());
        assert!(!store.add_raw("text/plain", b"disabled", 0, 64));
        assert!(store.blob_path("../outside").starts_with(dir.join("blobs")));
        cleanup(&dir);
    }
}
