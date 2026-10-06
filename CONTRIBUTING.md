# Contributing

Naarchy is a native Rust and GTK4 desktop application. Keep changes focused on
behaviors people can use, and update the relevant user documentation when a
setting, command, dependency, or supported behavior changes.

Install the dependencies in [the installation guide](docs/INSTALL.md), then run:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --bins
cargo build --locked --bins
bash scripts/smoke.sh
```

Tests must use temporary state paths, never your actual clipboard, preferences,
or desktop session. Preserve meaningful behavioral tests for persistence, input
validation, and lifecycle failures. Prototype probes and speculative design plans
do not belong in the shipped product tree.

Changes to GTK interactions also need a real Wayland session: check keyboard and
pointer access, light and dark themes, scaling, empty states, a file drop, and a
complete timer countdown. Headless tests do not establish visual quality.

For seasonal overlays, `bash scripts/check-seasonal.sh /tmp/seasonal-previews`
runs the opt-in native fixture on a separate software-rendered Sway output.
It requires Sway, grim, wtype, and dbus-run-session. Explicit test dates and
temporary XDG paths leave your clock, clipboard, preferences, and first-run
marker untouched. The fixture verifies typing, pointer pass-through, Dismiss,
clipboard preservation, and cleanup, and saves demonstration screenshots.
This supplements the physical desktop checks above.

Before a release, validate both supported architectures, confirm installation
from the generated archive on a clean desktop, and capture current screenshots.
The release workflow drafts artifacts; review their checksums and the release
notes before publishing. Follow the privacy model in the README when introducing
network requests or storing personal data.
