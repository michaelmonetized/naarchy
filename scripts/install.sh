#!/usr/bin/env bash
# Install a built binary, launcher, icon, and user unit. Never starts a service.
set -euo pipefail

source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
default_binary="$source_root/target/release/naarchy"
[[ -x "$default_binary" ]] || default_binary="$source_root/bin/naarchy"
source_binary="${1:-$default_binary}"
install_prefix="${PREFIX:-$HOME/.local}"
install_config="${XDG_CONFIG_HOME:-$HOME/.config}"
install_stage="${DESTDIR:-}"

if [[ "$#" -gt 1 || ! -x "$source_binary" ]]; then
  printf 'Usage: %s [path/to/naarchy]\nBuild first with: cargo build --release --locked\n' "$0" >&2
  exit 2
fi
for path in "$install_prefix" "$install_config"; do
  if [[ "$path" != /* || "$path" == *$'\n'* || "$path" == *$'\r'* ]]; then
    printf 'Installation paths must be absolute and contain no line breaks.\n' >&2
    exit 2
  fi
done

install_bin="$install_prefix/bin/naarchy"
install -Dm755 "$source_binary" "$install_stage$install_bin"
install -Dm644 "$source_root/contrib/app.naarchy.Naarchy.svg" \
  "$install_stage$install_prefix/share/icons/hicolor/scalable/apps/app.naarchy.Naarchy.svg"

# Escape quoted desktop/systemd executable paths, including literal percent signs.
quoted_bin="${install_bin//\\/\\\\}"
quoted_bin="${quoted_bin//\"/\\\"}"
quoted_bin="${quoted_bin//%/%%}"
mkdir -p "$install_stage$install_prefix/share/applications" "$install_stage$install_config/systemd/user"
while IFS= read -r line; do
  case "$line" in
    Exec=*) printf 'Exec="%s" run\n' "$quoted_bin" ;;
    TryExec=*) printf 'TryExec=%s\n' "$install_bin" ;;
    *) printf '%s\n' "$line" ;;
  esac
done < "$source_root/contrib/naarchy.desktop" > "$install_stage$install_prefix/share/applications/naarchy.desktop"
while IFS= read -r line; do
  case "$line" in
    ExecStart=*) printf 'ExecStart="%s" run\n' "$quoted_bin" ;;
    *) printf '%s\n' "$line" ;;
  esac
done < "$source_root/contrib/naarchy.service" > "$install_stage$install_config/systemd/user/naarchy.service"

if [[ -z "$install_stage" ]]; then
  command -v update-desktop-database >/dev/null && update-desktop-database "$install_prefix/share/applications" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$install_prefix/share/icons/hicolor" >/dev/null 2>&1 || true
fi
printf 'Installed Naarchy to %s\n' "$install_stage$install_bin"
printf 'Start: %s run\n' "$install_bin"
printf 'Autostart after importing your Wayland environment: systemctl --user daemon-reload && systemctl --user enable --now naarchy.service\n'
