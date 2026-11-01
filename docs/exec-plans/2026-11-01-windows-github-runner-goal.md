---
template: doc
theme: paper
lang: zh
title: Norma OS Windows 核心预览与 GitHub Runner 验收 Goal
subtitle: 在 Mac 开发，在组织公开仓库的标准 Windows runner 构建与验收
日期: 2026-11-01
代码基准: f0740f6c；探针分支 codex/windows-feasibility-20261101
状态: VAD 原 5 错误已消除；App storage 与 ASR/A1 阻塞未通过
---

交付 Windows x64 核心预览包，并以 GitHub Actions 的运行记录作为验收依据。
默认使用标准 `windows-2025` runner，无需迁移日常开发环境。

## A 可直接启动的 Goal 任务

```callout info Goal 定义
适配 Norma OS 的 Windows x64 核心工作流。
保留现有 Rust、GPUI、域层和 macOS 行为。
使用 GitHub runner 完成构建、测试、真实程序场景及预览包验证。
所有必验项目通过，并交付对应源码版本的程序与证据后，目标才完成。
```

首版交付项目、画布、终端、文本任务分发、保存和恢复。
Claude 与 Codex 是两个候选 Agent，G1 验证后冻结支持矩阵。
本轮不把普通终端启动算作 Agent 控制完成。

内置 Browser、语音、动态壁纸、Ego、手机连接和自动更新沿用后续阶段。
Windows 11 用户设备验收与 Windows ARM64 支持单独记录。
本 Goal 的完成名称为“Windows x64 云端验收预览版”。
它不代表 Windows 11 所有设备能力已经验收。

| 边界 | 约定 |
|---|---|
| 开发环境 | 继续在 Mac 修改代码；Windows 特有行为由 runner 验证 |
| 主要执行环境 | `multisoul-ai/Norma-AI-OS` 公开仓库，标准 `windows-2025` x64 runner |
| 技术版本 | 保持 `rust-toolchain.toml` 的 Rust 1.91.1；不直接采用镜像自带 Rust |
| 产品结构 | 不升级 GPUI，不重写画布、grid 或 Agent 协议，不新建通用平台框架 |
| 本次交付 | 修复三处 Sherpa 平台依赖声明，保留 VAD 和默认产品能力；新增语音编译与测试 gate |
| 后续启动 | 先补 ASR/A1 平台边界和 storage/workspace 安全实现，再持续复验 App |

实施时保留工作区已有改动；只提交本 Goal 的完整变更。
任务拆分与平台改动范围沿用 `docs/exec-plans/2026-11-01-windows-port-plan.md`。

## B Runner 能验证什么

标准 `windows-2025` 镜像运行 Windows Server 2025。
这与 Windows 11 是两个运行环境，不能合并声明支持状态。
当前镜像包含 Visual Studio、Windows SDK、PowerShell 和 Git。
镜像会更新，每次运行仍须记录实际版本。

本次实测镜像为 `win25-vs2026`，版本 `20261021.278.1`。
OS 为 Windows Server 2025 x64，Rust 固定为 1.91.1，Node 为 22.23.4。
进程与活动控制台均在 session 2；存在 1024×768 显示器。
D3D11 设备创建与 WARP 查询均成功，返回 feature level 11.0。
这些环境事实不能单独替代 GPUI 像素和 OS 输入验证。

| 验证项目 | 默认环境与方式 | 通过依据 |
|---|---|---|
| Rust 构建、单测与集成测试 | 标准 hosted Windows runner | 实际 MSVC 编译、测试退出码与报告 |
| ConPTY、IPC、文件和进程 | 标准 hosted Windows runner | 真实 OS 资源测试，包含失败和清理路径 |
| 窗口、渲染和基础交互 | 先由 G1 探测显示会话、DirectX、窗口与输入 | 实际程序、可见像素、OS 输入和行为结果 |
| 两个真实 Agent | 受保护的 Actions 手动验收任务 | 固定 CLI 版本、真实认证、任务产物及 session 回执 |
| 便携包 | 新的 hosted Windows job，下载同一候选包 | 无源码构建步骤，解压后真实运行核心场景 |
| macOS 回归 | 既有 macOS runner；新增差异按需验收 | 相关测试与真实 App 证据，不破坏现有流程 |
| Windows 11、物理设备和音频 | 标准 Server runner 不作为最终证据 | 保持未验收；后续由具备设备的 GitHub runner 覆盖 |

```callout warn 图形能力是首个环境门槛
不预设 hosted runner 能满足 GPUI 的图形与输入需求。
先运行窗口探针，检查是否有可用桌面和实际渲染。
若失败，记录 ENV_UNSUPPORTED；不能跳过真实窗口验收后宣布目标完成。
```

若标准 runner 无法承载核心桌面场景，继续完成可独立验证的迁移任务。
桌面验收阶段保持未通过，并提交探针日志和可复现原因。
后续可接入交互式 self-hosted Windows GitHub runner。
该机器须由用户提供或授权，不自动购买、扩容或注册机器。
不能为得到绿色结果，自动改成只编译不验收。

## C 阶段任务与退出门槛

| 阶段 | 投入估算 | 实施任务 | 退出门槛 | 本轮状态 |
|---|---|---|---|---|
| G0 基线与 CI 入口 | 1–2 人日 | 固定支持范围；修复现有域契约检查错误；建立 Windows 探针工作流 | 原护栏保留；失败测试先复现；探针可在迁移分支执行 | 最小探针入口通过；完整阶段未关闭 |
| G1 Runner 能力探针 | 1–2 人日 | 检查 MSVC、SDK、磁盘、DirectX、桌面输入、截图、ConPTY、CLI 支持与文件语义 | 产出能力报告；核心桌面路径可行；冻结 Agent 版本及支持矩阵 | 基础 GPUI 与 CLI 前置通过；ConPTY、文件语义和真实 Agent 待验 |
| G2 编译与窗口适配 | 3–5 人日 | 隔离 macOS 原生模块；补 Windows 能力状态；默认与 release 构建 | 真实 MSVC 构建通过；主程序和 helper 可启动 | norma 与 voice_input check 通过；App storage 和语音 A1 编译失败；阶段未通过 |
| G3 IPC 与安全存储 | 5–8 人日 | 本地传输、身份隔离、记忆文件、路径、锁、冲突和恢复 | 越权、junction、硬链接、并发及恢复测试通过 | NOT_RUN |
| G4 PTY 与 Agent | 6–10 人日 | PowerShell、参数、hook、提问、取消、恢复和整树退出 | 两个冻结 Agent 的真实工作流通过；无串卡片或遗留进程 | NOT_RUN |
| G5 核心场景与预览包 | 4–6 人日 | Windows 桌面驱动、双语场景、便携包复验和 macOS 回归 | 必验矩阵通过；新 job 验证同一包；发布证据可追溯 | NOT_RUN |

G0–G5 合计估算 20–33 人日。
估算按一名熟悉 Rust 的工程师全职投入计算。
G1 若发现图形、CLI 或文件语义阻塞，更新排期。
Runner 排队、外部认证和机器准备时间不折算为编码人日。

G0 的已知故障是 `scripts/check-domain-contracts.py:57` 的解析异常。
现已先复现失败，再改读实际 schema 子模块并显式使用 UTF-8。
缺失工具会返回契约失败，不再抛出 `IndexError`；原护栏保留。
本地报告 CLI 6 项、契约检查 4 项测试通过，Windows job 亦已执行通过。
基线运行：[41282011261](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41282011261)。

### 首轮最小可行性验证

本轮验证编译、图形及 CLI 前置，分别对应 AC01、AC04、AC05/AC06。
AC10 明确保留未执行，不用 GPUI 示例冒充产品候选包。
G0 的探针入口与契约脚本修复已通过；G1 仍是部分验证。
ConPTY 生命周期、文件安全、完整 Agent 控制与产品 GUI 尚未验收。

第一轮运行：[41295234651](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41295234651)。
源码为 `cbd146d02e1d98ac51d3d8e9deb8879b01e0f9b4`，汇总门槛结果为 failure。
`canvas` 52 项测试通过；`norma` 与 `app` 的 Windows check 均退出 101。
当前暴露 14 个错误，集中在 norma 的 Unix 导入、权限和条件编译引用。
App 检查被该依赖阻塞，14 个错误不是所有下游问题的总数。

GPUI 示例构建通过，但前台窗口控制未取得，图形门槛未通过。

第二轮运行：[41296333985](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41296333985)。
源码为 `366b47182f12c7ba68b589cc7bed8148fecdca11`。
驱动按实际 `Zed::Window` 类定位，取得前台并保存实际窗口原图。
模型查看原图，确认 Hello World 文本与六色方块可见。

自动 RGB 检查仍失败，原因为探针误将 CSS green 的 128 亮度排除。
此轮失败记录保留，不追改成 PASS；未执行的键盘场景仍未通过。

第三轮运行：[41297206819](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819)。
源码为 `58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4`。
运行已结束，整体结论为 failure；汇总没有缺失报告，SHA 与 attempt 一致。

固定色样自检通过；GPUI 示例构建耗时 352.25 秒，退出码为零。
真实前台窗口截图通过，RGB 采样为 100、100、267。
OS Alt+F4 输入后目标窗口销毁，键盘关闭检查为 PASS。

模型读取 640×560 原图，确认文本和六色方块可见。
这仅验证示例的基础绘制、截图和 OS 键盘路径，不代表 Norma 场景通过。
尚未验证中文输入、画布、终端、双语主题或 Windows 11 设备。

| 本轮门槛 | 第三轮实测结果 | 结论边界 |
|---|---|---|
| 产品编译 | FAIL：norma/app check 均退出 101 | norma 的 14 个错误仍在；release、主程序与 helper 未执行 |
| 域层样本 | PASS：canvas 52 项通过、0 失败、0 忽略 | 只运行 canvas lib 测试，没有宣称全 workspace 通过 |
| GPUI 基础图形 | PASS：构建、原图像素、前台归属、OS 键盘关闭 | 示例使用独立锁；真实 Norma 的 AC04、AC09 仍为 NOT_RUN |
| CLI 前置 | PASS：Codex 0.166.0、Claude Code 2.1.316 安装与启动 | Codex stdio initialize 通过；未验证原生 TUI relay |
| 真实 Agent | NOT_RUN | 未提供凭据、未授权模型预算，真实任务数为 0 |
| 产品候选包 | NOT_RUN | 未 release 构建、未打包，也未在新 job 复验 |

该历史运行的四个迁移门槛为 1 PASS、1 FAIL、2 NOT_RUN。
当时 AC01–AC12 为 0 PASS、1 FAIL、11 NOT_RUN。
采集 job 的 success 仅代表报告已生成；最终 gate 保留 failure。
本轮最小验证已完成，完整迁移 Goal 仍为未完成。

![标准 Windows runner 上的 GPUI 示例原图；不是 Norma 成品](../quality/evidence/windows-feasibility-20261101/41297206819/graphics/gpui-window.png)

各轮完整证据与 SHA-256 清单位于 `docs/quality/evidence/windows-feasibility-20261101/`。
证据索引为 `docs/quality/evidence/windows-feasibility-20261101/README.md`。
保留每轮原始报告、日志和失败状态，不跨 SHA 拼接通过结果。

### norma 编译入口迁移与回归（前批）

先增加独立编译门槛，再提交产品修复。
失败基线：[41306097667](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41306097667)。
源码 `bf03d7ef434a77dc5268d7ef257a4290c45cb031` 的独立门槛失败，原 14 个错误仍在。
`canvas` 为 52 passed、0 failed、0 ignored。

| 根因 | 原错误数 | 最小修复 | 聚焦证据 |
|---|---|---|---|
| 本地 Socket 导入 | 1 | Windows 复用已锁定的 uds_windows 1.2.1；Unix 仍用标准库 | 真实本地 Socket 往返与错请求 ID 拒绝 |
| Unix 文件权限 API | 8 | Unix 保留 0700/0600/0755；Windows 创建时即使用当前用户专属的受保护 DACL | 放宽 ACL 后检测与修复、原子替换、junction/符号链接拒绝 |
| 过宽的模块条件编译 | 5 | 开放提问与历史模块；跨平台读取打开句柄的稳定文件身份 | Codex/Cursor 问题协议、追加与等长替换、原有 macOS 历史读取 |

Windows 权限查询与修复绑定同一文件句柄，并拒绝 reparse point。
没有加入 chmod 空实现、无认证 TCP、跳过失败测试或隐藏构建参数。
Codex 的既有 POSIX relay 仍未移植，不据此声明完整 Agent 可用。

macOS 的 `cargo test --locked -p norma`：340 passed、0 failed、1 个既有 ignored。
忽略项是需要已安装 Grok Build 的只读发现测试；本次未新增或修改忽略设置。
新增 Socket 测试单独复跑通过，正常提交钩子的 workspace check 与 fmt check 通过。
本机验证使用独立任务缓存，验收后已定向回收 1.8 GiB。

修复版：[41307703559](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559)。
源码为 `dae96214474474dffe6e0fc96efabb9d2e14990f`，运行已完成，整体结论仍为 failure。

| 本批验收项 | 实际结果 | 证据与限制 |
|---|---|---|
| 原 14 个 norma 错误 | PASS：全部消除，当前编译诊断为 0 | 与失败基线 41306097667 比较 |
| norma Windows 编译 | PASS：lib 与默认 CLI/helper 编译检查通过 | lib 检查 185.69 秒；独立 gate 也通过；未使用 feature 覆盖 |
| norma 平台回归 | PASS：43 passed、0 failed、0 ignored | lib 26 项，加 5 个集成测试目标，共 43 项 |
| canvas | PASS：52 passed、0 failed、0 ignored | 58.67 秒；原测试未修改 |
| App 检查 | FAIL：退出 101，5 个 E0433 | 首个失败依赖为 voice_input；不是全部下游问题总数 |
| GPUI 示例 | PASS：原图、前台、OS 键盘关闭通过 | 示例仍不等于真实 Norma 图形验收 |
| 真实 Norma 图形、真实 Agent、候选包 | NOT_RUN | 没有以编译或模拟任务替代真实产品验收 |

App 新阻塞位于 `crates/voice_input/src/vad.rs:31`、45、64–66 行。
该文件无条件引用 sherpa_onnx，但 `crates/voice_input/Cargo.toml` 只在 macOS 目标声明依赖。
下一步先验证 Windows 的 Sherpa/VAD 原生依赖与构建路径，保留能力和默认构建语义。
本批没有禁用 VAD、语音或其他产品能力来消除错误。

本批 norma 入口验收已达成，G2 与整体 Goal 仍未通过。
前批 AC 矩阵为 0 PASS、1 FAIL、11 NOT_RUN；本批新增 Mac 环境失败见下文。
原始日志、测试统计与源码身份见 `docs/quality/evidence/windows-feasibility-20261101/41307703559/manifest.json`。

额外 i18n 护栏在 `crates/app/src/settings/voice/view/gpt_live.rs:70` 报既有硬编码。
该文件与修复前基线逐字节一致，本批未改 App 文案；此护栏未标为通过。
Windows Installer 的随机数来源与 Cursor 项目路径编码也仍需后续运行验证。

### Sherpa/VAD 平台依赖修复

本批针对运行 [41307703559](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559) 暴露的 5 个 VAD 引用错误。
先增加真实 Cargo 目标依赖图检查：Windows 三个子用例失败，macOS 三个子用例通过。
修复将 voice_input、asr、voice_sherpa 的 Sherpa 1.13 声明移入普通依赖。
锁定版本仍为 1.13.3，Cargo.lock、VAD 和其他产品 Rust 实现均未改动。
修复后六个子用例全部通过，不使用 test-support 或功能关闭选项。

新增独立 Windows speech job，记录 voice_input 默认编译及三个 crate 的默认测试。
先上传原始报告与日志，再只读校验本次 SHA/run/attempt 和准确退出码。
首轮报告测试为 8 项；后续增加状态与退出码矛盾回归。
最终 9 项报告测试通过；PASS 标签必须对应退出码 0。

首轮修复运行：[41312023075](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312023075)。
源码 `e0e75071fb431d9ff93d432f7f95f899802f57a4`。
voice_input 默认 Windows check 为 PASS，退出 0、116.39 秒，原 5 个错误已消除。
语音测试编译为 FAIL：ASR 引用了被 Unix cfg 排除的 A1RecognizerFactory，实际测试尚未执行。

同轮 norma lib 与默认 CLI/helper check 通过；43 项平台回归与 canvas 52 项均通过，无失败或忽略。
App check 退出 101，879.12 秒，在 storage 编译出现 25 条诊断。
其中 2 条源于 workspace::review 被 cfg(unix) 排除，其余 23 条为 memory/fs 的 Unix 文件 API。
它们涉及目录句柄、openat/renameat、文件描述符与硬链接数，不是下游完整错误清单。

最终报告一致性复验：[41312790317](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312790317)。
源码 `e29f676e6fce66894b87a8ab7fb1e23e3538d8b0`，attempt 1，运行已结束，整体 failure。
该版只补强报告退出码校验，产品依赖修复相同。

| 最终复验项 | 实际结果 | 日志与边界 |
|---|---|---|
| 原 5 个 VAD 错误 | 全部消除；voice_input 默认 check PASS，退出 0，117.0 秒 | speech/voice_input_compile.log；未禁用 VAD |
| norma Windows lib | PASS，退出 0，187.72 秒 | compile/norma_compile.log |
| norma 默认 CLI/helper | PASS，独立编译 gate success | workflow.log；不代表 helper 已启动 |
| norma 平台回归 | 43 passed、0 failed、0 ignored，149.73 秒 | compile/norma_platform_tests.log |
| canvas | 52 passed、0 failed、0 ignored，48.47 秒 | compile/canvas_tests.log |
| App 默认 check | FAIL，退出 101，866.02 秒；storage 25 条诊断 | compile/app_compile.log；不是完整下游清单 |
| 默认语音测试 | 命令 FAIL，退出 101，149.92 秒；A1 编译阻塞 | speech/speech_tests.log；实际测试 NOT_RUN，不填通过数 |
| GPUI 示例 | PASS，窗口、像素、前台与 OS 键盘关闭通过 | graphics/ 原始日志与 PNG；已看图，非真实 Norma |
| 真实产品能力 | 真实 Norma 图形、Agent、候选包、物理录音与模型 VAD 均 NOT_RUN | 未用示例、夹具或编译代替产品验收 |

原始证据与校验清单：`docs/quality/evidence/windows-feasibility-20261101/41312790317/manifest.json`。
App 的 2 条 Review 错误位于两个 mobile_task store 导入；根因是 workspace 的 cfg(unix)。
其余 23 条位于 memory/fs 的 POSIX 目录和文件句柄实现。

语音的 A1 错误位于 `crates/asr/src/providers/mod.rs:114`。
`crates/asr/src/providers/local/a1/mod.rs:8` 在 Windows 排除了该工厂定义。
两轮均保留失败日志，没有删除产品分支或绕过必验门槛。

当前五个探针门槛：1 PASS、2 FAIL、2 NOT_RUN，goal_complete=false。
当前 AC 矩阵：0 PASS、1 FAIL、1 ENV_UNSUPPORTED、10 NOT_RUN。
G1、G2 与整体 Goal 均未完成。


Mac 默认三个 crate 的测试合计 90 passed、4 failed、17 个既有 ignored。
失败均为 A1 host 资源检查；内核内存压力为 WARN（2），未修改的护栏要求 NORMAL（1）。
首次和一次重试日志均保留，重试使用 no-fail-fast 收集其余测试，不修改护栏或增加忽略。

Mac workspace check、fmt、依赖图、边界与性能护栏通过；i18n 仍有原有 GPT-Live 文案失败。
真实 Norma 图形、真实 Agent、候选包与物理录音/模型验收仍为 NOT_RUN。
本机两轮日志、内核查询和源码 SHA 归档在最终运行的 local 子目录。

### 已实测的阻塞与后续顺序

| 优先级 | 实测或尚缺条件 | 下一批迁移任务 | 退出依据 |
|---|---|---|---|
| 已解决 | norma 原 14 个错误、VAD 原 5 个 Sherpa 错误 | norma、CLI/helper 与 voice_input 默认 check 已通过 | 各轮独立日志与源码身份；43/52 项无回退 |
| 1 | 语音默认测试在 A1 工厂引用处编译失败 | 为 A1 host/transport 增加真实 Windows 平台实现及聚焦回归 | 三个语音 crate 默认测试实际执行并通过，不删除 A1 分支 |
| 2 | App 在 storage 出现 25 条诊断；安全保存未验 | 移植 workspace Review 与 memory/fs 的目录句柄、reparse point、硬链接和原子保存 | App 默认 check 与 Windows OS 资源正反测试通过 |
| 3 | ConPTY、Shell 与进程收尾尚未执行 | 适配 PowerShell、参数、hook、session 归属和进程树 | 输入、resize、取消、恢复及整树退出有实测证据 |
| 4 | GPUI 示例不代表真实 Norma 可运行 | 隔离 macOS 原生能力，构建并验收真实 Norma 窗口 | AC04、AC09 的实际画布、终端、OS 输入和持久化结果 |
| 5 | 只有 CLI 安装与无认证握手 | 取得受保护凭据和调用预算后，运行两个真实 Agent 场景 | AC05、AC06 的任务文件、回执、提问、取消和恢复 |
| 6 | 未产出产品候选包 | release 构建、打包、哈希，新的 job 下载同一包复验 | AC10 与 macOS 回归通过 |

首批 14 个 norma 错误和前批 5 个 VAD 错误已消除。
新暴露的 A1 与 storage 错误仍只是编译前沿。
没有据此下调 20–33 人日估算。
G1 还未覆盖全部退出条件，两个 Agent 的产品支持矩阵尚未冻结。

每个功能先补可失败的聚焦测试，再实现平台分支。
每个阶段使用小批次提交，避免等到全部迁移后才跑 Windows。

## D Actions 工作流设计

以下为完整迁移阶段的后续路径；本轮仅新增最小探针入口。
优先复用现有 Rust 缓存、协议、场景语义与检查脚本。
Windows 桌面入口与现有 Swift/macOS 驱动分开维护。

| 路径 | 职责 |
|---|---|
| `.github/workflows/windows-ci.yml` | PR 与迁移分支的环境、构建、域测试；不读取模型凭据 |
| `.github/workflows/windows-acceptance.yml` | 指定源码版本的完整验收、真实 Agent 和包复验 |
| `scripts/windows/preflight.ps1` | 输出工具链、资源、桌面、图形和设备能力报告 |
| `scripts/desktop-e2e-windows.ps1` | 启动真实 Windows 程序、执行场景、收集失败证据 |
| `test/desktop-e2e/Windows/Scenarios/` | 按模块维护 Windows 场景；不改写 macOS 场景注册表 |

本轮已实现 `.github/workflows/windows-feasibility.yml`。
采集程序为 `scripts/windows/probe.py` 与 `scripts/windows/native.ps1`。
固定 GPUI 示例入口和依赖锁位于 `test/windows-probe/Cargo.toml`、`test/windows-probe/Cargo.lock`。
示例复用 vendor 原代码，使用独立依赖锁，不代表产品依赖图通过。

报告记录 job 的 SHA、run ID、attempt、日志和实际镜像版本。
汇总严格核对证据身份；缺项、未执行或产品编译失败均不能通过。

| Job | 执行内容 | 初始超时建议 |
|---|---|---|
| preflight | 精确源码版本、工具链、磁盘、图形、桌面与依赖检查 | 15 分钟 |
| domain-tests | 受影响 crate、Windows OS 资源集成测试及对应护栏 | 45 分钟 |
| candidate-build | 一次 release 构建；生成候选包与哈希 | 120 分钟 |
| desktop-acceptance | 下载候选包，在实际窗口完成核心场景 | 30 分钟 |
| live-agents | 已受信源码、两个真实 CLI 与最小任务 | 30 分钟 |
| package-verify | 新 job 下载同一候选包，解压并运行，不从源码重建 | 30 分钟 |
| acceptance-gate | 汇总必验项、哈希和报告完整性，生成最终结论 | 5 分钟 |

普通 PR 不执行真实模型调用，也不接触发布或用户凭据。
完整验收先通过 `workflow_dispatch` 指定受信源码版本。
初始工作流可由迁移分支 push 验证。
`workflow_dispatch` 稳定入口须按正常 PR 流程进入默认分支。

本轮已获用户授权，提交探针到独立分支并触发真实 Actions。
未合并主分支，未修改分支保护，未启用付费 runner。
工作流按路径触发，只有文档更新不会重新跑全部构建。

PR 同分支任务可取消旧运行；正式候选验收串行执行。
固定 Actions 依赖版本，并记录实际镜像版本。
复用仓库钉死的 Rust；先检查 LFS 资源完整性。

后续构建记录磁盘峰值；本轮仅记录各采集任务末尾的可用空间。
本轮关闭增量缓存，编译并发固定为 2，没有上传构建二进制或缓存。
不能靠删除 runner 的系统工具和 SDK 掩盖容量问题。

## E 必须通过的验收矩阵

| 编号 | 场景与执行方式 | 必须观察到的结果 | 必须排除的失败 | 本轮状态 |
|---|---|---|---|---|
| AC01 | Windows 默认构建、release 构建及 helper 启动 | MSVC 退出码为零；工具版本可查 | 只在 Mac 交叉检查，或靠隐藏参数才能启动 | FAIL：App check 退出 101；norma 与 CLI/helper check 通过；release 未执行 |
| AC02 | IPC 与身份、端点冲突集成测试 | 同用户正确请求成功，越权和陈旧端点被拒绝 | 无认证 TCP、跨项目误投递、后台线程不退出 | NOT_RUN |
| AC03 | 真实 ConPTY 输入、resize、中文和特殊路径 | 真实 Shell 返回唯一标记，尺寸与 cwd 正确 | 只比较命令字符串、转义失败或等待无限挂起 | NOT_RUN |
| AC04 | 真实窗口建立项目和终端，移动缩放后重启 | 画布与终端可见，OS 输入生效，布局和内容恢复 | 黑屏、冻结、只改内部状态或仅检查 PNG 存在 | NOT_RUN：未运行真实 Norma；图形示例前置另列 |
| AC05 | 两个冻结 Agent 各完成一个最小任务 | 真实 CLI 创建预期文件；session、卡片和回执对应 | fixture 冒充真实 Agent、只有 version 检查或串卡片 | NOT_RUN：无真实模型任务 |
| AC06 | 提问、回答、取消、退出和恢复 | 请求归属正确，取消收口，恢复仍是同一会话 | 重复派工、假完成、取消后迟到动作 | NOT_RUN：未验证提问、取消与恢复 |
| AC07 | 关项目及主动退出 App | 受控子进程和后代退出并等待；其他任务保留 | 只杀 Shell、孤儿 CLI、误杀无关进程 | NOT_RUN |
| AC08 | 安全存储与编辑冲突测试 | 路径越界被拒绝，记忆可恢复，冲突内容不丢失 | junction、路径交换、硬链接或备份回滚失效 | NOT_RUN |
| AC09 | 中英文、亮暗主题、键盘与能力状态 | 文案可切换，焦点可见，终端 Ctrl+C 保留中断 | 硬编码文案、不可用能力显示就绪或快捷键冲突 | NOT_RUN：未验真实 Norma 的双语、主题与键盘 |
| AC10 | 新 job 下载候选便携包 | 哈希一致；无开发目录依赖；真实启动、命令与恢复成功 | 用另一份构建替代候选包，或只验证能解压 | NOT_RUN：未构建或复验候选包 |
| AC11 | macOS 增量回归 | 受影响 crate、App 构建和关键既有场景通过 | 用 Windows 通过掩盖 macOS 行为回退 | ENV_UNSUPPORTED：语音测试 90 通过、4 项内存压力护栏失败、17 既有忽略；workspace check 通过；真实 App 未验 |
| AC12 | 最终证据核对 | 每条必验项都有 PASS、执行版本与证据入口 | 缺项、跳过、continue-on-error 或拿旧版本证据拼接 | NOT_RUN：最终必验未齐；本轮证据独立留存 |

AC04、AC09 包含模型查看实际窗口原图。
实际输入、可见结果与持久状态需要相互印证。
桌面驱动不可只调用 `TestAppContext` 或模拟点击。
测试支持构建和候选构建若不同，必须分别记录。
AC10 仍须验证交付的候选包本身。

新 hosted job 仍预装开发工具，不能证明纯净 Windows 11 安装就绪。
另做 DLL 与运行库依赖审计，把用户设备安装验证留在后续矩阵。

真实 Agent 任务需要受保护的凭据与已授权调用预算。
缺少凭据时记 NOT_RUN，不能以协议替身替代 AC05、AC06。
不得从本机复制已有登录凭据到 GitHub。
凭据使用 GitHub Secrets 和受保护环境安全接入。
Fork PR、任意未受信 ref 和可执行测试输入均不得获得凭据。

本轮探针未新增 Secrets，未提交模型任务。
两个 CLI 的安装与启动通过；Codex stdio 初始化返回 Windows 平台结果。
该握手不验证 Norma 的 Unix TUI relay，也不验证真实任务、提问或恢复。

输入法、DPI、多显示器和物理音频只声明实际测到的环境。
Windows 11 专项、Browser 与语音验收仍属于后续阶段。
内容编辑若 G1 无法证明安全保存，首版明确为只读。
该范围变更必须在支持矩阵中冻结，不能运行失败后临时移出必验项。

## F 结果判定与交付证据

| 状态 | 含义 | 能否作为必验项通过 |
|---|---|---|
| PASS | 预期行为通过，反向断言及证据完整 | 能 |
| FAIL | 实现、断言、退出码或产物有误 | 不能 |
| ENV_UNSUPPORTED | 图形、桌面、设备或资源条件不具备 | 不能 |
| NOT_RUN | 未执行，或缺少必要认证、预算、依赖 | 不能 |
| NOT_IN_SCOPE | 在执行前已冻结的后续功能 | 不计入当前必验项，也不宣称支持 |

未被本轮证据覆盖的阶段与 AC 项保持未开始或未通过。
失败后只重试已判定为暂态的网络或环境问题一次。
断言失败不靠重跑覆盖；每个 attempt 保存独立结果。
阶段只有通过退出门槛才能标记完成。
若 hosted 桌面门槛未通过，整体 Goal 仍未完成。

每次运行输出以下证据；文件路径中的占位符由 run ID 和 attempt 替换：

- `artifacts/windows-acceptance/<run-id>/<attempt>/source-manifest.json`：实际 checkout SHA、事件 SHA、workflow SHA、CLI 与工具链版本。
- `artifacts/windows-acceptance/<run-id>/<attempt>/runner-capabilities.json`：镜像、OS、GPU、显示会话、输入、截图与可用设备。
- `artifacts/windows-acceptance/<run-id>/<attempt>/acceptance-results.json`：AC 编号、状态、耗时、失败类别与证据相对位置。
- 测试与构建日志：命令、退出码、测试统计和资源峰值。
- PNG 原图：关键窗口状态与失败现场，不能只有截图哈希。
- `artifacts/windows-acceptance/<run-id>/<attempt>/candidate-manifest.json`：包、主程序、helper 的 SHA-256 与构建 features。
- 对应 SHA 的便携包、功能支持矩阵和已知限制说明。

报告展示通过数、失败数、未执行数和未覆盖数。
工作流上传失败证据；下载后复核哈希和内容。
上传失败或证据缺失时，最终 gate 不能通过。
临时文件、真实用户数据库、凭据和完整模型对话不上传。
测试使用专属数据目录、合成项目和可公开的固定内容。

## G 费用、权限与资源控制

当前仓库公开，标准 hosted Windows runner 的计算免费。
该使用量归组织，不消耗个人账号的免费计算额度。
免费计算不代表构建产物和所有 runner 类型都免费。

| 项目 | 默认策略 |
|---|---|
| Runner | 只使用标准 `windows-2025`；不启用 larger runner 或 GPU 付费规格 |
| PR 权限 | 最小只读仓库权限；不传递 Secrets；不使用高权限事件执行不受信代码 |
| Artifact 留存 | 预检小报告 7 天；截图和验收日志 3 天；便携包默认 1 天 |
| Artifact 容量 | 本 Goal 当前保留数据控制在约 350 MiB 内；组织 Free 500 MB 为共享额度 |
| 缓存 | 复用现有 Rust 缓存；不提高每仓库 10 GB 的免费缓存上限 |
| 并发与超时 | 按阶段限时；正式候选验收串行；临时环境失败最多重试一次 |
| 真实模型调用 | 只在用户已配置凭据和预算的受保护验收任务执行 |
| 签名与正式发布 | 当前只交付 CI 预览包；不购买证书、不发布 Release、不更改账单设置 |

每次大包上传前盘点当前仓库留存量。
组织其他仓库也会共享产物存储，需在阶段验收时核对账单页。
空间不足时停止新增大包，不删除其他任务的验收记录。
可回收本 Goal 明确过期的自有产物，保留当前候选的完整证据。
免费计算不能作为所有用量都不会产生费用的证明。

## H 完成定义与后续入口

只有以下条件全部满足，才能宣布本适配 Goal 完成：

1. G0–G5 的退出门槛全部通过。
2. AC01–AC12 必验项均有 PASS 和对应版本证据。
3. 干净 runner 已运行同一个 Windows x64 候选包。
4. 两个支持 Agent 的真实控制流程均通过。
5. macOS 回归通过，平台能力边界与已知限制清楚。
6. 用户拿到包、源码 SHA、运行链接、截图与最终验收表。

Beta 后续按原计划补内置浏览器、语音与安装发布。
主流功能 Beta 累计估算仍为 36–60 人日，G1 后重估。
Windows 11 用户设备验证使用具备交互桌面的 GitHub runner。
如果尚未接入该环境，则相应支持状态保持未验收。

依据与现行入口：

- 原适配计划：`docs/exec-plans/2026-11-01-windows-port-plan.md`。
- GitHub runner 规格：https://docs.github.com/en/actions/reference/runners/github-hosted-runners 。
- Windows 镜像说明：https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md 。
- Actions 计费规则：https://docs.github.com/en/billing/concepts/product-billing/github-actions 。

本轮已运行最小 Windows 探针；产品迁移与整体 Goal 仍未完成。
