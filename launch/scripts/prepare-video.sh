#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p launch/video/public/motion
cp launch/brand/naarchy-logo.svg launch/video/public/logo.svg
for name in home inbox clipboard calendar; do
  cp "docs/screenshots/v0.4/$name.png" "launch/video/public/$name.png"
done
cp docs/recordings/island-demo.mp4 launch/video/public/island-demo.mp4
cp launch/sources/omadesign-process.mp4 launch/video/public/omadesign-process.mp4
cp launch/artifacts/motion-frames/*.png launch/video/public/motion/
