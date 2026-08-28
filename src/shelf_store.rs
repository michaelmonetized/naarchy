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

#[allow(dead_code)]
impl ShelfItem {
    #[allow(dead_code)]
    pub fn uris(&self) -> Vec<String> {
        match self.kind.as_str() {
            "file" => vec![gio_uri(&self.path)],
            _ => vec![],
        }
    }
}

#[allow(dead_code)]
fn gio_uri(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("file://") {
        return path.to_string();
    }
    format!("file://{}", urlencode_path(std::path::Path::new(path)))
}

#[allow(dead_code)]
fn urlencode_path(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Default)]
pub struct ShelfStore {
    items: Vec<ShelfItem>,
    path: PathBuf,
    blobs_dir: PathBuf,
}

impl ShelfStore {
    /// Load the shelf from `$XDG_DATA_HOME/naarchy`.
    ///
    /// Missing or unreadable `shelf.json` yields an empty shelf. File items
    /// whose path no longer exists are dropped.
    ///
    /// Returns: a store rooted at the user data dir.
    pub fn load() -> Self {
        Self::open(crate::util::data_dir())
    }

    /// Load (or create) a shelf rooted at `dir`.
    ///
    /// Arguments:
    /// - `dir`: directory holding `shelf.json` and `blobs/`
    ///
    /// Returns: the store, creating the directory tree as needed.
    pub fn open(dir: PathBuf) -> Self {
        let blobs = dir.join("blobs");
        let _ = std::fs::create_dir_all(&blobs);
        let path = dir.join("shelf.json");
        let items = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<ShelfItem>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.kind != "file" || std::path::Path::new(&i.path).exists())
            .collect();
        Self {
            items,
            path,
            blobs_dir: blobs,
        }
    }

    pub fn items(&self) -> &[ShelfItem] {
        &self.items
    }

    pub fn add_file(&mut self, path: &str) -> bool {
        if self
            .items
            .iter()
            .any(|i| i.path == path && i.kind == "file")
        {
            return false;
        }
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        self.items.push(ShelfItem {
            id: new_id(),
            kind: "file".into(),
            name,
            path: path.into(),
            mime: guess_mime(path),
            text: String::new(),
            data_ref: String::new(),
            added_at: now(),
            pinned: false,
        });
        self.persist();
        true
    }

    pub fn add_text(&mut self, text: &str) -> bool {
        let preview: String = text.trim().chars().take(120).collect();
        if preview.is_empty() {
            return false;
        }
        if let Some(last) = self.items.iter().rev().find(|i| i.kind == "text") {
            if last.text == text {
                return false;
            }
        }
        self.items.push(ShelfItem {
            id: new_id(),
            kind: "text".into(),
            name: format!("Text — {preview}"),
            path: String::new(),
            mime: "text/plain;charset=utf-8".into(),
            text: text.to_string(),
            data_ref: String::new(),
            added_at: now(),
            pinned: false,
        });
        self.persist();
        true
    }

    pub fn add_image(&mut self, png: Vec<u8>) -> bool {
        let r = crate::util::cache_key(&png);
        let dest = self.blobs_dir.join(format!("img-{r}.png"));
        if std::fs::write(&dest, &png).is_err() {
            return false;
        }
        self.items.push(ShelfItem {
            id: new_id(),
            kind: "image".into(),
            name: format!("Image {r}.png"),
            path: dest.to_string_lossy().into_owned(),
            mime: "image/png".into(),
            text: String::new(),
            data_ref: String::new(),
            added_at: now(),
            pinned: false,
        });
        self.persist();
        true
    }

    pub fn remove(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
        self.persist();
    }

    pub fn toggle_pin(&mut self, id: &str) {
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.pinned = !i.pinned;
        }
        // keep pinned first
        self.items.sort_by_key(|i| !i.pinned);
        self.persist();
    }

    pub fn clear(&mut self) {
        self.items.retain(|i| i.pinned);
        self.persist();
    }

    fn persist(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(&self.items) {
            let tmp = self.path.with_extension("tmp");
            if std::fs::write(&tmp, &json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.path);
            }
        }
    }
}

pub fn new_id() -> String {
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (
        now(),
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed),
    )
        .hash(&mut h);
    format!("{:016x}", h.finish())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn guess_mime(p: &str) -> String {
    let ext = std::path::Path::new(p)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "mp4" | "mkv" | "webm" => "video/mp4",
        "zip" | "tar" | "gz" | "xz" | "zst" => "application/zip",
        "txt" | "md" | "log" | "rs" | "py" | "js" | "ts" | "toml" | "json" | "sh" => "text/plain",
        _ => "application/octet-stream",
    }
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
            assert!(s.add_file(file.to_str().unwrap()));
            assert!(!s.add_file(file.to_str().unwrap())); // dedupe
            assert!(s.add_text("hello shelf"));
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
            s.add_file(ghost.to_str().unwrap());
        }
        std::fs::remove_file(&ghost).unwrap();
        let s = ShelfStore::open(dir.clone());
        assert!(s.items().is_empty());
        cleanup(&dir);
    }
}
