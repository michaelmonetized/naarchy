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

Desktop builds, tests, and packaging run locally. Do not run them on GitHub-hosted
Actions or Blacksmith; Blacksmith is for website work only. Hosted status is not
desktop validation and is not a release gate unless repository protection
actually requires it. Do not change billing or protection settings to release.

Before a release, build both supported architectures, run applicable local
checks, verify the generated archives and stage installation into temporary
paths. Run each binary's smoke checks on a compatible local runtime where
available and disclose any unavailable coverage. Physical desktop, scaling,
and multi-monitor review is useful supplementary QA; report what was tested.
Use `scripts/package-release.sh` to package the reviewed binaries and exact
commit locally. Review checksums and release notes, then create a GitHub draft
and publish the local files. Follow the privacy model in the README when
introducing network requests or storing personal data.
