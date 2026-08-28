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
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    format!("{:016x}", h.finish())
}
