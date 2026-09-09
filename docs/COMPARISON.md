# Product scope

Naarchy is a Linux companion for Omarchy and Hyprland. Its focus is quick access
to the files, copied content, media, timers, and calendar events in your day.
macOS applications such as Droppy are useful design references; this document
describes Naarchy's implemented behavior without claiming feature parity or
maintaining an unverifiable competitor pricing matrix.

| Area | Available | Boundaries |
|---|---|---|
| Island | Click or hover to open; notch and island sizing; monitor selection and hotplug | Hover and fullscreen detection depend on Hyprland; other desktops need layer-shell support |
| Inbox | Persistent file, text, and image shelf; pin, open, reveal, copy paths, remove and clear; file drag out | No watched folders, batch conversion, OCR, or floating drag basket |
| Clipboard | Local text/image history; search, pin, recopy, and clear; bounded unpinned history | No encrypted vault, device sync, or guaranteed detection of sensitive clipboard content |
| Media | MPRIS player discovery, artwork, and transport; launchers when idle | No lyrics, media queue editor, or audio visualizer |
| Timer | Countdown, pause/resume, live activity, visual bell, sound | No scheduled reminders, persistent countdown after process exit, or automatic Pomodoro cycles |
| Calendar | Month and ICS agenda; meeting links; directions; optional travel estimates | Read-only feed integration; no event editing or account OAuth; bounded recurring-event expansion; complex provider-specific exceptions may need verification |
| Desktop | CLI HUDs, optional notification service, keyboard bindings, Omarchy theme follow | No macOS build, AirDrop, camera mirror, terminal, or plugin marketplace |

The current source is an improvement on the 0.3 series. Production readiness
still requires successful CI, release artifact verification, and hands-on testing
on the desktop and hardware you intend to support. Screenshots and a source
build alone do not establish parity with another product.
