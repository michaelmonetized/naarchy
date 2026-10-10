# Roadmap

Direction, not promises. Each item links to its tracking issue when one exists.

## Now

- **Plugin package system** — plugins as packages in
  `~/.config/naarchy/plugins/<name>/` with a `plugin.toml` manifest, any-language
  programs speaking JSON lines over stdio, a live-activity slot on the island,
  and `naarchy plugin install | list | remove | enable | disable | run`.
  Proof of concept ships with a T3 Code live-activities plugin.
  See [docs/PLUGINS.md](docs/PLUGINS.md) and the tracking issue
  ([#4](https://github.com/michaelmonetized/naarchy/issues/4)).

## Next (plugin system)

- Hot reload: start/stop plugins when packages change, without restarting Naarchy.
- Click actions: a plugin activity can declare an action Naarchy runs on click
  (open URL, or a verb sent back to the plugin on stdin).
- A Home widget slot and a plugin page listing every activity, not just the headline.
- Preferences page: enable/disable plugins and show their status and last log line.
- `naarchy doctor` reports plugin problems.
- Optional sandboxing (bubblewrap / Landlock) for plugins that opt in.

## Later

- Plugin index / `naarchy plugin install <git url>` with pinned revisions and checksums.
- D-Bus surface for long-running services that prefer it over stdio.
