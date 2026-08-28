# Install

## Dependencies

**Arch / Omarchy**

```bash
sudo pacman -S --needed gtk4 gtk4-layer-shell rust
```

Optional: `pamixer`, `brightnessctl`, `ttf-jetbrains-mono-nerd` (dock glyphs),
PipeWire (`pw-play` for the timer chime).

**From source**

```bash
cargo install --path . --locked
```

Binary lands in `~/.cargo/bin/naarchy`. Put that on `PATH`.

**PKGBUILD**

`contrib/PKGBUILD` is a template (`x86_64` + `aarch64`). `source=` stays
commented until the GitHub tag exists. Do not uncomment it until then.

## Autostart — pick one

**systemd (recommended)**

Packaged unit (`/usr/bin/naarchy`):

```bash
systemctl --user enable --now naarchy.service
```

cargo-install: copy `contrib/naarchy.service` to `~/.config/systemd/user/` and
set:

```
ExecStart=%h/.cargo/bin/naarchy run
```

Then `systemctl --user daemon-reload && systemctl --user enable --now naarchy.service`.

`Restart=on-abnormal`. A second `naarchy run` while the daemon is live **exits 0**
(idempotent). Do not pair this with desktop autostart.

**Desktop file**

`contrib/naarchy.desktop` — only if you are **not** using the unit. Copy to
`~/.config/autostart/` for a user-level path. `Exec=naarchy run` needs `naarchy`
on the session `PATH` (`~/.cargo/bin` is often missing there). Source-first
users should use systemd.

**Do not** `exec-once = naarchy run` in Hyprland if either of the above is on.

Hyprland must import compositor env so the user manager sees it:

```
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP
```

`naarchy run` **requires** `WAYLAND_DISPLAY`. If it is unset, the process exits 1
and says so. It does not silently skip.

## Hyprland

```bash
naarchy install-binds >> ~/.config/hypr/user-bindings.conf
hyprctl reload
```

Checked-in copy: `contrib/hyprland.conf`. Must include:

```
layerrule = blur, naarchy
layerrule = ignorealpha 0.2, naarchy
```

Without those, the capsule is a dark blob, not glass.

## Asahi / physical notch

In `~/.config/naarchy/config.toml`:

```toml
[appearance]
notch_mode = true
pill_width_notch = 190
margin_top = 0
```

Island is the default. No auto-detect.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| Two pills | Old second daemon, or leftover `exec-once` plus systemd | `pkill -x naarchy`; pick one autostart |
| `naarchy already running` (exit 0) | Lock is working | That's success |
| Unit `status=203/EXEC` | `ExecStart=/usr/bin/naarchy` after cargo-install | Use `%h/.cargo/bin/naarchy run` |
| Unit inactive, no log | You still had `ConditionEnvironment` from an old unit | Current unit has none; `WAYLAND_DISPLAY` missing now fails loudly in the process |
| No hover | Not Hyprland, or `hover_open = false` | Click the pill or `naarchy toggle` |
| Clicks eaten at the top | Input region bug — should not happen | File an issue with `WAYLAND_DISPLAY` + Hyprland version |
| Glass is a flat black card | No blur layerrule | Add the two `layerrule` lines |
| No dock glyphs | Missing nerd font | `ttf-jetbrains-mono-nerd` or set `icon_font` |
| Mako died | You turned `notifications = true` and naarchy won the name | Set it back to `false` (the default) |
| Monitor list ignored | Old build | v0.1 uses `connector()`; empty connector always paints |
| `naarchy run` exits 1 immediately | No `WAYLAND_DISPLAY` | Start from the graphical session |

Optional display smoke (not CI):

```bash
NAARCHY_SMOKE_DISPLAY=1 WAYLAND_DISPLAY=$WAYLAND_DISPLAY ./scripts/smoke.sh
```
