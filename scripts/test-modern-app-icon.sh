#!/usr/bin/env bash
# 场景：新版 macOS 使用原生满幅图标，旧版继续使用带光学边距的 ICNS。
#
# 数据构造（含关键数值推导）：
#   artwork = 1024 × 1024；覆盖率 = 1024 / 1024 = 100%，alpha 必须全为 1。
#   modern name = NormaIcon；legacy file = AppIcon.icns，名称不同以免覆盖旧图标。
#   configs = release + dev，两条打包路径都必须包含 Assets.car 和编译步骤。
#
# 执行过程：
#   1. 检查两条打包路径与 Info.plist → 原生资源能被系统找到。
#   2. 检查 Icon Composer 配置和源图 → 没有缩放、透明边距或二次圆角。
#      与共享满幅母版的 1024px 缩略图逐像素对照 → 保留品牌图案且可重建。
#   3. 用真实 actool 编译临时资源 → 验证不是仅仅复制一个无效 CAR 文件。
#   4. assetutil 读取实际 CAR 内容 → 原生图标以 NormaIcon 的名字存在。
#
# 预期结果：
#   - 正：release/dev 都编译并打包 Assets.car，CFBundleIconName 指向 NormaIcon。
#   - 正：原生图标保留现有品牌图案，完整覆盖正方形底板。
#   - 正：CAR 含原生图标组与图案资源。
#   - 负：不移除旧版 AppIcon.icns、不保留缩小或透明的原生图层。

set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

for config in "$repo_root/Packager.toml" "$repo_root/Packager.dev.toml"; do
  if ! grep -Fq '"build/icons/Assets.car"' "$config"; then
    fail "$(basename "$config") must package the native icon Assets.car"
  fi
  if ! grep -Fq 'scripts/prepare-app-icon.sh' "$config"; then
    fail "$(basename "$config") must compile the native icon before packaging"
  fi
  if ! grep -Fq 'icons = ["assets/AppIcon.icns"]' "$config"; then
    fail "$(basename "$config") must retain AppIcon.icns for older macOS"
  fi
done

native_name=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconName' \
  "$repo_root/packaging/macos/Info.plist" 2>/dev/null || true)
[ "$native_name" = NormaIcon ] || fail "CFBundleIconName must select NormaIcon"

icon_source="$repo_root/assets/NormaIcon.icon/Assets/Artwork.png"
magick_bin=$(command -v magick || true)
[ -n "$magick_bin" ] || fail "ImageMagick is required to measure the icon source"
[ -f "$icon_source" ] || fail "the native full-bleed icon source must exist"

geometry=$("$magick_bin" identify -format '%wx%h' "$icon_source")
[ "$geometry" = 1024x1024 ] || fail "native artwork must be 1024x1024, got $geometry"
alpha_min=$("$magick_bin" "$icon_source" -alpha extract -format '%[fx:minima]' info:)
[ "$alpha_min" = 1 ] || fail "native artwork must be fully opaque, got alpha minimum $alpha_min"

python3 - "$repo_root/assets/NormaIcon.icon/icon.json" <<'PY'
import json
import sys

with open(sys.argv[1]) as source:
    icon = json.load(source)
assert icon["supported-platforms"]["squares"] == ["macOS"], "native icon must target macOS"
assert len(icon["groups"]) == 1, "native icon must preserve one existing artwork group"
group = icon["groups"][0]
assert len(group["layers"]) == 1, "native icon must preserve the existing composite artwork"
layer = group["layers"][0]
assert layer["image-name"] == "Artwork.png", "native icon must reference the full-bleed artwork"
assert layer.get("position", {}).get("scale", 1) == 1, "native artwork must not be inset or enlarged"
assert layer.get("position", {}).get("translation-in-points", [0, 0]) == [0, 0], "native artwork must remain centered"
assert layer.get("glass") is False, "brand artwork must retain its original colors"
assert group["translucency"]["enabled"] is False, "background must not become transparent"
PY

temporary_output=$(mktemp -d "${TMPDIR:-/tmp}/norma-native-icon-test.XXXXXX")
trap 'rm -rf "$temporary_output"' EXIT
"$magick_bin" "$repo_root/assets/app-icon-ios-source.png" \
  -filter Lanczos -resize 1024x1024 "$temporary_output/expected-artwork.png"
if ! "$magick_bin" compare -metric AE "$icon_source" \
  "$temporary_output/expected-artwork.png" null: 2> "$temporary_output/pixel-difference.txt"; then
  fail "native artwork must match the shared full-bleed source; run scripts/render-app-icon.sh"
fi
bash "$repo_root/scripts/prepare-app-icon.sh" "$temporary_output"
[ -s "$temporary_output/Assets.car" ] || fail "actool must produce a nonempty Assets.car"

xcrun assetutil --info "$temporary_output/Assets.car" > "$temporary_output/assets.json"
python3 - "$temporary_output/assets.json" <<'PY'
import json
import sys

with open(sys.argv[1]) as source:
    assets = json.load(source)
assert any(item.get("Name") == "NormaIcon" and item.get("AssetType") == "IconImageStack" for item in assets), "CAR must contain the native NormaIcon stack"
assert any(item.get("Name") == "NormaIcon/Brand artwork" and item.get("AssetType") == "IconGroup" for item in assets), "CAR must contain the native artwork group"
assert any(item.get("Name") == "NormaIcon" and item.get("AssetType") == "Icon Image" for item in assets), "CAR must contain NormaIcon fallback renditions"
artwork = next(item for item in assets if item.get("Name") == "NormaIcon_Assets/Artwork" and item.get("AssetType") == "Image")
assert artwork["Opaque"] is True, "compiled artwork must not have transparent padding"
assert (artwork["PixelWidth"], artwork["PixelHeight"]) == (1024, 1024), "compiled artwork must fill the native 1024px canvas"
assert not any(item.get("Name") == "AppIcon" for item in assets), "modern CAR must not replace the legacy AppIcon name"
PY

echo "modern app icon test passed"
