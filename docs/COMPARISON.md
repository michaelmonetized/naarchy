# Feature comparison — Naarchy vs the macOS notch apps

Compiled Aug 2026 from public docs of [Droppy](https://getdroppy.app) (one-time $9.99),
[NotchNook](https://lo.cafe/notchnook) (lo.cafe, $25 or $3/mo) and
[Boring.Notch](https://github.com/TheBoredTeam/boring.notch) (FOSS). Naarchy is MIT-licensed
and runs on Linux/Wayland (Omarchy + Hyprland).

Legend: ✅ ships in v0.1 · ⚠️ ships with a documented caveat · 🚧 planned · ➖ N/A on Linux
or superseded by a Linux-native equivalent.

## Core

| Feature | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| Notch pill / Dynamic-Island mode | ✅ | ✅ | ✅ | ✅ `notch_mode` / island default |
| Hover-to-open, click-to-open, global hotkey | ✅ | ✅ | ✅ | ✅ hover is Hyprland IPC; else click/CLI |
| Drag-over auto-open drop zone | ✅ | ✅ | ✅ | ⚠️ Hyprland hover-band + expanded panel drop. Collapsed pill has no DropTarget. |
| Multi-monitor | ✅ | ✅ | 🚧 | ⚠️ one pill per output at start; named list uses connector(); no hotplug |
| Fullscreen courtesy hiding | ✅ | ✅ | ✅ | ✅ (Hyprland IPC) |
| Themes / accent / radius / opacity | ✅ | ✅ | partial | ✅ omarchy follow + TOML; hot-reload restyles, does not rebuild surfaces |

## Files

| Feature | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| File shelf in notch (drop → park → drag out) | ✅ | ✅ | 🚧 | ✅ Inbox tab; `DragSource` on tiles |
| Shelf persists across restarts | ✅ | ✅ | ➖ | ✅ |
| Thumbnails / previews | ✅ | ✅ | ➖ | ✅ pixbuf tiles |
| Open / reveal / copy paths / remove / clear | ✅ | ✅ | ➖ | ✅ right-click menu (button 3) |
| Accept plain text & raw images | ✅ | ✅ | ➖ | ✅ |
| Floating Basket (mouse-jiggle mid-drag) | ✅ | ✅ | ➖ | 🚧 |
| Power Folders / watched dirs | ✅ | ✅ | ➖ | 🚧 |
| Quick actions: zip / convert | ✅ | partial | ➖ | 🚧 (CLI is the seam) |
| Quick Look-style preview | ✅ | ✅ | ➖ | 🚧 |
| OCR | ✅ | ➖ | ➖ | 🚧 |
| AirDrop sharing | ✅ | ✅ | ➖ | 🚧 LocalSend / KDE Connect |

## Clipboard

| Feature | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| History (text/images), persistent | ✅ | ✅ | ➖ | ✅ 600 ms Wayland poll |
| Search + pin | ✅ | ✅ | ➖ | ✅ |
| Click to re-copy | ✅ | ✅ | ➖ | ✅ |
| Drag-out | ✅ | ✅ | ➖ | 🚧 (not in v0.1) |
| Tags / PDF viewers | ✅ | ➖ | ➖ | 🚧 |

## Media

| Feature | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| Now-playing chip + full player | ✅ | ✅ | ✅ | ✅ MPRIS — Home widget, not a tab |
| Album art, cached | ✅ | ✅ | ✅ | ✅ `DefaultHasher` disk cache |
| Seek, shuffle/repeat, prev/play/next | ✅ | ✅ | partial | ✅ |
| Raise player on art click | ✅ | ➖ | ➖ | 🚧 (`MediaCmd::Raise` exists, UI does not send it) |
| Queue | ✅ | ➖ | ➖ | 🚧 |
| Audio visualizer | ✅ | ✅ | ➖ | 🚧 |
| Live lyrics | ✅ | ➖ | ➖ | 🚧 |

## System & widgets

| Feature | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| Volume/brightness HUDs | ✅ | ✅ | ➖ | ⚠️ CLI + binds; **no** background pamixer watcher |
| Battery + charging | ✅ | ✅ | ✅ | HUD only (`naarchy hud battery`) |
| Notification banners in notch | ✅ | partial | ➖ | ⚠️ `naarchy notify` always. Bus-name grab **defaults off** (mako keeps the name). |
| Calendar month view | ✅ | ✅ | 🚧 | ✅ + optional ICS feeds |
| Timers as live activity | ✅ | ✅ | ➖ | ✅ 1/5/10/25 presets. 25m is not a pomodoro cycle. |
| Notes scratchpad | ✅ | ✅ | ➖ | 🚧 |
| Embedded terminal | ✅ | ➖ | ➖ | 🚧 |
| Voice transcribe | ✅ | ➖ | ➖ | 🚧 |
| Keystroke sounds | ✅ | ➖ | ➖ | 🚧 |
| Lock-screen takeover | ✅ | ✅ | ➖ | 🚧 hyprlock |
| Camera mirror | ✅ | ✅ | ➖ | 🚧 |
| Plugin ecosystem | ✅ | ➖ | ➖ | 🚧 |
| Shortcuts launcher | ✅ | ✅ | ➖ | 🚧 (rofi/walker first) |

## Platform honesty

* macOS-only items map to Linux equivalents: AirDrop → LocalSend/KDE Connect;
  Apple Notes → local markdown (+ Syncthing); lock screen → hyprlock; menu-bar
  icon → waybar / omarchy-shell.
* Wayland has no global mouse-jiggle without compositor help. Hover uses Hyprland
  `cursorpos`. Everywhere else, binds.
* Droppy and NotchNook are paid and closed. Boring.Notch is FOSS and macOS-only.
  Naarchy is the FOSS one that ships on Omarchy.

## Price

| | Droppy | NotchNook | Boring.Notch | **Naarchy** |
|---|---|---|---|---|
| License | $9.99 one-time | $25 or $3/mo | Free (MIT) | **Free (MIT)** |
| Source | ⚠️ partial | ❌ | ✅ | ✅ |
