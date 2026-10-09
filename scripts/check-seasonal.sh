#!/usr/bin/env bash
# Native seasonal verification on a separate software-rendered Wayland desktop.
set -euo pipefail
for program in sway swaymsg grim wtype dbus-run-session cargo; do
  command -v "$program" >/dev/null || { echo "Missing test tool: $program" >&2; exit 1; }
done
repo="$(cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo"
cargo test --locked --bins --no-run
mkdir -p /tmp/naarchy-seasonal-visual
test_root="$(mktemp -d /tmp/naarchy-seasonal-visual/run.XXXXXX)"
compositor_pid=""
output_dir="${1:-}"
cleanup() {
  if [[ -n "$compositor_pid" ]]; then
    kill "$compositor_pid" 2>/dev/null || true
    wait "$compositor_pid" 2>/dev/null || true
  fi
  if [[ -n "$output_dir" && -d "$test_root/captures" ]]; then
    mkdir -p "$output_dir"
    cp "$test_root/captures/"*.png "$output_dir/"
    cp "$test_root/sway.log" "$output_dir/"
  fi
  rm -rf "$test_root"
}
trap cleanup EXIT
export XDG_RUNTIME_DIR="$test_root"
unset HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY DISPLAY SWAYSOCK DBUS_SESSION_BUS_ADDRESS
cat > "$test_root/sway.conf" <<'EOF'
output HEADLESS-1 mode 1280x720 scale 1
seat seat0 fallback true
focus_follows_mouse no
xwayland disable
EOF
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
  sway -c "$test_root/sway.conf" > "$test_root/sway.log" 2>&1 &
compositor_pid=$!
for _ in {1..50}; do
  for socket in "$test_root"/wayland-*; do
    if [[ -S "$socket" ]]; then
      export WAYLAND_DISPLAY="${socket##*/}"
      break 2
    fi
  done
  kill -0 "$compositor_pid" 2>/dev/null || { cat "$test_root/sway.log" >&2; exit 1; }
  sleep 0.1
done
[[ -n "${WAYLAND_DISPLAY:-}" ]] || { cat "$test_root/sway.log" >&2; exit 1; }
for socket in "$test_root"/sway-ipc.*.sock; do
  [[ -S "$socket" ]] && export SWAYSOCK="$socket"
done
export GDK_BACKEND=wayland GSK_RENDERER=cairo GTK_A11Y=none
export GIO_USE_VFS=local GTK_USE_PORTAL=0
cat > "$test_root/dbus.conf" <<EOF
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=$test_root</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
export NAARCHY_NATIVE_CAPTURE_DIR="$test_root/captures"
for case in ${NAARCHY_NATIVE_CASES:-welcome reduced dismiss interrupt offdate fangs drips bat reduced-bat}; do
  export NAARCHY_NATIVE_SEASONAL="$case"
  export XDG_CONFIG_HOME="$test_root/$case/config"
  export XDG_DATA_HOME="$test_root/$case/data"
  export XDG_CACHE_HOME="$test_root/$case/cache"
  dbus-run-session --config-file "$test_root/dbus.conf" -- cargo test --locked --bins \
    ui::seasonal_tests::isolated_native_seasonal_lifecycle -- --exact --nocapture
done
if [[ -n "$output_dir" ]]; then
  echo "Native seasonal checks passed; previews: $output_dir"
else
  echo 'Native seasonal checks passed'
fi
