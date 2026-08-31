# Changelog

## 0.2.2 — calendar docs, media observer, clipboard facelift

- **Calendar**: explain where to add ICS in `~/.config/naarchy/config.toml:36` at bottom `[calendar]` `feeds = ["https://.../basic.ics"]` with Google (Settings → Integrate calendar → Secret address in iCal format) and iCloud (Share Calendar → Public Calendar) examples (`docs/CONFIG.md:62`); auto-migrate old configs missing `[calendar]` (`src/config.rs:220`).
- **Media**: make pill & widget a universal MPRIS observer — not cliamp-only. Pill now shows only playing (`src/ui/pill.rs:326` `music_on = s.playing`), icon-only (art or `MUSIC`) to fix cut-off text to right of notch; stale `cliamp quit` no longer lingers because `scan_and_emit` ranked snapshot is tried in order and failed snaps clear to `Media(None)` (`src/services/mpris.rs:227`). Any app that speaks MPRIS — `mpv`, `vlc`, `ncspot`/`spotify`, `chromium --app` (Spotify web shows as `org.mpris.MediaPlayer2.chromium.*` with title/artist/artUrl) — now lights the pill icon and full Home Media widget (play/pause, seek, art) with live 900 ms ticker. `cliamp` without MPRIS will still say nothing playing — use an MPRIS player.
- **Clipboard**: ditch fugly white card — rows now `glass` with `border` (`src/theme.rs:405`), `na-clip-time` `0.72` vs `0.52` so `now/1m/7m` visible (`src/ui/clipview.rs:141`), pin `★` in accent, `ListBox` transparent viewport (`src/theme.rs:155`), `na-clip-list` class.

## 0.2.1 — settings hit-test + cursor

- Fix settings gear not clickable: `dock_wrap` was outside `apply_input_region` so hover counted as leave → panel collapsed before click; now `dock_wrap` rect (dock + gear) is the hit region and throttled to 20 Hz (`src/ui/panel.rs:363`).
- Gear now shows `pointer` cursor (`gdk::Cursor::from_name("pointer")` `src/ui/panel.rs:206`) instead of text caret, and fades with dock.

## 0.2.0 — perf + timer + settings

- **Perf**: stable FNV cache_key fixes art/shelf/clip re-download each restart; cached `Palette` removes per-frame omarchy file I/O (pill/timer/liquid); `idle`→`timeout 16ms` stops busy-loop wake; throttled `apply_input_region` (~20 Hz); liquid `COPIES 32→18 LAYERS 16→8`; hyprland hover `33→60ms` + surface-aware stay-open; mpris `1.5s→2s` + `400→900ms`; clipboard `600→900ms`; config watcher `700→1100ms`; pill `measure` only on mood change; clipview borrow without 400 `to_lowercase` allocs per keystroke; `parse_payload` O(n²)→O(n).
- **Timer**: replaces ugly ruler with circular ring + big clock + presets + `25m / 90s` entry; visual bell pulsates + `Revealer` shake; audible loop every 3 s for 60 s via `pw-play→paplay→aplay→ffplay→canberra` + `display.beep()` fallback; `Done` state lasts 60 s.
- **HUD**: fix `|| true` bug that dismissed critical banners; destroy oldest banner instead of leaking `ApplicationWindow`.
- **Settings**: gear `na-dock-btn` in dock opens `~/.config/naarchy/config.toml` via `omarchy-launch-config-editor` (→ `omarchy-launch-editor` → `nvim` in `omarchy-launch-tui`), mirroring every Omarchy plugin.
- **UI polish**: dock settings button, `na-preset` hover border, `na-timer-*` animations, less jank on expand/collapse, pill width spring gated.

## 0.1.0

First GTM cut.

- Liquid-capsule island / notch on Hyprland. Home · Inbox · Clipboard · Widgets · Calendar.
- Timer and Media are Home widgets. Omarchy theme follow.
- Exclusive daemon (unix socket, mode 600). Second `run` exits 0.
- CLI: canonical tab names, `shelf list|clear|remove`, `timer stop`.
- Notifications bus-name grab defaults **off** (mako keeps the name). `naarchy notify` still paints a banner.
- Packaging: desktop file, systemd user unit, PKGBUILD template, Hyprland snippet.
- Tests: config, shelf store, clipboard ring, CLI verbs. Headless `scripts/smoke.sh`. GitHub Actions on Ubuntu 24.04.
