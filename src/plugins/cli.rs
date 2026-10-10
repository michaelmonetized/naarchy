//! `naarchy plugin …`: manage packages without the daemon.

use super::{discover, manifest, plugins_dir, DISABLED_MARKER};
use std::path::Path;

const USAGE: &str = "usage: naarchy plugin <list|install DIR [--force]|remove NAME|enable NAME|disable NAME|run NAME|path>";
/// Packages are small scripts or binaries plus assets, not datasets.
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

pub fn run(args: &[String]) {
    let root = plugins_dir();
    let result = match (args.first().map(String::as_str), args.get(1)) {
        (None | Some("list"), _) => {
            list(&root);
            Ok(())
        }
        (Some("path"), _) => {
            println!("{}", root.display());
            Ok(())
        }
        (Some("install"), Some(source)) => {
            let force = args.iter().skip(2).any(|a| a == "--force");
            install(Path::new(source), &root, force).map(|name| {
                println!("Installed {name} in {}", root.join(&name).display());
                println!("Restart Naarchy to start it: systemctl --user restart naarchy (or naarchy quit && naarchy run)");
            })
        }
        (Some("remove" | "uninstall"), Some(name)) => {
            remove(name, &root).map(|()| println!("Removed {name}. Restart Naarchy to stop it."))
        }
        (Some("enable"), Some(name)) => set_enabled(name, &root, true)
            .map(|()| println!("Enabled {name}. Restart Naarchy to start it.")),
        (Some("disable"), Some(name)) => set_enabled(name, &root, false)
            .map(|()| println!("Disabled {name}. Restart Naarchy to stop it.")),
        (Some("run"), Some(name)) => run_foreground(name, &root),
        _ => Err(USAGE.to_string()),
    };
    if let Err(error) = result {
        eprintln!("naarchy: {error}");
        std::process::exit(2);
    }
}

fn list(root: &Path) {
    let found = discover(root);
    if found.is_empty() {
        println!("No plugins in {}", root.display());
        return;
    }
    for f in found {
        match f.package {
            Ok(p) => println!(
                "{:<20} {:<10} {:<9} {:<14} {}",
                p.manifest.name,
                p.manifest.version,
                if f.enabled { "enabled" } else { "disabled" },
                p.manifest.slot.as_str(),
                p.manifest.description
            ),
            Err(error) => println!(
                "{:<20} {:<10} {:<9} {:<14} {error}",
                f.dir_name, "?", "invalid", "-"
            ),
        }
    }
}

fn installed_dir(name: &str, root: &Path) -> Result<std::path::PathBuf, String> {
    if !manifest::valid_name(name) {
        return Err(format!("{name:?} is not a plugin name"));
    }
    let dir = root.join(name);
    let meta = std::fs::symlink_metadata(&dir).map_err(|_| format!("{name} is not installed"))?;
    if !meta.is_dir() || !dir.join(manifest::MANIFEST_FILE).exists() {
        return Err(format!("{} is not a plugin package", dir.display()));
    }
    Ok(dir)
}

/// Copy a local package directory into the plugins directory. Symlinks and
/// special files are refused, so nothing outside the package comes along.
pub fn install(source: &Path, root: &Path, force: bool) -> Result<String, String> {
    let text = std::fs::read_to_string(source.join(manifest::MANIFEST_FILE))
        .map_err(|e| format!("{}: {e}", source.join(manifest::MANIFEST_FILE).display()))?;
    let name = manifest::parse(&text)?.name;
    let mut total = 0u64;
    check_tree(source, &mut total)?;
    let dest = root.join(&name);
    if dest.exists() {
        if !force {
            return Err(format!(
                "{name} is already installed; pass --force to replace it"
            ));
        }
        installed_dir(&name, root)?;
    }
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|e| e.to_string())?;
    }
    let staging = root.join(format!(".{name}.installing.{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let copied = copy_tree(source, &staging).and_then(|()| {
        manifest::load(&staging)?;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&staging, &dest).map_err(|e| e.to_string())
    });
    if copied.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    copied.map(|()| name)
}

fn check_tree(dir: &Path, total: &mut u64) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?; // does not follow symlinks
        let path = entry.path();
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{} is a symlink; packages must not contain links",
                path.display()
            ));
        }
        if meta.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            check_tree(&path, total)?;
        } else if meta.is_file() {
            *total += meta.len();
            if *total > MAX_PACKAGE_BYTES {
                return Err("package is larger than 64 MiB".into());
            }
        } else {
            return Err(format!("{} is not a regular file", path.display()));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(dest)
        .map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        if meta.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            copy_tree(&entry.path(), &target)?;
        } else if meta.is_file() {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            // Keep the execute bits, drop group/world write.
            let mode = if meta.permissions().mode() & 0o111 != 0 {
                0o700
            } else {
                0o600
            };
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn remove(name: &str, root: &Path) -> Result<(), String> {
    let dir = installed_dir(name, root)?;
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    // Private plugin data (tokens, caches) goes too.
    let _ = std::fs::remove_dir_all(super::host::data_root().join(name));
    Ok(())
}

fn set_enabled(name: &str, root: &Path, enabled: bool) -> Result<(), String> {
    let marker = installed_dir(name, root)?.join(DISABLED_MARKER);
    if enabled {
        match std::fs::remove_file(&marker) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    } else {
        std::fs::write(&marker, b"").map_err(|e| e.to_string())
    }
}

/// Run one plugin in the foreground and print what the island would show.
/// For plugin authors; Ctrl-C to stop.
fn run_foreground(name: &str, root: &Path) -> Result<(), String> {
    let package = manifest::load(&installed_dir(name, root)?)?;
    let mut package = package;
    package.manifest.restart = manifest::Restart::Never;
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let sink: super::host::Sink = std::sync::Arc::new(move |u| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(u);
        }
    });
    let _host = super::host::Host::start(vec![package], super::host::data_root(), sink);
    let mut board = super::Board::default();
    while let Ok(update) = rx.recv() {
        let done = matches!(update, super::host::Update::Clear { id: None, .. });
        println!("{update:?}");
        board.apply(update);
        match board.headline(crate::util::now_unix()) {
            Some(h) => println!(
                "  island: {} {} (+{} more, priority {})",
                h.icon, h.text, h.more, h.priority
            ),
            None => println!("  island: (nothing from plugins)"),
        }
        if done {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_copies_privately_and_refuses_links() {
        use std::os::unix::fs::PermissionsExt;
        let base =
            std::env::temp_dir().join(format!("naarchy-inst-{}", crate::shelf_store::new_id()));
        let src = base.join("src");
        let root = base.join("plugins");
        std::fs::create_dir_all(src.join("bin")).unwrap();
        std::fs::write(
            src.join("plugin.toml"),
            "name = \"demo\"\nversion = \"1\"\napi = 1\nexec = \"bin/run\"\n",
        )
        .unwrap();
        std::fs::write(src.join("bin/run"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(src.join("bin/run"), std::fs::Permissions::from_mode(0o775))
            .unwrap();
        assert_eq!(install(&src, &root, false).unwrap(), "demo");
        let mode = std::fs::metadata(root.join("demo/bin/run"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        assert!(manifest::load(&root.join("demo")).is_ok());
        assert!(install(&src, &root, false).unwrap_err().contains("--force"));
        assert!(install(&src, &root, true).is_ok());

        std::os::unix::fs::symlink("/etc/passwd", src.join("leak")).unwrap();
        assert!(install(&src, &root, true).unwrap_err().contains("symlink"));
        assert!(
            root.join("demo/plugin.toml").exists(),
            "failed reinstall keeps the old copy"
        );

        set_enabled("demo", &root, false).unwrap();
        assert!(root.join("demo").join(DISABLED_MARKER).exists());
        set_enabled("demo", &root, true).unwrap();
        assert!(!root.join("demo").join(DISABLED_MARKER).exists());
        assert!(remove("../etc", &root).is_err());
        std::fs::remove_dir_all(root.join("demo")).unwrap();
        assert!(remove("demo", &root).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
