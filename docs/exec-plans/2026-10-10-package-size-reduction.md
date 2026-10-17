# Norma OS 安装包体积优化执行计划

状态：实现与聚焦验证完成；用户已授权本地 ad-hoc 测试，当前版本 full/light App 和 DMG 已生成并校验。真实用户流程与性能对照待测试。日期：2026-10-10。

采用资源分层分发、发布白名单与依赖瘦身，减少默认安装包下载体积。所有可选视频和图片保留原始文件字节；下载后逐文件校验长度与 SHA-256。用户已确定使用现有 GitHub Releases 分发资源。

## 目标与体积口径

| 阶段 | 交付目标 | 当前媒体基线 | 预计包内媒体 | 验收目标 |
|---|---|---:|---:|---|
| M0 | 当前版本基线与资源清单 | 167.312523 MB | 167.312523 MB | 30 文件逐项可核对；新增正式构建基线 |
| M1 | 5 段可选视频壁纸、2 张 GIF 按需下载 | 167.312523 MB | 53.588639 MB | 媒体减少约 67.97%；含新预览与清单后不超过 55 MB |
| M2 | 再外置 5 段教程视频，保留封面与文字 | 167.312523 MB | 31.530913 MB | 媒体减少约 81.15%；含新预览与清单后不超过 33 MB |
| M3 | 发布依赖与构建参数瘦身 | M0 的新构建 | 实测决定 | 收益为正且用户流程、性能不退化才采用 |
| M4 | 新 App、DMG 与真实用户流程验收 | M0 同条件 DMG | 实测决定 | DMG 小于基线；单列是否达到 50%，不以媒体降幅替代 |

MB 为十进制。M1/M2 是源媒体字节推算，未扣除待核验的未使用资源，也不代表整个 DMG 的降幅。完整离线版保留全部受管资源；下载全部后的磁盘占用与总下载量单列报告。

原无损压缩实验最小产物为 Brotli 11：167.312523 → 166.072291 MB，仅节省 0.7413%。模拟 DMG 的 UDZO 基线为 190.751686 MB，Brotli 预压缩为 190.143814 MB，ULMO 原资源为 183.576328 MB。它们不替代 M0 当前正式源码的基线。

## 工作区与执行边界

- 基准 commit：`2ae94d1bf811e894bdfc6e776c983bce43dc9468`。
- 分支：`codex/package-size-reduction`。
- Worktree：`/Users/cengwei/Documents/code/Norma-AI-OS-worktrees/package-size-reduction`。
- 原工作区的图标、背景、脚本、文档和其他未提交改动保持原状；仅复制本任务已完成研究证据供参考。
- 原资源不转码、不降采样、不删除；发布时通过生成的资源目录决定随包内容。
- 下载、哈希、磁盘扫描和安装在后台运行；GPUI 前台只处理状态与 UI 更新。
- 视频继续走后台解码与 420f `surface()` 低拷贝路径。
- 不运行或新增已禁用的 `crates/e2e`；真实 App 验收使用 Computer Use。
- 当前工作产出为可审查实现、本地候选包、资源分发材料与验收报告。远程资源上传、正式公证和发版通过仓库 release 流程另行执行，不把尚未上线的 URL 当作可用生产资源。

## 任务与依赖

| 任务 | 内容与主要修改位置 | 交付物 | 前置 |
|---|---|---|---|
| T00 | 创建 worktree、记录基准与计划 | 本计划、分支、Goal | 无，已完成 |
| T01 | 盘点 source/App/DMG，记录版本、features、架构、Git 状态、字节与哈希 | 基线 JSON/CSV、原始验证日志 | T00 |
| T02 | 建立显式资源清单与 staging 生成器，支持完整、壁纸按需、全部可选媒体按需三种配置 | `packaging/media/`、准备脚本、脚本测试 | T01 的源清单 |
| T03 | 生成可信媒体 catalog 与 GitHub Release 上传材料，使用固定资源版本和哈希文件名 | catalog、上传目录、发布说明、可用性检查 | T02 |
| T04 | 实现零 GPUI 的媒体安装与完整性验证；后台下载适配、取消、进度、重试 | `crates/media_store/`、`crates/app/src/media/`、聚焦测试 | T03 |
| T05 | 将动态壁纸接入按需下载，保留旧引用和作用范围；新增静态预览与下载反馈 | wallpaper 适配、i18n、真实 App 证据 | T04 |
| T06 | 教程视频按需下载，封面与文字离线可用，切换/关闭后迟到结果不自动播放 | feature guide 适配与测试 | T04、T05 |
| T07 | 打包接入 staging 与资源可用性检查，支持轻量与完整离线候选包 | 配置、检查脚本、新 App/DMG | T02、T05、T06 |
| T08 | GPUI 测试依赖隔离、图片格式 feature 收窄与构建矩阵 | 对照数据、保留/放弃决定 | T01；独立评估 |
| T09 | 独立对抗审查、真实 App 检查、镜像验证、全量证据汇总 | 中文验收报告、资源上线所需文件 | T07、T08 |
| T10（后续） | 评估 Sparkle 差分更新，保留完整包回退；不改变首期发布配置 | 旧/新签名 App 对照、差分下载量与失败回退证据 | M4 验收、资源上线与后续版本 |

实现按小切片推进：每条新行为先写一个能失败的聚焦测试，再写最小实现并运行受影响范围验证。表中的任务是依赖顺序，不是一次性写完所有测试。

## T01 基线与对照方法

建立两套清单：源媒体逐文件清单，以及新 `.app` 内全部文件的逐项清单。源媒体同时记录用途、随包策略、长度和 SHA-256；App 清单包含主程序、helper、framework、图标及其他 sidecar，不能只统计媒体。

记录 Git revision 与 diff 摘要，避免未提交实现与 HEAD 被混同；记录 packager、Rust、macOS 版本、架构、有效 features 与 release profile。对比统一镜像格式、图标配置、签名模式与测试机器，并报告 release hook 的版本变化。原 `build/` 历史产物不作为当前构建成功证据。

基线先以完整资源构建记录，候选包采用相同基础配置。若编译参数或媒体清单变化，分组报告；最终收益使用实际总字节计算，不把独立方案节省率相加。

## T02 发布清单与资源白名单

清单将资源分为核心静态内容、可选动态壁纸、可选教程视频和按 feature 启用的内容。第一版保留 30 个原始文件的完整模式，M1 外置 7 份动态壁纸，M2 外置 12 份可选媒体。

生成器只读源资源，创建独立 staging 输出，拒绝缺失文件、重复目标、路径穿越、符号链接和 Git LFS pointer。失败时不提升未完成目录；重复执行不会遗留上一种 profile 的文件。输出 bytes/SHA256 清单与分组预算。

`home.mp4`、`desktop-earth.png` 与默认未启用功能的 `license-gate.mp4` 共 10.536464 MB 仅为待核验候选。通过源码、配置、历史选择与真实 App 流程证据后才能从对应发布 profile 排除；原文件继续留在仓库。

## T03 媒体 catalog 与 GitHub 分发

继续使用 `multisoul-ai/norma-os-releases`。资源版本独立于 App patch 版本；Release tag 与文件名固定，单个文件名包含 SHA-256，避免 App 自动更新时重复下载未变化的视频。

首版使用随已签名 App 携带的固定 catalog，记录资源相对标识、类别、大小、SHA-256、预览标识、固定版本下载 URL。catalog 的信任来自应用签名，不接受远程未签名清单覆盖这些哈希。此设计实现此前的清单认证要求，并避免首期新增远程动态 catalog 与密钥管理。

发布材料包含全部可选原文件、catalog、文件清单与校验说明。上传前后逐文件核对。轻量正式发布必须先确认需要的下载 URL 可用且内容匹配；在资源尚未上线时，保留完整模式或明确停止轻量生产发布，不静默生成不可下载资源的正式安装包。

本地验收使用受控 HTTP 服务和原始媒体；测试下载源覆盖仅作用于 QA 配置。生产使用 HTTPS GitHub URL，并验证最终文件哈希。

## T04 安装与后台下载

`media_store` 只负责可信 catalog、路径约束、本地安装与验证，不依赖 GPUI、storage、app 或网络。网络请求及 App 生命周期适配放 `app::media`，遵守现有 SOCKS 代理支持与加密 Provider 初始化规则。`model_store` 的领域不扩展为 UI 或网络模块。

媒体安装目录通过现有数据根定位，为 `~/Library/Application Support/Norma OS/media/` 下的受管子目录；测试使用隔离数据根。自定义壁纸目录不被下载清理影响。缓存键由内容 SHA-256 决定，旧内置相对路径仍作为稳定选择标识。

下载使用受限并发和后台流式 IO；单个资源只保留一份进行中任务。先写临时文件、核对长度和哈希，再原子提升并发布 Ready。文件截断、响应失败、哈希不符、取消或退出均不能标为可用。重复请求复用已有任务和已验证文件。

进度按时间或字节批量更新，最终事件保证送达；不能每个网络块重绘设置页。首版取消后可清除 partial 并完整重试；断点续传作为后续扩展，不将未验证的 partial 冒充缓存。应用退出设置取消信号，不在前台 join 或等待网络。

所有目录扫描和哈希验证在后台建立可用性快照，渲染与输入热路径只查内存状态。下载服务只改变资源可用性，不能改写用户已选择的工作空间或项目身份。

## T05 动态壁纸交互与迁移

保留已有 16 项壁纸目录、全局/工作空间/项目覆盖与跟随规则；内置路径、旧 PNG 映射和用户自定义壁纸继续有效。资源未安装属于 Pending 或 Missing，不能因此把持久化引用重写为 Default。

选择尚未下载的壁纸时保存用户选择，立即显示静态回退并开始后台下载。下载成功后，仅当当前有效选择仍引用该资源时刷新背景；切换空间、选择另一张壁纸或关闭页面后，迟到任务仅更新全局资源状态，不能覆盖后来选择。

素材库继续使用静态预览。为 7 份动态壁纸预生成小型预览随轻量包携带，预览是新增展示文件，不替代或重写原始 GIF/MP4。已下载资源断网可播放；未下载时提供重试/取消，核心工作台保持可用。

落实 `.agents/skills/apple-design`：点击即时反馈；当前选择与下载进度分开显示；下载不抢焦点；取消和重试可中断；尊重 Reduced Motion；hover、pressed、focus、selected 与 disabled 在浅深主题清晰可辨。保留现有 Material 壁纸布局与 macOS 键盘习惯，控件优先 `ui::component`。

## T06 教程视频与离线使用

保留 5 张现有封面（合计 0.763190 MB）和文字。进入需要视频的主题时后台请求对应资源；下载期间教程导航、文字和快捷键可用。

下载完成只有当前相同主题、页面可见、允许动画且未手动暂停时才播放。切换主题、关闭教程、Reduced Motion 或退出后，迟到结果不能恢复旧视频。已下载视频可离线观看。

完整离线 profile 将全部可选原文件随包分发。先交付两种打包模式；“下载全部”和独立离线资源导入可在后续迭代实现，当前不据此声称减少完整资源总量。

## T07 打包接入与产物验证

`Packager.toml` 与开发配置的核心资源、浏览器扩展、planner、图标、Sparkle、helper 和本地化目录继续按清单进入包。媒体目录改由准备脚本生成 staging，资源必须在签名前就位。

轻量模式初期显式选择；资源上线与全部验收通过后再设置正式默认模式。完整模式用于基线与离线交付。打包前检查 profile、catalog、预算、源文件 LFS 状态和正式下载可用性，不依赖开发源码目录的回退使轻量包“看起来正常”。

本地新候选包走 `./scripts/release-local.sh` 并标明 Developer ID 已签名但未公证。版本变化、SHA-256、`codesign`、`hdiutil verify` 与只读挂载结果入报告。正式公证与远程发布遵守 release-dmg 技能及已有发布流程。

UDZO 作为首轮统一比较格式。ULMO 单独评估，仅当最低支持 macOS 12 的挂载证据齐全后才考虑采用；资源外置收益不依赖该算法。

## T08 依赖与构建实验

先核验发布包中 GPUI `test-support` 是否为产品代码所需，尝试隔离到开发依赖/显式测试 feature；使用有效 feature tree 与最终 Mach-O 体积证明结果。若必须保留，记录具体调用，不能为尺寸强行改变测试或运行语义。

图片格式收窄同时检查 App、GPUI 与其他启用点，保留实际使用与承诺支持的导入格式。先证明 PNG/JPEG/WebP/GIF 与浏览器/图标路径可用，再比较体积；必要时保留现有格式。

构建矩阵使用 `opt-level=3/s/z × thin/fat`，记录每组二进制、App、DMG 大小、构建耗时、终端输入响应与音频/视频性能。现有 strip、Thin LTO、codegen-units=1 和 arm64 已实现，不重复计入收益。`panic=abort` 不进入首轮默认修改。

## T10 后续：减少升级下载量

采用现有 Sparkle 差分更新能力，目标是减少已安装用户的升级下载量；首次安装仍下载完整 DMG，不能把差分收益计入首包体积。当前 `scripts/finalize-sparkle-release.sh` 明确传入 `--maximum-deltas 0`，首期保持该行为。

后续执行顺序：保留至少一个正式旧版本的签名 App 与完整更新归档；用相同架构和分发配置生成旧版到新版的差分；对照记录完整/差分字节与签名校验；在实际旧版 App 上验证升级成功，以及旧版本不匹配、差分损坏或服务不可用时回退完整包。最后经过正式发布验证再调整最大差分数量与 appcast，不仅删除禁用参数就声称已实现。

依据为已归档的 [Sparkle 官方差分文档](https://sparkle-project.org/documentation/delta-updates/)；本 Goal 交付后续任务设计，不上传差分或修改生产 appcast。

## 验收与自动化证据

| 验收条件 | 聚焦自动化或真实程序证据 |
|---|---|
| profile 复制正确、原文件不变、无旧 staging 残留 | 准备脚本测试；源清单前后 SHA-256；三种 profile 实测目录 |
| catalog 拒绝非法路径、长度、重复项与不可信配置 | `media_store` 单元/集成测试 |
| 成功下载与本地安装逐字节一致 | 本地 HTTP fixture 下载、长度/SHA、安装后读取原字节 |
| 截断、损坏、取消、服务器错误不能 Ready | 下载与安装负向测试；partial 清理断言 |
| 同资源请求合并，退出不阻塞前台 | manager 状态测试、真实程序取消/退出记录 |
| 未下载壁纸回退，旧选择与空间隔离保留 | wallpaper 聚焦测试；真实 App A/B 空间切换与重启 |
| 迟到下载不覆盖新选择或播放旧教程 | revision/身份聚焦测试；真实快速切换流程 |
| 动态媒体下载后断网可用，完整模式离线可用 | 隔离 QA 数据根、真实 App 与受控服务开/关对照 |
| 状态反馈、主题、键盘与 Reduced Motion 正确 | Computer Use 操作真实 `.app`，截图与最终状态记录 |
| 网络/哈希后台化、低拷贝播放保留 | 性能/存储/SOCKS/视频护栏与实际播放证据 |
| 打包确实外置资源且签名/镜像有效 | 新 App 文件清单、签名校验、DMG 验证与挂载 |
| 最终收益口径正确 | 实测 source/App/DMG/缓存四组表格，负收益和未达标如实保留 |

按改动范围运行聚焦测试、`cargo test -p media_store`、`cargo check -p app`、发布构建，以及 storage/performance/video/SOCKS/文件行数/禁止 allow 护栏。跨 crate 实际变更时运行所需测试；独立 reviewer 必须寻找取消、路径、缓存、作用范围和测试假阳性等反例。不能用 headless GPUI 或静态截图替代真实 App 验收。

## 交付与完成条件

1. 执行计划、可信 catalog、三种资源 profile 与可复现准备入口。
2. 动态壁纸与教程按需下载实现、聚焦测试及独立审查结论。
3. 完整基线与轻量候选包，媒体预算及实际 DMG 对照数据。
4. 真实 App 下载、离线、取消、快速切换、升级配置和退出证据。
5. 中文结果报告，列明正式资源上线状态、待执行发布步骤与未采用的瘦身实验。

M4 本地验收完成前不标记整个 Goal 完成。生产资源未上线时，轻量发布可用性仍是待执行项，完整发布模式保持可用；不能声称已正式部署。

## 执行记录

- [x] T00 创建 Goal 与独立 worktree。
- [x] 确定媒体下载源为现有 GitHub Releases。
- [x] 写入本计划并迁移本任务研究证据。
- [ ] T01 源清单及同条件主程序基线已完成；App/DMG 基线待本地签名许可。
- [x] T02 发布清单、三 profile、7 份静态预览；准备脚本 7 条、预览 1 条测试通过。
- [x] T03 可信 catalog、135.781610 MB 原媒体上传材料已准备；尚未上传。
- [ ] T04 后台下载与受控安装已实现；完整域库 10 条、下载/QA 5 条测试通过，真实程序退出待验。
- [ ] T05 动态壁纸实现与聚焦验证完成，真实用户流程待验。
- [ ] T06 教程视频实现与聚焦验证完成，真实用户流程待验。
- [ ] T07 本地 full/light ad-hoc 包已校验；原冻结源码基线及正式轻量发布可用性仍待验。
- [ ] T08 依赖隔离与六组构建实验已完成，真实性能对照待验。
- [ ] T09 独立审查及易读阶段报告已完成，真实 App/DMG 验收待执行。

相关依据：[无损压缩报告](../research/2026-10-09-lossless-resource-compression-report.md)、[资源分发与构建研究](../research/2026-10-10-package-size-options-sources.md)、[现有工作空间壁纸行为](../design-docs/2026-10-05-workspace-wallpapers.md)。

### 本轮实测（含额外展示文件）

| Profile | 原始媒体 MB | 随包原媒体 MB | 预览＋清单 MB | 实际媒体负载 MB | 相对原媒体净节省 |
|---|---:|---:|---:|---:|---:|
| full | 167.312523 | 167.312523 | 1.099617 | 168.412140 | −0.66% |
| wallpapers-on-demand | 167.312523 | 53.588639 | 1.099624 | 54.688263 | 67.31% |
| light | 167.312523 | 31.530913 | 1.099629 | 32.630542 | 80.50% |

上表仅统计媒体与新增清单/预览，不能替代 App 或 DMG。所有原文件长度与 SHA 保持一致。旧无损压缩最佳算法节省 0.74%，本方案减少的是默认随包内容。完整资源下载后并不节省这些原资源的安装占用。

- 原 commit 的 production-data release 主程序：58,244,960 字节，SHA-256 `3a5f562bece5cc841e3439509593a35b153ddb3ad952940186ee57e1d896672d`；arm64、macOS 27.0.1、Rust 1.91.1。
- 正式默认打包已尝试，因配置的 Developer ID 证书缺失而停止；尚未生成可用于比较的新 App/DMG。ad-hoc 本地测试包许可已提出，未获回复前不切换模式。
- 12 个 GitHub optional 地址在 2026-10-10 02:28 UTC 的检查中均为 404。轻量 production gate 实际运行失败并保持原完整 staging；full 为默认值，资源上线前须重新验证实际内容。
- Wallpaper 正式 Cargo 聚焦测试 58 通过、1 条既有 HD benchmark 忽略；下载/QA 正式 Cargo 5 条通过，涵盖原字节、离线、损坏/截断/HTTP 错误、并发与取消。
- 独立审查已促成祖先 symlink、成功提升后的清理错误、并发限制与 Ready 缓存修复；同主题教程重开、HTTPS 中间跳转两问题已补失败测试并修复，等待最终复验。
- 包配置、视频、SOCKS、storage/performance、行数、禁止 allow 与 Agent 指南检查已通过。

### 审查与执行入口补充

- 同主题教程重开已补真实 GPUI 控制器测试，旧代码实际失败，修复后教程 20 条测试全部通过。显式打开/恢复播放重新验证；后台迟到刷新保持原有动画和可见性守卫。
- Production gate 现在逐跳限制 HTTPS，单独维护已请求跳数，拒绝第 11 跳；3 条重定向回归测试通过。
- 独立 reviewer 最后只读复核没有新增 P1/P2；较早的 TLS→HTTP→TLS 链确已复现，不是理论风险。
- QA 专用 `media-download-qa` feature 可使用显式 loopback 服务，普通 release 不编译覆盖入口，`production-data` 会拒绝覆盖。使用 `NORMA_TEST_DATA_ROOT` 同时隔离数据库和媒体缓存。开发 runner 同一 staging/catalog/预览的契约测试通过。
- 媒体域层在全新 target 中运行完整 `cargo test -p media_store` 通过：10 条行为测试及文档测试；缓存副本原有 rustdoc 依赖错误保留日志，未绕过测试。
- `scripts/benchmark-release-size.py` 提供 3/s/z × thin/fat 串行主程序实验；不修改正式 profile，保留每组二进制/哈希/日志，仅清理本次实验缓存。冷启动/音视频性能和 App/DMG 在真实程序可运行后另测，不以 Mach-O 最小体积直接决定发布参数。
- `license-gate.mp4` 仍有真实产品调用，`home.mp4` 在旧验收和性能对照仍有引用。因此候选闲置内容继续保留；本轮没有贸然从清单删除。
- 图片 feature tree 表明 App 和 vendor GPUI 均启用 image default；只改 App 声明不会关闭 AVIF/EXR/TIFF。收窄所有调用点将改变解码兼容范围，本轮保留原解码支持，后续另测 vendor 补丁。
- 易读报告：`docs/research/2026-10-10-package-size-implementation-report.html`；已用浏览器打开并核对数据，包含历史完整算法表及当前媒体分发表。该浏览器检查不代替真实 Norma App 验收。

- 按需下载实现、尚未隔离 GPUI 测试依赖的发布对照构建已通过（17m36s）：主程序 58,429,552 字节，较原基线增加 184,592 字节。该组 SHA-256：`916175e28c5516aa43222b1a6d7ce530d890b45c33d0210bae89e7474232e8ee`。
- GPUI 测试支持已移至 dev-dependencies，显式 test feature 保留；正式 feature tree 的红绿验证通过。同样 Rust 源码、仅依赖配置变更的对照构建通过（10m03s）：主程序 58,079,520 字节，SHA-256 `94385a85cf90998235046b87c9740d8a1425781b269e97c23817fa45e3e3ee4e`。隔离减少 350,032 字节；连同下载实现比原基线减少 165,440 字节。GPUI 执行分支的真实程序验证尚未完成，当前不据此声称性能无回退。
- 编译机器为 8GB 内存，重型构建已改为串行、矩阵设置 jobs=1。UI 完整测试已完成串行重跑：7 条单元、5 条 i18n 集成及文档测试通过；先前中断不作为失败或通过证据。
- 当前媒体域层已扩展为 10 条测试，通过 schema/版本/重复 ID/路径/URL/长度/SHA 的负向验证；全新 target 文档测试亦通过。
- 矩阵工具的独立审查发现不同源码/工具链可混用旧结果；已先补失败测试，再记录构建输入指纹、Cargo.lock、工具链、features 与环境。3 条测试通过，分次续跑和构建过程中输入漂移均拒绝混入比较。
- 报告工具原先固定的标题和状态被独立审查构造反例；已先补失败测试，再将收益、URL 检查分布/时间、主程序与包体积全部改从证据读取，缺证据显示“未测量”。2 条回归测试通过。
- `production-data,media-download-qa` 配置的完整 App 编译检查通过（37.27s）。钥匙串再次检查仍为 0 个有效签名身份，真实 App 和新 DMG 验收尚未完成。
- 最终格式化后的 App 聚焦复验通过：wallpaper 58 条（1 条既有 benchmark 忽略）、feature guide 20 条、media 5 条。测试专用启动函数的 4 条 dead-code warning 保留，生产发布与编译检查无此警告；未使用 `allow` 或绕过验证。
- 矩阵另一个旧缓存误采集反例已修：记录 `CARGO_BUILD_TARGET`，通过 Cargo JSON 的本次 `compiler-artifact` 获取主程序，不读取固定缓存路径。5 条测试通过，独立 reviewer 复验 85 字节新产物未误报成 9 字节旧缓存，所有新增 P2 已关闭。六组矩阵已开始串行构建。
- 当前 30 个源媒体与冻结基线的长度、SHA-256 全部一致，总计 167,312,523 字节；证据见 `docs/research/assets/2026-10-10-package-size-implementation/source-integrity.json`。
- 易读 HTML 的更新数据已用 Computer Use 刷新并核对，截图保存在同一证据目录 `report-browser.png`。这只验证报告显示，不计为真实 Norma App 下载或性能验收。
- 矩阵第一组默认环境实验在链接阶段失败（480.20s，退出 101），未产生成功体积；失败记录与日志另存 `compiler-matrix-failed.json` / `compiler-matrix-failed-sdk.log`。实际环境把 SDKROOT 指向 CLT SDK 27，其 `arm64e.x1` 标记不被当前链接工具识别。
- `vtool -show-build` 核验基线及两个成功对照均为 SDK 26.5、LD 1267.0。重跑在进程环境显式使用 Xcode 的 SDK 26.5 / Developer 目录，不修改系统选择或原工作区。SDK 配置、解析后的 SDK 路径、SDKSettings 哈希及 Apple 编译器/链接器版本已加入指纹；6 条矩阵回归测试通过。
- 有效重跑目录为 `build/package-size-reduction/compiler-matrix-xcode-26.5/`，原失败目录保留。两组不合并、不将失败缓存或已有旧程序记为成功样本。

- SDK 小范围独立审查通过：26.5 的 SDKSettings 哈希及 LD 1267 与记录一致；SDKROOT、Developer 路径、SDK 哈希、编译器或链接器变化均拒绝续跑混用。旧失败样本保持退出 101、无成功体积。
- 固定 SDK 的有效矩阵首组 `3-thin` 已成功：58,079,520 字节，479.40 秒，SHA-256 `39acf65eac1da07fbec5605c7161cf71f0b3003995a5031b4a5700a20a62f1d7`。后续五组仍在串行运行；只将已完成行写入 HTML/CSV，App/DMG 和性能继续标明未测量。

- 最大原视频（30,692,231 字节）通过安装域层实际实验：首次安装、同长度单字节损坏修复、读取 1 MiB 后取消各 3 次全部通过；安装/修复后逐字节一致，取消后无 Ready/partial。安装中位数 0.112 秒、修复 0.211 秒、离线 SHA 校验 0.095 秒；独立探针峰值 RSS 2.30 MiB，读取缓冲上限 64 KiB。仅覆盖 debug 域库本地文件流，不包含 HTTP/GPUI/生产 App 性能；完整方法及证据见 [真实大文件安装域层实验](../research/2026-10-10-media-install-probe.md)。

- 大文件探针补强后的独立审查无新增实质发现：新/旧结果分开保留，原视频及探针/域库/源码 SHA、九份 time/stdout、逐次 CSV 和汇总口径一致。该证据没有替代真实 App 验收。
- Companion 只读清点已归档：基线与候选的 agent-hook 为 1,128,992 字节、browser-auth-host 为 1,461,104 字节，哈希分别保留；冻结基线 target 中尚无 `norma` CLI，打包前必须在冻结源码明确构建，不能冒用候选或历史缓存。

- 有效矩阵第二组 `3-fat` 已成功：55,236,624 字节，1,314.19 秒，SHA-256 `e1aa2277ef7b388ffb57ec91dcda7b5cf1b686bb5856d4041ea3ab895f4eb7f1`；较同输入 `3-thin` 减少 2,842,896 字节（4.89%）。两组实际 Mach-O 均为 SDK 26.5 / LD 1267.0。热缓存构建耗时不作冷构建排名，性能/App/DMG 仍未测量；其余四组继续串行运行。

- 已通过仓库入口准备 Sparkle 2.9.5 官方框架与原生图标：归档 SHA 与固定值一致，框架普通文件 2,777,894 字节，`Assets.car` 2,861,624 字节；这不是 App/DMG 测量。准备时环境中的 `SPARKLE_PUBLIC_ED_KEY` 未设置；后续已复用现有签名 App 的公开配置生成正式 release Info.plist，见下方来源记录，不创建替代更新密钥。
- 额外静态候选：Sparkle 五个 Mach-O 均含 x86_64/arm64。在独立副本中保留 arm64 后框架从 2,777,894 降至 1,532,710 字节，少 1,245,184 字节（框架自身 44.82%），原始/副本 deep strict 签名检查均通过。未修改正式准备脚本/配置，未计入媒体或主程序收益；采用仍待 updater/最低 macOS/新 App 验收，见 [Sparkle 架构切片实验](../research/2026-10-10-sparkle-arm64-size-probe.md)。
- 第二组矩阵及 Sparkle 切片的独立复核无新增实质发现：两组构建输入一致，实际二进制/哈希/SDK 与记录匹配；框架按普通文件计数、排除 symlink 别名，原始框架未修改，静态切片收益未冒充 App/DMG 或性能结论。
- 已从现有 `/Applications/Norma OS.app` 的公开签名元数据恢复既有更新公钥：deep/strict 签名验证通过，Bundle ID、Team ID 与 GitHub appcast 地址匹配。仅将该公开配置传入准备子进程，生成 1,053 字节的 `build/packaging/Info.plist`，未读取/生成私钥，未启动或修改已安装 App，也未修改用户数据。来源及输出 SHA 见 `installed-public-key-source.json`；公钥配置缺口已解除，Developer ID 私钥/签名身份缺口仍存在。
- 有效矩阵第三组 `s-thin` 已成功：52,737,568 字节，849.25 秒，SHA-256 `9d13223946e326de60de40cd977c5abc88d518183029bd97f3d66d56b2cb490c`。同输入相对默认 `3-thin` 减少 5,341,952 字节（9.20%），是当前已完成组中最小值；实际 SDK 26.5 / LD 1267.0 与前两组一致。已更新 JSON/CSV/HTML，仍不据此选定正式参数，`s-fat` 及两组 `z` 继续串行实验。

| 已完成参数组 | 主程序字节 | 主程序 MB | 相对 `3-thin` 减少 | 热缓存构建秒 | 采用状态 |
|---|---:|---:|---:|---:|---|
| `3-thin` | 58,079,520 | 58.080 | 0.00% | 479.40 | 默认；真实程序待验 |
| `3-fat` | 55,236,624 | 55.237 | 4.89% | 1,314.19 | 候选；真实程序待验 |
| `s-thin` | 52,737,568 | 52.738 | 9.20% | 849.25 | 候选；真实程序待验 |
| `s-fat` | 46,685,520 | 46.686 | 19.62% | 1,014.12 | 候选；真实程序待验 |
| `z-thin` | 45,073,824 | 45.074 | 22.39% | 770.85 | 候选；真实程序待验 |
| **`z-fat`** | **39,205,408** | **39.205** | **32.50%** | 952.52 | 主程序体积最小；真实程序待验 |

表中只比较同源码、同工具链、同 SDK 的主程序；未包含 helper、App 资源或 DMG。耗时是单次热缓存实验，不作冷构建速度排名。

- 三行矩阵及公钥准备的独立复核无新增实质发现：`s-thin` 的本次 Cargo artifact、字节/SHA/输入与 SDK 均吻合；既有签名 App 的公开更新配置与生成的 Info.plist 一致，未声称获得私钥。报告已通过 Computer Use 刷新并核对三行及最小值标注，证据为 `compiler-matrix-browser-verification.json` 和对应 AX 文本；此检查只覆盖报告显示，不替代真实 Norma App 验收。
- 已完成三组主程序的静态体积拆分：逐项记录 Mach-O segment 的文件字节，并与实际产物长度相等核对。`s-thin` 主要减少代码区，部分被链接/异常回溯元数据增加抵消；`__PAGEZERO` 不计入磁盘或峰值内存。见 [构建参数体积拆分](../research/2026-10-10-compiler-size-breakdown.md)，该分析不启动真实 App，也不冒充运行时验收。
- 当前正式配置声明的输入已逐项清点，图标、本地化、完整媒体 staging、预览/catalog、浏览器扩展、planner、Sparkle、生成 plist、entitlements 和候选 binary 路径均存在。目录字节只计普通文件，排除框架 symlink 别名；这不证明已生成 App，也不证明 target 缓存是本次新构建。`packaging-input-preflight.json` 明确冻结基线的 `norma` CLI 仍缺失，等待矩阵结束后串行补齐明确构建。
- 有效矩阵第四组 `s-fat` 已成功：46,685,520 字节，1,014.12 秒，SHA-256 `306384c562df223ab897a3695a80d316ea7a85f5627a59679bd9f2d4aa9c189a`。相对同输入 `3-thin` 减少 11,394,000 字节（19.62%）；实际 SDK 26.5 / LD 1267.0 匹配。JSON/CSV/HTML 和静态区域拆分已补第四行，当前最小标记改为 `s-fat`，剩余两组 `z` 继续串行；未修改正式 profile，也未把体积最小当作性能验收通过。
- 第四组独立复核无新增实质发现：本次 artifact 与日志/字节/SHA 相符，四个文件区间连续覆盖全文件，节省率及四行报告一致。已通过 Computer Use 刷新核对 `s-fat` 最小标记，浏览器验证证据更新为四组；所有真实 App、DMG 和性能字段继续待验。
- 原资源分布补充：30 项包括 29 个媒体和 1 份原有 README，MP4/GIF 合计 144,145,159 字节（86.15%）。按格式与 core/dynamic-wallpaper/tutorial-video 策略分组都与 167,312,523 字节总清单一致；按 SHA 分组无完全相同文件，精确文件级去重无额外收益。见 [原资源占用分布表](../research/2026-10-10-source-resource-distribution.md)，这仍是源资源口径，不替代新 App/DMG 测量。
- 已安装 App 只读参考清点：0.1.73 的 101 个普通文件共 236,229,548 字节，排除 6 个 symlink 别名；壁纸/教程目录为 167,312,523 字节，占该样本 70.83%。deep/strict 签名通过，未启动或修改 App。文件清单、分类和主程序/plist SHA 存于 `installed-app-distribution.json`；其构建来源未与本 Goal 基线确认一致，仍不代替同条件新 App/DMG 基线。
- 有效矩阵第五组 `z-thin` 已成功：45,073,824 字节，770.85 秒，SHA-256 `016101a3ef99216414c312c3a816ce0c932494eeddf449f4a2dfc3988b087fc6`。较同输入 `3-thin` 减少 13,005,696 字节（22.39%）；实际 SDK 26.5 / LD 1267.0 匹配。五行 JSON/CSV/HTML 与文件区域拆分已更新，当前最小标记为 `z-thin`，`z-fat` 为最后一组仍在运行；正式参数未调整，性能/App/DMG 仍待验。
- 易读报告扩展先补失败测试：在忽略的 `build/package-size-reduction/` 中构造不同的已安装样本、资源分布、局部安装耗时/峰值和框架切片数据，4 条测试中 3 条按预期失败，缺证据不借旧数值的用例通过。测试专门区分修复与预装耗时、旧样本与新 App/DMG 基线；记录为 `report-extra-red.json`。矩阵结束前不修改编译输入，之后再将测试纳入脚本测试并实现集中展示。

### 后续候选包的同条件比较

六组矩阵已全部完成，正式 release profile 仍为 `3-thin`。`z-fat` 的 SHA-256 为 `9230bc659adbf0f770a70a2bb9aa5eb7e29e7e6797780bd9ea48e7f1a3a0dfff`，构建 952.52 秒、减少 18,874,112 字节；SDK 26.5 / LD 1267.0 与其它五组一致。JSON/CSV/HTML 和静态区域拆分均已补齐；真实 App/DMG/性能仍待验。矩阵结束后解除源码冻结，报告脚本和文档的后续改动不改写本次编译输入指纹。

易读报告扩展已完成 TDD：纳入 `scripts/test-report-extra-evidence.py` 后再次确认 3 条预期失败，再实现四个证据表；原 2 条与新增 4 条均通过。报告集中呈现原资源格式分布、已安装参考样本、局部文件安装/修复/取消探针及 Sparkle arm64 副本候选；明确区分旧样本与新基线、修复与预装耗时、局部进程 RSS 与整个 App 性能、未采用的框架候选与真实包收益。

最终六行数据及新增报告表格已由独立 subagent 对抗复核，无新增实质发现；报告 6 条测试通过。通过 Computer Use 刷新 Chrome，核对六行最小标记、资源表与安装/框架表，保存实际视口截图和 AX 文本；证据为 `report-extra-browser-verification.json`、`compiler-matrix-browser-verification.json`。浏览器检查覆盖报告显示，不能替代 Norma App 的下载、视觉和性能验收。

矩阵结束后已串行完成打包输入构建：冻结基线的 `norma --bins` 97 秒；候选 `app` production-data（关闭默认 features 与 QA）528 秒，再执行 `norma --bins` 95 秒。候选最终主程序 58,079,520 字节、browser auth host 959,968 字节、CLI 1,378,112 字节、hook 1,093,920 字节；四份 SDK 均为 26.5 / LD 1267.0。App 与 norma 都输出同名 hook，以后执行的 norma 产物为最终文件，与正式 hook 顺序一致。日志/产物 SHA 记录在 `candidate-packaging-build.json` 和 `baseline-companions-build.json`；不覆盖既有矩阵或候选快照的哈希。17 项声明输入现均存在，基线 CLI 已补齐；旧预检保留，新状态为 `packaging-input-preflight-latest.json`。输入存在和构建成功仍不能代替新 App/DMG 或签名验证。

### 继续执行的条件

此前默认 wrapper 因 Developer ID 缺失而停止，签名检查为 0 个有效身份。[release-dmg/SKILL.md](../../.agents/skills/release-dmg/SKILL.md) 要求："Use `./scripts/release-local.sh --ad-hoc` only when the user explicitly asks for an ad-hoc local test package and the release binaries already exist." 用户随后明确要求“你本地打个包我测试一下看看”，本地测试模式已获得授权；现已通过该 wrapper 生成 full/light 两个 ad-hoc 包，不绕过 wrapper 或生成替代身份。Developer ID 正式签名与公证仍未执行。

条件满足后的工作顺序：构建同版本、同 core/helper/SDK/签名/镜像配置的基线和候选包，记录实际 App/DMG 字节及验证；另外使用隔离数据根的非 production-data QA App 完成下载、离线、取消、修复、快速切换与浅深主题验收；对体积候选完成冷启动、Terminal、音视频和退出的真实对照，再决定 release profile。GitHub 原文件上传材料已就绪，资源上线与正式发版未执行；轻量生产包上线前仍须逐个 HTTPS 下载验证长度/SHA。

打包输入构建及最新预检收据也已独立复核，无新增实质发现；四个最终文件及 producer 顺序、17 项输入、基线 CLI 均吻合。此前签名阻塞和剩余验收历史保存在 `execution-readiness.json`；用户授权后的新状态见 `local-test-packages.json`。整个 Goal 仍有真实用户流程、原基线与性能验收待完成。

### 本地测试包实测

| 当前实现的资源模式 | App 普通文件字节 | DMG 字节 | DMG MB | 相对本次 full 减少 |
|---|---:|---:|---:|---:|
| full | 236,370,857 | 200,662,810 | 200.663 | 0.00% |
| light | 100,586,081 | 64,847,177 | 64.847 | 67.68% |

两组版本均为 0.1.73 / arm64，使用相同默认 `3-thin` release 输入、core/helper/framework、ad-hoc 签名及 UDZO 格式；未改为 `z-fat`。上述对照是当前实现的 full/light，不替代原冻结源码 M0 App/DMG 基线。App 字节只计普通文件，不计 symlink 别名、文件系统分配或 RSS。

两个 wrapper 均成功；codesign deep/strict、hdiutil verify、Finder 布局检查通过。分别只读挂载镜像，核对内置 App 的逐文件字节/SHA 与归档 App 一致。full 保留 30 项原资源；light 保留 18 项核心资源及 7 份预览，12 项可选原资源确认缺省。原输入二进制未被打包改写。两组均未公证，Gatekeeper 分发检查拒绝、无 stapled ticket，属于此次 ad-hoc 测试包限制。

可先用完整离线版测试功能；轻量版暂只测体积、预览及下载失败反馈，GitHub 可选原媒体尚未上线。本次没有启动 App 或修改用户运行数据，真实流程待用户测试及后续隔离 QA 验收。文件、哈希和 CSV 见 [本地包证据](../research/assets/2026-10-10-package-size-implementation/local-test-packages.json)。

签名条件满足后，基线使用冻结源码的 production-data 主程序和对应 companion binaries，候选使用 production-data 且关闭 QA 下载覆盖的主程序。两个包保持同一版本、图标、core/helper/Sparkle 清单、镜像格式和签名模式。基线携带原来的 30 份媒体；候选另外计入预览和 catalog，不给基线添加原先不存在的展示文件来放大收益。

若获许可改用 ad-hoc，本地对照仍通过 `release-local.sh --ad-hoc`，对照配置与文件调整限于本 worktree，保存并恢复；每次新 App/DMG 验证成功后另存不可混用的快照。旧开发配置没有携带的核心 helper/framework 需按正式配置在两组一致装配，并明确这属于统一对照配置。

真实下载/取消/离线交互另外使用非 production-data 的 QA App 和独立 `NORMA_TEST_DATA_ROOT`；production-data 会忽略该测试根，不能用它冒充隔离测试程序。QA 包的体积不进入正式候选对照；不启动、修改已有 `/Applications/Norma OS.app`，也不更改用户现有项目数据。已安装 App 只用于读取公开元数据和验证签名，不作为候选运行或性能样本。

### 2026-10-11 后续上线与轻量包

上述资源未上线和流程待验是当时的状态。后续已完成 [壁纸下载状态反馈与真实 App 验收](2026-10-10-wallpaper-download-feedback.md)，并按用户授权公开发布 [media-v1 原始资源](../references/macos-release-and-notarization.md#media-v1)：7 个动态壁纸、5 个教程视频，共 135,781,610 字节。全部公开 HTTPS 原文件的长度与 SHA-256 校验通过；隔离真实 App 从 GitHub 下载潮汐后显示已下载并自动应用。

最新本地轻量包保持 0.1.73 / arm64 / ad-hoc，64,868,153 字节（64.87 MB），SHA-256 `355c1e2e256dbbceb863ef8d703d422fd7c282a2dd1669544bdc1cc2bb9189c0`。当前 production-data 构建、签名完整性、DMG/Finder 布局、102 项包内文件/链接核对和 12 项远端原文件复查通过。[包信息](../research/assets/2026-10-11-cloud-light-package/package.json)、[包内核验](../research/assets/2026-10-11-cloud-light-package/contents-validation.json)、[本次打包日志](../research/assets/2026-10-11-cloud-light-package/packaging.log)。仍未公证；新轻量包不替代上表的同轮 full/light 对照或原冻结源码基线。
