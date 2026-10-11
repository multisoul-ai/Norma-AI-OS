# Sparkle 架构切片体积实验

当前正式 App 是 arm64，但 Sparkle 2.9.5 的五个 Mach-O 文件同时包含 x86_64 和 arm64。对独立副本移除 x86_64 后，框架普通文件总量减少 1.245 MB。原框架和打包配置保持原状；这只是候选方案的静态实验，尚未通过真实 App 更新流程验收。

## 实测表格

| 对象 | 原始大小 MB | 仅 arm64 大小 MB | 节省 MB | 节省占比 |
|---|---:|---:|---:|---:|
| Sparkle.framework 普通文件总量 | 2.778 | 1.533 | **1.245** | **44.82%** |

MB 使用十进制。普通文件只计一次，不将 `Versions/Current`、框架顶层等符号链接重复计算。本表不包含目录元数据，也不是 App 或 DMG 的体积；不能把 44.82% 用作整个安装包收益。

## 实验步骤与结果

1. 使用 `scripts/prepare-sparkle.sh` 下载固定版本官方归档，核对 SHA-256 `015336b601493e05c237964954bff6191370003d94edefe663724c88840d73cc`。
2. 保留 `build/sparkle/Sparkle.framework`，将完整框架复制到 `build/package-size-reduction/sparkle-arm64-probe-*/Sparkle.framework`，保留全部符号链接和非二进制资源。
3. 对 `Autoupdate`、`Sparkle`、`Updater.app`、`Downloader.xpc`、`Installer.xpc` 的真实可执行文件，使用 `xcrun lipo <input> -thin arm64 -output <temporary>`，保留文件权限后替换副本。五份副本逐一确认只有 arm64；原文件长度与哈希另行保留。
4. 对原始与副本各执行 `codesign --verify --deep --strict --verbose=2`，两者均退出 0、签名完整性验证通过。这是本次文件副本的结果，不是打包后 App 的签名、公证或 Gatekeeper 验证。

完整逐文件大小、架构、哈希、两次签名输出和实际副本路径见 [JSON 证据](assets/2026-10-10-package-size-implementation/sparkle-arm64-probe.json)。[核心资源准备证据](assets/2026-10-10-package-size-implementation/packaging-core-resources.json) 同时记录了官方归档与图标 `Assets.car` 的体积/哈希。

## 采用条件与后续任务

采用前按 TDD 给准备脚本增加显式目标架构参数和五份嵌套代码的完整性测试；正式 hook 将切片过程放在整个 App 签名前，避免只裁剪顶层框架而遗漏 updater/XPC 程序。

在新的 arm64 App 上检查启动、Check for Updates、下载和实际安装成功，并验证错误时的完整更新回退。最后重新验证整个 App 签名、最低支持 macOS 行为和 DMG 实际收益。未来提供 x86_64 或 universal App 时，应根据目标架构重新选择切片，不能复用仅 arm64 的缓存。

本轮基线与轻量候选仍统一使用原始 universal 框架，避免把未经运行验收的收益计入媒体方案。该方案不影响视频、壁纸字节；实现与采用都留在后续验收之后。
