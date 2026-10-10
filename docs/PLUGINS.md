# Plugins

Plugins put live activities on the island: the ears around the notch where a
running timer or the current track already shows. A plugin is a small program
in any language. Naarchy starts it, it prints JSON lines, the island shows the
most important one.

> Status: proof of concept (plugin API 1). One slot, `live-activity`. The
> protocol may grow; `api` in the manifest lets Naarchy refuse plugins written
> for a different revision instead of misreading them.

## Package layout

```
~/.config/naarchy/plugins/<name>/
├── plugin.toml        # manifest
└── bin/<program>      # anything executable: script, binary
```

```toml
name = "t3-live"                 # a-z 0-9 - _, max 64, must match the directory
version = "0.1.0"
description = "Active T3 Code agent runs as island live activities"
api = 1                          # protocol revision
exec = "bin/t3-live"             # relative to the package; no shell, no ..
args = []                        # optional, max 16
slot = "live-activity"           # the only slot in api 1
restart = "on-failure"           # never | on-failure | always
```

Unknown manifest keys are errors, so a typo never silently changes behavior.

## Commands

```
naarchy plugin list                  installed packages, version, state, problems
naarchy plugin install DIR [--force] copy a local package directory
naarchy plugin remove NAME           delete the package and its private data
naarchy plugin disable NAME          keep it installed, do not start it
naarchy plugin enable NAME
naarchy plugin run NAME              run in the foreground and print island output
naarchy plugin path                  print the plugins directory
```

Install, remove, enable, and disable take effect the next time Naarchy starts
(`systemctl --user restart naarchy`). `install` copies only regular files and
directories, refuses symlinks and anything over 64 MiB, and gives the copy
owner-only permissions. Nothing is downloaded.

Set `features.plugins = false` in `config.toml` to start no plugins at all.

## Protocol (api 1)

Newline-delimited JSON. Lines over 16 KiB, unknown types, and malformed JSON
are ignored.

**Naarchy → plugin (stdin).** One line, then nothing:

```json
{"type":"hello","api":1,"naarchy":"0.5.2","plugin":"t3-live"}
```

When stdin reaches end of file, Naarchy has gone away: **exit**. Naarchy also
sends SIGTERM to running plugins when it shuts down.

**Plugin → Naarchy (stdout).**

```json
{"type":"activity","id":"run-42","icon":"","title":"naarchy","detail":"running · fix pill width","started_at":1791636747,"priority":40}
{"type":"clear","id":"run-42"}
{"type":"clear"}
{"type":"log","level":"warn","message":"server unavailable"}
```

| Field | Meaning |
|---|---|
| `id` | Your key for this activity (max 128 bytes). Sending the same `id` replaces it. |
| `title` | Required. Shown first; trimmed to 48 characters. |
| `detail` | Optional; trimmed to 96 characters. Control characters become spaces. |
| `icon` | Optional Nerd Font glyph (up to 4 characters). |
| `started_at` | Optional Unix seconds. Naarchy renders the elapsed time (`42s`, `12m`, `1h04m`) itself, so only write when something changes. |
| `priority` | 0–100, default 40. **50 or more** outranks parked files and music. |

Each plugin can show at most 8 activities; extras are ignored. When a plugin
exits, everything it showed is cleared. `log` lines go to Naarchy's log
(`RUST_LOG=info naarchy run`); stderr is logged at debug level.

### What the island shows

One activity at a time, by priority: finished timer > running timer > plugin
activity with priority 50+ > files in Inbox > playing music > other plugin
activity. Among plugin activities the highest `priority` wins, then the
longest-running one. `+N` means N more are waiting.

### Environment

| Variable | Value |
|---|---|
| `NAARCHY_API` | `1` |
| `NAARCHY_PLUGIN` | the plugin name |
| `NAARCHY_PLUGIN_DIR` | the package directory (also the working directory) |
| `NAARCHY_PLUGIN_DATA` | `~/.local/share/naarchy/plugins/<name>/`, owner-only, for tokens and caches |

The rest of your session environment is inherited.

## Trust and safety

Installing a plugin is the trust decision: it runs as you, like any program
you start. Naarchy's part is to keep that decision honest and the blast radius
small:

- The program must live inside its package (after resolving links), be a
  regular executable file owned by you, and not be group- or world-writable.
  The package directory and manifest get the same ownership check.
- It is executed directly, never through a shell, with a bounded argument list.
- It has no access to Naarchy's control socket, clipboard history, or shelf.
  It can only send bounded text to one display slot.
- Crashes restart with backoff (2 s doubling to 5 minutes; a run longer than a
  minute resets it). `restart = "never"` turns that off.

## Writing a plugin

A complete plugin in shell:

```sh
#!/bin/sh
read hello                                   # {"type":"hello",...}
while :; do
  n=$(pgrep -c cargo)
  if [ "$n" -gt 0 ]; then
    printf '{"type":"activity","id":"cargo","icon":"","title":"cargo","detail":"%s builds"}\n' "$n"
  else
    printf '{"type":"clear","id":"cargo"}\n'
  fi
  sleep 5
done
```

Test it without the daemon: `naarchy plugin install ./my-plugin && naarchy plugin run my-plugin`.

## Bundled example: T3 Code live activities

[`contrib/plugins/t3-live`](../contrib/plugins/t3-live/README.md) shows active
T3 Code agent threads (project, status, elapsed) from T3 Code's MCP server.

```bash
naarchy plugin install contrib/plugins/t3-live
systemctl --user restart naarchy
```
