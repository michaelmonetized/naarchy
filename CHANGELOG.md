# Changelog

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
