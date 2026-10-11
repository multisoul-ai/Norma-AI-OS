# Norma OS 安装包体积优化：资源分发与构建方案

研究日期：2026-10-10。本文结合主 Agent 的当前仓库只读盘点，以及 Apple、Rust、Sparkle 第一方资料。没有修改资源、产品代码或发布流程，没有重新运行压缩实验。

## 结论

在“视频、图片必须能还原原文件的每个字节”约束下，最有把握的大幅收益来自**减少默认安装包携带的可选资源**，通过按需下载或独立资源包保留完整能力。此举改变资源分发方式，原始文件保持不变。它降低初次下载体积；如果用户最终下载全部资源，所有媒体的总字节数仍会回到原来的水平。

现有媒体压缩实验包含 30 个文件、167,312,523 字节，最小的通用压缩结果仅减少 0.7413%。见[已完成的无损实验报告](2026-10-09-lossless-resource-compression-report.md)。以下收益必须分别以源资源字节数、实际 App 大小、实际 DMG 大小或更新下载大小衡量，不能混算。

## 1. 首选：默认资源与可选资源分层

当前 `Packager.toml` 的 `resources` 直接包含整个 `wallpapers` 和 `feature-guide` 目录。主 Agent 盘点得到以下互不重叠的媒体分类：

| 可移出默认包的候选 | 源文件总大小（MB） | 占全部媒体字节 | 建议分发方式 |
|---|---:|---:|---|
| 壁纸目录中 5 段可选 MP4 | 98.991676 | 59.1657% | 选择相应动态壁纸时下载原始 MP4 |
| 2 个动画 GIF | 14.732208 | 8.8052% | 使用对应动画时下载原始 GIF |
| 5 段功能教程 MP4 | 22.057726 | 13.1835% | 点击观看时下载；保留文字说明与封面 |
| **三类合计** | **135.781610** | **81.1545%** | 默认包留下 31.530913 MB 媒体 |

MB 按十进制计。以上是**当前源媒体字节推算**，不是修改后 DMG 的实测降幅，也不是整体应用减少 81.1545%；新建缩略图、下载 manifest 与下载管理代码的开销尚未计入。5 张现有教程封面合计仅 0.763190 MB，可与文字说明继续随包携带。

可先只移出可选壁纸 MP4 与 GIF：减少默认媒体 113.723884 MB（67.9709%），剩余 53.588639 MB，教程仍可离线观看。若再移出教程视频，默认媒体可减少至 31.530913 MB。是否移出应由默认体验与离线要求决定。

### 可行的下载与存储设计

Apple 明确将应用管理的下载数据列为 `Application Support` 的用途。非沙盒 macOS 应用的标准位置在 `~/Library/Application Support`；沙盒应用应通过系统接口定位自身容器内对应目录。[Apple 文件管理指南](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/AccessingFilesandDirectories/AccessingFilesandDirectories.html)、[Foundation applicationSupportDirectory](https://developer.apple.com/documentation/foundation/url/applicationsupportdirectory)

以下为基于这些能力的工程设计，尚未实现：

1. 默认包保留静态壁纸、教程封面、必要图标与可离线使用的主流程。
2. 普通媒体从受控 HTTPS 源下载到 `Application Support/com.norma.os/media/`，临时下载在后台进行；不回写 `.app`。
3. 每个资源记录 ID、版本、下载 URL、长度、SHA256；manifest 使用发布方密钥签名，客户端内置验证公钥。先验证 manifest 签名，再核验文件长度与 SHA256，成功后原子提升临时文件为可用文件。文件摘要证明内容匹配预期；manifest 签名用于认证预期内容的发布方，二者用途不同。Apple 的 CryptoKit 官方支持安全摘要与公钥数字签名；Rust 实现可以使用合适的已有库，不必为了这个方案引入 Swift。[Apple CryptoKit](https://developer.apple.com/documentation/cryptokit)
4. 设置应提供下载状态、失败重试、取消、清除可选资源与“下载全部供离线使用”。选用尚未下载的视频时保留静态回退，核心工作流程可以继续。
5. 使用文件原始字节直接播放，并复用现有视频后台解码、低拷贝上屏路径。不要因下载方案改变为逐帧 CPU RGBA 渲染。

Apple 文档不允许普通资源在安装后任意改写已签名 bundle；自修改 bundle 会破坏签名封印。上述外置媒体方案因此应使用应用支持目录。TN2206 同时列出少数不破坏签名的变更，例如删除 Universal 的架构切片；这不是在 `.app/Contents/Resources` 解压和增删普通媒体的通用授权。[Apple TN2206：Moving Away From Custom Resource Rules / Changes That Don't Invalidate a Code Signature](https://developer.apple.com/library/archive/technotes/tn2206/)

长期离线媒体宜由应用管理；可再下载的短期预览可以使用 `Caches`，但不能依赖缓存永远存在。Apple 建议将可重新下载的大媒体排除备份。清除旧版本、取消后残留和临时压缩包能降低安装后的实际占用，但不缩小已经发布的 DMG。[Apple：Using the file system effectively](https://developer.apple.com/documentation/foundation/using-the-file-system-effectively)

### 离线分发变体

可同时发布“轻量应用包”和“完整离线包”，或提供独立、带签名 manifest 的媒体包导入。这样用户可以自行选择初次下载的资源量，无需改视频格式。完整离线包携带同样全部资源时，总体积不会凭拆包自动减少；要分别报告轻量包、资源包、两者合计与下载全部后的磁盘占用。此处是产品分发方案，不能直接照搬面向 App Store 的系统 On-Demand Resources 为当前 DMG 提供托管能力。

## 2. 低影响优先项：发布资源白名单与功能配置对应

建议为发布阶段生成受控 staging 目录，仅复制运行时实际使用的资源，替代把完整开发资源目录复制进安装包。需维护“功能 → 资源”映射，并让清单缺失或意外新增文件在打包检查中暴露。

主 Agent 发现以下待核验候选，**当前没有删除，也没有计入上述资源外置收益**：

| 文件或条件 | 当前大小 | 检索发现与限制 |
|---|---:|---|
| `wallpapers/home.mp4` | 5.152500 MB | 生产代码与注册 catalog 未发现直接引用；动态路径与首次启动流程仍需核验 |
| `wallpapers/desktop-earth.png` | 2.172915 MB | 未发现生产直接引用；需核验备用资源、回退与配置恢复 |
| `wallpapers/license-gate.mp4` | 3.211049 MB | `licenses` 是可选 feature；默认未启用时仍被整目录打包，需验证 production 发布 feature 与授权入口 |

“没有直接引用”仅表示值得排查，并不等于可以安全删除。实施时要用相同源码、feature、架构和版本重打两个包比较，并操作真实 App 验证壁纸选择、回退、教程与授权页面。现有仓库的 30 份媒体 SHA256 全部不同，不能承诺整文件去重可直接节省空间。

## 3. 程序与依赖体积：只把未做项计为机会

当前 [`Cargo.toml`](../../Cargo.toml) 已配置 `lto = "thin"`、`codegen-units = 1`、`strip = "symbols"`。再建议“开启 LTO/去符号/合并 codegen”不能算新的收益。未显式覆盖的 release `opt-level` 默认是 3；环境或 Cargo config 可以覆盖，因此实验需记录有效配置。[Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)

可用相同源码与依赖锁文件比较 `3/s/z × thin/fat` 的构建体积、链接耗时与关键运行性能。`s` 优先体积，`z` 还关闭循环向量化，官方明确两者不保证一定比其他级别更小；`fat` 做跨 crate 全程序优化，`thin` 通常链接更快。终端交互、音频处理与视频路径必须保留性能验收。[Cargo：opt-level / lto](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level)

`panic = "abort"` 可以作为单独实验候选，但不能直接默认启用。它终止进程，不能依靠展开执行清理；`catch_unwind` 不能捕获 abort。Norma 的 PTY 子进程收口与迟到后台任务清理需要明确核验。普通 `cargo test` 不按 release 的 abort 策略运行，不能仅以单测通过证明此变更安全。[Rust Book：panic](https://doc.rust-lang.org/book/ch09-01-unrecoverable-errors-with-panic.html)、[std::panic::catch_unwind](https://doc.rust-lang.org/stable/std/panic/fn.catch_unwind.html)、[std::process::abort](https://doc.rust-lang.org/std/process/fn.abort.html)、[Cargo：panic](https://doc.rust-lang.org/cargo/reference/profiles.html#panic)

同时可以分析最终二进制的尺寸贡献，寻找未使用的可选依赖、重复引入的库版本、被无意启用的默认 feature，以及打包进入 Frameworks/sidecar 的开发附件。是否存在以及能节省多少需对链接产物和包内容实测；源目录或 `target` 目录很大，不代表其中全部内容进入发布包。当前旧 `.app` 与未 fresh build 的约 53.031 MB 主程序只能作历史参考，不能充当当前正式包基线。

当前已发现两项具体待实验候选：`crates/app/Cargo.toml` 在普通依赖中启用了 GPUI `test-support`，可评估移到开发依赖或显式测试 feature；应用和 vendored GPUI 的 `image = 0.25` 均启用默认 feature，主 Agent 的 feature tree 发现 AVIF、EXR、TIFF 等格式活跃。若产品及用户导入流程只需 PNG/JPEG/WebP/GIF，可评估缩窄格式集。Cargo 会合并依赖的 feature，必须处理所有启用点，不能只在应用 manifest 写一次 `default-features = false`。启用某个 feature 也不证明其全部代码最终保留在 Mach-O 中，需用最终链接产物和真实 App 验证收益与行为。[Cargo：Feature unification / Inspecting resolved features](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification)

## 4. 单架构分发：有条件收益，当前主程序不能重复计功

Apple 的 Universal 二进制把 `arm64` 与 `x86_64` 两种可执行代码合在同一个文件。若产品需要同时支持两类设备，可分别发布架构对应包，下载页面或更新 feed 选择匹配版本；静态媒体本身不会因架构分包而减少一半。[Apple：Building a universal macOS binary](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary)

主 Agent 已确认当前主程序为 arm64，因此不能将“从 Universal 变单架构”列为当前主程序尚未实现的巨大收益。嵌入 framework、helper、sidecar 是否含多余架构切片需逐项检查。正式发布时应在构建、签名、公证与验收链中生成匹配的完整产物，不在用户机器上随意改写发布包。

## 5. Sparkle 增量更新：降低更新下载，不降低首次安装包

Sparkle 官方支持从特定旧版本到新版本的 delta，只下载变化部分；不支持对应 delta、应用失败或校验失败时回退完整更新。生成器能够自动生成并签名 delta。稳定文件路径、减少不必要的媒体重写、把常变内容与不变内容拆成独立文件，有助于减小差分。[Sparkle：Delta Updates](https://sparkle-project.org/documentation/delta-updates/)

仓库当前 `scripts/finalize-sparkle-release.sh` 显式传入 `--maximum-deltas 0`；相关测试也约束首期关闭 delta。后续可以设计保存历史 `.app`、生成适配版本的补丁、签名与发布、失败回退验证的流程。不要将关闭 delta 的参数机械删除后宣称功能完成。

这一方案降低的是老用户升级时的下载量与可能的安装时间；新用户首次完整下载、最终安装的 `.app` 大小不因此减少。不同版本改动量不同，不能未经实测承诺固定降幅。

## 6. 不满足当前原字节约束的候选

Apple 官方说明 HEIF/HEVC 相比 JPEG/H.264 可以在相近视觉质量下使用更少空间，并指出部分旧设备受到分辨率与帧率兼容限制。[Apple：Using HEIF or HEVC media](https://support.apple.com/en-us/116944)

但“视觉质量相同”不等于能恢复原始文件字节。H.264 转 HEVC、JPEG 转 HEIF、GIF 转视频、降低分辨率/帧率、裁剪时长、移除音轨或元数据、重新封装 MP4，均会改变原文件。即使某种操作保持所有像素或编码视频流，也不能只靠新文件重建原容器的每个字节。因此这些方案不进入当前合规主方案，也不计入无损压缩结果。只有用户另行允许改变媒体文件时，才适合独立做画质、兼容性与体积实验。

## 推荐实施顺序与验收

1. 建立当前正式产物基线：记录版本、commit、feature、架构、工具版本，列出 App 内每个文件大小与 SHA256，并记录同条件 DMG 大小。
2. 优先引入发布 staging 清单，排查遗漏或多余文件，核验待清理候选；不要直接删除源资产。
3. 提供静态默认资源与动态媒体按需下载，优先移出 113.724 MB 可选壁纸视频/GIF；保留教程离线或逐步改成点击下载。
4. 对每个阶段实测默认 DMG、默认安装占用、下载全部后的占用，以及首次资源使用时间；验证断网、损坏下载、取消、升级、资源清理与签名仍有效。
5. 并列进行构建参数小矩阵与可选依赖审计；独立开展 Sparkle delta 验收。不要将二者的收益相加后未经重打包就作为最终降幅。

本文全部百分比都是现有源媒体分组计算或已完成实验结果。尚未实现的方案没有声称正式 DMG 实测收益。
