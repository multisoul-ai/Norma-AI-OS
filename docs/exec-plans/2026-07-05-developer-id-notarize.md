# Developer ID 签名 + 公证 Implementation Plan（阶段 A：本地跑通）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `cargo packager` 在本地产出 ad-hoc 签名 + entitlements 的 `.app`/`.dmg`（用 `Packager.dev.toml`），验证签名/公证流水线代码就绪；真实 Developer ID 签名待阶段 0 证书签发后启用。

**Architecture:** cargo-packager 0.11.8 原生覆盖深签 + entitlements + DMG + notarize + staple（已实证）。本计划只改配置与脚本：新增 entitlements 文件、修补 Info.plist、给 `Packager.toml`/`Packager.dev.toml` 加 `resources`+`entitlements`+`signing-identity`、新增 `check-codesign-readiness.sh` 与 `release-local.sh`、退役 `copy-wallpaper-resources.sh`。

**Tech Stack:** cargo-packager 0.11.8、bash、plist、git-lfs。

## Global Constraints

- 公司账号 Team ID `2BF8G9AN4L`；notarize 复用 mobile 的 `APP_STORE_CONNECT_*` env（映射到 `APPLE_API_*`）。
- 阶段 0（Developer ID Application 证书签发）**挂起**——本计划用 ad-hoc 签名（`-`）验证流水线，`Packager.toml` 的 `signing-identity` 以注释占位，证书到位后取消注释。
- 不引入 `#[allow(...)]`；shell 脚本遵循 `scripts/check-*.sh` 既有风格（`set -euo pipefail`、清晰注释）。
- `copy-wallpaper-resources.sh` 被 `resources` glob 替代；退役时同步更新 `docs/references/packaging.md`。
- sherpa-onnx 静态链接（已验证 `otool -L` 无非系统依赖），entitlements 仅保留 `disable-library-validation` 作 v1 保险。
- 目标架构：v1 仅 arm64。

---

### Task 1: entitlements 文件 + Info.plist 修补 + 校验脚本（entitlements/Info.plist 部分）

**Files:**
- Create: `packaging/macos/Multisoul.entitlements`
- Modify: `packaging/macos/Info.plist`
- Create: `scripts/check-codesign-readiness.sh`

**Interfaces:**
- Produces: `scripts/check-codesign-readiness.sh`（退出码 0=就绪，1=缺失；Task 2 会扩展 Packager 校验段）
- Produces: `packaging/macos/Multisoul.entitlements`（被 `Packager.toml`/`Packager.dev.toml` 的 `entitlements` 字段引用）

- [ ] **Step 1: 写校验脚本（entitlements + Info.plist 部分，此时应失败）**

创建 `scripts/check-codesign-readiness.sh`：

```bash
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
ENT="$ROOT/packaging/macos/Multisoul.entitlements"
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

echo "OK: codesign readiness check passed"
```

- [ ] **Step 2: 运行校验，确认失败**

Run: `bash scripts/check-codesign-readiness.sh`
Expected: FAIL，提示 entitlements 文件缺失（此时 Info.plist 也缺两个键，但 entitlements 检查先失败）。

- [ ] **Step 3: 创建 entitlements 文件**

创建 `packaging/macos/Multisoul.entitlements`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <!-- sherpa-onnx 已静态链接，理论上不需要；保留为 v1 保险，
       首次发版验证通过后可尝试移除以收紧权限。 -->
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
```

- [ ] **Step 4: 修补 Info.plist**

在 `packaging/macos/Info.plist` 的 `<dict>` 内追加两个键（与现有 NS* 键并列）：

```xml
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
```

- [ ] **Step 5: 运行校验，确认通过**

Run: `bash scripts/check-codesign-readiness.sh`
Expected: `OK: codesign readiness check passed`

- [ ] **Step 6: 赋可执行权限并提交**

```bash
chmod +x scripts/check-codesign-readiness.sh
git add packaging/macos/Multisoul.entitlements packaging/macos/Info.plist scripts/check-codesign-readiness.sh
git commit -m "feat(packaging): add entitlements, Info.plist keys, codesign readiness check"
```

---

### Task 2: Packager.toml / Packager.dev.toml 配置 + 扩展校验

**Files:**
- Modify: `Packager.toml`
- Create: `Packager.dev.toml`
- Modify: `scripts/check-codesign-readiness.sh`

**Interfaces:**
- Consumes: `packaging/macos/Multisoul.entitlements`（Task 1）
- Produces: `Packager.dev.toml`（ad-hoc 本地测试配置，被 Task 4 的 `release-local.sh` 与 Task 5 的 e2e 验证使用）
- Produces: `Packager.toml` 的 `resources`+`entitlements` 字段（`signing-identity` 注释占位，阶段 0 启用）

- [ ] **Step 1: 扩展校验脚本，加入 Packager 校验段（此时应失败）**

在 `scripts/check-codesign-readiness.sh` 末尾的 `echo "OK: ..."` 之前插入：

```bash
# 3. Packager.toml 含 entitlements + resources 字段
grep -q '^entitlements' "$PKG" || fail "Packager.toml 缺 entitlements 字段"
grep -q '^resources' "$PKG" || fail "Packager.toml 缺 resources 字段"

# 4. Packager.dev.toml 存在且 signing-identity = "-"
[ -f "$PKG_DEV" ] || fail "Packager.dev.toml 缺失: $PKG_DEV"
grep -q 'signing-identity = "-"' "$PKG_DEV" \
  || fail "Packager.dev.toml 缺 signing-identity = \"-\"（ad-hoc 测试签名）"
```

- [ ] **Step 2: 运行校验，确认失败**

Run: `bash scripts/check-codesign-readiness.sh`
Expected: FAIL，提示 Packager.toml 缺 entitlements/resources 字段。

- [ ] **Step 3: 修改 Packager.toml**

在 `Packager.toml` 顶层（`formats` 行附近）追加 `resources`：

```toml
resources = ["crates/app/assets/wallpapers"]
```

在 `[macos]` 段追加 `entitlements` 与注释占位的 `signing-identity`：

```toml
[macos]
minimum-system-version = "12.0"
info-plist-path = "packaging/macos/Info.plist"
entitlements = "packaging/macos/Multisoul.entitlements"
# 阶段 0 启用：签发 Developer ID Application (2BF8G9AN4L) 证书后取消注释并填入证书 CN。
# 用 security find-identity -v -p codesigning | grep "Developer ID Application.*2BF8G9AN4L" 查到。
# signing-identity = "Developer ID Application: <公司证书名> (2BF8G9AN4L)"
```

- [ ] **Step 4: 创建 Packager.dev.toml（ad-hoc 本地测试配置）**

创建 `Packager.dev.toml`（与 `Packager.toml` 区别：不 bump 版本、不 rebuild、ad-hoc 签名、用现有 release 二进制）：

```toml
# 本地签名测试配置：ad-hoc 签名（"-"），不 bump 版本，直接用 target/release 现有二进制。
# 用法：cargo packager -c Packager.dev.toml
# 真实发版走 Packager.toml（阶段 0 启用 signing-identity 后）。
name = "app"
product-name = "Multisoul"
version = "0.1.14"
identifier = "com.multisoul.desktop"
authors = ["multisoul"]
formats = ["app", "dmg"]
icons = ["assets/AppIcon.icns"]
out-dir = "build/dev"
binaries-dir = "."
resources = ["crates/app/assets/wallpapers"]

[[binaries]]
path = "target/release/multisoul"
main = true

[[binaries]]
path = "target/release/multisoul-agent-hook"
main = false

[macos]
minimum-system-version = "12.0"
info-plist-path = "packaging/macos/Info.plist"
signing-identity = "-"
entitlements = "packaging/macos/Multisoul.entitlements"
```

- [ ] **Step 5: 运行校验，确认通过**

Run: `bash scripts/check-codesign-readiness.sh`
Expected: `OK: codesign readiness check passed`

- [ ] **Step 6: 提交**

```bash
git add Packager.toml Packager.dev.toml scripts/check-codesign-readiness.sh
git commit -m "feat(packaging): add resources/entitlements to Packager configs, dev ad-hoc config"
```

---

### Task 3: release-local.sh 本地发版脚本

**Files:**
- Create: `scripts/release-local.sh`

**Interfaces:**
- Consumes: `APP_STORE_CONNECT_API_KEY_ID` / `_ISSUER_ID` / `_KEY_PATH` env（mobile 复用）
- Consumes: `Packager.toml`（真实发版）或 `Packager.dev.toml`（`--ad-hoc` 测试）
- Produces: `build/bundle/osx/Multisoul.app` + `Multisoul_*.dmg`（签名 + 公证）

- [ ] **Step 1: 创建 release-local.sh**

创建 `scripts/release-local.sh`：

```bash
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
  cargo packager --release -c Packager.toml
fi

# 3. 验证产物
APP=$(ls -d build/bundle/osx/Multisoul.app build/dev/Multisoul.app 2>/dev/null | head -1)
[ -n "$APP" ] || die "未找到产物 Multisoul.app"
info "验证 $APP"
codesign --verify --deep --strict --verbose=2 "$APP"
spctl --assess --type execute --verbose "$APP" || warn "spctl 在 ad-hoc 下会失败，真实签名时应 accepted"
xcrun stapler validate "$APP" || warn "stapler 在 ad-hoc/notarize 跳过时无票据"

info "Done: $APP"
```

- [ ] **Step 2: 赋可执行权限**

Run: `chmod +x scripts/release-local.sh`

- [ ] **Step 3: 语法检查**

Run: `bash -n scripts/release-local.sh`
Expected: 无输出（语法正确）。

- [ ] **Step 4: 提交**

```bash
git add scripts/release-local.sh
git commit -m "feat(packaging): add release-local.sh for local sign+notarize"
```

---

### Task 4: 退役 copy-wallpaper-resources.sh + 更新 packaging.md

**Files:**
- Delete: `scripts/copy-wallpaper-resources.sh`
- Modify: `docs/references/packaging.md`

**Interfaces:**
- Consumes: `Packager.toml` 的 `resources` glob（Task 2）已替代该脚本

- [ ] **Step 1: 确认无其他引用（除文档）**

Run: `grep -rl "copy-wallpaper-resources" --include="*.sh" --include="*.toml" --include="*.rs" . | grep -v target`
Expected: 仅 `docs/` 与脚本自身（无构建脚本/CI 引用）。

- [ ] **Step 2: 删除脚本**

```bash
git rm scripts/copy-wallpaper-resources.sh
```

- [ ] **Step 3: 更新 docs/references/packaging.md**

将「Wallpaper resources」整节（原第 33–52 行附近）替换为：

```markdown
## Wallpaper resources

Built-in wallpapers live in `crates/app/assets/wallpapers/` (3 PNG, 2 GIF, 5 MP4)。
The MP4s are tracked by **git-lfs** (see root `.gitattributes`)；commit 的是 LFS
指针，`git lfs pull` 后工作树才有真实字节（~141 MB），打包前 `release-local.sh`
会强制 lfs pull。

`Packager.toml` 的 `resources` glob 在签名前把整个目录拷进 app bundle 的
`Contents/Resources/wallpapers/`：

```toml
resources = ["crates/app/assets/wallpapers"]
```

不再需要手动 `copy-wallpaper-resources.sh`——`resources` glob 在 cargo-packager
签名 pass 之前执行，确保 wallpapers 进入已签名 bundle。
```

并在文末「Codesigning and notarization are explicitly Phase 6.」一行更新为：

```markdown
Codesigning and notarization 配置见 `packaging/macos/Multisoul.entitlements` 与
`Packager.toml` 的 `[macos]` 段；本地发版用 `scripts/release-local.sh`，详见
`docs/design-docs/2026-07-05-developer-id-notarize-design.md`。
```

- [ ] **Step 4: 提交**

```bash
git add docs/references/packaging.md
git commit -m "refactor(packaging): retire copy-wallpaper-resources.sh in favor of resources glob"
```

---

### Task 5: ad-hoc 端到端验证

**Files:** 无新增/修改（纯验证；发现问题则回到对应 Task 修复）

**Interfaces:**
- Consumes: Task 1–4 全部产物

- [ ] **Step 1: 确保 release 二进制存在**

Run: `cargo build --release -p app`
Expected: `target/release/multisoul` 与 `target/release/multisoul-agent-hook` 产出（若已存在则跳过）。

- [ ] **Step 2: 跑 ad-hoc 签名打包**

Run: `./scripts/release-local.sh --ad-hoc`
Expected: 产出 `build/dev/Multisoul.app` + `build/dev/Multisoul_0.1.14_aarch64.dmg`；`codesign --verify` 通过；`spctl` 在 ad-hoc 下 warn（可接受）。

- [ ] **Step 3: 验证 entitlements 已嵌入签名**

Run:
```bash
codesign -d --entitlements - build/dev/Multisoul.app | grep disable-library-validation
```
Expected: 输出包含 `com.apple.security.cs.disable-library-validation` 且 `Bool=true`。

- [ ] **Step 4: 验证 wallpapers 进入 bundle**

Run: `ls build/dev/Multisoul.app/Contents/Resources/wallpapers/ | wc -l`
Expected: `10`（3 PNG + 2 GIF + 5 MP4）。

- [ ] **Step 5: 验证 sherpa 静态链接（无非系统 dylib 依赖）**

Run:
```bash
otool -L build/dev/Multisoul.app/Contents/MacOS/multisoul | grep -vE "/usr/lib|/System" | grep -v "^$"
```
Expected: 无输出（所有动态依赖均为系统库）。

- [ ] **Step 6: 验证 DMG 内含已签名 .app**

Run:
```bash
DMG=$(ls build/dev/Multisoul_*.dmg | head -1)
hdiutil attach -nobrowse -readonly "$DMG" >/dev/null
codesign -dv /Volumes/Multisoul/Multisoul.app 2>&1 | grep -E "flags|Signature"
hdiutil detach /Volumes/Multisoul >/dev/null
```
Expected: `flags=0x10002(adhoc,runtime)`（hardened runtime 已启用）。

- [ ] **Step 7: 若全部通过，无需提交（本任务无代码变更）；若有修复则提交修复**

---

## Self-Review

**Spec coverage:**
- §1.1 账号决策 → Global Constraints + Task 2 注释占位 ✓
- §4.1 entitlements → Task 1 ✓
- §4.2 Packager.toml → Task 2 ✓
- §4.3 Packager.dev.toml → Task 2 ✓
- §4.4 Info.plist → Task 1 ✓
- §4.5 release-local.sh → Task 3 ✓
- §4.6 notary 凭证复用 → Task 3 env 映射 ✓
- §4.7 CI → **不在本计划**（阶段 B，本计划仅阶段 A）✓
- §5 错误处理 → Task 3 前置检查 + Task 5 验证 ✓
- §6 机械化检查 → Task 1/2 check-codesign-readiness.sh ✓
- 退役 copy-wallpaper-resources.sh → Task 4 ✓
- 更新 packaging.md → Task 4 ✓

**Placeholder scan:** `signing-identity` 注释占位是设计意图（阶段 0 启用），非计划缺陷；其余无 TBD/TODO。

**Type/命名一致性:** `Multisoul.entitlements`、`Packager.dev.toml`、`check-codesign-readiness.sh`、`release-local.sh` 在各 Task 间引用一致。

**范围:** 仅阶段 A（本地 ad-hoc 跑通），CI 与真实签名属阶段 B / 阶段 0，本计划不覆盖。
