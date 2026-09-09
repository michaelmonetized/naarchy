# Config

Path: `$XDG_CONFIG_HOME/naarchy/config.toml` (normally `~/.config/naarchy/config.toml`).
The settings button opens native Preferences. Its advanced configuration action
opens this file in your desktop editor. Preferences updates only the settings you
changed, preserving other settings. Saving through Preferences removes TOML
comments; use the editor when you want to preserve comments.

Naarchy writes defaults on first run. Valid appearance changes apply while it is
running, including resizing the surfaces. Invalid edits leave the last working
configuration active. Restart after changing service feature flags or calendar
feeds: `systemctl --user restart naarchy.service`.

Existing configuration files are never rewritten during startup. Add a calendar
section manually or use Preferences to connect a feed.

## Schema (defaults)

```toml
[appearance]
theme = "auto"            # auto | dark | light
omarchy = true            # follow the active omarchy colors.toml
# accent = "#89b4fa"      # override
# bg = "#1e1e20"
# fg = "#cdd6f4"
# icon_font = "JetBrainsMono Nerd Font"
notch_mode = false
# pill_width_notch = 190
# pill_width_island = 370
# margin_top = 0
panel_width = 680
panel_height = 460
opacity = 0.98
reduce_motion = false    # skip spring animations

[behavior]
hover_open = true
hover_ms = 180
hover_band_px = 8
collapse_on_leave_ms = 180
hide_fullscreen = true
monitors = "all"          # "all" | "primary" | ["DP-1", "HDMI-A-1"]
                          # primary = GDK monitor index 0

[features]
media = true              # media discovery, controls, and live activity
shelf = true              # Inbox page and file drops
clipboard = true          # clipboard capture and history page
calendar = true           # Calendar page and feed refresh
timer = true              # timer controls and live activity
notifications = false     # own org.freedesktop.Notifications (leave false for mako)

[clipboard]
max_entries = 80
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
travel_times = false      # opt in to IP location, address geocoding, and route services
```

Colors use six-digit hexadecimal values such as `#89b4fa`; `rgba()` values are
not supported. The legacy `appearance.pill_bg` and `appearance.radius` keys are
still accepted for compatibility, but the current capsule renderer does not use
them. Its shape follows the island geometry and active theme.

## Features and Home widgets

The feature flags control the relevant service or page. Restart after changing
these switches. Disabling Clipboard also stops capture; hiding its page is not a
substitute for clearing history already stored on disk.

Use Widgets to add or remove Timer, Media, and Clock from Home. A deliberately
empty Home stays empty after a restart. These layout choices live in
`widgets.json`; the feature flags remain in `config.toml`.

## Clipboard retention

`max_entries` caps unpinned entries, and at most 24 unpinned images are kept.
Pinned items remain until you remove or unpin them. `max_entries = 0` stops new
history entries. `max_image_bytes` rejects images larger than its limit.
Clipboard reads are capped at 8 MiB; this setting can impose a smaller image
limit but cannot increase the read cap.
Removing history deletes owned image data after the updated index is saved;
shared references are retained until the final item is removed.

History and images are local, owner-readable files, not encrypted storage.
Clearing unpinned history preserves your pins. To remove all history, unpin those
items and clear again, or remove each pinned entry.

## Calendar privacy

Feed URLs may contain private access tokens. Keep this file private and avoid
including the URLs in logs or screenshots. With `travel_times = false`, Naarchy
does not request automatic geolocation or send calendar addresses to routing
services. Enabling it uses IPinfo (or ipapi.co) for an approximate starting location, Nominatim
for event addresses, and OSRM for estimated driving time. Estimates are approximate.

## Notification banners

Naarchy shows at most three banners at once and queues up to 32 more. Persistent
banners stay visible until dismissed, and a banner's expiration timer starts
when it becomes visible. If the pending queue fills, overflow is reported to the
sender as a closed notification with an undefined reason. Retention is bounded;
Naarchy is not a notification archive.

Leave `features.notifications = false` to keep your existing desktop notification
service. `naarchy notify` can still show a local banner with that setting off.

## Files naarchy owns

```
~/.config/naarchy/config.toml
~/.config/naarchy/widgets.json          # Home widget set
~/.local/share/naarchy/shelf.json
~/.local/share/naarchy/clipboard.json
~/.local/share/naarchy/blobs/
~/.cache/naarchy/art/
~/.cache/naarchy/calendar/
~/.cache/naarchy/alarm-v2.wav
~/.cache/naarchy/geocode.json
~/.cache/naarchy/route.json
$XDG_RUNTIME_DIR/naarchy.sock
```

`widgets.json` stores layout preferences, while shelf and clipboard files store
content. The XDG config, data, and cache environment variables override these
locations. Malformed state files are backed up beside the original as
`*.json.recovery-*` before starting an empty store; keep those backups if you
need to recover content.

The image cache uses `blobs/`; shelf entries referencing your original files do
not own those files. Removing a shelf entry never removes the original.

A legacy `accent = "#7aa2f7"` with `omarchy = true` is treated as unset so the
current Omarchy accent can take effect.
