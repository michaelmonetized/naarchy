# Changelog

## 0.5.1

- Plugin packages (proof of concept, plugin API 1): programs in any language
  installed under `~/.config/naarchy/plugins/<name>/` with a `plugin.toml`
  manifest publish live activities to the island over JSON lines on stdio.
  Packages are validated (inside their directory, owned by you, not
  group/world-writable), run without a shell, restart with backoff, and can
  show at most 8 bounded activities. New `naarchy plugin` commands install,
  list, remove, enable, disable, and run packages; `features.plugins` turns
  the host off. See docs/PLUGINS.md.
- Bundled example plugin `contrib/plugins/t3-live`: active T3 Code agent
  threads with project, status, and elapsed time, read from T3 Code's MCP
  server with a read-only credential.
- Notifications rework (`features.notifications = true`): no more pile of cards.
  The island shows a bell with the waiting count; expanding it shows the list on
  Home. New notifications peek as one card under the notch, then collapse into
  the bell (`[notifications] peek = false` for bell only). Left click runs the
  default action, focuses or opens the source app, and dismisses; right click
  dismisses; no close buttons. Do-not-disturb follows Omarchy's shell DND and
  freedesktop notification inhibitions (`Inhibit`/`UnInhibit`/`Inhibited`):
  notifications are counted, not shown.

## 0.5.0

- On a fresh installation's first daemon start on local October 10, binary
  confetti settles into `thanks for installing naarchy happy 10/10`, then fades.
  Existing configuration or data counts as prior use. A private decision is
  saved before the welcome, so dismissal, interruption, and relaunch do not
  replay it. Other first-run dates also consume the decision.
- The welcome follows reduced-motion preferences with a static message and
  supports pointer dismissal while normal typing and clipboard use continue.
  Transient windows and animation sources are cleaned up on expiry and desktop
  lifecycle changes.
- On local October 31, occasional solid black fangs, drips, or an upside-down
  bat gripping the island directly alternate with quiet stretches. Disable
  costumes with the Halloween switch in Preferences or `appearance.halloween`.
  Reduced motion shows static artwork immediately; decorations remain passive.
- Added isolated native Wayland checks for seasonal rendering, input access,
  clipboard preservation, dismissal, persistence, and cleanup.
- Saved widget settings also count as prior use after the main configuration
  is removed. Desktop build checks and release packaging now run locally;
  removed the GitHub-hosted desktop build workflows.

## 0.4.0

- Refined the island, Home, and collection pages with clearer typography,
  spacing, empty states, tooltips, and keyboard access. Added native Preferences
  and reduced-motion controls.
- Reduced repeated service startup, unnecessary redraws, and work performed on
  the desktop thread. Monitor connections and appearance geometry now refresh
  the application surfaces.
- Added `naarchy --version` and read-only `naarchy doctor` diagnostics. CLI
  commands now reject malformed durations and invalid input more consistently.
- Clipboard history keeps its newest item first independently of pinning.
  Recopying an older entry promotes it instead of creating another copy.
- Clipboard, Inbox, and Home preferences use private atomic state writes.
  Failed writes retain the previous state; corrupt JSON is backed up for recovery.
- File and text drops save in batches, avoiding repeated index writes for large drops.
- Removing images cleans up owned blobs only after the last reference is removed.
  Clearing Inbox preserves original files. Duplicate files and images are rejected.
- Home now preserves Clock and deliberately empty widget selections after restart.
  Widget tests use isolated state instead of changing the user's preferences.
- Calendar travel estimates require explicit opt-in. Disabling clipboard capture
  also stops its background service. Documentation describes local storage and
  network access.
- Removed obsolete specification documents, implementation plans, and display
  probe examples. Kept behavioral regression coverage and current user guides.
- Updated the declared build requirement to Rust 1.92, consolidated CI setup,
  and made release artifacts depend on checks. Releases are drafted with checksums
  for review before publication.

## 0.3.3

- Fixed artwork for Chromium-backed media players and adjusted compact player layout.
- Removed the media action that added a track note to Inbox.
- Removed the Battery widget; the manual battery HUD remains available.

## 0.3.2

- Show media launchers when a stopped player has no useful track metadata.

## 0.3.1

- Prevented running timers from expanding the island across the entire display.
- Added product screenshots and an interaction recording.

## 0.3.0

- Fixed redraw-related borrow panics and hardened MPRIS startup.
- Reduced idle polling and redundant media position updates.
- Added ruler timer interaction, live activities, and a fullscreen visual bell.
- Improved file drop support, Inbox thumbnails, and image history retention.

## 0.2.5

- Refined compact media controls and corrected media pause targeting.
- Changed the default Home layout to Timer and Media.

## 0.2.4

- Added calendar meeting links, directions, and estimated departure times.

## 0.2.3

- Fixed parsing of ICS start times with timezone and date parameters.

## 0.2.2

- Added calendar configuration guidance and feed setup migration.
- Improved MPRIS discovery and clipboard readability.

## 0.2.1

- Fixed settings-button hit testing and cursor behavior.

## 0.2.0

- Reduced theme, rendering, clipboard, and media polling overhead.
- Added timer sound, HUD lifecycle fixes, and the settings shortcut.

## 0.1.0

- Initial Linux island with Home, Inbox, Clipboard, Widgets, and Calendar.
- Added local persistence, CLI control, Omarchy theming, packaging templates,
  behavioral tests, smoke checks, and CI.
