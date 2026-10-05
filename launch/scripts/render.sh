#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../video"
mkdir -p ../artifacts/delivery ../artifacts/render-tmp
export TMPDIR="$PWD/../artifacts/render-tmp"
for pair in Release:naarchy-release-1080p ReleaseSquare:naarchy-release-square ReleaseVertical:naarchy-release-vertical MakingOf:made-in-omadesign MakingOfVertical:made-in-omadesign-vertical; do
  bun x remotion render src/index.tsx "${pair%%:*}" "../artifacts/delivery/${pair#*:}.mp4" --browser-executable="${REMOTION_BROWSER:-/usr/bin/chromium}" --codec=h264 --crf=18 --log=error
done
