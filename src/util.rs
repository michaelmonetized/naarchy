use std::path::PathBuf;

/// Replace a private state file without exposing partial contents to readers.
pub fn atomic_write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "state path has no filename",
        )
    })?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = parent.join(tmp_name);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        // The rename has committed. A directory-sync failure must not make callers
        // roll back memory to a state that no longer matches the file on disk.
        if let Err(err) = std::fs::File::open(parent).and_then(|dir| dir.sync_all()) {
            log::warn!("could not sync state directory {}: {err}", parent.display());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Blob references are filenames, never paths outside the store.
pub fn valid_blob_ref(value: &str) -> bool {
    let mut parts = std::path::Path::new(value).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none()
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("naarchy")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("naarchy")
}

pub fn config_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("naarchy/config.toml")
}

pub fn runtime_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(format!("/run/user/{}", libc_getuid())))
}

fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

fn spawn_cmd(program: &str, args: &[&str]) -> bool {
    match std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Reap launchers after they exit; dropping Child alone leaks zombies.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(error) => {
            log::debug!("cannot launch {program}: {error}");
            false
        }
    }
}

/// Open the Spotify web app, or focus it if it is already open.
///
/// Uses Omarchy's focus-or-launch helper so we do not trip
/// `omarchy launch spotify` (that installs the native client when
/// `/usr/bin/spotify` is missing). Falls back to the desktop file.
///
/// Arguments: none.
///
/// Returns: nothing. Fire-and-forget.
pub fn launch_spotify() {
    if spawn_cmd(
        "omarchy-launch-or-focus-webapp",
        &["spotify", "https://open.spotify.com/"],
    ) {
        return;
    }
    let _ = spawn_cmd("gtk-launch", &["Spotify"]);
}

/// Open cliamp in a terminal, or focus an existing cliamp TUI.
///
/// Arguments: none.
///
/// Returns: nothing. Fire-and-forget.
pub fn launch_cliamp() {
    if spawn_cmd("omarchy-launch-or-focus-tui", &["cliamp"]) {
        return;
    }
    let _ = spawn_cmd("gtk-launch", &["cliamp"]);
}

/// Open paths/uris with the system opener without blocking the UI.
pub fn open_paths(paths: &[String]) {
    for path in paths {
        if !spawn_cmd("xdg-open", &[path]) {
            crate::app::notify_ui(
                "Couldn't open this item",
                "Install xdg-utils or choose a default application for this file type.",
            );
        }
    }
}

pub fn reveal_in_files(paths: &[String]) {
    if let Some(first) = paths.first() {
        let target = std::path::Path::new(first);
        if spawn_cmd("dolphin", &["--select", first]) {
            return;
        }
        let directory = if target.is_dir() {
            target
        } else {
            target.parent().unwrap_or(std::path::Path::new("/"))
        };
        open_paths(&[directory.to_string_lossy().into_owned()]);
    }
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Open the naarchy config in the user's editor via Omarchy's launcher,
/// mirroring every other Omarchy plugin (`omarchy-launch-config-editor` →
/// `omarchy-launch-editor` → `nvim` in `omarchy-launch-tui`).
pub fn open_config_in_editor() {
    let cfg = config_file();
    // ensure parent dir exists so the editor doesn't fail on missing path
    if let Some(parent) = cfg.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // create empty file if missing so editor has something to open
    if !cfg.exists() {
        let _ = std::fs::write(&cfg, "");
    }
    let path = cfg.to_string_lossy().to_string();
    // Preferred: omarchy-launch-config-editor (sends low-urgency notification then editor)
    // Fallback: omarchy-launch-editor, then direct `xdg-terminal-exec -e nvim`
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "if command -v omarchy-launch-config-editor >/dev/null 2>&1; then \
                 exec omarchy-launch-config-editor {} >/dev/null 2>&1 & \
             elif command -v omarchy-launch-editor >/dev/null 2>&1; then \
                 exec omarchy-launch-editor {} >/dev/null 2>&1 & \
             else \
                 exec xdg-terminal-exec -e nvim {} >/dev/null 2>&1 & \
             fi",
            shell_quote(&path),
            shell_quote(&path),
            shell_quote(&path)
        ))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn human_size(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn cache_key(data: &[u8]) -> String {
    // Stable FNV-1a 64 keeps cache names deterministic across Rust releases.
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut h = FNV_OFFSET;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // mix length to avoid prefix collisions
    h ^= data.len() as u64;
    h = h.wrapping_mul(FNV_PRIME);
    format!("{h:016x}")
}
