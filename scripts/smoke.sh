#!/usr/bin/env bash
set -euo pipefail

BIN="${BIN:-./target/debug/naarchy}"
if [[ ! -x "$BIN" ]]; then
  cargo build --bins
  BIN=./target/debug/naarchy
fi

# Isolate from a live user daemon so `toggle` is a negative test.
SMOKE_RT="$(mktemp -d /tmp/naarchy-smoke-rt.XXXXXX)"
trap 'rm -rf "$SMOKE_RT"' EXIT
export XDG_RUNTIME_DIR="$SMOKE_RT"

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

echo "smoke ok"
