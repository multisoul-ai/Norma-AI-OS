#!/usr/bin/env bash
# Mechanized constraint: macOS codesign/notarize 所需配置就绪校验。
#
# Scope: entitlements 文件、Info.plist 关键键、Packager 配置字段。
#
# On failure: 打印缺失项，退出码 1。挂 pre-commit 与 CI。
#
# Background: docs/design-docs/2026-07-05-developer-id-notarize-design.md
#
# Linux / 无 Apple 工具链时：用 Python plistlib 做等价合法性与关键键检查，
# 避免 cloud agent 等非 macOS 环境因缺少 plutil/PlistBuddy 误杀无关提交。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENT="$ROOT/packaging/macos/NormaOS.entitlements"
INFO="$ROOT/packaging/macos/Info.plist"
PKG="$ROOT/Packager.toml"
PKG_DEV="$ROOT/Packager.dev.toml"

fail() { echo "FAIL: $*" >&2; exit 1; }

plist_has_key() {
  local file="$1"
  local key="$2"
  python3 - "$file" "$key" <<'PY'
import plistlib, sys
path, key = sys.argv[1], sys.argv[2]
with open(path, "rb") as fh:
    data = plistlib.load(fh)
if key not in data:
    raise SystemExit(1)
PY
}

# 1. entitlements 存在且为合法 plist
[ -f "$ENT" ] || fail "entitlements 文件缺失: $ENT"
if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$ENT" >/dev/null || fail "entitlements 不是合法 plist: $ENT"
else
  python3 - "$ENT" <<'PY' || fail "entitlements 不是合法 plist: $ENT"
import plistlib, sys
with open(sys.argv[1], "rb") as fh:
    plistlib.load(fh)
PY
fi

# 2. Info.plist 关键键
[ -f "$INFO" ] || fail "Info.plist 缺失: $INFO"
if [ -x /usr/libexec/PlistBuddy ]; then
  /usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$INFO" >/dev/null 2>&1 \
    || fail "Info.plist 缺 LSMinimumSystemVersion"
  /usr/libexec/PlistBuddy -c "Print :LSApplicationCategoryType" "$INFO" >/dev/null 2>&1 \
    || fail "Info.plist 缺 LSApplicationCategoryType"
  /usr/libexec/PlistBuddy -c "Print :NSMicrophoneUsageDescription" "$INFO" >/dev/null 2>&1 \
    || fail "Info.plist 缺 NSMicrophoneUsageDescription"
  /usr/libexec/PlistBuddy -c "Print :NSSpeechRecognitionUsageDescription" "$INFO" >/dev/null 2>&1 \
    || fail "Info.plist 缺 NSSpeechRecognitionUsageDescription"
else
  plist_has_key "$INFO" "LSMinimumSystemVersion" \
    || fail "Info.plist 缺 LSMinimumSystemVersion"
  plist_has_key "$INFO" "LSApplicationCategoryType" \
    || fail "Info.plist 缺 LSApplicationCategoryType"
  plist_has_key "$INFO" "NSMicrophoneUsageDescription" \
    || fail "Info.plist 缺 NSMicrophoneUsageDescription"
  plist_has_key "$INFO" "NSSpeechRecognitionUsageDescription" \
    || fail "Info.plist 缺 NSSpeechRecognitionUsageDescription"
fi

# 3. Packager.toml 含 entitlements + resources 字段
grep -q '^entitlements' "$PKG" || fail "Packager.toml 缺 entitlements 字段"
grep -q '^resources' "$PKG" || fail "Packager.toml 缺 resources 字段"
grep -Fq 'frameworks = ["build/sparkle/Sparkle.framework"]' "$PKG" \
  || fail "Packager.toml 缺 Sparkle.framework 嵌入配置"
grep -Fq 'info-plist-path = "build/packaging/Info.plist"' "$PKG" \
  || fail "Packager.toml 未使用 release 专用生成 plist"

# 4. Packager.dev.toml 存在且 signing-identity = "-"
[ -f "$PKG_DEV" ] || fail "Packager.dev.toml 缺失: $PKG_DEV"
grep -q 'signing-identity = "-"' "$PKG_DEV" \
  || fail "Packager.dev.toml 缺 signing-identity = \"-\"（ad-hoc 测试签名）"

echo "OK: codesign readiness check passed"
