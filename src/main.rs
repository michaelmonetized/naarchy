mod app;
mod chime;
mod clip_store;
mod config;
mod omarchy;
mod services;
mod shelf_store;
mod theme;
mod timefmt;
mod ui;
mod util;
mod widget_store;

use config::Config;
use gtk4::prelude::*;
use services::{Event, Verb};

struct Startup {
    cfg: Config,
    event_rx: mpsc::Receiver<Event>,
    verb_rx: mpsc::Receiver<Verb>,
    event_tx: services::EventTx,
}

static STARTUP: std::sync::Mutex<Option<Startup>> = std::sync::Mutex::new(None);
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;

/// Path of the private IPC socket.
///
/// `$XDG_RUNTIME_DIR/naarchy.sock`. The exclusive bind on this path is the
/// single-instance lock. `quit` may leave a stale socket; a failed connect
/// unlinks it on the next `run`.
fn sock_path() -> PathBuf {
    util::runtime_dir().join("naarchy.sock")
}

/// Bind the IPC socket exclusively.
///
/// If a live daemon already holds the socket, prints `naarchy already running`
/// and exits 0 so `systemctl start` is idempotent. Bind failure is fatal.
fn bind_socket() -> UnixListener {
    let path = sock_path();
    if UnixStream::connect(&path).is_ok() {
        eprintln!("naarchy already running");
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&path);
    match UnixListener::bind(&path) {
        Ok(listener) => {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            listener
        }
        Err(e) => {
            eprintln!(
                "naarchy: could not bind IPC socket at {}: {e}",
                path.display()
            );
            std::process::exit(1);
        }
    }
}

/// Print `$XDG_DATA_HOME/naarchy/shelf.json` as a pretty JSON array.
///
/// Client-side. Does not talk to the daemon. Missing file → `[]`.
fn print_shelf_list() {
    let path = util::data_dir().join("shelf.json");
    match std::fs::read_to_string(&path) {
        Ok(s) if s.trim().is_empty() => println!("[]"),
        Ok(s) => match serde_json::from_str::<serde_json::Value>(&s) {
            Ok(v) => match serde_json::to_string_pretty(&v) {
                Ok(pretty) => println!("{pretty}"),
                Err(_) => print!("{s}"),
            },
            Err(_) => {
                eprintln!("naarchy: shelf.json is not valid JSON");
                std::process::exit(1);
            }
        },
        Err(_) => println!("[]"),
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        None | Some("run") | Some("daemon") => start_daemon(),
        Some("install-binds") => print_binds(),
        Some("--help") | Some("-h") | Some("help") => print_help(),
        Some("shelf") if args.get(1).map(|s| s.as_str()) == Some("list") => print_shelf_list(),
        Some(verb) => forward_verb(verb, &args[1..]),
    }
}

fn print_help() {
    println!(
        "naarchy {} — notch island for Omarchy/Hyprland

USAGE:
  naarchy run                     launch (foreground)
  naarchy toggle                  expand/collapse
  naarchy expand | collapse
  naarchy tab <home|inbox|clipboard|widgets|calendar>
                                  aliases: start=home, shelf|files|drops=inbox,
                                  clip=clipboard, drawer|grid=widgets, cal=calendar
  naarchy hud <volume|brightness|mic|battery|caps|custom> [value|+N|-N]
                                  [--icon GLYPH] [--label TEXT]
  naarchy notify SUMMARY [BODY]   banner (does not need notifd)
  naarchy shelf add PATH…
  naarchy shelf list              print shelf.json as a JSON array (no daemon)
  naarchy shelf clear | remove ID
  naarchy clipboard paste-last    aliases: clip, copy-last
  naarchy timer <30s|25m|1h> | stop
  naarchy quit
  naarchy install-binds           print recommended hyprland binds",
        env!("CARGO_PKG_VERSION")
    );
}

/// Parse a timer duration token.
///
/// Accepts `30s` / `25m` / `1h` (and aliases `sec`, `min`, `hr`, …). A bare
/// number is seconds.
///
/// Arguments:
/// - `s`: duration token from the CLI
///
/// Returns: seconds, or `None` if the token is not a duration.
fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit): (String, String) = s.chars().partition(|c| c.is_ascii_digit());
    let n: u64 = num.parse().ok()?;
    match unit.as_str() {
        "s" | "sec" | "secs" => Some(n),
        "m" | "min" | "mins" => Some(n * 60),
        "h" | "hr" | "hour" | "hours" => Some(n * 3600),
        "" => Some(n),
        _ => None,
    }
}

/// Build a Verb from raw CLI tokens.
///
/// Resolves HUD auto-detect when no value/step is given. Unknown tab names
/// fail here (exit 2) so the daemon is not required to reject them.
///
/// Arguments:
/// - `verb`: first argv token after `naarchy`
/// - `rest`: remaining argv tokens
///
/// Returns: a `Verb` to forward, or an error string for stderr.
fn verb_from_args(verb: &str, rest: &[String]) -> Result<Verb, String> {
    match verb {
        "toggle" => Ok(Verb::Toggle),
        "expand" => Ok(Verb::Expand),
        "collapse" => Ok(Verb::Collapse),
        "tab" => {
            let name = rest
                .first()
                .ok_or_else(|| "tab requires a name".to_string())?;
            if crate::ui::Tab::from_cli(name).is_none() {
                return Err(format!("unknown tab: {name}"));
            }
            Ok(Verb::Tab(name.clone()))
        }
        "quit" => Ok(Verb::Quit),
        "timer" => match rest.first().map(|s| s.as_str()) {
            Some("stop") | Some("reset") => Ok(Verb::TimerStop),
            Some(d) => parse_duration(d)
                .map(Verb::Timer)
                .ok_or_else(|| "timer needs e.g. 25m".into()),
            None => Err("timer needs e.g. 25m".into()),
        },
        "notify" => {
            let summary = rest.first().cloned().unwrap_or_else(|| "naarchy".into());
            let body = rest.get(1).cloned().unwrap_or_default();
            Ok(Verb::Notify { summary, body })
        }
        "shelf" => match rest.first().map(|s| s.as_str()) {
            Some("add") if rest.len() > 1 => Ok(Verb::ShelfAdd(rest[1..].to_vec())),
            Some("add") => Err("usage: naarchy shelf add PATH…".into()),
            Some("clear") => Ok(Verb::ShelfClear),
            Some("remove") => rest
                .get(1)
                .cloned()
                .map(Verb::ShelfRemove)
                .ok_or_else(|| "usage: naarchy shelf remove ID".into()),
            Some("list") => Err("shelf list is handled client-side".into()),
            _ => Err("usage: naarchy shelf add PATH… | list | clear | remove ID".into()),
        },
        "clipboard" | "clip" => match rest.first().map(|s| s.as_str()) {
            Some("paste-last") | Some("copy-last") => Ok(Verb::ClipboardPasteLast),
            _ => Err("usage: naarchy clipboard paste-last".into()),
        },
        "hud" => {
            let kind = rest
                .first()
                .cloned()
                .filter(|k| !k.starts_with('-'))
                .unwrap_or_else(|| "volume".into());
            let mut value: Option<f64> = None;
            let mut step: Option<f64> = None;
            let mut icon = None;
            let mut label = None;
            let mut skip_next = false;
            for (idx, a) in rest.iter().enumerate().skip(1) {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                if a == "--icon" {
                    icon = rest.get(idx + 1).cloned();
                    skip_next = true;
                } else if a == "--label" {
                    label = rest.get(idx + 1).cloned();
                    skip_next = true;
                } else if let Some(stripped) = a.strip_prefix('+') {
                    step = stripped.parse::<f64>().ok().or(Some(5.0));
                } else if let Some(stripped) = a.strip_prefix('-') {
                    step = stripped.parse::<f64>().ok().map(|v| -v).or(Some(-5.0));
                } else if let Ok(v) = a.parse::<f64>() {
                    value = Some(v);
                }
            }
            if value.is_none() && step.is_none() {
                value = detect_value(&kind);
            }
            Ok(Verb::Hud {
                kind,
                value,
                step,
                icon,
                label,
            })
        }
        other => Err(format!("unknown verb: {other}")),
    }
}

/// Best-effort current value detection for HUD auto mode.
fn detect_value(kind: &str) -> Option<f64> {
    let run = |cmd: &str| -> Option<String> {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
    };
    match kind {
        "volume" | "vol" => run("command -v pamixer >/dev/null && pamixer --get-volume || wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null | grep -o '[0-9.]*$'").and_then(|s| s.trim().parse::<f64>().ok()).map(|mut v| {
            if v <= 1.5 { v *= 100.0; } // wpctl prints 0..=1.5
            v.clamp(0.0, 100.0)
        }),
        "brightness" | "bright" => run(
            "command -v brightnessctl >/dev/null && brightnessctl info | grep -o '[0-9]*%' | head -1 | tr -d '%' || cat /sys/class/backlight/*/brightness /sys/class/backlight/*/max_brightness 2>/dev/null | paste -sd/ - | awk -F/ '{printf \"%d\", $1*100/$2}'",
        )
        .and_then(|s| s.trim().parse::<f64>().ok()),
        "mic" => run("pamixer --default-source --get-volume").and_then(|s| s.parse().ok()),
        "battery" => run(
            "cat /sys/class/power_supply/BAT*/capacity 2>/dev/null | head -1",
        )
        .and_then(|s| s.trim().parse().ok()),
        _ => None,
    }
}

fn forward_verb(verb: &str, rest: &[String]) {
    match verb_from_args(verb, rest) {
        Ok(v) => send_to_daemon(&v),
        Err(e) => {
            eprintln!("naarchy: {e}");
            std::process::exit(2);
        }
    }
}

fn send_to_daemon(v: &Verb) {
    let path = sock_path();
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            let json = serde_json::to_string(v).expect("serialize verb");
            if stream.write_all(json.as_bytes()).is_ok() && stream.write_all(b"\n").is_ok() {
                return;
            }
            eprintln!("naarchy: failed to talk to daemon");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("naarchy daemon not running (start with: naarchy run)");
            std::process::exit(1);
        }
    }
}

fn start_daemon() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("naarchy: WAYLAND_DISPLAY is not set");
        std::process::exit(1);
    }

    let listener = bind_socket();

    let cfg_path = util::config_file();
    Config::save_default_if_missing(&cfg_path);
    let cfg = Config::load(&cfg_path);

    let (event_tx, event_rx) = services::EventTx::pair();
    let (verb_tx, verb_rx) = mpsc::channel::<Verb>();

    {
        let vt = verb_tx.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    if let Ok(v) = serde_json::from_str::<Verb>(&line) {
                        if vt.send(v).is_ok() {
                            services::wake_ui();
                        }
                    }
                }
            }
        });
    }

    // One tokio runtime for every zbus/ICS task. Keep it alive with pending().
    {
        let tx = event_tx.clone();
        let feeds = cfg.calendar.feeds.clone();
        let refresh = cfg.calendar.refresh_min;
        let want_notif = cfg.features.notifications;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .thread_name("naarchy-io")
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                match services::mpris::run(tx.clone()).await {
                    Ok(h) => {
                        *MEDIA_CMD.lock().unwrap() = Some(h.cmd_tx);
                    }
                    Err(e) => log::info!("mpris unavailable: {e}"),
                }

                {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = services::upower::run(tx).await {
                            log::debug!("upower unavailable: {e}");
                        }
                    });
                }

                {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = services::settings::run(tx).await {
                            log::debug!("portal settings unavailable: {e}");
                        }
                    });
                }

                if !feeds.is_empty() {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        services::calendar::run(tx, feeds, refresh).await;
                    });
                }

                if want_notif {
                    if let Ok(cmd_tx) = services::notifd::run(tx).await {
                        *NOTIF_CMD.lock().unwrap() = Some(cmd_tx);
                    }
                }

                std::future::pending::<()>().await;
            });
        });
    }

    // Clipboard watcher thread
    {
        let tx = event_tx.clone();
        services::clipboard::spawn(tx);
    }

    // Hyprland integration (hover band, fullscreen events)
    {
        let tx = event_tx.clone();
        let hover_open = cfg.behavior.hover_open;
        let hover_ms = cfg.behavior.hover_ms;
        let zone = services::hyprland::HoverZone {
            band_px: cfg.behavior.hover_band_px as f64,
            pill_w: cfg
                .appearance
                .pill_width_island
                .max(ui::liquid::NOTCH_W as i32) as f64,
            pill_h: ui::liquid::LIVE_H,
            panel_w: cfg.appearance.panel_width as f64 * ui::liquid::PANEL_WINDOW_SCALE,
            panel_h: cfg.appearance.panel_height as f64,
        };
        services::hyprland::spawn(tx, zone, hover_ms, hover_open);
    }

    // Config hot-reload → forwarded as events
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            let (ctx, crx) = mpsc::channel::<Config>();
            let _watcher = config::ConfigWatcher::spawn(cfg_path, ctx);
            while let Ok(new_cfg) = crx.recv() {
                tx.send(Event::ConfigChanged(Box::new(new_cfg)));
            }
        });
    }

    // GTK application
    let gtk_app = gtk4::Application::builder()
        .application_id("app.naarchy.Naarchy")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    {
        *STARTUP.lock().unwrap() = Some(Startup {
            cfg,
            event_rx,
            verb_rx,
            event_tx,
        });
        gtk_app.connect_activate(|gtk_app| {
            let s = STARTUP.lock().unwrap().take().expect("startup state");
            let media_cmd = MEDIA_CMD.lock().unwrap().take();
            let notif_cmd = NOTIF_CMD.lock().unwrap().take();
            app::run(
                gtk_app, s.cfg, s.event_rx, s.verb_rx, s.event_tx, media_cmd, notif_cmd,
            );
        });
    }

    gtk_app.run_with_args(&["naarchy"]);
    let _ = std::fs::remove_file(sock_path());
}

static MEDIA_CMD: std::sync::Mutex<
    Option<tokio::sync::mpsc::UnboundedSender<services::mpris::MediaCmd>>,
> = std::sync::Mutex::new(None);
static NOTIF_CMD: std::sync::Mutex<
    Option<tokio::sync::mpsc::UnboundedSender<services::notifd::NotifCmd>>,
> = std::sync::Mutex::new(None);

fn print_binds() {
    println!(
        r#"# ── naarchy ─ Hyprland bindings ─────────────────────────────
# Add to ~/.config/hypr/user-bindings.conf (or hyprland.conf).
# Checked-in copy: contrib/hyprland.conf

# So the systemd user manager sees the compositor env.
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP

# Toggle the notch panel
bind = SUPER, N, exec, naarchy toggle

# Inbox-focused open
bind = SUPER SHIFT, N, exec, naarchy tab inbox

# Clipboard history
bind = SUPER, V, exec, naarchy tab clipboard

# Timer presets
bind = SUPER ALT, T, exec, naarchy timer 25m

# HUDs that replace system overlays (chain your real volume/brightness tools).
# `auto` is not a parser token — with no value/step, naarchy reads pamixer/brightnessctl.
binde = , XF86AudioRaiseVolume, exec, pamixer -ui 5 && naarchy hud volume auto
binde = , XF86AudioLowerVolume, exec, pamixer -ud 5 && naarchy hud volume auto
bind  = , XF86AudioMute,       exec, pamixer -t && naarchy hud volume $(pamixer --get-volume)
binde = , XF86MonBrightnessUp, exec, brightnessctl set +5% && naarchy hud brightness auto
binde = , XF86MonBrightnessDown, exec, brightnessctl set 5%- && naarchy hud brightness auto

# Liquid glass — blur the shelf so the capsule reads as glass.
layerrule = blur, naarchy
layerrule = ignorealpha 0.2, naarchy

# Autostart: pick systemd XOR the desktop file. Do not also exec-once naarchy.
# Packaged:  systemctl --user enable --now naarchy.service
# cargo-install: copy contrib/naarchy.service to ~/.config/systemd/user/
#                and set ExecStart=%h/.cargo/bin/naarchy run"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_units() {
        assert_eq!(parse_duration("25m"), Some(1500));
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("90"), Some(90));
        assert_eq!(parse_duration("foo"), None);
        assert_eq!(parse_duration("min"), None);
    }

    #[test]
    fn verb_hud_step() {
        let v = verb_from_args("hud", &["volume".into(), "+5".into()]).unwrap();
        match v {
            Verb::Hud { kind, step, .. } => {
                assert_eq!(kind, "volume");
                assert_eq!(step, Some(5.0));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn verb_shelf_add_needs_path() {
        assert!(verb_from_args("shelf", &["add".into()]).is_err());
        assert!(verb_from_args("shelf", &["add".into(), "/tmp/a".into()]).is_ok());
    }

    #[test]
    fn verb_unknown_tab() {
        assert!(verb_from_args("tab", &["media".into()]).is_err());
        assert!(verb_from_args("tab", &["nosuch".into()]).is_err());
        assert!(verb_from_args("tab", &["inbox".into()]).is_ok());
    }

    #[test]
    fn verb_timer_stop() {
        match verb_from_args("timer", &["stop".into()]).unwrap() {
            Verb::TimerStop => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
