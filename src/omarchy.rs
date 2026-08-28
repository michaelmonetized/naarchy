//! Consume the active omarchy theme so naarchy follows the desktop look:
//! accent / background / foreground colors plus the icon font family.
//!
//! Omarchy stores the current theme slug in
//! `$XDG_STATE_HOME/omarchy/current/theme.name` (or `~/.local/state/...`) and
//! the theme itself (with `colors.toml`) under
//! `~/.config/omarchy/themes/<slug>/`. If anything is missing we fall back to
//! naarchy's own appearance settings; no errors bubble up.

use std::path::{Path, PathBuf};

pub struct OmarchyPalette {
    pub mode: String,
    pub accent: String,
    pub background: String,
    pub dark_background: String,
    pub foreground: String,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn expand(path: impl AsRef<Path>) -> PathBuf {
    let p = path.as_ref();
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = home() {
            return h.join(rest);
        }
    }
    p.to_path_buf()
}

pub fn state_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join("omarchy"));
        }
    }
    home().map(|h| h.join(".local/state/omarchy"))
}

pub fn theme_dir(slug: &str) -> Option<PathBuf> {
    home().map(|h| h.join(".config/omarchy/themes").join(slug))
}

/// The active theme's slug (e.g. "hurleyus"), lowercased, if discoverable.
pub fn current_theme_slug() -> Option<String> {
    let p = state_root()?.join("current").join("theme.name");
    let raw = std::fs::read_to_string(p).ok()?;
    let slug = raw.trim().to_ascii_lowercase();
    if slug.is_empty() || slug == "unknown" {
        return None;
    }
    Some(slug)
}

/// Resolve the palette for the active theme. Returns None when omarchy can't
/// be reached so callers fall back to their own appearance settings.
pub fn palette() -> Option<OmarchyPalette> {
    let slug = current_theme_slug()?;
    let dir = theme_dir(&slug)?;
    parse_colors_file(&dir.join("colors.toml"))
}

/// Best-effort icon font family: the desktop font, discovered from the
/// terminal configs omarchy owns (foot/ghostty/kitty/alacritty). Returns None
/// when nothing can be read so the caller keeps its own default.
pub fn icon_font() -> Option<String> {
    for (path, pattern) in [
        (Path::new("~/.config/foot/foot.ini"), "font="),
        (Path::new("~/.config/ghostty/config"), "font-family"),
        (Path::new("~/.config/kitty/kitty.conf"), "font_family"),
        (Path::new("~/.config/alacritty/alacritty.toml"), "font"),
    ] {
        if let Some(f) = grep_first(expand(path), pattern)
            .map(|v| v.trim().to_string())
            .map(|v| v.split(':').next().unwrap_or(&v).to_string())
            .map(|v| trim_quotes(&v))
            .filter(|f| !f.is_empty())
        {
            return Some(f);
        }
    }
    None
}

fn parse_colors_file(path: &Path) -> Option<OmarchyPalette> {
    let body = std::fs::read_to_string(path).ok()?;
    let val: toml::Value = toml::from_str(&body).ok()?;
    let get = |k: &str| val.get(k).and_then(toml::Value::as_str).map(trim_quotes);
    let accent = get("accent").or_else(|| get("blue"))?;
    let background = get("background").or_else(|| get("bg"))?;
    let foreground = get("foreground").or_else(|| get("fg"))?;
    Some(OmarchyPalette {
        mode: get("mode").unwrap_or_else(|| "dark".into()),
        accent,
        background: background.clone(),
        dark_background: get("dark_background").unwrap_or(background),
        foreground,
    })
}

fn grep_first(path: impl AsRef<Path>, key: &str) -> Option<String> {
    let body = std::fs::read_to_string(path).ok()?;
    let line = body.lines().find(|l| l.trim_start().starts_with(key))?;
    let (_, v) = line.split_once('=')?;
    Some(v.trim().to_string())
}

fn trim_quotes(s: &str) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}
