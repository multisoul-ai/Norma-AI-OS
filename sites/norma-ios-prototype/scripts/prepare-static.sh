#!/usr/bin/env bash
# Assemble a slim static root that preserves relative paths from docs/:
#   /prototypes/*  -> CSS/JS/HTML/assets
#   /product-specs/norma-ios-prototypes-2026-09-26/*  -> nine-screen PNGs
#   /design-docs/assets/norma-realtime/cua-companion-larger-v54.png  -> realtime portrait
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SITE="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$SITE/dist"

rm -rf "$DIST"
mkdir -p \
  "$DIST/prototypes" \
  "$DIST/product-specs/norma-ios-prototypes-2026-09-26" \
  "$DIST/design-docs/assets/norma-realtime"

cp -a "$ROOT/docs/prototypes/." "$DIST/prototypes/"
cp -a "$ROOT/docs/product-specs/norma-ios-prototypes-2026-09-26/." \
  "$DIST/product-specs/norma-ios-prototypes-2026-09-26/"
cp -a "$ROOT/docs/design-docs/assets/norma-realtime/cua-companion-larger-v54.png" \
  "$DIST/design-docs/assets/norma-realtime/"

cat > "$DIST/index.html" <<'HTML'
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta http-equiv="refresh" content="0; url=/prototypes/norma-ios-workspace.html">
  <title>Norma iOS Prototype</title>
</head>
<body>
  <p><a href="/prototypes/norma-ios-workspace.html">打开九屏工作空间原型</a></p>
  <p><a href="/prototypes/norma-realtime.html">打开实时频道原型</a></p>
</body>
</html>
HTML

# Copy vercel.json beside dist contents via deploy cwd=dist: vercel reads parent when --local-config set.
cp "$SITE/vercel.json" "$DIST/vercel.json"

python3 - "$DIST" <<'PY'
import sys
from pathlib import Path
root = Path(sys.argv[1])
need = [
    "prototypes/norma-ios-workspace.html",
    "prototypes/norma-ios-workspace.js",
    "prototypes/norma-ios-workspace.css",
    "prototypes/norma-channel-model.js",
    "prototypes/norma-realtime.html",
    "prototypes/norma-channel-prototype.css",
    "product-specs/norma-ios-prototypes-2026-09-26/W01-workspace-list.png",
    "product-specs/norma-ios-prototypes-2026-09-26/W09-acceptance.png",
    "design-docs/assets/norma-realtime/cua-companion-larger-v54.png",
    "vercel.json",
]
missing = [p for p in need if not (root / p).exists()]
if missing:
    raise SystemExit(f"prepare-static missing: {missing}")
w01 = (root / "prototypes").joinpath(
    "../product-specs/norma-ios-prototypes-2026-09-26/W01-workspace-list.png"
).resolve()
portrait = (root / "prototypes").joinpath(
    "../design-docs/assets/norma-realtime/cua-companion-larger-v54.png"
).resolve()
if not w01.exists() or not portrait.exists():
    raise SystemExit("relative path resolution failed for screen or portrait assets")
print(f"prepare-static ok -> {root} ({sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) / 1024 / 1024:.1f} MiB)")
PY
