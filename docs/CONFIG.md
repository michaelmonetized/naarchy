# Config

Path: `~/.config/naarchy/config.toml` (`~/.config/naarchy/config.toml:1` in your screenshot — hot-reloads on save, settings gear opens it in Neovim via `omarchy-launch-config-editor`).

Written on first run if missing. **Hot-reload** every 1100 ms compares the file
as a string (not mtime). A save restyles colors, sizes, opacity. It does **not**
rebuild surfaces. Changing `behavior.monitors`, `calendar.feeds`, or dock-hiding feature flags
requires a restart (`systemctl --user restart naarchy`).

Existing files are never overwritten, but if `[calendar]` is missing (pre-0.2 installs like yours) naarchy appends it on next run. You can also add it manually at the bottom.

## Schema (defaults)

```toml
[appearance]
theme = "auto"            # auto | dark | light
omarchy = true            # follow the active omarchy colors.toml
# accent = "#89b4fa"      # override
# pill_bg = "#000000"
# bg = "rgba(0,0,0,0.62)"
# fg = "#cdd6f4"
# icon_font = "JetBrainsMono Nerd Font"
radius = 24
notch_mode = false
# pill_width_notch = 190
# pill_width_island = 392
# margin_top = 0
panel_width = 760
panel_height = 540
opacity = 0.98

[behavior]
hover_open = true
hover_ms = 180
hover_band_px = 8
collapse_on_leave_ms = 180
hide_fullscreen = true
monitors = "all"          # "all" | "primary" | ["DP-1", "HDMI-A-1"]
                          # primary = GDK monitor index 0

[features]
media = true              # pill live-activity chip (Home widget still exists)
shelf = true              # Inbox dock item
clipboard = true          # Clipboard dock item
calendar = true           # Calendar dock item
timer = true              # pill live-activity chip (Home widget still exists)
notifications = false     # own org.freedesktop.Notifications (leave false for mako)
battery_chip = true

[clipboard]
max_entries = 200
max_image_bytes = 8388608

[hud]
timeout_ms = 1400

[clock]
format = "%H:%M"
show_in_pill = false      # the bar already has a clock

[calendar]
feeds = []                # public iCloud / Google ICS feed URLs (one per line)
# feeds = ["https://calendar.google.com/calendar/ical/xxxxxxxx%40gmail.com/public/basic.ics"]
# feeds = ["webcal://p123-caldav.icloud.com/published/2/xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.ics"]
#   Google: Calendar → Settings → Integrate calendar → Public URL / Secret address in iCal format
#   iCloud: Calendar → Share Calendar → Public Calendar → Copy Link (webcal:// → https://)
#   After editing feeds: systemctl --user restart naarchy
refresh_min = 5           # minutes between fetches
```

## Feature flags vs widgets

`features.shelf` / `clipboard` / `calendar` hide dock items.

`features.media` / `features.timer` hide the **pill live-activity chips**.
They do not remove the Home widgets. Home content is `widgets.json`.

Home and Widgets dock items are always present.

## Files naarchy owns

```
~/.config/naarchy/config.toml
~/.config/naarchy/widgets.json          # Home widget set
~/.local/share/naarchy/shelf.json
~/.local/share/naarchy/clipboard.json
~/.local/share/naarchy/blobs/
~/.cache/naarchy/art/
~/.cache/naarchy/calendar/
~/.cache/naarchy/chime.wav
$XDG_RUNTIME_DIR/naarchy.sock
```

`widgets.json` lives in the config dir on purpose (layout preference, not content).
Do not move it.

A leftover `accent = "#7aa2f7"` with `omarchy = true` is stripped so the theme
accent wins.

Clipboard polling (600 ms) is **always on** in v0.1, even if the Clipboard dock
item is hidden.
