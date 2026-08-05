#!/usr/bin/env bash
# release-local.sh - 本地签名打包；显式 --notarize 时提交 Apple 公证。
#
# 用法：
#   ./scripts/release-local.sh             # Developer ID 签名打包（不 notarize）
#   ./scripts/release-local.sh --notarize  # 真实发版（需 Developer ID 证书 + APP_STORE_CONNECT_* env）
#   ./scripts/release-local.sh --ad-hoc    # ad-hoc 签名测试（不 notarize，用 Packager.dev.toml）
#
# notarize 凭证复用 mobile 的 App Store Connect API Key（同属 2BF8G9AN4L）：
#   APP_STORE_CONNECT_API_KEY_ID / _ISSUER_ID / _KEY_PATH
# cargo-packager 读 APPLE_API_*，这里做映射。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info() { echo -e "${GREEN}[release-local]${NC} $*"; }
warn() { echo -e "${YELLOW}[release-local]${NC} $*"; }
die()  { echo -e "${RED}[release-local] error:${NC} $*" >&2; exit 1; }

AD_HOC=false
NOTARIZE=false
for arg in "$@"; do
  case "$arg" in
    --notarize) NOTARIZE=true ;;
    --ad-hoc) AD_HOC=true ;;
    *) die "未知参数: $arg" ;;
  esac
done

if [ "$AD_HOC" = true ] && [ "$NOTARIZE" = true ]; then
  die "--ad-hoc 与 --notarize 不能同时使用"
fi

# 只接受本次调用新生成/更新的 DMG，避免把 build/ 中的历史包误报为成功。
PACKAGE_MARKER=$(mktemp "${TMPDIR:-/tmp}/norma-release-dmg.XXXXXX")
trap 'rm -f "$PACKAGE_MARKER"' EXIT

if [ "$AD_HOC" = true ]; then
  OUTPUT_DIR="build/dev"
else
  OUTPUT_DIR="build"
fi

# 1. 前置：codesign readiness
bash scripts/check-codesign-readiness.sh

# 2. git lfs pull（确保 wallpaper MP4 真实字节在树里）
command -v git-lfs >/dev/null 2>&1 || die "git-lfs 未安装，见 docs/references/packaging.md"
info "git lfs pull"
git lfs pull

if [ "$AD_HOC" = true ]; then
  # ad-hoc 测试：不 notarize，用 Packager.dev.toml
  [ -f target/release/norma-os ] || die "target/release/norma-os 不存在，先 cargo build --release -p app"
  info "ad-hoc 打包（不 notarize）"
  env -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
      -u APPLE_KEYCHAIN_PROFILE -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID \
      -u APPLE_CERTIFICATE -u APPLE_CERTIFICATE_PASSWORD \
      cargo-packager -c Packager.dev.toml
elif [ "$NOTARIZE" = true ]; then
  # 真实发版：映射 APP_STORE_CONNECT_* -> APPLE_API_*，启用 notarize
  : "${APP_STORE_CONNECT_API_KEY_ID:?需设置 APP_STORE_CONNECT_API_KEY_ID（同 mobile）}"
  : "${APP_STORE_CONNECT_API_ISSUER_ID:?需设置 APP_STORE_CONNECT_API_ISSUER_ID}"
  : "${APP_STORE_CONNECT_API_KEY_PATH:?需设置 APP_STORE_CONNECT_API_KEY_PATH}"
  export APPLE_API_KEY="$APP_STORE_CONNECT_API_KEY_ID"
  export APPLE_API_ISSUER="$APP_STORE_CONNECT_API_ISSUER_ID"
  export APPLE_API_KEY_PATH="$APP_STORE_CONNECT_API_KEY_PATH"

  # 检查 Developer ID Application (2BF8G9AN4L) 证书在钥匙串
  security find-identity -v -p codesigning | grep -q "Developer ID Application.*2BF8G9AN4L" \
    || die "未找到 2BF8G9AN4L 的 Developer ID Application 证书（阶段 0 未完成）"

  info "签名 + notarize + staple"
  # env -u 清掉 AppleId 方式（APPLE_ID/PASSWORD/TEAM_ID 可能来自 ~/.zshrc 的个人账号），
  # 强制 cargo-packager notarize_auth() 走 ApiKey 方式（APPLE_API_* = 公司 2BF8G9AN4L key）。
  # 直接调用外部二进制，避免 `cargo run` wrapper 被开发 App 的 target runner 截获。
  env -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID -u APPLE_KEYCHAIN_PROFILE \
      cargo-packager --release -c Packager.toml
else
  # 默认：Developer ID 签名打包，但显式移除 notary 凭证，避免 cargo-packager 自动公证。
  security find-identity -v -p codesigning | grep -q "Developer ID Application.*2BF8G9AN4L" \
    || die "未找到 2BF8G9AN4L 的 Developer ID Application 证书（阶段 0 未完成）"

  info "签名打包（不 notarize；需要公证请传 --notarize）"
  env -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
      -u APPLE_KEYCHAIN_PROFILE -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID \
      cargo-packager --release -c Packager.toml
fi

# 3. 验证产物
# 真实发版产物在 build/Norma OS.app（Packager.toml out-dir="build"，product-name="Norma OS"），
# ad-hoc 测试产物在 build/dev/Norma OS.app（Packager.dev.toml out-dir="build/dev"）。
APP="$OUTPUT_DIR/Norma OS.app"
[ -d "$APP" ] || die "未找到本次产物 $APP"
info "验证 $APP"
codesign --verify --deep --strict --verbose=2 "$APP"
if [ "$NOTARIZE" = true ]; then
  spctl --assess --type execute --verbose "$APP" || die "spctl 拒绝真实签名 .app（--notarize 后应 accepted）"
  xcrun stapler validate "$APP" || die "stapler 无公证票据（--notarize 必须有）"
else
  spctl --assess --type execute --verbose "$APP" || warn "spctl 在未公证/ad-hoc 下可能失败，--notarize 后应 accepted"
  xcrun stapler validate "$APP" || warn "stapler 在未公证/ad-hoc 下无票据"
fi

DMG=""
while IFS= read -r candidate; do
  if [ -z "$DMG" ] || [ "$candidate" -nt "$DMG" ]; then
    DMG="$candidate"
  fi
done < <(find "$OUTPUT_DIR" -maxdepth 1 -type f -name '*.dmg' -newer "$PACKAGE_MARKER" -print)

[ -n "$DMG" ] || die "未找到本次新生成的 DMG（目录: $OUTPUT_DIR）"
if [ "$NOTARIZE" = true ]; then
  info "提交最终 DMG 公证并 staple"
  xcrun notarytool submit "$DMG" \
    --key "$APP_STORE_CONNECT_API_KEY_PATH" \
    --key-id "$APP_STORE_CONNECT_API_KEY_ID" \
    --issuer "$APP_STORE_CONNECT_API_ISSUER_ID" \
    --wait
  xcrun stapler staple "$DMG"
fi

info "验证 $DMG"
hdiutil verify "$DMG"
if [ "$NOTARIZE" = true ]; then
  spctl --assess --type open --context context:primary-signature --verbose "$DMG" \
    || die "Gatekeeper 拒绝最终 DMG"
  xcrun stapler validate "$DMG" || die "最终 DMG 缺少有效公证票据"
fi

DMG_SHA256=$(shasum -a 256 "$DMG" | awk '{print $1}')
DMG_SIZE_BYTES=$(stat -f '%z' "$DMG")
DMG_CHECKSUM_FILE="${DMG}.sha256"
printf '%s  %s\n' "$DMG_SHA256" "$(basename "$DMG")" > "$DMG_CHECKSUM_FILE"

info "Done"
info "App: $APP"
info "DMG: $DMG"
info "Size: $DMG_SIZE_BYTES bytes"
info "SHA-256: $DMG_SHA256"
info "Checksum: $DMG_CHECKSUM_FILE"
