#!/usr/bin/env bash
# release-local.sh - 本地签名 + 公证发版。
#
# 用法：
#   ./scripts/release-local.sh             # 真实发版（需 Developer ID 证书 + APP_STORE_CONNECT_* env）
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
for arg in "$@"; do
  case "$arg" in
    --ad-hoc) AD_HOC=true ;;
    *) die "未知参数: $arg" ;;
  esac
done

# 1. 前置：codesign readiness
bash scripts/check-codesign-readiness.sh

# 2. git lfs pull（确保 wallpaper MP4 真实字节在树里）
command -v git-lfs >/dev/null 2>&1 || die "git-lfs 未安装，见 docs/references/packaging.md"
info "git lfs pull"
git lfs pull

if [ "$AD_HOC" = true ]; then
  # ad-hoc 测试：不 notarize，用 Packager.dev.toml
  [ -f target/release/multisoul ] || die "target/release/multisoul 不存在，先 cargo build --release -p app"
  info "ad-hoc 打包（不 notarize）"
  env -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
      -u APPLE_KEYCHAIN_PROFILE -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID \
      -u APPLE_CERTIFICATE -u APPLE_CERTIFICATE_PASSWORD \
      cargo packager -c Packager.dev.toml
else
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
  env -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID -u APPLE_KEYCHAIN_PROFILE \
      cargo packager --release -c Packager.toml
fi

# 3. 验证产物
# 真实发版产物在 build/Multisoul.app（Packager.toml out-dir="build"），
# ad-hoc 测试产物在 build/dev/Multisoul.app（Packager.dev.toml out-dir="build/dev"）。
# 用循环而非 `ls -d a b | head -1`：当 a 不存在时 ls 退出非零，
# 在 `set -euo pipefail` 下会直接终止脚本，跳过整段验证。
APP=""
for p in build/Multisoul.app build/dev/Multisoul.app; do
  [ -d "$p" ] && APP="$p" && break
done
[ -n "$APP" ] || die "未找到产物 Multisoul.app"
info "验证 $APP"
codesign --verify --deep --strict --verbose=2 "$APP"
if [ "$AD_HOC" = true ]; then
  spctl --assess --type execute --verbose "$APP" || warn "spctl 在 ad-hoc 下会失败，真实签名 + notarize + staple 后应 accepted"
  xcrun stapler validate "$APP" || warn "stapler 在 ad-hoc 下无票据"
else
  spctl --assess --type execute --verbose "$APP" || die "spctl 拒绝真实签名 .app（应 accepted）"
  xcrun stapler validate "$APP" || die "stapler 无公证票据（真实发版必须有）"
fi

info "Done: $APP"
