use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: String,
    pub kind: String, // "file" | "text" | "image"
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub text: String,
    /// For image kind without a file on disk
    #[serde(default)]
    pub data_ref: String,
    pub added_at: u64,
    #[serde(default)]
    pub pinned: bool,
}

impl ShelfItem {
    #[allow(dead_code)]
    pub fn uris(&self) -> Vec<String> {
        if matches!(self.kind.as_str(), "file" | "image") && !self.path.is_empty() {
            use gtk4::prelude::FileExt;
            vec![gtk4::gio::File::for_path(&self.path).uri().into()]
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
pub struct ShelfStore {
    items: Vec<ShelfItem>,
    path: PathBuf,
    blobs_dir: PathBuf,
}

impl ShelfStore {
    pub fn load() -> Self {
        Self::open(crate::util::data_dir())
    }

    /// Restore parked items. File entries reference originals and are never deleted.
    pub fn open(dir: PathBuf) -> Self {
        let path = dir.join("shelf.json");
        let blobs_dir = dir.join("blobs");
        let mut items: Vec<ShelfItem> = load_state::<Vec<ShelfItem>>(&path)
            .unwrap_or_default()
            .into_iter()
            .filter(|item| match item.kind.as_str() {
                "file" | "image" => std::path::Path::new(&item.path).exists(),
                "text" => true,
                _ => false,
            })
            .collect();
        items.sort_by_key(|item| !item.pinned);
        for item in &items {
            if item.kind == "image"
                && std::path::Path::new(&item.path).parent() == Some(blobs_dir.as_path())
            {
                make_private(std::path::Path::new(&item.path));
            }
        }
        Self {
            items,
            path,
            blobs_dir,
        }
    }

    pub fn items(&self) -> &[ShelfItem] {
        &self.items
    }

    /// Park valid, distinct files with one durable write for the entire batch.
    /// Invalid or duplicate paths are skipped; a write failure accepts nothing.
    pub fn add_files(&mut self, paths: &[String]) -> usize {
        let mut seen: std::collections::HashSet<String> = self
            .items
            .iter()
            .filter(|item| item.kind == "file")
            .map(|item| item.path.clone())
            .collect();
        let added_at = crate::util::now_unix();
        let pending = paths
            .iter()
            .filter_map(|path| {
                let path = std::fs::canonicalize(path)
                    .ok()?
                    .to_string_lossy()
                    .into_owned();
                if !seen.insert(path.clone()) {
                    return None;
                }
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                let mime = guess_mime(&path);
                Some(ShelfItem {
                    id: new_id(),
                    kind: "file".into(),
                    name,
                    path,
                    mime,
                    text: String::new(),
                    data_ref: String::new(),
                    added_at,
                    pinned: false,
                })
            })
            .collect();
        self.append_many(pending)
    }

    /// Park distinct text snippets or links without rewriting the shelf per item.
    pub fn add_texts(&mut self, texts: &[String]) -> usize {
        let mut seen: std::collections::HashSet<&str> = self
            .items
            .iter()
            .filter(|item| item.kind == "text")
            .map(|item| item.text.as_str())
            .collect();
        let added_at = crate::util::now_unix();
        let pending = texts
            .iter()
            .filter_map(|text| {
                let preview: String = text.trim().chars().take(120).collect();
                if preview.is_empty() || !seen.insert(text.as_str()) {
                    return None;
                }
                Some(ShelfItem {
                    id: new_id(),
                    kind: "text".into(),
                    name: format!("Text — {preview}"),
                    path: String::new(),
                    mime: "text/plain;charset=utf-8".into(),
                    text: text.clone(),
                    data_ref: String::new(),
                    added_at,
                    pinned: false,
                })
            })
            .collect();
        self.append_many(pending)
    }

    pub fn add_image(&mut self, png: Vec<u8>) -> bool {
        if png.is_empty() {
            return false;
        }
        let reference = format!("img-{}.png", crate::util::cache_key(&png));
        let dest = self.blobs_dir.join(&reference);
        let path = dest.to_string_lossy().into_owned();
        if self
            .items
            .iter()
            .any(|item| item.kind == "image" && item.path == path)
        {
            return false;
        }
        if let Err(err) = crate::util::atomic_write_private(&dest, &png) {
            log::warn!("could not save shelf image: {err}");
            return false;
        }
        let added = self.append(ShelfItem {
            id: new_id(),
            kind: "image".into(),
            name: "Pasted image.png".into(),
            path,
            mime: "image/png".into(),
            text: String::new(),
            data_ref: reference,
            added_at: crate::util::now_unix(),
            pinned: false,
        });
        if !added {
            let _ = std::fs::remove_file(dest);
        }
        added
    }

    fn append(&mut self, item: ShelfItem) -> bool {
        self.append_many(vec![item]) == 1
    }

    fn append_many(&mut self, pending: Vec<ShelfItem>) -> usize {
        if pending.is_empty() {
            return 0;
        }
        // Serialize borrowed items so large existing snippets are not cloned.
        let next: Vec<&ShelfItem> = self.items.iter().chain(pending.iter()).collect();
        if !save_state(&self.path, &next) {
            return 0;
        }
        let added = pending.len();
        self.items.extend(pending);
        added
    }

    pub fn remove(&mut self, id: &str) {
        if !self.items.iter().any(|item| item.id == id) {
            return;
        }
        self.retain(|item| item.id != id);
    }

    pub fn toggle_pin(&mut self, id: &str) {
        let Some(index) = self.items.iter().position(|item| item.id == id) else {
            return;
        };
        self.items[index].pinned = !self.items[index].pinned;
        if save_state(&self.path, &self.items) {
            self.items.sort_by_key(|item| !item.pinned);
        } else {
            self.items[index].pinned = !self.items[index].pinned;
        }
    }

    pub fn clear(&mut self) {
        if self.items.iter().any(|item| !item.pinned) {
            self.retain(|item| item.pinned);
        }
    }

    fn retain(&mut self, keep: impl Fn(&ShelfItem) -> bool) {
        let next: Vec<&ShelfItem> = self.items.iter().filter(|item| keep(item)).collect();
        if !save_state(&self.path, &next) {
            return;
        }
        for item in self.items.iter().filter(|item| !keep(item)) {
            let path = std::path::Path::new(&item.path);
            // Only images created by this store are owned; original files are references.
            if item.kind == "image"
                && path.parent() == Some(self.blobs_dir.as_path())
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("img-"))
                && !next.iter().any(|other| other.path == item.path)
            {
                let _ = std::fs::remove_file(path);
            }
        }
        self.items.retain(keep);
    }
}

pub(crate) fn make_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.is_file() && metadata.permissions().mode() & 0o077 != 0 {
            if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            {
                log::warn!("could not protect state file {}: {err}", path.display());
            }
        }
    }
}

pub(crate) fn load_state<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    make_private(path);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            log::warn!("could not read {}: {err}", path.display());
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(err) => {
            let backup = path.with_extension(format!("json.recovery-{}", new_id()));
            match crate::util::atomic_write_private(&backup, &bytes) {
                Ok(()) => log::warn!(
                    "invalid state in {}; preserved at {}: {err}",
                    path.display(),
                    backup.display()
                ),
                Err(backup_err) => log::warn!(
                    "could not back up invalid state {}: {backup_err}; parse error: {err}",
                    path.display()
                ),
            }
            None
        }
    }
}

pub(crate) fn save_state<T: Serialize>(path: &std::path::Path, value: &T) -> bool {
    let result = serde_json::to_vec(value)
        .map_err(std::io::Error::other)
        .and_then(|bytes| crate::util::atomic_write_private(path, &bytes));
    if let Err(err) = result {
        log::warn!("could not save {}: {err}", path.display());
        return false;
    }
    true
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        "{nanos:x}-{:x}-{:x}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

fn guess_mime(path: &str) -> String {
    gtk4::gio::content_type_guess(Some(std::path::Path::new(path)), None::<&[u8]>)
        .0
        .into()
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
    fn roundtrip_file_text_image_pin_clear() {
        let dir = tmp("naarchy-shelf");
        let file = dir.join("shot.png");
        std::fs::write(&file, b"png").unwrap();

        {
            let mut s = ShelfStore::open(dir.clone());
            assert_eq!(s.add_files(&[file.to_string_lossy().into_owned()]), 1);
            assert_eq!(s.add_files(&[file.to_string_lossy().into_owned()]), 0); // dedupe
            assert_eq!(s.add_texts(&["hello shelf".into()]), 1);
            assert!(s.add_image(b"\x89PNG".to_vec()));
            assert_eq!(s.items().len(), 3);
            let id = s.items()[0].id.clone();
            s.toggle_pin(&id);
            assert!(s.items()[0].pinned);
        }

        {
            let mut s = ShelfStore::open(dir.clone());
            assert_eq!(s.items().len(), 3);
            assert!(s.items()[0].pinned);
            s.clear();
            assert_eq!(s.items().len(), 1);
            assert!(s.items()[0].pinned);
        }
        cleanup(&dir);
    }

    #[test]
    fn missing_files_filtered_on_load() {
        let dir = tmp("naarchy-shelf-gone");
        let ghost = dir.join("ghost.txt");
        std::fs::write(&ghost, b"x").unwrap();
        {
            let mut s = ShelfStore::open(dir.clone());
            assert_eq!(s.add_files(&[ghost.to_string_lossy().into_owned()]), 1);
        }
        std::fs::remove_file(&ghost).unwrap();
        let s = ShelfStore::open(dir.clone());
        assert!(s.items().is_empty());
        cleanup(&dir);
    }
    #[test]
    fn clear_deletes_owned_images_but_preserves_original_files() {
        let dir = tmp("naarchy-shelf-owned");
        let original = dir.join("original.txt");
        std::fs::write(&original, "keep me").unwrap();
        let mut store = ShelfStore::open(dir.clone());
        assert_eq!(
            store.add_files(&[original.to_string_lossy().into_owned()]),
            1
        );
        assert!(store.add_image(b"image".to_vec()));
        let image = PathBuf::from(&store.items()[1].path);
        assert!(!store.add_image(b"image".to_vec()));
        store.clear();
        assert!(original.exists());
        assert!(!image.exists());
        assert!(ShelfStore::open(dir.clone()).items().is_empty());
        cleanup(&dir);
    }

    #[test]
    fn state_is_private_and_invalid_data_is_preserved_for_recovery() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("naarchy-shelf-recovery");
        let path = dir.join("shelf.json");
        std::fs::write(&path, b"broken JSON").unwrap();
        let mut store = ShelfStore::open(dir.clone());
        assert_eq!(store.add_texts(&["new item".into()]), 1);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let backup = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains("recovery-"))
            .unwrap();
        assert_eq!(std::fs::read(backup.path()).unwrap(), b"broken JSON");
        cleanup(&dir);
    }
    #[test]
    fn batch_files_skip_invalid_and_duplicate_paths_and_roundtrip_together() {
        let dir = tmp("naarchy-shelf-batch");
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let inputs = vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
            dir.join("./first.txt").to_string_lossy().into_owned(),
            dir.join("missing.txt").to_string_lossy().into_owned(),
        ];
        let mut store = ShelfStore::open(dir.clone());
        assert_eq!(store.add_files(&inputs), 2);
        assert_eq!(store.add_files(&inputs), 0);
        assert_eq!(ShelfStore::open(dir.clone()).items().len(), 2);
        assert!(first.exists() && second.exists());
        cleanup(&dir);
    }

    #[test]
    fn batch_failure_preserves_existing_items_and_accepts_none() {
        let dir = tmp("naarchy-shelf-batch-failure");
        let original = dir.join("original.txt");
        std::fs::write(&original, "keep").unwrap();
        let mut store = ShelfStore::open(dir.clone());
        assert_eq!(store.add_texts(&["existing".into()]), 1);
        std::fs::remove_file(dir.join("shelf.json")).unwrap();
        std::fs::create_dir(dir.join("shelf.json")).unwrap();
        assert_eq!(
            store.add_files(&[original.to_string_lossy().into_owned()]),
            0
        );
        assert_eq!(store.add_texts(&["new one".into(), "new two".into()]), 0);
        assert_eq!(store.items().len(), 1);
        assert_eq!(store.items()[0].text, "existing");
        assert!(original.exists());
        cleanup(&dir);
    }

    #[test]
    fn batch_text_deduplicates_existing_and_incoming_content() {
        let dir = tmp("naarchy-shelf-text-batch");
        let mut store = ShelfStore::open(dir.clone());
        assert_eq!(store.add_texts(&["existing".into()]), 1);
        assert_eq!(
            store.add_texts(&["existing".into(), "new".into(), "new".into(), "  ".into()]),
            1
        );
        assert_eq!(ShelfStore::open(dir.clone()).items().len(), 2);
        cleanup(&dir);
    }
}
