#!/usr/bin/env bash
set -euo pipefail

BIN="${BIN:-./target/debug/naarchy}"
if [[ ! -x "$BIN" ]]; then
  cargo build --locked --bins
  BIN=./target/debug/naarchy
fi

# Isolate from a live user daemon so `toggle` is a negative test.
SMOKE_RT="$(mktemp -d /tmp/naarchy-smoke-rt.XXXXXX)"
trap 'rm -rf "$SMOKE_RT"' EXIT
export XDG_RUNTIME_DIR="$SMOKE_RT"
export XDG_CONFIG_HOME="$SMOKE_RT/config"
export XDG_DATA_HOME="$SMOKE_RT/data"
export XDG_CACHE_HOME="$SMOKE_RT/cache"

"$BIN" --version | grep -E '^naarchy [0-9]+\.[0-9]+\.[0-9]+'
"$BIN" doctor >/dev/null
[[ "$("$BIN" shelf list)" == '[]' ]]

for duration in 0 999999999999999999999999h invalid; do
  set +e
  "$BIN" timer "$duration" >/dev/null 2>&1
  ec=$?
  set -e
  [[ "$ec" -eq 2 ]]
done

# help lists the real tabs (grep the tab *usage line* so a later "media widget"
# one-liner cannot false-positive)
tab_line="$("$BIN" --help | grep -E '^  naarchy tab ')"
echo "$tab_line" | grep -F 'home|inbox|clipboard|widgets|calendar'
if echo "$tab_line" | grep -Eq 'media|settings'; then
  echo "help tab line still advertises media/settings" >&2
  exit 1
fi

# naarchy tab (no name) already exits 2
set +e
"$BIN" tab >/dev/null 2>&1
ec=$?
set -e
[[ "$ec" -eq 2 ]]

# naarchy toggle with no socket already exits 1 and prints the hint
set +e
out="$("$BIN" toggle 2>&1)"
ec=$?
set -e
[[ "$ec" -eq 1 ]]
echo "$out" | grep -q 'daemon not running'

# unknown tab → exit 2
set +e
"$BIN" tab nosuch >/dev/null 2>&1
ec=$?
set -e
[[ "$ec" -eq 2 ]]

set +e
"$BIN" tab media >/dev/null 2>&1
ec=$?
set -e
[[ "$ec" -eq 2 ]]

binds="$("$BIN" install-binds)"
echo "$binds" | grep -F 'naarchy tab inbox'
if echo "$binds" | grep -Fq 'tab shelf'; then
  echo "install-binds still prints tab shelf" >&2
  exit 1
fi
echo "$binds" | grep -F 'layerrule = blur, naarchy'
echo "$binds" | grep -F 'dbus-update-activation-environment'

# Exercise installation in a temporary staging tree, never the real desktop.
DESTDIR="$SMOKE_RT/install" PREFIX=/usr XDG_CONFIG_HOME=/home/test/.config \
  bash scripts/install.sh "$BIN" >/dev/null
[[ -x "$SMOKE_RT/install/usr/bin/naarchy" ]]
[[ -f "$SMOKE_RT/install/usr/share/icons/hicolor/scalable/apps/app.naarchy.Naarchy.svg" ]]
grep -Fq 'ExecStart="/usr/bin/naarchy" run' "$SMOKE_RT/install/home/test/.config/systemd/user/naarchy.service"

# Plugin packages: install, list, disable, remove in the isolated config.
"$BIN" plugin list | grep -F 'No plugins'
"$BIN" plugin install contrib/plugins/t3-live >/dev/null
"$BIN" plugin list | grep -E '^t3-live +0\.1\.0 +enabled +live-activity'
"$BIN" plugin disable t3-live >/dev/null
"$BIN" plugin list | grep -E '^t3-live .* disabled '
set +e
"$BIN" plugin remove ../etc >/dev/null 2>&1
ec=$?
set -e
[[ "$ec" -eq 2 ]]
"$BIN" plugin remove t3-live >/dev/null
"$BIN" plugin list | grep -F 'No plugins'
if command -v python3 >/dev/null; then
  PYTHONDONTWRITEBYTECODE=1 python3 contrib/plugins/t3-live/test_t3_live.py
fi

echo "smoke ok"
