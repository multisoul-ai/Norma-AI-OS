# Packaging Notes

签名、公证与 Sparkle 的决策记录见 [macOS 发布与公证](macos-release-and-notarization.md)。本文保留当前打包命令。

`Packager.toml` configures a macOS app and DMG with entitlements + Developer ID signing:

- product name: `Norma OS`
- bundle id: `com.norma.os`
- formats: `app`, `dmg`
- binary: `target/release/norma-os`
- secondary binary: `target/release/multisoul-agent-hook`
- minimum macOS: `12.0`

正式发布使用仓库入口；公证模式默认打轻量包：

```bash
./scripts/release-local.sh
./scripts/release-local.sh --notarize
```

Release binaries use the workspace `[profile.release]` settings: ThinLTO,
one codegen unit, and symbol stripping. Keep these settings covered by
`scripts/test-packager-config.sh`; removing them makes the statically linked
Rust/macOS binary materially larger.

仓库里的 `.cargo/config.toml` 会把 `cargo packager ...` 路由到
`packaging/cargo-packager-wrapper/`，再由 wrapper 调用真实的 `cargo-packager`。
默认执行 `cargo packager --release -c Packager.toml` 时，wrapper 会清掉
`APPLE_API_*`、`APPLE_KEYCHAIN_PROFILE`、`APPLE_ID`、`APPLE_PASSWORD`、
`APPLE_TEAM_ID`，因此只做 Developer ID 签名打包，不提交 Apple 公证。

只验证 Cargo wrapper 的 App 公证路径时，可显式传 `--notarize` 并指定轻量资源：

```bash
NORMA_MEDIA_PROFILE=light cargo packager --notarize --release -c Packager.toml
```

这个低层入口不会对最终 DMG 单独公证和验收，不用于线上发布。

wrapper 会把
`APP_STORE_CONNECT_API_KEY_ID` / `APP_STORE_CONNECT_API_ISSUER_ID` /
`APP_STORE_CONNECT_API_KEY_PATH` 映射成 cargo-packager 读取的 `APPLE_API_*`，
并移除个人 Apple ID 方式的 notary 凭证，避免账号串用。

`./scripts/release-local.sh` 保留为完整本地 release 流程入口：它会先做
codesign readiness 检查和 `git lfs pull`，清理或映射 notary 环境变量，再直接调用
外部 `cargo-packager`。这里刻意不经过 `cargo run` wrapper，避免 CI 中用于启动
`Norma OS Dev.app` 的 target runner 截获打包工具本身。
`./scripts/release-local.sh --notarize` 与 `cargo packager --notarize ...` 使用同一组
`APP_STORE_CONNECT_*` 凭证约定。

流程成功后会验证本次新生成的 `.app` 和 `.dmg`，对 DMG 执行
`hdiutil verify`，并在 DMG 旁写入同名 `.sha256` 校验文件。项目内 Agent
应通过 [`.agents/skills/release-dmg/SKILL.md`](../../.agents/skills/release-dmg/SKILL.md)
选择普通签名打包或显式公证发版模式，不要把 `build/` 中的历史文件误报为新产物。

`cargo-packager 0.11.8` 会先公证并 staple `.app`，再创建 DMG；该票据不会自动成为
最终 DMG 自身的票据。因此 `release-local.sh --notarize` 在 DMG 生成后会再次使用同一组
App Store Connect API Key 提交最终 DMG，等待 Apple 接受，执行 `stapler staple`，并要求
`spctl --type open` 与 `stapler validate` 都通过后才计算发布校验和。

注意：人工绕过 `release-local.sh` 或 Cargo alias，直接执行
`cargo-packager --release -c Packager.toml` 时，上游 cargo-packager 0.11.8 会直接读取当前
shell 的 `APPLE_*` notary 环境变量。只有 `release-local.sh` 的直接调用会先完成环境清理，
不要把裸 `cargo-packager` 命令当作安全的日常入口。

`Cargo.toml` 的 `[workspace.package].version` 与 `Packager.toml` 顶层 `version`
表示**本次待发布版本**。`scripts/release-version.sh` 只读校验二者均为
`MAJOR.MINOR.PATCH` 且完全一致，并输出该版本。`before-packaging-command`
先调用这个只读入口，再准备 Sparkle / release plist 并构建二进制；它不会修改 manifest。

这是有意消除 split-brain：`cargo-packager 0.11.8` 会先解析 `Packager.toml`，
再运行 `before-packaging-command`。若 hook 在此时 bump，当前 App/DMG 仍使用旧 packager
版本，而随后构建的 Rust 二进制和磁盘 manifest 已是新版本。现在只有 GitHub Release
成功公开后，workflow 才独立运行 `scripts/bump-release-version.sh`，同步递增下一 patch，
更新 `Cargo.lock` 并创建 PR。人工检查与推进命令为：

```bash
scripts/release-version.sh
scripts/bump-release-version.sh # 只在已成功发布后运行
```

CI 在构建前会读取公开更新仓库的 latest Release tag，并调用
`scripts/release-version.sh --require-newer-than vX.Y.Z` 做严格递增校验。当前 manifest
版本等于或低于公开 latest 时立即失败，避免把较旧但尚未使用的 tag 错误标记为 latest，
或生成 Sparkle 永远不会采用的降级更新。

## DMG Finder layout

The release and development configurations use the same 640x400-point Finder
layout. The app and Applications anchors are `(180, 175)` and `(460, 175)`.
Finder draws its 16pt item labels below those anchors; the previous `y=200`
positions put both labels across the bottom border of the background wells.

The Retina background is authored in `assets/dmg-background.svg` and rendered
to the packaged `assets/dmg-background.png` with:

```bash
bash scripts/render-dmg-background.sh
```

The renderer composites the real app icon into the header, emits a 1280x800 PNG
at 144 DPI, and keeps the release asset reproducible. The two Finder label
landing zones use light backing plates so the system-owned dark labels remain
readable over the dark installer background. `scripts/test-packager-config.sh`
checks the exact label zones for sufficient luminance and verifies that the
header mark was not dropped during SVG rendering.

Finder owns the item-label rendering. Its icon-view scripting interface exposes
icon size, text size, label position, background picture, and background color,
but not label text color. `cargo-packager`/`create-dmg` therefore cannot force
these two labels to white. Keep the background readable with Finder's native
label color and verify actual appearance on the supported macOS versions.

`cargo-packager 0.11.8` 会把精确等于 `true` 的 `CI` 环境变量误判为 Jenkins 环境，向
`create-dmg` 追加 `--skip-jenkins`。该参数会跳过负责写入 `.DS_Store` 的 Finder
AppleScript；背景文件虽然仍被复制进镜像，但用户打开后只会看到普通 Finder 文件夹。
`release-local.sh` 因此只在调用 cargo-packager 时移除 `CI`，让 GitHub Actions 与本地
构建走同一套 Finder 定制流程。`scripts/verify-dmg-layout.sh` 随后挂载新产物，检查
`.DS_Store`、背景引用、图标位置记录、Applications 链接与 App，并在最终公证、校验和及
发布资产复验阶段 fail closed，避免再次发布普通 Finder 布局。

## macOS app icon

macOS 26 及以后使用 `assets/NormaIcon.icon`：现有品牌图案以不透明的
1024×1024 图片铺满画布，由系统处理圆角与显示尺寸。`CFBundleIconName = NormaIcon`
选择 `scripts/prepare-app-icon.sh` 编译出的 `build/icons/Assets.car`；release 和 dev
打包均包含该资源。不要给原生图案再加透明边距，否则系统会把旧图标缩进底板，
产生叠加留白。满幅母版是 `assets/app-icon-ios-source.png`，由
`render-app-icon.sh` 重建原生图案；关闭图案层的玻璃和透明效果以保持识别度。

`assets/app-icon-source.png` is the tightly cropped brand artwork used by
product surfaces and the DMG header. It is not packaged directly as the Dock
icon. The legacy macOS ICNS retains transparent optical padding: the generated
`assets/app-icon-macos.png` keeps the visible artwork at approximately
824×769 px on a 1024×1024 canvas, then `assets/AppIcon.icns` supplies the
standard 16–1024 px representations consumed by `cargo-packager`.

Regenerate the macOS assets after changing the brand artwork:

```bash
bash scripts/render-app-icon.sh
bash scripts/prepare-app-icon.sh
bash scripts/test-packager-config.sh
bash scripts/test-modern-app-icon.sh
```

The packaging test checks the safe-area dimensions, centering, and pixel-level
agreement between the reviewable 1024 px PNG and the largest ICNS entry. This
prevents a tightly cropped source from making the Dock icon appear larger than
neighboring apps on older macOS. The native icon test compiles the real CAR and
checks the full-bleed opaque artwork, native stack, and both packaging paths.

Codesigning 配置见 `packaging/macos/NormaOS.entitlements` 与
`Packager.toml` 的 `[macos]` 段；notarization 只通过
`cargo packager --notarize ...` 或 `scripts/release-local.sh --notarize` 显式开启，详见
[macOS 发布与公证](macos-release-and-notarization.md#notarize)。

## Sparkle 2 更新与发布资产

发行版固定为 Sparkle `2.9.5`。仓库内
`packaging/macos/sparkle-release.env` 同时固定官方 tar.xz URL、SHA-256
`015336b601493e05c237964954bff6191370003d94edefe663724c88840d73cc`
与 feed URL。`scripts/prepare-sparkle.sh` 只有在 archive 校验通过后才复用缓存，
并把 `Sparkle.framework` 与官方 `generate_appcast` 准备到 `build/sparkle/`；
`Packager.toml` 将 framework 复制到 App 的 `Contents/Frameworks/`。

Release build 必须设置非敏感环境变量 `SPARKLE_PUBLIC_ED_KEY`。它是 Sparkle
`generate_keys` 产生的 44 字符 base64 公钥，不是假值，也不是私钥。
`scripts/prepare-release-info-plist.sh` 从受版本控制的基础 plist 生成
`build/packaging/Info.plist`，保留 macOS 12.0、麦克风与语音说明，并注入：

```text
SUFeedURL=https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/appcast.xml
SUPublicEDKey=<SPARKLE_PUBLIC_ED_KEY>
```

应用在主线程创建并持有一个 App-global `SPUStandardUpdaterController`，使用 Sparkle
标准 UI 与调度，不自建轮询。原生 Norma OS 应用菜单以 macOS 熟悉的
“Check for Updates…” 文案放在 Settings 之前。`cargo run`、单测或未嵌入 Framework
的开发 bundle 会记录 updater unavailable 并安全降级；不会弹自造错误窗口。

真实发布把公证构建与 Sparkle 私钥步骤隔离；不要把永久更新私钥放进
`release-local.sh` 的环境，因为 cargo 依赖的任意 `build.rs` 都能读取构建环境。
本地等价操作为先完成公证，再在单独 shell 中 finalizer；tag 必须与 manifest 一致：

```bash
SPARKLE_PUBLIC_ED_KEY='…' ./scripts/release-local.sh --notarize
SPARKLE_EDDSA_PRIVATE_KEY='…' scripts/finalize-sparkle-release.sh \
  --tag v0.1.70 --app 'build/Norma OS.app' --dmg '<本次生成的已公证 DMG>'
scripts/verify-release-artifacts.sh \
  --tag v0.1.70 --app 'build/Norma OS.app' --assets build/release-assets
```

公证、staple 与 DMG 验证完成后，`scripts/finalize-sparkle-release.sh` 将最终 DMG
规范化为以下三个、也是唯一允许上传的资产：

```text
Norma-OS_aarch64.dmg
Norma-OS_aarch64.dmg.sha256
appcast.xml
```

私钥仅通过 stdin 进入官方命令 `generate_appcast --ed-key-file -`，不出现在 argv、
日志或仓库文件中；首期使用 `--maximum-deltas 0`。appcast enclosure 必须指向
`https://github.com/multisoul-ai/norma-os-releases/releases/download/<tag>/Norma-OS_aarch64.dmg`，
不能使用 mutable `latest` DMG URL。`scripts/verify-release-artifacts.sh` 同时验证
codesign、Gatekeeper、stapler、DMG、SHA-256、bundle short/build version、文件长度、
`sparkle:edSignature` 与 immutable URL。

## GitHub Actions 配置

`.github/workflows/release.yml` 只有 `workflow_dispatch`，并在运行时拒绝非 `main`。
同一 release concurrency 不会取消进行中的任务。Runner、工具链固定为
`macos-15`、Rust `1.91.1`、cargo-packager `0.11.8`；第三方 Actions 使用完整 commit
SHA。手动触发不接收版本输入；workflow 直接以 `scripts/release-version.sh` 的同步
manifest 输出作为唯一版本来源。

优先在仓库 Settings → Environments 新建受保护的 `release` Environment，设置 required
reviewers / deployment branch `main`，并把下表 Secrets / Variables 配置在该 Environment。
workflow 的 release job 显式绑定 `environment: release`，因此审批通过前读不到发布凭据。
若当前 GitHub 套餐不支持 Environment secrets/variables，才退回 repository-level Actions
Secrets/Variables，并用 branch protection 与 workflow 审批承担同等保护。

| 类型 | 名称 | 用途 |
|---|---|---|
| Secret | `DEVELOPER_ID_P12_BASE64` | Developer ID Application P12 的 base64 |
| Secret | `DEVELOPER_ID_P12_PASSWORD` | 上述 P12 密码 |
| Secret | `APP_STORE_CONNECT_API_KEY_ID` | Apple notary API Key ID |
| Secret | `APP_STORE_CONNECT_API_ISSUER_ID` | Apple notary Issuer ID |
| Secret | `APP_STORE_CONNECT_API_PRIVATE_KEY` | API `.p8` 完整文本；只写入临时文件 |
| Secret | `SPARKLE_EDDSA_PRIVATE_KEY` | Sparkle Ed25519 私钥；只送入 stdin |
| Secret | `RELEASE_GITHUB_APP_PRIVATE_KEY` | 跨仓库 GitHub App 私钥 |
| Variable | `SPARKLE_PUBLIC_ED_KEY` | 与私钥配对的 44 字符 Sparkle 公钥 |
| Variable | `RELEASE_GITHUB_APP_ID` | GitHub App ID |

GitHub App 必须安装到私有源码仓库和公开
`multisoul-ai/norma-os-releases`，最小 repository permissions 为：

- Metadata: Read（GitHub 固有）。
- Contents: Read and write（两仓库；发布资产、源码 provenance tag、next-version branch）。
- Pull requests: Read and write（源码仓库；创建下一 patch PR）。

workflow 不使用默认 `GITHUB_TOKEN` 做跨仓库写入。installation token 通常约一小时
过期，因此 token 创建步骤刻意位于 Rust 构建、Apple 公证、appcast 生成和本地验收
之后，紧邻远端发布。临时 Keychain、P12 与 API `.p8` 均由 trap 在成功或失败时清理。
Sparkle 永久私钥比构建凭据隔离得更窄：Rust/cargo-packager/公证步骤完全不接收它，
只有后续 appcast finalizer 单独一步把该 Secret 通过 stdin 交给官方工具，避免任意依赖的
`build.rs` 在编译期间读取更新签名根密钥。

## Draft 原子发布与恢复

发布器先创建 Draft，逐个上传 DMG、checksum、appcast，再从 GitHub 下载三项资产，
按字节摘要和 appcast 语义重新验证。全部通过后才将 Draft 改为 public/latest。
正式 Release/tag 已存在时直接失败，不覆盖；已有 Draft 只有在 body 中的 tag 与本地
DMG SHA-256 摘要完全一致时才可恢复。Draft 中已有同名资产也必须与本地摘要一致，
缺失资产可以补传，冲突资产绝不 `--clobber`。

PATCH 公开后发布器还会通过**不带认证**的
`/releases/latest/download/{appcast.xml,Norma-OS_aarch64.dmg,Norma-OS_aarch64.dmg.sha256}`
URL 做有限重试，
再次比对三项摘要与 appcast tag/version。这样不仅证明认证态 Draft 正确，也证明用户和
Sparkle 实际访问的 public/latest 链路已传播；有限次数后仍不一致则 workflow 失败并要求
人工检查，不能把静态或认证态检查冒充公开链路通过。

若未公开 Draft 因错误摘要或资产中断，先在 GitHub 核对创建者、tag 和全部摘要；
确认它是失败的本次尝试后可人工删除 Draft，再重跑 workflow。不要为通过自动恢复而
替换/改写 Draft 资产。若 Release 已公开，禁止复用 tag 或原地换包。

公开完成后 workflow 才在源码仓库创建同版本 annotated provenance tag，并运行独立
bump 命令创建下一 patch PR。如果公开成功但后置 tag/PR 因权限或网络失败，Release
保持有效；人工用同一已审核 commit 补 annotated tag，再运行 bump 脚本创建 PR，
不要重跑或覆盖已经公开的 Release。

## 首次 A → B L3 验收

真实证书、Apple 服务、GitHub 写权限与更新安装不可由离线测试替代。首次上线按以下
顺序执行并保存 workflow、notary、Release 与客户端日志证据：

1. 生成一对真实 Sparkle Ed25519 key；私钥只进入 Secret，公钥进入 Variable。
2. 在 `main` 上准备版本 A（两个 manifest 均为 A），手动运行 workflow，确认公开
   Release 只有三个规范资产，appcast enclosure 指向 A 的 immutable tag URL。
3. 在干净的受支持 macOS 机器下载并安装 A；验证 Gatekeeper 接受、App 可启动、
   “Check for Updates…” 位于应用菜单且检查无崩溃。
4. 合并 workflow 创建的下一 patch PR，在 `main` 构建版本 B，再次手动发布。
5. 保持 A 正在运行，先用菜单主动检查，验证标准 Sparkle UI 发现 B、下载、签名校验、
   安装、重启后 bundle short/build version 均为 B；再验证 Sparkle 自身调度的后台检查。
6. 记录 A→B 的 Console/Sparkle 日志、最终 appcast、DMG SHA、notary request、stapler、
   Gatekeeper 与 GitHub 远端回读结果。只有这些真实证据齐全才算 L3 通过。

## 回滚与未来域名桥接

已公开坏版本不能把同 tag 资产替换成“修复包”，也不要把 appcast 降回较低版本；
Sparkle 按递增 build version 更新。正确回滚是从已知良好 commit 制作一个版本更高的 C，
走完整签名、公证、Draft 回读流程发布。若只是尚未公开的 Draft，按上一节删除后重建。
必要时可在 GitHub UI 标注坏版本说明，但不可破坏旧客户端仍可能读取的签名资产。

迁移到自有域名必须先发布“桥接版本”：该版本仍从现有 GitHub feed 获得，但 bundle
里的 `SUFeedURL` 改为新 HTTPS 域名；确认桥接版本广泛安装后，新域名继续提供相同
Sparkle 协议、签名 appcast 与 immutable 资产。GitHub feed 至少保留 6–12 个月，
覆盖未升级到桥接版本的客户端；在该窗口结束前不得删除旧 appcast 或 tag 资产。

## Wallpaper resources

Built-in wallpapers live in `crates/app/assets/wallpapers/`: the Canvas catalog
contains 9 static images, 2 GIFs, and 5 MP4s; `home.mp4` and
`license-gate.mp4` are dedicated shell backgrounds. The MP4s are tracked by
**git-lfs** (see root `.gitattributes`); `git lfs pull` materializes the real
bytes before `release-local.sh` packages them.

`Packager.toml` / `Packager.dev.toml` 在签名前从可信清单生成的 staging 目录复制媒体：

```toml
resources = [
  { src = "build/release-media/wallpapers", target = "wallpapers" },
  { src = "build/release-media/feature-guide", target = "feature-guide" },
  { src = "build/release-media/wallpaper-previews", target = "wallpaper-previews" },
  "build/release-media/media-catalog.json",
]
```

不再需要手动 `copy-wallpaper-resources.sh`——`resources` glob 在 cargo-packager
签名 pass 之前执行，确保发布媒体进入已签名 bundle。

### 按需媒体与轻量包

`NORMA_MEDIA_PROFILE` 支持 `full`（全部内置）、`wallpapers-on-demand`
（动态壁纸按需下载）和 `light`（动态壁纸与教程视频均按需下载）。
`release-local.sh --notarize` 的线上发布默认使用 `light`；其他打包模式仍默认
`full`，需要完整离线发布时可显式设置 `NORMA_MEDIA_PROFILE=full`。清单
`packaging/media/resources.json` 固定原始文件大小、SHA-256、分类与资源版本。
`light` 保留 18 项核心媒体、7 份动态壁纸静态预览和可信目录；12 项可选媒体
下载后经长度、SHA-256 校验及原子提升进入用户缓存，不转码原文件。

`media-v1` 已在现有公开 [GitHub Releases](https://github.com/multisoul-ai/norma-os-releases/releases/tag/media-v1)
发布。普通轻量发布在 staging 之前会通过匿名 HTTPS 完整下载并核验所需原文件，
地址不可用或字节不符时拒绝打包，不能仅用 HEAD 或本机 fixture 替代：

```bash
NORMA_MEDIA_PROFILE=light SPARKLE_PUBLIC_ED_KEY='<现有 Sparkle 公钥>' ./scripts/release-local.sh
```

本地 ad-hoc 测试保留相同核心 helper、Sparkle 和浏览器扩展。先构建全部输入；
同样须提供现有 Sparkle 公钥，不能创建替代更新密钥。`--ad-hoc` 不构建 Rust：

```bash
cargo build --release -p app --no-default-features --features production-data
cargo build --release -p norma --bins
NORMA_MEDIA_PROFILE=light SPARKLE_PUBLIC_ED_KEY='<现有 Sparkle 公钥>' ./scripts/release-local.sh --ad-hoc
```

ad-hoc 的 `--local-test` 只跳过远端可用性 gate，仍执行原始资源、预览和白名单
校验。普通 production-data 构建不启用 `media-download-qa` 传输覆盖。
壁纸页逐项显示需下载、进度、校验、已下载、取消或失败重试状态；完成后仅应用
当前仍选中的壁纸。发布证据见 [云端媒体验收](macos-release-and-notarization.md#media-v1)。

Static/GIF wallpapers are loaded from their resolved files under
`Contents/Resources/wallpapers/`; they must not also be served by
`EmbeddedAssets`/`include_bytes!`. The old dual path stored roughly 81 MB twice:
once in the `norma-os` Mach-O `__const` section and once in Resources.

Six large static PNGs use WebP in the current catalog. The always-visible default
uses lossless WebP (SSIM 1.0); the other five use `cwebp -q 100` and retain their
original pixel dimensions (measured SSIM 0.9896–0.9977). Persisted selections that
still name the old PNGs are migrated and rewritten to the matching WebP path.

`scripts/test-packager-config.sh` enforces a 140 MiB source budget for the
packaged wallpaper directory (current total: 135.72 MiB). Video optimization
keeps the original dimensions, frame rate, H.264/yuv420p compatibility, and
removes unused audio; changes must be compared against the source before
replacing an asset.
