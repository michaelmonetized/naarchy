//! Read-only installation diagnostics. Never prints feed URLs or clipboard data.
use std::path::Path;

fn available(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            use std::os::unix::fs::PermissionsExt;
            dir.join(program)
                .metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}

fn line(label: &str, ok: bool, help: &str) {
    println!(
        "{}  {label}{}",
        if ok { "OK  " } else { "INFO" },
        if ok {
            String::new()
        } else {
            format!(" — {help}")
        }
    );
}

pub fn run() {
    println!("Naarchy {} · desktop check\n", env!("CARGO_PKG_VERSION"));
    line(
        "Wayland session",
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "run inside a Wayland session",
    );
    line(
        "Hyprland integration",
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        "global hover and fullscreen detection require Hyprland",
    );
    line(
        "Session bus",
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some(),
        "media controls use the session D-Bus",
    );
    line(
        "Daemon",
        std::os::unix::net::UnixStream::connect(crate::ipc::socket_path()).is_ok(),
        "start with naarchy run",
    );
    let path = crate::util::config_file();
    match std::fs::read_to_string(&path) {
        Ok(text) => line(
            "Configuration",
            crate::config::Config::from_toml(&text).is_some(),
            "invalid TOML; the running app keeps its last valid settings",
        ),
        Err(_) => println!("INFO  Configuration — defaults will be created on first launch"),
    }
    line("File opening", available("xdg-open"), "install xdg-utils");
    line(
        "Timer sound",
        ["pw-play", "paplay", "aplay", "ffplay", "canberra-gtk-play"]
            .iter()
            .any(|p| available(p)),
        "install pipewire-audio or another supported audio player",
    );
    line(
        "Volume HUD detection",
        available("pamixer") || available("wpctl"),
        "install pamixer or wireplumber",
    );
    line(
        "Brightness HUD detection",
        available("brightnessctl")
            || Path::new("/sys/class/backlight")
                .read_dir()
                .is_ok_and(|mut entries| entries.next().is_some()),
        "optional: install brightnessctl",
    );
    println!(
        "\nConfig: {}\nBindings: naarchy install-binds",
        path.display()
    );
}
