#!/usr/bin/env bash
# Mechanized constraint: macOS codesign/notarize 所需配置就绪校验。
#
# Scope: entitlements 文件、Info.plist 关键键、Packager 配置字段。
#
# On failure: 打印缺失项，退出码 1。挂 pre-commit 与 CI。
#
# Background: docs/design-docs/2026-07-05-developer-id-notarize-design.md

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENT="$ROOT/packaging/macos/NormaOS.entitlements"
INFO="$ROOT/packaging/macos/Info.plist"
PKG="$ROOT/Packager.toml"
PKG_DEV="$ROOT/Packager.dev.toml"

fail() { echo "FAIL: $*" >&2; exit 1; }

# 1. entitlements 存在且为合法 plist
[ -f "$ENT" ] || fail "entitlements 文件缺失: $ENT"
plutil -lint "$ENT" >/dev/null || fail "entitlements 不是合法 plist: $ENT"

# 2. Info.plist 关键键
[ -f "$INFO" ] || fail "Info.plist 缺失: $INFO"
/usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$INFO" >/dev/null 2>&1 \
  || fail "Info.plist 缺 LSMinimumSystemVersion"
/usr/libexec/PlistBuddy -c "Print :LSApplicationCategoryType" "$INFO" >/dev/null 2>&1 \
  || fail "Info.plist 缺 LSApplicationCategoryType"
/usr/libexec/PlistBuddy -c "Print :NSMicrophoneUsageDescription" "$INFO" >/dev/null 2>&1 \
  || fail "Info.plist 缺 NSMicrophoneUsageDescription"
/usr/libexec/PlistBuddy -c "Print :NSSpeechRecognitionUsageDescription" "$INFO" >/dev/null 2>&1 \
  || fail "Info.plist 缺 NSSpeechRecognitionUsageDescription"

# 3. Packager.toml 含 entitlements + resources 字段
grep -q '^entitlements' "$PKG" || fail "Packager.toml 缺 entitlements 字段"
grep -q '^resources' "$PKG" || fail "Packager.toml 缺 resources 字段"

# 4. Packager.dev.toml 存在且 signing-identity = "-"
[ -f "$PKG_DEV" ] || fail "Packager.dev.toml 缺失: $PKG_DEV"
grep -q 'signing-identity = "-"' "$PKG_DEV" \
  || fail "Packager.dev.toml 缺 signing-identity = \"-\"（ad-hoc 测试签名）"

echo "OK: codesign readiness check passed"
