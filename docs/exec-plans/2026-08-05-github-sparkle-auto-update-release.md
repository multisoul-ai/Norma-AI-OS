# GitHub + Sparkle 自动更新发布实施计划

## 目标

为 Norma OS 建立可重复、可审计的一键 macOS 发布流程：应用通过 Sparkle 2
从公开 GitHub Releases 获取更新；私有源码仓库的 GitHub Actions 自动完成版本校验、
构建、Developer ID 签名、Apple 公证、DMG 验证、appcast 生成和 Draft Release 原子发布。

首期更新源固定为：

```text
https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/appcast.xml
```

首期不创建域名，也不替换任何已公开资产。`release` Environment、Apple
签名/公证 Secrets 与 Sparkle 密钥已配置；GitHub App ID/私钥、提交推送和首次真实
Release 仍需用户授权或提供凭据后完成。

## 当前实施状态

- 已完成发布工作流、Sparkle 打包与客户端接入、Settings 左下角品牌卡片检查更新按钮、
  产物校验脚本、发布脚本、测试和操作文档；独立 Software Update 导航与页面已移除。
- 已在源码仓库创建仅允许 `main` 的 `release` Environment，并配置 Developer ID、
  App Store Connect 与 Sparkle 所需凭据。
- 已创建组织 GitHub App `norma-os-release`，仅安装到源码与公开发布仓库；
  `Contents`/`Pull requests` 为读写、`Metadata` 为只读，App ID 与私钥已配置到
  `release` Environment。
- 尚未提交、推送或触发真实发布；首个真实更新验证仍按 A → B 两版本执行。

## 已确认的测试 seam

用户已确认按完整推荐方案直接实现，测试以以下公开边界为准：

1. **版本生命周期 seam**：`Cargo.toml` 与 `Packager.toml` 表示本次待发布版本；
   打包不得在已解析配置后改版本；发布成功后的独立命令才递增下一 patch。
2. **客户端更新 seam**：应用启动时持有一个 Sparkle 2
   `SPUStandardUpdaterController`，原生应用菜单的 “Check for Updates…”
   action 能触发用户主动检查；Framework 缺失时开发构建应安全降级并记录错误。
3. **发布产物 seam**：最终 notarized/stapled DMG 经 Sparkle EdDSA 签名后，
   `appcast.xml` 引用不可变 tag URL，并与 bundle short/build version、文件长度一致。
4. **GitHub 发布 seam**：工作流只从 `main` 手动触发，使用 Draft Release
   先上传并验证全部资产，最后才公开；同 tag 或摘要冲突必须失败，禁止静默覆盖。

所有新测试遵守仓库测试注释、具体正负断言和数值推导规范；按 red → green
逐个垂直切片实现，不先批量堆测试。

## 实施步骤

### 1. 收口版本生命周期

- 先修改/新增脚本测试，证明 `before-packaging-command` 不得调用版本递增脚本。
- 增加只读版本校验入口，输出同步的 `MAJOR.MINOR.PATCH`，拒绝 manifest 不一致。
- 保留自动 patch 能力，但只供发布成功后的版本推进步骤调用。
- 更新 `Packager.toml`、相关测试和打包文档，使 App、CLI、DMG、tag、appcast
  使用同一个本次发布版本。

### 2. 固定并准备 Sparkle 发行版

- 固定 Sparkle `2.9.5` 发行包与 SHA-256：
  `015336b601493e05c237964954bff6191370003d94edefe663724c88840d73cc`。
- 新增可重复执行的准备脚本：下载、校验、解压 Sparkle.framework 与官方工具；
  缓存只能在校验通过后复用。
- 生成 release 专用 Info.plist，注入非敏感的 `SPARKLE_PUBLIC_ED_KEY`，写入
  `SUFeedURL`、`SUPublicEDKey`，并保留 macOS 12 最低版本和既有权限说明。
- `Packager.toml` 从受控 build 目录打包 Sparkle.framework；开发 ad-hoc 配置不强制联网。

### 3. 接入应用更新入口

- 在 macOS App-global 层增加 Sparkle updater owner，保证 controller 生命周期覆盖整个 App。
- updater 必须在主线程启动；不自行追加后台轮询，由 Sparkle 管理调度。
- 增加 `CheckForUpdates` action，放在 Norma OS 应用菜单的 Settings 之前。
- Linux/测试构建提供明确 no-op/错误 seam，不让 macOS Framework 影响域 crate。
- 先补菜单语义单测，再实现 action；必要的 native loader 测试只验证公开状态和失败行为。

### 4. 生成并验证发布产物

- 扩展仓库 release 入口，在公证和 staple 完成后定位本次新 DMG。
- 将最终 DMG 规范化为 `Norma-OS_aarch64.dmg`，重新生成对应 `.sha256`。
- 使用官方 `generate_appcast --ed-key-file -` 从 stdin 读取私钥，关闭首期 delta，
  生成 tag-specific 下载 URL 的 `appcast.xml`。
- 新增验证脚本，检查 codesign、Gatekeeper、stapler、DMG、SHA-256、bundle 版本、
  `sparkle:edSignature`、长度和不可变 URL；私钥不得出现在命令行或日志。

### 5. GitHub Actions 原子发布

- 新增 `.github/workflows/release.yml`，仅 `workflow_dispatch`，且运行时拒绝非 `main`。
- 固定 `macos-15`、Rust `1.91.1`、cargo-packager `0.11.8`；第三方 Action 使用完整 SHA。
- 使用临时 Keychain 导入 P12，写入临时 App Store Connect API key，所有路径在退出时清理。
- 使用 GitHub App installation token 操作
  `multisoul-ai/norma-os-releases`，默认 `GITHUB_TOKEN` 不跨仓库。
- 创建 Draft Release，上传 DMG、SHA256、appcast，远端回读验证后才 publish/latest。
- 正式 tag 已存在时拒绝覆盖；可恢复的 Draft 仅在远端资产摘要一致时继续。
- 发布成功后在私有源码仓库建立同版本 provenance tag，并创建下一 patch 版本 PR。
- 工作流不得真实运行发布；仓库只交付可配置、可静态验证的流程。

### 6. 文档和操作手册

- 更新 `docs/references/packaging.md`，记录本地、CI、Sparkle key、GitHub App、Secrets、
  Draft 恢复和回滚步骤。
- 记录未来域名迁移策略：先发布桥接版本改变 `SUFeedURL`，GitHub feed 至少继续保留
  6–12 个月。
- 明确外部必备项和首个真实验证顺序：先发布带 Sparkle 的 A 版本，再以 B 版本验证更新。

## 文件所有权与现有修改

Ruby 负责本计划涉及的发布、打包、Sparkle App-global 接入、测试、工作流与文档文件。
工作区当前已有用户改动，尤其是 `Cargo.toml`、`Packager.toml`、`Cargo.lock`、
`scripts/release-local.sh`、`docs/references/packaging.md`、`AGENTS.md`、`.gitignore`
及 `.agents/skills/release-dmg/`；实现必须保留并适配这些改动，不得回退、覆盖或清理。

## 验证矩阵

- 聚焦脚本测试：版本、Packager 配置、Sparkle 准备、appcast、发布工作流、notarize 模式。
- `cargo test -p app menu_bar` 或等价聚焦 App 测试。
- `cargo check -p app`。
- `bash scripts/check-codesign-readiness.sh`。
- `bash scripts/check-max-file-lines.sh`。
- `bash scripts/check-no-allow.sh`。
- `bash scripts/check-agent-guide-lines.sh`。
- `cargo build -p app --release`（若不需要真实发布凭证即可完成）。
- 真实签名、公证、GitHub Draft 发布和 A→B 自动更新属于凭证/远端依赖 L3；
  若本机或仓库未配置凭证，必须明确标记为未执行，不能用静态测试冒充通过。

## 完成标准

- 当前自动版本能力被保留，但不再产生 App/CLI split-brain 版本。
- 应用具备 Sparkle 2 updater 生命周期和用户主动检查入口。
- 发布工作流能在配置凭证后从 main 一键产生原子 GitHub Release。
- 更新 feed 继续使用 GitHub，未来域名迁移无需改发布协议，只需桥接版本更换 feed URL。
- 所有可离线验证项通过；所有必须依赖真实秘密或远端写入的项目被准确列出。
