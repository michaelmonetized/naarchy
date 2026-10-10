//! `plugin.toml`: what a plugin package declares, and the checks that keep a
//! package inside its own directory before Naarchy ever executes it.

use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

/// The only protocol revision this host speaks. Plugins declare `api = 1`.
pub const API_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "plugin.toml";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024;
const MAX_ARGS: usize = 16;

/// Where a plugin's output appears. API 1 has one slot: live activities on
/// the island (the ears around the notch), the same place a running timer
/// or the current track shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Slot {
    #[default]
    LiveActivity,
}

impl Slot {
    pub fn as_str(self) -> &'static str {
        match self {
            Slot::LiveActivity => "live-activity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Restart {
    Never,
    #[default]
    OnFailure,
    Always,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub api: u32,
    /// Program to run, relative to the package directory. Never a shell line.
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub slot: Slot,
    #[serde(default)]
    pub restart: Restart,
}

/// A manifest that passed validation, with the program resolved on disk.
#[derive(Debug, Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub program: PathBuf,
}

/// Plugin names double as directory names and activity namespaces.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// `exec` must name a file below the package: relative, no `..`, no root.
fn relative_inside(exec: &str) -> Option<PathBuf> {
    let path = Path::new(exec);
    if exec.is_empty() || path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => clean.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

pub fn parse(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest =
        toml::from_str(text).map_err(|e| format!("invalid {MANIFEST_FILE}: {e}"))?;
    if !valid_name(&manifest.name) {
        return Err("name must be 1-64 characters of a-z, 0-9, '-' or '_'".into());
    }
    if manifest.api != API_VERSION {
        return Err(format!(
            "plugin speaks api {} but this Naarchy speaks api {API_VERSION}",
            manifest.api
        ));
    }
    if manifest.version.trim().is_empty() || manifest.version.len() > 32 {
        return Err("version must be 1-32 characters".into());
    }
    if relative_inside(&manifest.exec).is_none() {
        return Err("exec must be a relative path inside the plugin directory".into());
    }
    if manifest.args.len() > MAX_ARGS || manifest.args.iter().any(|a| a.len() > 256) {
        return Err(format!("at most {MAX_ARGS} args of 256 bytes each"));
    }
    Ok(manifest)
}

/// Group- or world-writable files could be swapped by another account
/// between install and launch, so the host refuses to execute them.
fn private_enough(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.uid() == current_uid() && meta.mode() & 0o022 == 0
}

pub(crate) fn current_uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { getuid() }
}

/// Validate a package directory: manifest, ownership, and the program file.
pub fn load(dir: &Path) -> Result<Package, String> {
    let dir_meta = std::fs::symlink_metadata(dir).map_err(|e| format!("cannot read: {e}"))?;
    if !dir_meta.is_dir() {
        return Err("not a directory".into());
    }
    if !private_enough(&dir_meta) {
        return Err("directory must be yours and not group/world-writable".into());
    }
    let manifest_path = dir.join(MANIFEST_FILE);
    let meta = std::fs::symlink_metadata(&manifest_path)
        .map_err(|_| format!("missing {MANIFEST_FILE}"))?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "{MANIFEST_FILE} must be a regular file under 16 KiB"
        ));
    }
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let manifest = parse(&text)?;
    let program = dir.join(relative_inside(&manifest.exec).expect("validated by parse"));
    let canonical_dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let canonical = program
        .canonicalize()
        .map_err(|_| format!("exec {} does not exist", manifest.exec))?;
    if !canonical.starts_with(&canonical_dir) {
        return Err("exec resolves outside the plugin directory".into());
    }
    let meta = std::fs::metadata(&canonical).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return Err(format!("exec {} is not an executable file", manifest.exec));
    }
    if !private_enough(&meta) {
        return Err("exec must be yours and not group/world-writable".into());
    }
    Ok(Package {
        dir: canonical_dir,
        manifest,
        program: canonical,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "t3-live"
version = "0.1.0"
api = 1
exec = "bin/t3-live"
"#;

    #[test]
    fn minimal_manifest_parses_with_defaults() {
        let m = parse(GOOD).unwrap();
        assert_eq!(m.name, "t3-live");
        assert_eq!(m.slot, Slot::LiveActivity);
        assert_eq!(m.restart, Restart::OnFailure);
        assert!(m.args.is_empty());
    }

    #[test]
    fn names_are_directory_safe() {
        assert!(valid_name("t3-live"));
        assert!(valid_name("a_1"));
        for bad in ["", "-x", "T3", "a/b", "..", "a b", &"x".repeat(65)] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn exec_cannot_escape_the_package() {
        for exec in ["/bin/sh", "../x", "bin/../../x", "", "."] {
            let text = GOOD.replace("bin/t3-live", exec);
            assert!(parse(&text).is_err(), "{exec:?}");
        }
        assert!(parse(&GOOD.replace("bin/t3-live", "./run")).is_ok());
    }

    #[test]
    fn wrong_api_and_unknown_keys_are_rejected() {
        assert!(parse(&GOOD.replace("api = 1", "api = 2")).is_err());
        assert!(parse(&format!("{GOOD}\nshell = \"rm -rf\"")).is_err());
        assert!(parse(&format!("{GOOD}\nslot = \"bar\"")).is_err());
    }

    #[test]
    fn load_checks_the_program_on_disk() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("naarchy-plugin-{}", crate::shelf_store::new_id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), GOOD).unwrap();
        assert!(load(&dir).unwrap_err().contains("does not exist"));
        let program = dir.join("bin/t3-live");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&dir).unwrap_err().contains("not an executable"));
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o766)).unwrap();
        assert!(load(&dir).unwrap_err().contains("group/world-writable"));
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let package = load(&dir).unwrap();
        assert!(package.program.ends_with("bin/t3-live"));
        // A symlink pointing out of the package is refused.
        std::fs::remove_file(&program).unwrap();
        std::os::unix::fs::symlink("/bin/sh", &program).unwrap();
        assert!(load(&dir).unwrap_err().contains("outside"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
