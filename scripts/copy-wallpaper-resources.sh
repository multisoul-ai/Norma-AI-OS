#!/usr/bin/env bash
# 把内置壁纸资源拷进 macOS app bundle 的 Contents/Resources/wallpapers/。
# 用法: copy-wallpaper-resources.sh <bundle_resources_dir>
set -euo pipefail

RES_DIR="${1:?usage: $0 <bundle_resources_dir>}"
SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)/crates/app/assets/wallpapers"

if [ ! -d "$SRC_DIR" ]; then
    echo "error: wallpaper source dir not found: $SRC_DIR" >&2
    exit 1
fi

mkdir -p "$RES_DIR/wallpapers"
cp -R "$SRC_DIR/." "$RES_DIR/wallpapers/"
echo "copied wallpapers -> $RES_DIR/wallpapers"
