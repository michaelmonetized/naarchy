use crate::services::Verb;
use gtk4::prelude::FileExt;

pub fn print_help() {
    println!(
        "naarchy {} — notch island for Omarchy/Hyprland

USAGE:
  naarchy --version               print version
  naarchy doctor                  check desktop and optional integrations
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
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: u64 = s[..split].parse().ok()?;
    let multiplier = match &s[split..] {
        "" | "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hour" | "hours" => 3600,
        _ => return None,
    };
    n.checked_mul(multiplier)
        .filter(|n| (1..=crate::ui::timer::MAX_SECS).contains(n))
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
            Some("add") if rest.len() > 1 => {
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let mut paths = Vec::new();
                for value in &rest[1..] {
                    if value.starts_with("https://") || value.starts_with("http://") {
                        paths.push(value.clone());
                    } else {
                        let path = if value.starts_with("file://") {
                            gtk4::gio::File::for_uri(value)
                                .path()
                                .ok_or("invalid file URI")?
                        } else {
                            cwd.join(value)
                        };
                        let path = path.canonicalize().map_err(|e| format!("{value}: {e}"))?;
                        paths.push(path.to_string_lossy().into_owned());
                    }
                }
                Ok(Verb::ShelfAdd(paths))
            }
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
                    icon = Some(rest.get(idx + 1).ok_or("--icon requires a glyph")?.clone());
                    skip_next = true;
                } else if a == "--label" {
                    label = Some(rest.get(idx + 1).ok_or("--label requires text")?.clone());
                    skip_next = true;
                } else if let Some(stripped) = a.strip_prefix('+') {
                    step = Some(stripped.parse::<f64>().map_err(|_| "invalid HUD step")?);
                } else if let Some(stripped) = a.strip_prefix('-') {
                    step = Some(-stripped.parse::<f64>().map_err(|_| "invalid HUD step")?);
                } else if let Ok(v) = a.parse::<f64>() {
                    value = Some(v);
                } else if a != "auto" {
                    return Err(format!("unknown HUD argument: {a}"));
                }
            }
            if value.iter().chain(step.iter()).any(|v| !v.is_finite()) {
                return Err("HUD values must be finite numbers".into());
            }
            if value.is_some() && step.is_some() {
                return Err("use a HUD value or a step, not both".into());
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
fn command_value(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("timeout")
        .args(["2s", program])
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn wpctl_percent(text: &str) -> Option<f64> {
    let value = text
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    value.is_finite().then(|| (value * 100.0).clamp(0.0, 100.0))
}

fn detect_value(kind: &str) -> Option<f64> {
    match kind {
        "volume" | "vol" => command_value("pamixer", &["--get-volume"])
            .and_then(|value| value.parse::<f64>().ok())
            .or_else(|| {
                command_value("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
                    .and_then(|v| wpctl_percent(&v))
            }),
        "mic" => command_value("pamixer", &["--default-source", "--get-volume"])
            .and_then(|value| value.parse().ok()),
        "brightness" | "bright" => {
            let current =
                command_value("brightnessctl", &["get"]).and_then(|s| s.parse::<f64>().ok());
            let maximum =
                command_value("brightnessctl", &["max"]).and_then(|s| s.parse::<f64>().ok());
            if let (Some(current), Some(maximum)) = (current, maximum) {
                if maximum > 0.0 {
                    return Some((current * 100.0 / maximum).clamp(0.0, 100.0));
                }
            }
            std::fs::read_dir("/sys/class/backlight")
                .ok()?
                .flatten()
                .find_map(|entry| {
                    let path = entry.path();
                    let current = std::fs::read_to_string(path.join("brightness"))
                        .ok()?
                        .trim()
                        .parse::<f64>()
                        .ok()?;
                    let maximum = std::fs::read_to_string(path.join("max_brightness"))
                        .ok()?
                        .trim()
                        .parse::<f64>()
                        .ok()?;
                    (maximum > 0.0).then(|| (current * 100.0 / maximum).clamp(0.0, 100.0))
                })
        }
        "battery" => std::fs::read_dir("/sys/class/power_supply")
            .ok()?
            .flatten()
            .find_map(|entry| {
                let path = entry.path();
                if std::fs::read_to_string(path.join("type")).ok()?.trim() != "Battery" {
                    return None;
                }
                std::fs::read_to_string(path.join("capacity"))
                    .ok()?
                    .trim()
                    .parse()
                    .ok()
            }),
        _ => None,
    }
}

pub fn forward_verb(verb: &str, rest: &[String]) {
    match verb_from_args(verb, rest) {
        Ok(v) => send_to_daemon(&v),
        Err(e) => {
            eprintln!("naarchy: {e}");
            std::process::exit(2);
        }
    }
}

fn send_to_daemon(v: &Verb) {
    if let Err(error) = crate::ipc::send(v) {
        eprintln!("naarchy: {error}");
        std::process::exit(1);
    }
}

pub fn print_binds() {
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
    fn wpctl_parser_handles_mute_and_low_volume() {
        assert_eq!(wpctl_percent("Volume: 0.01"), Some(1.0));
        assert_eq!(wpctl_percent("Volume: 0.42 [MUTED]"), Some(42.0));
        assert_eq!(wpctl_percent("Volume: nan"), None);
    }

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
    fn duration_rejects_overflow_and_malformed_units() {
        for value in [
            "0",
            "1m2",
            "m25",
            "18446744073709551615h",
            "1.5h",
            "999999999s",
        ] {
            assert_eq!(parse_duration(value), None, "{value}");
        }
    }

    #[test]
    fn hud_rejects_invalid_values() {
        for value in ["NaN", "inf", "+nope", "--unknown"] {
            assert!(verb_from_args("hud", &["volume".into(), value.into()]).is_err());
        }
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
        assert!(verb_from_args("shelf", &["add".into(), "/tmp".into()]).is_ok());
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
