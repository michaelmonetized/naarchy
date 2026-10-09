# Validation

## 0.5 release candidate

Local candidate checks on October 9, 2026, used the isolated seasonal worktree
on aarch64 Linux, GTK 4.22.5, and gtk4-layer-shell 1.3.0. Formatting, Clippy
with warnings denied, and all 100 Rust tests passed with the supported Rust
1.92.0 toolchain. The ordinary suite's native fixture returns without GTK;
it was separately run on a private, software-rendered Sway output.
The optimized ARM64 binary built successfully with Rust 1.92.0 and passed
CLI and installer smoke checks using temporary installation staging.

All nine native seasonal scenarios passed: welcome, reduced motion, pointer
dismissal, interruption, off-date start, fangs, drips, bat, and reduced-motion
bat. They verify continued typing, pointer pass-through, clipboard preservation,
restoration, no replay, and transient-window cleanup. Dates are injected into
test policy only; temporary XDG paths and a private D-Bus leave real first-run
state, clipboard content, system time, and the installed app untouched. Fresh
native screenshots were inspected. These checks supplement physical Hyprland,
scaling, and multi-monitor review; they do not establish those physical checks.

The optimized x86-64 binary and test binary cross-compiled against Ubuntu
24.04's GTK dependencies using Rust 1.92.0. The temporary environment's QEMU crashed with an internal
SIGBUS before any test started. This is not an x86-64 runtime pass. Successful
native x86-64 validation, remote CI, checksum/archive review, and installation
from the generated archives remain required before publication. Local build
evidence must not be substituted for these release gates.

## 0.4 validation (historical)

Validation performed on September 5, 2026, on an aarch64 Arch Linux desktop
running Hyprland, GTK 4.22.4, gtk4-layer-shell 1.3.0, and Rust 1.98.0.
Native interaction checks used a separate
configuration, data directory, runtime socket, and temporary headless output.
Screenshots contain demonstration content.

## Build and regression checks

Passed: formatting, locked dependency resolution, Clippy with warnings denied,
all 86 Rust tests (none ignored), debug and optimized builds, and CLI/installer
smoke checks against both binaries. The smoke installer uses a temporary staging
directory. Behavioral coverage includes persistence failure recovery, bounded
IPC, clipboard and shelf handling, calendar recurrence and timezones, media
updates on an isolated D-Bus session, and notification replacement handling.

## Native desktop checks

- Calendar previous/next month and Today navigation.
- Timer presets, a 48-hour duration, pause, resume, reset, and Home widget
  selection persistence.
- Clipboard filtering against seeded history.
- Preferences opening as a bounded floating window, saving and rebuilding
  surfaces without losing advanced settings.
- Three monitor add/remove cycles and three settings rebuild cycles without
  a crash; one expanded panel on the selected output.
- Fullscreen hides the island and exiting fullscreen restores it.
- Notification replacement cancels the original expiry and the replacement
  expires on its own deadline.
- On a private D-Bus, three persistent alerts remain visible while a fourth
  waits; dismissing the first reveals the fourth, and explicit closure removes
  all banners without disturbing the island.
- Multiple shelf previews render without GTK size warnings.

The lifecycle checks caught and fixed a GTK 4.22.4 crash when destroying a
never-realized hidden application window. Hidden panels are now realized before
they can be disposed, and unchanged monitor surfaces are retained during hotplug.

## Short performance sample

Matched eight-second samples used the installed 0.3.3 release and a 0.4.0
optimized build with the same isolated settings, clipboard/media services
enabled, no calendar feeds, and hover disabled. CPU is a percentage of one core.

| State | 0.3.3 CPU | 0.4.0 CPU | 0.3.3 RSS | 0.4.0 RSS |
|---|---:|---:|---:|---:|
| Collapsed | 0.125% | Below sampling resolution | 82.0 MiB | 84.3 MiB |
| Expanded | 0.625% | 0.375% | 102.0 MiB | 100.6 MiB |

This is a brief directional sample, not a battery-life benchmark or a guarantee.
It predates the final notification queue and lifecycle refinements. Media work
now follows D-Bus signals with a repair interval, unchanged rows are reused, and
shelf batches persist once per operation.

## Release boundaries

The local archive targets aarch64 Linux and dynamically links GTK and the host
system libraries; its glibc requirement is 2.39 or newer. The configured x86_64
and aarch64 GitHub release jobs still need a successful remote run before public
distribution, including the pinned Rust 1.92 toolchain check. This session did
not publish a release or replace the installed
desktop binary. Sustained everyday use and physical multi-monitor testing on
supported target systems remain release gates. See [product scope](COMPARISON.md)
for integration boundaries and features not implemented.
