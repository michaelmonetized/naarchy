use std::path::PathBuf;

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
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
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
    for p in paths {
        let _ = std::process::Command::new("xdg-open")
            .arg(p)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

pub fn reveal_in_files(paths: &[String]) {
    // Reveal the first existing parent directory selection in the file manager.
    if let Some(first) = paths.first() {
        let target = std::path::Path::new(first);
        let dir = if target.is_dir() {
            target.to_path_buf()
        } else {
            target
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"))
        };
        // Try `xdg-open dir` — most file managers handle it; dolphin/konquerer variants accept --select.
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "command -v dolphin >/dev/null && exec dolphin --select {} >/dev/null 2>&1 || exec xdg-open {}",
                shell_quote(&dir.to_string_lossy()),
                shell_quote(&dir.to_string_lossy())
            ))
            .spawn();
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
    // Stable FNV-1a 64 — DefaultHasher is SipHash with random key per process
    // so art/shelf/clip cache filenames would churn every restart.
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
