#!/usr/bin/env bash
# Mechanized constraint: macOS codesign/notarize 所需配置就绪校验。
#
# Scope: entitlements 文件、Info.plist 关键键、Packager 配置字段。
#
# On failure: 打印缺失项，退出码 1。挂 pre-commit 与 CI。
#
# Background: docs/references/macos-release-and-notarization.md
#
# On non-macOS hosts (e.g. Linux Cloud Agents), plist tooling is unavailable.
# File presence and Packager.toml field checks still run; macOS-only plist
# key lint is skipped with a clear notice rather than a hard fail.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENT="$ROOT/packaging/macos/NormaOS.entitlements"
INFO="$ROOT/packaging/macos/Info.plist"
PKG="$ROOT/Packager.toml"
PKG_DEV="$ROOT/Packager.dev.toml"

fail() { echo "FAIL: $*" >&2; exit 1; }

# 1. entitlements 存在；macOS 上再做合法 plist 校验
[ -f "$ENT" ] || fail "entitlements 文件缺失: $ENT"
if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$ENT" >/dev/null || fail "entitlements 不是合法 plist: $ENT"
else
  echo "SKIP: plutil 不可用，跳过 entitlements plist lint（非 macOS）"
fi

# 2. Info.plist 关键键（需要 PlistBuddy）
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
  for key in \
    LSMinimumSystemVersion \
    LSApplicationCategoryType \
    NSMicrophoneUsageDescription \
    NSSpeechRecognitionUsageDescription
  do
    grep -Fq "<key>$key</key>" "$INFO" \
      || fail "Info.plist 缺 $key"
  done
  echo "SKIP: PlistBuddy 不可用，已用 XML key 扫描代替（非 macOS）"
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
