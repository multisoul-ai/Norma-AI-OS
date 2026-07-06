# Norma OS — Developer ID 签名 + 公证 设计文档

- 日期: 2026-07-05
- 状态: 已批准（待实施）
- 阶段: Phase 6 落地
- 关联: `docs/references/packaging.md`、`Packager.toml`、`packaging/macos/Info.plist`

## 1. 背景与目标

Norma OS 当前打包流程产出**未签名**的 `.app` / `.dmg`（`Packager.toml` 注释「本切片不签名/不公证（Phase 6）」）。用户下载后无法通过 Gatekeeper，终端节点与外部 agent exec 的架构也与 Mac App Store 的 App Sandbox 根本冲突（已确认走路线 B：Developer ID 直接分发，不上 MAS）。

**目标**：产出经 codesign + notarize + staple 的 DMG，用户从 GitHub Releases 下载后可直接打开，无「未识别开发者」警告。

**非目标**：Mac App Store 上架、Sparkle 自动更新、universal2 双架构（v1 仅 arm64）、官网下载页。

## 1.1 发布账号与凭证复用决策

**发布账号**：公司账号 **Team ID `2BF8G9AN4L`**（Shanghai Jieyu Future Intelligent Network Technology Co., Ltd）。与 mobile（iOS）项目同一账号，便于凭证复用。

**凭证复用关系**（mobile 项目位于 `/Users/alan/Documents/codes/yakami0129/multisoul/mobile`）：

| 凭证 | 来源 | 复用方式 |
|---|---|---|
| App Store Connect API Key（.p8） | mobile 已配置，env: `APP_STORE_CONNECT_API_KEY_ID` / `APP_STORE_CONNECT_API_ISSUER_ID` / `APP_STORE_CONNECT_API_KEY_PATH` | **直接复用**——同属 `2BF8G9AN4L`，notarytool 接受 |
| Developer ID Application 证书 | **需新签发**——公司账号当前仅有 `Apple Distribution`（iOS 用），无 Developer ID Application | 在 [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list) 为 `2BF8G9AN4L` 签发 `Developer ID Application` 证书并导入登录钥匙串 |

**关键约束**：notarize 用的 API Key 必须与 Developer ID 签名证书**同属一个 team**。本设计选 `2BF8G9AN4L` 后，mobile 的 `.p8` 可直接复用；若改用个人账号 `Z9N4YHC5PC`（现有 Developer ID 证书），则需另建 API Key，不能复用 mobile 凭证。

**env 变量映射**：cargo-packager 的 `notarize_auth()` 读取 `APPLE_API_KEY` / `APPLE_API_ISSUER` / `APPLE_API_KEY_PATH`；mobile 用 `APP_STORE_CONNECT_*` 命名。`release-local.sh` 内做映射（见 §4.5），用户无需重复配置。

## 2. 关键验证结论（已实证）

对 `cargo-packager 0.11.8` 做了端到端实测（ad-hoc 签名 `-` + entitlements + resources glob），结论：

| 验证项 | 结论 | 证据 |
|---|---|---|
| 深签所有 Mach-O | ✅ | 日志逐个签 `multisoul` → `multisoul-agent-hook` → `.app`（inside-out，符合 Apple 建议） |
| hardened runtime | ✅ | `flags=0x10002(adhoc,runtime)` |
| entitlements 应用 | ✅ | `codesign -d --entitlements -` 显示 entitlements 嵌入签名 |
| `resources` glob 落位 | ✅ | wallpapers 在签名前进入 `Resources/wallpapers/` |
| DMG 内含已签名 .app | ✅ | DMG 内 `.app` 仍带 `runtime` flag + entitlements |
| DMG 本身签名 | ✅ | 日志 `Codesigning ...dmg` |
| 整包 verify | ✅ | `codesign --verify --deep --strict` 通过 |
| sherpa-onnx 链接 | ✅ 静态 | `otool -L` 无非系统依赖，bundle 内无 `.dylib`——库校验担忧不成立 |
| notarize | ✅ | `ditto`→签 zip→`notarytool submit --wait`→Accepted 自动 staple，被拒自动拉 log；缺 `APPLE_*` 时优雅跳过 |
| DMG 文件名 | — | 自动带架构后缀 `Norma OS_0.1.14_aarch64.dmg` |

**核心判断**：cargo-packager 0.11.8 原生覆盖完整流水线（深签 + entitlements + DMG + notarize + staple）。无需自写 `codesign-and-notarize.sh` 脚本。

## 3. 架构

### 3.1 流水线

```
本地/CI:
  cargo packager --release -c Packager.toml
    ├─ before-packaging-command: bump-release-version.sh + cargo build --release -p app
    ├─ 布局 .app（二进制 + Info.plist + icns + resources glob 拷 wallpapers）
    ├─ 深签 .app（signing-identity + entitlements，登录钥匙串取证书 / CI 导入 p12）
    ├─ notarize（APPLE_KEYCHAIN_PROFILE 本地 / APPLE_API_KEY+ISSUER+KEY_PATH CI）
    ├─ staple
    └─ 打包 DMG（保留背景）+ 签 DMG

CI 额外:
  └─ 上传 Norma OS_*.dmg 到 GitHub Release
```

### 3.2 组件清单

| 文件 | 状态 | 职责 |
|---|---|---|
| `packaging/macos/NormaOS.entitlements` | 新增 | hardened runtime entitlements plist |
| `packaging/macos/Info.plist` | 修改 | 补 `LSMinimumSystemVersion=12.0`、`LSApplicationCategoryType=public.app-category.developer-tools` |
| `Packager.toml` | 修改 | 加 `signing-identity`、`entitlements`、`resources` |
| `Packager.dev.toml` | 新增 | 本地签名测试用：不 bump 版本、不复用 release 版本号，避免反复测试导致版本号虚高 |
| `scripts/release-local.sh` | 新增 | 封装本地一键发版：检查 keychain profile → `cargo packager --release -c Packager.toml` → 验证 |
| `scripts/copy-wallpaper-resources.sh` | 退役 | `resources` glob 替代 |
| `.github/workflows/release.yml` | 新增（阶段 B） | tag 触发 CI 发版 |
| `docs/references/packaging.md` | 修改 | 更新签名/公证章节，Phase 6 落地 |
| `scripts/check-codesign-readiness.sh` | 新增 | mechanized constraint：打包前校验 entitlements/Info.plist 关键键存在 |

## 4. 详细设计

### 4.1 `packaging/macos/NormaOS.entitlements`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <!-- sherpa-onnx 已静态链接，理论上不需要；保留为 v1 保险，
       首次发版验证通过后可尝试移除 -->
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
```

**取舍**：不启用 `allow-jit` / `allow-unsigned-executable-memory`（GPUI/Metal 不需要 CPU 侧 JIT）。`device.audio-input` 在非沙盒 Developer ID 下由 TCC + Info.plist usage 字符串覆盖，无需 entitlement。首次发版后若运行时无异常，尝试移除 `disable-library-validation` 收紧权限。

### 4.2 `Packager.toml` 改动

```toml
# 新增字段
resources = ["crates/app/assets/wallpapers"]

[macos]
minimum-system-version = "12.0"
info-plist-path = "packaging/macos/Info.plist"
signing-identity = "Developer ID Application: <公司证书名> (2BF8G9AN4L)"  # 非密钥，明文
entitlements = "packaging/macos/NormaOS.entitlements"
# 签名/公证在 cargo packager pass 内自动完成
```

`signing-identity` 字符串非密钥，可入仓。`<公司证书名>` 在 §1.1 前置步骤签发 Developer ID Application 证书后确定（证书 CN 形如 `Developer ID Application: Shanghai Jieyu Future... (2BF8G9AN4L)`），用 `security find-identity -v -p codesigning` 查到后填入。

### 4.3 `Packager.dev.toml`（本地测试用）

与 `Packager.toml` 区别：
- 无 `before-packaging-command`（不 bump 版本、不 rebuild，直接用 `target/release/norma-os` 现有二进制）
- `version` 固定不递增
- 其余字段（signing-identity、entitlements、resources、dmg 背景）一致

用法：`APPLE_KEYCHAIN_PROFILE=multisoul-notary cargo packager -c Packager.dev.toml`

### 4.4 `packaging/macos/Info.plist` 改动

新增：
```xml
<key>LSMinimumSystemVersion</key>
<string>12.0</string>
<key>LSApplicationCategoryType</key>
<string>public.app-category.developer-tools</string>
```

保留现有 `NSMicrophoneUsageDescription` / `NSSpeechRecognitionUsageDescription`。

### 4.5 `scripts/release-local.sh`（草图）

伪代码，实施时补全错误处理与输出：

```bash
#!/usr/bin/env bash
set -euo pipefail

# 1. 复用 mobile 的 App Store Connect API Key（同属 2BF8G9AN4L）
#    cargo-packager 读 APPLE_API_*，mobile 用 APP_STORE_CONNECT_*，做映射
: "${APP_STORE_CONNECT_API_KEY_ID:?需设置 APP_STORE_CONNECT_API_KEY_ID（同 mobile）}"
: "${APP_STORE_CONNECT_API_ISSUER_ID:?需设置 APP_STORE_CONNECT_API_ISSUER_ID}"
: "${APP_STORE_CONNECT_API_KEY_PATH:?需设置 APP_STORE_CONNECT_API_KEY_PATH}"
export APPLE_API_KEY="$APP_STORE_CONNECT_API_KEY_ID"
export APPLE_API_ISSUER="$APP_STORE_CONNECT_API_ISSUER_ID"
export APPLE_API_KEY_PATH="$APP_STORE_CONNECT_API_KEY_PATH"

# 2. 检查登录钥匙串里有 Developer ID Application (2BF8G9AN4L) 证书
security find-identity -v -p codesigning | grep -q "Developer ID Application.*2BF8G9AN4L" \
  || die "未找到 2BF8G9AN4L 的 Developer ID Application 证书，见设计文档 §1.1 前置步骤"

# 3. git lfs pull（确保 wallpaper MP4 真实字节在树里）
git lfs pull

# 4. 打包 + 签名 + notarize + staple（cargo-packager 一条命令）
cargo packager --release -c Packager.toml

# 5. 验证
APP=$(ls -d build/bundle/osx/Norma OS.app 2>/dev/null | head -1)
codesign --verify --deep --strict --verbose=2 "$APP"
spctl --assess --type execute --verbose "$APP"
xcrun stapler validate "$APP"
```

### 4.6 notary 凭证（复用 mobile，无需新建）

**不使用** `xcrun notarytool store-credentials` keychain profile 方案——直接复用 mobile 的 App Store Connect API Key（`.p8` + key ID + issuer ID），通过 env 变量传入 cargo-packager（见 §4.5 映射）。`.p8` 文件路径由 `APP_STORE_CONNECT_API_KEY_PATH` 提供，本机已配置。

**前置一次性步骤**：在 [App Store Connect → Users and Access → Keys](https://appstoreconnect.apple.com/access/api) 确认该 API Key 已启用且具备 notarization 权限（App Manager / Admin 角色即可）。mobile 项目已在使用此 key 上传 IPA，权限通常已具备。

### 4.7 CI（阶段 B）

`.github/workflows/release.yml`，tag `v*` 触发，`macos-14` runner：
1. checkout + `git lfs pull`
2. 安装 Rust + cargo-packager
3. 导入 Developer ID Application 证书 p12：`APPLE_CERTIFICATE`（base64）+ `APPLE_CERTIFICATE_PASSWORD` → cargo-packager 自动 `setup_keychain`
4. 写 API Key 文件：从 secret `APP_STORE_CONNECT_API_KEY_B64`（.p8 base64）解码到 `./private_keys/AuthKey_${APP_STORE_CONNECT_API_KEY_ID}.p8`（cargo-packager 的 `notarize_auth()` 会搜索 `./private_keys/`），或显式设 `APPLE_API_KEY_PATH`
5. 映射 env：`APPLE_API_KEY`/`APPLE_API_ISSUER`/`APPLE_API_KEY_PATH` ← `APP_STORE_CONNECT_*`
6. `cargo packager --release -c Packager.toml`
7. `softprops/action-gh-release` 上传 `Norma OS_*.dmg`

**Secrets**（复用 mobile 命名 + 新增 p12）：
- `APP_STORE_CONNECT_API_KEY_ID`、`APP_STORE_CONNECT_API_ISSUER_ID`、`APP_STORE_CONNECT_API_KEY_B64`（.p8 base64；mobile 用路径，CI 用 base64）
- `APPLE_CERTIFICATE`（Developer ID Application p12 base64）、`APPLE_CERTIFICATE_PASSWORD`

## 5. 错误处理

| 失败场景 | 处理 |
|---|---|
| 公司账号未签发 Developer ID Application 证书 | `release-local.sh` 前置 grep 检查并提示 §1.1 链接 |
| notarize 被拒 | cargo-packager 自动拉 `notarytool log` 写入错误信息；CI 失败、不发布 Release |
| API Key team 与证书 team 不一致 | notarytool 报 `forbidden` / `not authorized`；前置检查证书 CN 含 `2BF8G9AN4L` |
| 证书不在钥匙串 | codesign 报 `errSecInternalComponent`；release-local.sh 前置检查并提示 |
| wallpaper LFS 未拉 | git lfs 指针文件 ~130 字节，打包后 MP4 无法播放；release-local.sh 强制 `git lfs pull` |
| 版本号不一致 | `bump-release-version.sh` 已校验 Packager.toml 与 Cargo.toml 一致，失败即中止 |

## 6. 测试与验证

### 6.1 机械化检查（`scripts/check-codesign-readiness.sh`，挂 pre-commit + CI）
- entitlements 文件存在且为合法 plist
- Info.plist 含 `LSMinimumSystemVersion`、`LSApplicationCategoryType`、两个 usage 字符串
- `signing-identity` / `entitlements` 字段在 Packager.toml 中已配置

### 6.2 本地发版验证清单
- `codesign --verify --deep --strict --verbose=2 Norma OS.app` → valid
- `spctl --assess --type execute --verbose Norma OS.app` → accepted
- `xcrun stapler validate Norma OS.app` → OK
- DMG 双击挂载、拖拽安装、启动无 Gatekeeper 警告
- 运行时：终端节点 spawn PTY、voice 麦克风、agent exec 均正常

### 6.3 无单元测试
签名/公证为打包时行为，不产生可单元测试的纯函数。机械化检查脚本覆盖配置正确性。

## 7. 分阶段交付

- **阶段 0（前置，人工一次性）**：在 Apple Developer 为公司账号 `2BF8G9AN4L` 签发 `Developer ID Application` 证书并导入登录钥匙串；确认 mobile 的 App Store Connect API Key 可用于 notarytool。用 `security find-identity -v -p codesigning | grep "Developer ID Application.*2BF8G9AN4L"` 验证。
- **阶段 A（本地跑通）**：§4.1–4.6 + §6 验证。产出可在本机签名+公证的 DMG。
- **阶段 B（CI）**：§4.7。阶段 A 验证通过后接续。

## 8. Follow-up（非本期）

- universal2 双架构（arm64 + x86_64，`lipo` 合并）
- Sparkle 自动更新（appcast + zip）
- 恢复 / 增强 DMG 背景与图标位（cargo-packager `[dmg]` 已保留，无需额外工作）
- 尝试移除 `disable-library-validation` 收紧权限
