# Naarchy — Product & Technical Specification

**Version:** 0.1.0 · **Status:** GTM · **License:** MIT
**Platforms:** Linux (Wayland) — aarch64-first (Apple Silicon / Asahi Macs running Omarchy) and x86_64

---

## 1. What Naarchy is

Naarchy turns the wasted strip at the top of your display — the physical notch on
MacBooks running Asahi/Omarchy, or a floating Dynamic Island pill on any other
monitor — into a productivity layer:

> **A file shelf, clipboard history, media controls, live activities, system HUDs,
> and notifications — all living in your notch. Free and open source forever.**

FOSS answer to macOS notch apps, native for Omarchy: Hyprland + Wayland + GTK4.

### Design principles

1. **Native, not Electron.** Rust + GTK4 + `wlr-layer-shell`. RSS target < 60 MB.
2. **Never steal your clicks.** Expanded panel input region is the glass capsule
   union the floating dock. Empty glass does not eat clicks. Hover-open is
   compositor-assisted (Hyprland `cursorpos`). The collapsed pill *is* the hit
   target (no input-region carve).
3. **Local-first & private.** No telemetry. Network is MPRIS album-art URLs your
   player already advertised, plus ICS feeds you put in config.
4. **Composable with Omarchy.** Every feature is a CLI verb. `naarchy install-binds`
   prints a Hyprland snippet. Autostart is a systemd user unit **or** a desktop
   file — pick one.
5. **One motion language.** Expand, collapse, pill width, HUD value: damped
   springs (`ui::motion::Spring::{OPEN,CLOSE,SNAP}`). Liquid capsule via cairo.

---

## 2. Feature specification (v0.1)

### 2.1 The pill (collapsed)

| ID | Feature | Spec |
|----|---------|------|
| P1 | Anchored top-center layer surface | `LayerShell` overlay, anchor TOP, exclusive zone −1, namespace `"naarchy"`, keyboard `OnDemand` |
| P2 | Notch / island | `appearance.notch_mode = true` hugs ~190 px (physical notch). Default is island (`pill_width_island = 367`). No auto-detect. |
| P3 | Live-activity chips | One pair at a time: timer-done > running timer > now-playing > file-count |
| P4 | Hover-to-open | Hyprland `cursorpos` ~30 Hz, dwell `hover_ms` (180), band `hover_band_px` (8). Non-Hyprland: click the pill or CLI. |
| P5 | Click / hotkey | Click pill, or `naarchy toggle` |
| P6 | Drag-over | Expanding while dragging is Hyprland hover-band + panel `DropTarget`. The pill itself has no `DropTarget`. Non-Hyprland drag onto the collapsed pill does not expand. |
| P7 | Fullscreen courtesy | Focused fullscreen window hides the pill (`hide_fullscreen`) |
| P8 | Multi-monitor | One pill per output whose `gdk::Monitor::connector()` matches `behavior.monitors`. Empty connector always paints (never zero pills). Index 0 is treated as primary. Hotplug: restart. |

### 2.2 Expanded panel

Grows out of the notch as one glass capsule (spring + overshoot). Tab rail is a
**floating dock** under the capsule: **Home · Inbox · Clipboard · Widgets · Calendar**.

Timer and Media are **Home widgets**, not tabs. There is no Settings tab.

| ID | Feature | Spec |
|----|---------|------|
| E1 | Home | Widget grid from `~/.config/naarchy/widgets.json`. Defaults: Timer + Media. Clock pins in from the Widgets drawer (`text/x-naarchy-widget`). |
| E2 | Inbox (file shelf) | Drop files/folders, text, images. Persist across restarts. Thumbnails. Open (double-click), right-click menu: Open / Reveal / Copy Path / Pin / Remove. Drag tiles **out** to any app (`DragSource`). |
| E3 | Clipboard history | Watches Wayland clipboard (600 ms poll, always on). Text + images. Search, pin, click to re-copy, right-click Copy / Pin / Remove. Last 200 unpinned. **No drag-out.** |
| E4 | Calendar | Month grid + today's agenda. Optional public ICS feeds. |
| E5 | Timer | Home widget. Presets 1/5/10/25 min + ruler. Live activity on the pill. Chime + banner on completion. 25m is a preset, **not** a pomodoro cycle. |
| E6 | Media | Home widget. MPRIS v2, art (cached), transport, seek, shuffle/repeat. |
| E7 | Panel sizing | Width/height/radius from config. Last tab is in-memory (not persisted). |

### 2.3 HUDs

| ID | Feature | Spec |
|----|---------|------|
| H1 | Kinds | `volume`, `brightness`, `mic`, `battery`, `caps`, `custom` |
| H2 | Trigger | `naarchy hud volume 45` / `naarchy hud volume +5`. No background pamixer watcher. `auto` is not a parser token — with no value/step, the CLI shells out to pamixer/wpctl/brightnessctl. |
| H3 | Rendering | Capsule below the notch, arc + %, auto-dismiss `hud_timeout_ms` |
| H4 | Notification banners | `naarchy notify` always paints a banner. Optional `features.notifications` owns `org.freedesktop.Notifications`. **Default off** so Omarchy's mako keeps the name. |

### 2.4 System integration

| ID | Feature | Spec |
|----|---------|------|
| S1 | Battery | Removed. The Omarchy bar already has it. `naarchy hud battery` remains. |
| S2 | Clock | Pill clock (`%H:%M`), date on Home clock widget |
| S3 | Shortcuts | CLI verbs. No xdg-desktop-portal GlobalShortcuts. |
| S4 | Autostart | `contrib/naarchy.desktop` + `contrib/naarchy.service`. XOR — do not enable both. |
| S5 | Single-instance | Exclusive unix bind on `$XDG_RUNTIME_DIR/naarchy.sock` mode 600. Second `naarchy run` prints `already running` and **exits 0**. Verbs forward. No reply protocol. |
| S6 | Config | `~/.config/naarchy/config.toml`; 700 ms content-equality poll. Restyles. Does **not** rebuild surfaces (monitor list / feature flags need a restart). |
| S7 | Theming | Follows the active omarchy `colors.toml` when `appearance.omarchy = true`. TOML overrides for accent/bg/fg. No user CSS file. Hyprland `layerrule = blur, naarchy`. |

### 2.5 Performance budget (targets, not lies)

| Metric | Target |
|---|---|
| Idle CPU (pill visible) | Small polling, not zero: GTK 1s tick, config 700 ms, clipboard 600 ms (always), Hyprland cursorpos 33 ms while collapsed, MPRIS 1500 ms |
| Expand animation | Frame-clock springs, ≥ 60 fps on integrated GPU |
| RSS | < 60 MB including clipboard cache |
| Cold start | < 400 ms to pill visible |

---

## 3. Architecture

One process. GTK4 main loop + a 2-thread Tokio worker (zbus) + dedicated threads
(clipboard, Hyprland, config poll, calendar ICS).

Surfaces: `PillUi` (overlay) + `PanelUi` (top) + `HudManager`. Layer namespace
`"naarchy"`. Exclusive zone −1.

```
CLI verbs  →  $XDG_RUNTIME_DIR/naarchy.sock  →  Verb mpsc  →  GTK
Tokio zbus (mpris, settings, notifd) → Event mpsc →  GTK
```

### Persistence

```
$XDG_CONFIG_HOME/naarchy/config.toml
$XDG_CONFIG_HOME/naarchy/widgets.json      # Home widget set (preference)
$XDG_DATA_HOME/naarchy/shelf.json
$XDG_DATA_HOME/naarchy/clipboard.json
$XDG_DATA_HOME/naarchy/blobs/
$XDG_CACHE_HOME/naarchy/art/               # MPRIS art, DefaultHasher key
$XDG_CACHE_HOME/naarchy/calendar/feed-N.ics
$XDG_CACHE_HOME/naarchy/chime.wav
$XDG_RUNTIME_DIR/naarchy.sock              # 0600
```

There is no `state.toml`. Last tab dies with the process.

### Wayland

`zwlr_layer_shell_v1` via gtk4-layer-shell, GDK DnD, OnDemand keyboard, GTK 4.14+
API floor (`v4_14` crate feature). No X11-native path.

---

## 4. Data formats

### shelf item

```json
{ "id": "…", "kind": "file|text|image", "name": "shot.png",
  "path": "/home/m/Pictures/shot.png", "mime": "image/png",
  "text": "", "data_ref": "", "added_at": 1756200000, "pinned": false }
```

### clipboard entry

```json
{ "id": "…", "kind": "Text|Image", "preview": "first 80 chars",
  "text": "…", "data_ref": "clip-….bin", "mime": "text/plain;charset=utf-8",
  "at": 1756200000, "pinned": false }
```

No distinct URI kind — URIs arrive as text if the source offers text.

---

## 5. Security & privacy

* Private IPC socket, mode 600. Exclusive bind is the lock.
* Album-art fetches: URLs from the user's own MPRIS players; 4 s connect / 8 s
  total; 24 MB cap; disk cache keyed by `DefaultHasher` (not SHA-256).
* ICS feeds: URLs you listed, 20 s timeout.
* No telemetry. No background network listeners.

---

## 6. Config schema (v0.1)

See `docs/CONFIG.md`. Defaults: `notifications = false`, `omarchy = true`,
`notch_mode = false`, `hover_open = true`.

---

## 7. CLI

See `docs/CLI.md`. Canonical tabs: `home|inbox|clipboard|widgets|calendar`.
Alias `shelf` → inbox still works; generated binds print `tab inbox`.
`shelf list` reads `shelf.json` on the client (pretty JSON array). Mutations
go to the daemon. Unknown tab: exit 2. Live daemon + `naarchy run`: exit 0.

---

## 8. Roadmap beyond v0.1

Floating Basket · VTE TermiNotch · Power Folders · Quick actions (zip, convert,
tesseract) · LocalSend / KDE Connect · PipeWire visualizer · live lyrics ·
whisper.cpp · keystroke sounds · hyprlock · plugin/Droplet system · per-workspace
shelves · clipboard drag-out · MPRIS Raise on art click · monitor hotplug ·
pomodoro cycle · portal GlobalShortcuts · Settings GUI.

---

## 9. Testing & QA

* `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --bins`
* `scripts/smoke.sh` (headless: help, unknown tab, binds, daemon-not-running)
* CI: Ubuntu 24.04, rustc 1.85.0, gtk4-layer-shell `v1.0.4` from source
* Manual: Hyprland aarch64 (M1 Asahi) + x86_64; notch and island; fractional scale; two monitors
