# CLI

Every feature is a verb. The running daemon owns GTK. A second process
forwards JSON over `$XDG_RUNTIME_DIR/naarchy.sock` and exits.

Exit codes: **0** success (including `naarchy run` when a daemon is already
live), **1** daemon missing / bind failure / missing `WAYLAND_DISPLAY`,
**2** usage error (unknown tab, missing args).

## Verbs

```
naarchy run                     launch (foreground). Requires WAYLAND_DISPLAY.
naarchy toggle                  expand/collapse
naarchy expand | collapse
naarchy tab <name>
naarchy hud <kind> [value|+N|-N] [--icon GLYPH] [--label TEXT]
naarchy notify SUMMARY [BODY]
naarchy shelf add PATH…
naarchy shelf list
naarchy shelf clear | remove ID
naarchy clipboard paste-last
naarchy timer <30s|25m|1h> | stop
naarchy quit
naarchy install-binds
```

Aliases: `daemon` = `run`. `clip` = `clipboard`. `copy-last` = `paste-last`.

## Tabs

Canonical: `home` · `inbox` · `clipboard` · `widgets` · `calendar`.

| Token | Tab |
|---|---|
| `home`, `start` | Home |
| `inbox`, `files`, `shelf`, `drops` | Inbox |
| `clipboard`, `clip` | Clipboard |
| `widgets`, `drawer`, `grid` | Widgets |
| `calendar`, `cal` | Calendar |

Unknown names (including `media`, `settings`, `timer`) → stderr + **exit 2**.
`naarchy tab` with no name is also exit 2.

Generated binds print **canonical** names (`tab inbox`, not `tab shelf`).

## HUD

Kinds: `volume`, `brightness`, `mic`, `battery`, `caps`, `custom`.

- A number is an absolute value (0–100).
- `+N` / `-N` is a step from the last shown value.
- No value and no step: the CLI shells out to pamixer / wpctl / brightnessctl /
  `/sys/class/power_supply`. **`auto` is not a parser token.** The bind line
  `naarchy hud volume auto` works because `auto` is ignored and detect runs.

`--icon` and `--label` apply to `custom` (and override the defaults).

There is no background watcher. Bind your real volume/brightness tools, then
call `naarchy hud`.

## Shelf

`add` / `clear` / `remove` go to the daemon (fire-and-forget).

`list` does **not**. It reads `$XDG_DATA_HOME/naarchy/shelf.json` in the CLI
process and prints one **pretty JSON array** of shelf items. Missing file → `[]`.
Invalid JSON → exit 1. No daemon required.

## Timer

`25m` → 1500 seconds. Units: `s`/`sec`/`secs`, `m`/`min`/`mins`,
`h`/`hr`/`hour`/`hours`. Bare number = seconds. `stop` (alias `reset`)
clears the running countdown.

## Notify

Paints a banner in the HUD. Does **not** require `features.notifications`.
Does not talk to mako.

## Single instance

`naarchy run` while the socket is live: prints `naarchy already running`, exit 0.
`quit` may leave a stale socket; the next `run` unlinks on connect-fail.
