# Changelog

## 0.1.0

First GTM cut.

- Liquid-capsule island / notch on Hyprland. Home · Inbox · Clipboard · Widgets · Calendar.
- Timer and Media are Home widgets. Omarchy theme follow.
- Exclusive daemon (unix socket, mode 600). Second `run` exits 0.
- CLI: canonical tab names, `shelf list|clear|remove`, `timer stop`.
- Notifications bus-name grab defaults **off** (mako keeps the name). `naarchy notify` still paints a banner.
- Packaging: desktop file, systemd user unit, PKGBUILD template, Hyprland snippet.
- Tests: config, shelf store, clipboard ring, CLI verbs. Headless `scripts/smoke.sh`. GitHub Actions on Ubuntu 24.04.
