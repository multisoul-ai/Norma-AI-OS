---
template: doc
theme: paper
lang: zh
title: Norma OS Windows 适配计划
subtitle: 先交付核心工作流，再补齐浏览器、语音与系统集成
日期: 2026-11-01
代码基准: 120492ca；探针分支 codex/windows-feasibility-20261101
状态: App18条消除、仍98条；关闭与Mac隔离门槛未关闭
---

这是一次中到大型平台移植。保留现有 Rust、GPUI 和域层，在系统能力边界增加 Windows 实现。

测试与验收以 `docs/exec-plans/2026-11-01-windows-github-runner-goal.md` 的 Goal 任务书为准。
默认在标准 GitHub Windows runner 构建并执行验收。
首版云端证据对应 Windows Server 2025，Windows 11 设备验证单独记录。

2026-11-01 已实现并运行标准 runner 最小探针。
实测环境与图形、编译、Agent 条件分开记录，未通过项保持未通过。
运行链接、日志索引与阶段状态见 Goal 任务书的本轮验证记录。

首轮收束：[41297206819](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819)，整体为 failure。
源码版本为 `58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4`。
GPUI 示例绘图与 OS 键盘关闭通过；产品编译失败。
真实 Agent 和候选包均未执行，不能据此宣布预览版就绪。

前批复验：[41307703559](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559)，整体仍为 failure。
源码为 `dae96214474474dffe6e0fc96efabb9d2e14990f`。
norma 编译与 43 项平台回归通过；App 检查仍失败，完整迁移尚未通过。

前批首轮修复：[41325043580](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41325043580)。
源码 `f2838b21e04f7eb5c7ccecfc45271ec521ebe186`，整体 failure。
原 storage 25 条诊断与 A1 工厂错误消除；语音实际 89 通过、0 失败、16 既有忽略。
原生文件回归 4 项、记忆发布 1 项、Review 中文 capture 与进程收口均通过。
App 默认检查仍在 runtime/Codex 的六项 Unix socket/relay 引用处失败。

Git 路径修复后复验：[41327614822](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41327614822)。
源码 `747777db081a4f06005ee3b8b617de91db29fe8c`，已结束，整体 failure。
平台回归 30/0/0、语音 89/0/16、原 43/52 项无回退；App 仍被 runtime/Codex 六项阻塞。

Mac 定向语音 96 通过、0 失败、17 既有忽略；storage/workspace 338 通过、0 失败。
Mac 原 4 项压力失败在本批基线复现；修复夹具资源输入，生产阈值保持不变。

A1 模型引擎仍要求 Apple Silicon macOS。
本轮 Windows 测试只验证工厂与管道。

完整 workspace 为 3415 通过、15 失败、52 既有忽略，仍为 FAIL。
隔离对话 38 项、提议 1 项和 OpenCode 1 项通过，不能覆盖原完整失败。
i18n 的既有 GPT-Live 文案失败保留；真实 Norma GUI、Agent 与候选包均未验。

前批复验：[41345708985](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41345708985)，源码`e7a7a78a9a871748f28becf1a6de32702ce41cd9`，整体failure。
原Runtime/Codex六条已消除；App当前116条按域为Browser59、IPC13、手机23、Preview11、生命周期/brain10。
Windows控制50/0/0、Runtime11/0/0、平台47/0/0、canvas52/0/0、语音89/0/16。

本轮关闭测试PASS，但旧FAIL尚无修复证据；外部瞬时背压未确定relay输出背压。
partial写额度为测试故障注入，不能代表OS关闭可靠性已验收。

Mac原15项前后逐项通过；最后完整3419/16/52，norma/runtime后续687/0/12。
OpenCode输入锁症状尚未归因，不能宣称Mac无新增回归。

完整逐项表与日志索引见 `docs/quality/evidence/windows-feasibility-20261101/runtime-codex-audit.md`。
真实Norma、真实Agent与候选包仍NOT_RUN；未新增忽略或禁用能力。

### App IPC/图片边界与 Mac 16 项复验（本批）

原 116 条诊断已按 12 类根因逐项归档。
实现复验：[41384140463](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41384140463)，源码 `a53698f635c1501dd37aa4799add30fc4f68b164`，attempt 1，整体 failure。
先行 RED：[41377422839](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41377422839)，源码 `1d5b8e2441f3a0f9515b589ee4f2bf0fe92b7d51`。

| 项目 | 实测 | 门槛 |
|---|---|---|
| App 默认 check | 原 13 条 IPC、5 条图片文件错误消除；仍有 10 类 98 条 | FAIL，未默认编译成功 |
| 实际 App 适配 | Windows 15/0/0，含请求响应、端点身份、超时、取消、图片边界 | 仅前置通过，不替代 App/权限完整验收 |
| norma 与原平台 | 47/1/0；原 43 项通过名全部保留 | 新背压 shutdown 见证失败，命令 FAIL |
| Codex/Runtime 控制 | 控制 49/2/0；Runtime 11/0/0 | 强见证与原 shutdown 契约失败 |
| 独立平台/canvas/语音 | 30/0/0、52/0/0、89/0/16 | 16 个语音 ignored 为既有项 |
| Mac 完整 A/B | A 3428/14/52；B 3443/0/52 | A 的 14 项既有语言/锁失败仍保留，不能宣称修复 |
| Mac 两项 OpenCode | 确定性输入锁 RED、HTTP 非阻塞继承；修复后 A/B 完整均通过 | 原 session、唯一 POST、不重发与门闩均保留 |
| 真实 Norma GUI/Agent/候选包 | NOT_RUN | 未以图形示例、协议夹具或 CLI version 冒充 |

App IPC 保留 32/64 队列与 4 个 worker，增加可取消的绝对 IO 预算。
Windows 以 AF_UNIX reparse tag 与 volume/fileIndex 识别端点，通过 pinned HANDLE 删除。
图片复用私有 DACL，拒绝 reparse；Unix 文件与 Socket 行为保留。
没有关闭 VAD、Sherpa、Codex、Browser 或默认产品 features，没有新增 ignore/allow。

关闭日志记录 131072 字节未读，mask0 没有事件，RDNORM 只有可读。
当时 writer 仍持有 source 句柄，只能证明 shutdown 后未在 3 秒退出，不能证明真实退出后的收口。
强见证已修正为 writer 先 drop 并 ack，join、释放最后 clone 后才开始原 3 秒窗口。
旧 shutdown 契约继续保留，不能用新测试覆盖旧 FAIL。

最终复验：[41386261217](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41386261217)，源码 `120492ca29ec49b12ed4368eb0d10e01b5d34845`，attempt 1，整体 failure。
App 默认 check 仍为 98 条，原 18 条消除；norma 库/CLI/helper 编译通过。
原平台目标为 47/1/0，原 43 项通过名完整保留；失败只有新增的严格 close 见证。

控制 50/1/0、Runtime 11/0/0、identity 1/0/0；两个 OpenCode 目标各 1/0/0。
实际 App 适配 15/0/0、独立平台 30/0/0、canvas 52/0/0、语音 89/0/16。

强见证记录真实输出 8 次连续 WouldBlock、5 个源帧、全部 source 句柄释放 true，原 3 秒内仍未退出。
原 shutdown 测试本轮通过，但历史 FAIL 没有生产修复证据，可靠性仍未收束。

当前 AC 为 0 PASS、3 FAIL、9 NOT_RUN；AC01、AC02、AC11 不标完成。

中间运行 41385201505 已因关闭见证前提修正主动取消，不用作通过证据。
报告 SHA、run/attempt 一致，未缺报告；完整日志和哈希清单保存在对应 run 目录。


原 116 与当前 98 逐项对照、Mac 16 项前后与所有日志入口：
`docs/quality/evidence/windows-feasibility-20261101/app-platform-audit.md`。
`docs/quality/evidence/windows-feasibility-20261101/app-116-comparison.json`。
`docs/quality/evidence/windows-feasibility-20261101/app-platform-local/macos-16-comparison.json`。
本分支提交 Draft PR，不合并；AC01、AC02、AC11 均未关闭，迁移 Goal 未完成。

## A 改动量与投入

```callout info 推荐先交付核心预览版
以 Windows 11 x64 为首个目标。先完成画布、终端、Agent、保存和恢复适配。
核心预览版估算 20–33 人日。浏览器和语音进入后续 Beta。
```

| 交付级别 | 累计投入估算 | 单人全职日历时间 | 交付含义 |
|---|---|---|---|
| 编译与启动样机 | 5–9 人日 | 约 1–2 周 | 能构建并打开窗口，尚未通过核心工作流 |
| 核心预览版 | 20–33 人日 | 约 4–7 周 | 项目、画布、终端、两个已验证 Agent、持久化和便携包 |
| 主流功能 Beta | 36–60 人日 | 约 7–12 周 | 增加内置浏览器、录音、播报及安装发布验收 |
| 接近当前功能覆盖 | 51–90 人日 | 约 10–18 周 | 增加动态壁纸、Ego、手机连接、更多 Agent 和自动更新 |

这些区间是工程估算，不能视为交付承诺。每周按 5 个工作日计算。
投入包含编码、聚焦测试与联调。估算假设由一名熟悉 Rust 的工程师执行。

估算假设标准 runner 可承载核心场景，并可安全接入已授权 CLI 凭据。
本轮只验证 CLI 无认证前置；真实调用仍需保护环境与预算。
外部 CLI 变更和跨架构支持另计。
各行是累计投入，不可相加。P0 结束后重新估算。

源码搜索命中 9 个 crate、109 个平台相关 Rust 文件。
其中 27 个文件含 Unix 或文件描述符接口；27 个涉及 Shell 或 Unix 端点。
搜索排除了测试目录，类别之间存在重叠。
已隔离的 macOS 实现也会命中，109 不代表编译错误数。

核心版预计触及 40–70 个 Rust 源文件。
另预计涉及 20–40 个测试、脚本和配置文件。
这些是改动范围估算，实际范围以 P0 的编译报告为准。
不使用代码行数推算交付时间。

## B 已确认的改动与证据

| 能力 | 当前事实及定位 | 适配方向 | 难度 |
|---|---|---|---|
| 窗口与渲染 | GPUI 已包含 Windows 后端；`vendor/gpui-0.2.2/src/platform.rs:24` | 保持钉死版本，先验证 Windows 渲染、字体和输入法 | 中 |
| 终端与命令 | 已使用 portable-pty；默认 Shell 为 zsh；`crates/terminal/src/pty.rs`、`crates/runtime/src/runtime.rs` | 复用 ConPTY，替换 Shell 选择、参数构造和进程管理 | 高 |
| App 本地通信 | CLI 与 App 直接使用 Unix Socket；`crates/norma/src/client.rs:8`、`crates/app/src/norma/transport.rs:7` | 给现有协议增加受限 Windows 本地传输 | 高 |
| Codex 控制 | 启动命令使用 unix 端点，relay 使用 POSIX poll；`crates/runtime/src/nodes/agent/codex.rs`、`crates/norma/src/codex/relay/transport.rs` | 单独验证 CLI 支持的 Windows 传输 | 高且待验证 |
| 安全文件访问 | 记忆存储与 Review 使用 openat、目录句柄；`crates/storage/src/memory/fs.rs`、`crates/workspace/src/review/scoped.rs` | 增加 Windows 句柄与路径边界实现 | 高 |
| 编辑保存 | 冲突保护依赖文件交换；`crates/workspace/src/content/atomic.rs` | 证明 Windows 替换、备份与冲突恢复语义 | 高 |
| 浏览器 | Wry 后端直接导入 AppKit 和 WebKit；`crates/app/src/app/browser/wry_backend.rs` | 拆开平台实现，验证 Wry 与 WebView2 | 中高 |
| 录音与播报 | 非 macOS 录音返回不支持；播报无 Windows 后端；`crates/voice_input/src/audio_capture.rs:57`、`crates/app/src/app/voice/playback.rs` | 增加 Windows 音频输入、输出与设备处理 | 中高 |
| 系统集成 | 手机地址枚举依赖 getifaddrs；更新使用 Sparkle；`crates/app/src/mobile/interfaces.rs`、`crates/app/src/updater.rs` | 替换系统能力，分阶段开放 | 中高 |
| 发布与验收 | 只有 macOS 构建发布；`Packager.toml:9`、`.github/workflows/ci.yml:15`、`.github/workflows/release.yml:19` | 增加 Windows 构建、测试、安装包和真实程序验收 | 中 |

画布几何、ANSI grid、协议数据和网络 Provider 逻辑优先复用。
域层不含 GPUI，不代表其全部文件已经跨平台。
存储安全与命令启动是本次移植的主要工作。

## C 首版范围与技术选择

| 项目 | 核心预览版 | Beta 与后续版本 |
|---|---|---|
| 平台 | Windows 11 x64，MSVC，原生运行 | Windows ARM64、Windows 10 和 WSL 集成另立范围 |
| 工作流 | 项目、画布、终端、保存恢复、文本任务分发 | 扩展设备和系统集成 |
| Agent | Claude 与 Codex 为候选；P0 验证后冻结两个支持项 | 逐个验证其余 Agent，不批量宣称兼容 |
| 浏览与预览 | 保留文本和 Markdown 预览；网页先交给系统浏览器 | WebView2 内置 Browser 与 HTML 预览 |
| 语音与媒体 | 保留现有能力，补Windows等价实现；未验收不发布 | 录音、播报、设备切换后进入 Beta |
| 高级能力 | 暂缓动态视频、Ego、手机连接和自动更新 | 单独通过验收后开放 |
| 发布 | 可复现的便携包，附功能支持说明 | Beta 使用签名安装包和手动升级 |

首版不要求用户安装 WSL，也不把 Git Bash 当作系统前提。
终端优先检测 PowerShell 7，回退到系统 Windows PowerShell。
外部 Agent 的 Windows 发行物和运行依赖须单独检查。
若候选 Agent 失败，更新范围与估算，不能用普通终端冒充支持。

在现有职责模块下增加平台实现，避免全仓搬家。
先使用同名接口和小范围条件编译，不建立包办一切的平台 crate。
能力可用性采用结构化状态，在 UI 边界翻译。
Windows 默认构建必须明确可用能力，不能依赖隐藏构建参数才能启动。

## D 核心预览版的实施顺序

```timeline v
P0 | 2–4 人日：Windows 实机探针 | 确定依赖、Agent 传输和测试驱动能否落地
P1 | 3–5 人日：编译与窗口 | 建立 Windows 构建，隔离原生模块并打开窗口
P2 | 5–8 人日：通信与安全存储 | 打通本地协议、记忆、路径与冲突保护
P3 | 6–10 人日：终端和 Agent | 打通输入、控制、恢复与子进程收尾
P4 | 4–6 人日：预览版验收 | 完成真实工作流、便携包和 macOS 回归
```

P0–P4 合计 20–33 人日。各阶段以验收结果作为下一阶段入口。

| 阶段 | 具体任务 | 涉及位置 | 完成条件 |
|---|---|---|---|
| P0 | 在标准 GitHub Windows runner 固定 MSVC、SDK 和 CLI 版本；先探测图形与显示会话，再验证 GPUI、ConPTY、CLI 传输、文件替换与测试驱动 | `vendor/gpui-0.2.2`、`crates/terminal/src/pty.rs`、`crates/norma/src/codex.rs` | 有编译错误分类、环境能力报告和冻结后的首版支持矩阵；桌面环境不足不能记为通过 |
| P1 | 隔离 WebKit、CEF、Ego、音频、视频和手机网络的原生入口；补 Windows 能力分支；增加 CI | `crates/app/src/app/mod.rs`、`crates/app/src/app/browser.rs`、各 crate 的 Cargo 配置、`.github/workflows/ci.yml` | Windows 默认构建和 release 构建通过，实际窗口可打开 |
| P2 | 实现 Norma 本地通信；保留协议与用户隔离；移植记忆文件访问、锁、路径判断、备份恢复和 Review 边界 | `crates/norma/src/client.rs`、`crates/app/src/norma/transport.rs`、`crates/storage/src/memory/fs.rs`、`crates/workspace/src/content/atomic.rs`、`crates/workspace/src/review/scoped.rs` | 同用户通信成功；越权、路径越界和并发写入测试通过 |
| P3 | 拆分程序参数与 Shell 脚本；支持可执行文件和命令包装器；适配 hook、Agent 恢复、进程观测与整树退出 | `crates/runtime/src/session_snapshot.rs`、`crates/runtime/src/nodes/agent.rs`、`crates/runtime/src/nodes/agent/codex.rs`、`crates/terminal/src/process_tree.rs`、`crates/norma/src/orchestration/installer.rs` | 两个已冻结 Agent 均通过任务、提问、取消、恢复及退出验收 |
| P4 | 补 runner 可覆盖的键盘与界面验收；打包主程序和 helper；新 job 下载同一包运行真实场景；验证现有 macOS 流程 | `crates/app/src/shortcuts.rs`、`crates/app/src/menu_bar.rs`、`test/desktop-e2e`、`packaging` | 干净 job 不从源码重建即可完成工作流；Windows 11 与物理设备能力单独记录 |

每个功能先补可失败的聚焦测试，再实现平台分支。
每个阶段分小批次合入，持续保留 macOS 构建和回归验证。

## E 必须先验证的技术难点

```callout warn 先解决会改变排期的未知项
GPUI 有 Windows 后端，只能证明存在移植基础。
CLI 传输、文件安全语义和真实 GUI 驱动仍需 Windows 实测。
P0 完成前，不把当前区间缩成固定交付日期。
```

| 难点 | 首选方案 | 必须证明的行为 | 未通过时的处理 |
|---|---|---|---|
| Norma 本地 IPC | Named Pipe；限制当前用户；禁用远程访问；保留现有消息边界与超时 | 重启、拥塞、取消、进程退出、跨用户拒绝；兼容现有环境变量 | 改变内部传输实现；不能退回无认证 TCP |
| 外部 CLI 控制 | 分别验证各 CLI 的正式 Windows 传输；与 Norma IPC 分开设计 | Codex 原生控制、提问、取消、完成与会话恢复；准确绑定 card 和 session | 记录阻塞并调整排期；Named Pipe 不能假定被 CLI 支持 |
| 安全文件访问 | 根目录句柄、reparse point 检查、硬链接检查、文件身份和私有访问控制 | junction、符号链接、路径替换、硬链接及并发访问不能越界 | 核心记忆存储不满足则阻塞发布 |
| 冲突保护保存 | 验证 Windows 替换与备份方案，不照搬 POSIX 文件交换 | 外部并发修改和进程中断后，两份用户内容均可恢复 | 若暂不能证明，首版内容编辑改只读并明确提示 |
| 进程生命周期 | 先核对 portable-pty 行为，再补 Windows Job Object 或受控进程树机制 | 取消、关项目、主动退出均无孤儿进程；不误杀其他卡片 | 阻塞核心预览版，不能只实现父进程 kill |

直接执行程序时保留独立参数，避免拼接 Shell 字符串。
对确需 Shell 的调用，分别实现 PowerShell 与 POSIX 转义。
测试包含中文、空格、引号、美元符号和多个同名 CLI。
Windows 路径覆盖盘符、UNC、大小写、保留名称和长路径。
默认数据目录、日志与会话标识须保持开发版和正式版隔离。

## F Beta 与完整能力的后续工作

| 阶段 | 新增投入估算 | 任务与交付 |
|---|---|---|
| P5 内置浏览器 | 6–10 人日 | 复用 Wry 接口，增加 WebView2 后端；验证自定义协议、子窗口裁剪、焦点、缩放、认证与原生消息注册 |
| P6 语音输入输出 | 6–10 人日 | 增加 Windows 录音与播放；验证取消、播报完成、设备切换和双工；复用云端协议；核对凭据存储和 Sherpa Windows 包 |
| P7 Beta 发布 | 4–7 人日 | 选择并验证 Windows 安装器；签名主程序和 helper；检查 WebView2 前提；覆盖升级、卸载保留数据和真实回归 |
| P8 能力扩展 | 15–30 人日 | 逐项移植其余 Agent、手机连接、Ego、视频壁纸和自动更新 |

P5–P7 使 Beta 累计投入达到 36–60 人日。
加入 P8 后，累计投入达到 51–90 人日。
多个功能可以分开交付，但单人排期不能按并行任务折半。

语音先支持现有云端 Provider 与硬件输入输出。
Local Sherpa、A1 和 MLX 相关路径分别列支持状态。
不以“有语音”代表所有本地模型均已支持。

视频阶段先验证 GPUI Windows 的低拷贝上屏能力。
候选为 Media Foundation 与 GPU 共享纹理。
若现有接口不足，单独评估 vendor 修改与性能验收。
禁止逐帧转换 CPU RGBA 作为正式实现。

手机连接保留配对、认证和后台项目生命周期。
移植地址枚举，并验证 Windows 防火墙与关闭窗口行为。
自动更新单独实现 Windows 包校验及失败恢复。
Beta 先使用签名安装包手动升级，不复用 Sparkle。

## G 验收标准与测试映射

| 验收项 | 单元或集成测试 | 真实环境证据 |
|---|---|---|
| 构建与启动 | Windows MSVC 默认与 release 构建；检查未隔离原生依赖 | Windows 安装或解压后实际启动；干净环境无开发目录依赖 |
| 项目与画布 | canvas、runtime、storage 的选择、布局和持久化测试 | 建项目、建终端、移动缩放、退出重开，状态一致 |
| PTY 与进程 | terminal 与 runtime 的参数、resize、取消及生命周期测试 | PowerShell 与交互式 CLI 可输入；Ctrl+C 可中断；退出无遗留进程 |
| Agent 控制 | norma、runtime、agent、brain 的协议、hook、session 归属测试 | 两个已支持 Agent 各完成一次任务；提问、取消、恢复不串卡片 |
| 文件与隔离 | storage、workspace 的 junction、硬链接、锁、并发和恢复测试 | 中文及空格项目路径可用；篡改与越界请求被拒绝 |
| 界面与键盘 | ui、app 的快捷键映射、能力状态及国际化测试 | runner 实际覆盖的中英文、亮暗主题和键盘；Windows 11、中文 IME、DPI 和多显示器另列设备验收 |
| Browser 与语音 | Provider 协议、音频队列和播放取消测试 | Beta 真正加载网页；麦克风录音与扬声器播报均成功 |
| 发布与回归 | 校验包布局、helper 发现、数据保留；Beta 补安装与升级 | 当前 Goal 验便携包；安装升级卸载进入 Beta；macOS 原有核心流程通过 |

运行受影响 crate 的测试；跨 crate 集成批次执行 workspace 验证。
执行对应的 i18n、依赖边界、性能和文件行数护栏。
POSIX 专属测试用条件编译分开，Windows 补等价断言。
不能通过跳过全部测试来获得绿色 CI。

现有 Swift 桌面驱动以 macOS 为目标。
Windows 先复用场景语义和应用内控制协议。
P0 选择薄 Windows 驱动，验证键盘输入和真实窗口截图。

真实 GUI 验收先探测标准 hosted runner 的交互桌面与图形能力。
通过后在同一 Actions 流程运行真实程序并收集证据。
环境不足则记录未通过，后续可接入已授权的 self-hosted GitHub runner。
内部状态、模拟点击和截图存在，均不能替代最终结果验证。

Windows 保留现有界面层级，并采用平台熟悉的窗口与键盘行为。
Ctrl+C 在终端保留中断含义，复制优先采用 Ctrl+Shift+C。
新增或调整的 App 快捷键均进入 Settings。
UI 实施前加载 apple-design 与对应 gpui 技能。
采用其层级、反馈和可访问性原则，并验证 Windows 原生习惯。

## H 下一步与执行边界

1. 先区分Windows shutdown与全部source close契约，再适配App生命周期/Brain/内容、手机与Browser；保留提问、取消、恢复及session归属。
2. 在已通过的原生文件基础上，补齐 storage/workspace 的冲突、恢复和完整安全矩阵。
3. 适配 Runtime 的 PowerShell、ConPTY、hook 和进程树收口。
4. 在已验证的 runner 图形条件下验收真实 Norma 窗口。
5. 安全接入 Agent 凭据与调用预算，执行真实控制场景。
6. 产出同一源码版本的候选包，再进行新 job 下载复验。

已经执行 Windows runner 编译探针，Windows 11 用户设备尚未验收。
原 14 个 norma 错误、5 个 VAD 错误及 25 个 storage 诊断已消除。
norma 默认 CLI/helper 编译、43 项平台回归与 canvas 52 项通过，无新增失败或忽略。

Windows 默认语音测试已实际执行，不再停于 A1 工厂编译。
Win32/NT 文件能力保留防 reparse、硬链接、ADS、目录身份和真实刷盘；并发旧读者不阻塞指针替换。
Unix 文件适配、原有 VAD、A1 模型运行边界与既有忽略保持不变。
没有通过禁用产品能力消除错误，也没有按局部通过下调工作量。

App尚未通过，原Runtime/Codex六条与本轮App18条已消除，当前前沿为98条。
Windows关闭契约与Mac既有语言/锁隔离仍阻塞；不能删导入、关闭Agent或放宽断言。

基础图形前置已经实测通过，P0 的 ConPTY、文件语义和真实 Agent 仍待验。
P0/G1 尚未完成，不下调现有估算，也不冻结两个 Agent 的产品支持矩阵。
原图、日志与哈希索引位于 `docs/quality/evidence/windows-feasibility-20261101/README.md`。

前批修复 norma 与 Sherpa 依赖；本批适配 NT 文件和 A1 管道/进程边界，保留既有 GPUI 版本与产品能力。
没有新增通用平台框架。
构建与验收日志保存到任务证据目录后，再定向回收缓存。
现有缓存清理脚本依赖 POSIX 工具，不能直接照搬到 Windows。
