# Windows runner 最小可行性验证证据

本批 norma 编译入口验收已达成；App 仍未编译通过，完整 Windows 迁移未通过。
范围包含 norma 编译修复、OS 资源回归、GPUI 基础图形与无认证 CLI 条件。
真实 Norma 图形、真实模型任务和候选包验收均未执行。
标准 runner 的实测 OS 为 Windows Server 2025，不能视作 Windows 11 设备验收。

## 实际运行与版本

| 运行 / attempt | 实际源码 SHA | 汇总结论 | 图形结果与调整原因 |
|---|---|---|---|
| [41295234651 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41295234651) | `cbd146d02e1d98ac51d3d8e9deb8879b01e0f9b4` | failure | ENV_UNSUPPORTED：未取得前台窗口控制 |
| [41296333985 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41296333985) | `366b47182f12c7ba68b589cc7bed8148fecdca11` | failure | FAIL：取得窗口原图，但探针误排除了 CSS green；键盘场景未执行 |
| [41297206819 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819) | `58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4` | failure | PASS：固定色样校准后，窗口原图与 OS Alt+F4 关闭通过 |
| [41306097667 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41306097667) | `bf03d7ef434a77dc5268d7ef257a4290c45cb031` | failure | 新增独立 norma 编译 gate，先复现原 14 个错误；GPUI 示例 PASS |
| [41307703559 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559) | `dae96214474474dffe6e0fc96efabb9d2e14990f` | failure | norma 编译/43 项回归通过，canvas 52 项通过；App 出现 voice_input 的 5 个错误；GPUI 示例 PASS |

前三次运行之间均有探针代码修正，不以无改动重跑覆盖断言失败。
新增回归门槛与产品修复分别提交，保留先失败、再通过的执行记录。
各轮原始报告均保留，没有把既有失败状态改成 PASS。
每轮结果来自其自身 SHA、run ID、attempt，不跨版本拼接成通过。

## 首轮收束运行的结果（41297206819）

| 门槛 | 状态 | 实测证据 | 尚未证明 |
|---|---|---|---|
| 产品编译 | FAIL | norma/app 的 MSVC check 均退出 101，首个失败依赖 norma 有 14 个错误 | 所有下游依赖、默认二进制、release、helper 启动 |
| GPUI 基础图形 | PASS | 钉死 GPUI 0.2.2 示例构建成功；真实窗口、RGB 像素、前台归属、OS Alt+F4 后窗口销毁 | 真实 Norma、文本输入、画布交互、主题、IME、DPI、多显示器 |
| 真实 Agent | NOT_RUN | Codex 0.166.0、Claude Code 2.1.316 安装/help/version 成功；Codex stdio initialize 成功 | 认证、任务、提问、取消、恢复、卡片与 session 归属、Norma Unix TUI relay 的替代 |
| 产品候选包 | NOT_RUN | 未产出产品包 | 同一候选包在新 job 下载、校验、启动与恢复 |

另有 canvas lib 测试 52 passed、0 failed、0 ignored。
四个门槛为 1 PASS、1 FAIL、2 NOT_RUN，`goal_complete` 为 false。
产品 AC01 为 FAIL，AC02–AC12 均为 NOT_RUN；完整矩阵见原 Goal 计划。
采集 job 成功表示报告生成成功，最终 gate 仍失败，不能把 workflow 当作产品验收绿色记录。

图形示例使用 `test/windows-probe/Cargo.lock` 的独立依赖图。
它复用 vendor 的示例源码，未改变产品 Rust 代码，也不证明产品依赖图已适配。
模型已读取首轮收束原图，核验记录见 `docs/quality/evidence/windows-feasibility-20261101/41297206819/visual-observation.md`。

## norma 编译入口修复（本批）

先建立独立编译失败门槛，再修复产品代码。
失败基线为 [41306097667](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41306097667)，源码 `bf03d7ef434a77dc5268d7ef257a4290c45cb031`。
该轮独立 norma gate 为 failure，原 14 个诊断仍在，canvas 52 项通过且无忽略。

| 原根因 | 个数 | 修复边界 |
|---|---|---|
| 无条件 Unix Socket 导入 | 1 | Windows AF_UNIX 使用锁内 uds_windows 1.2.1；Unix 仍为标准库 |
| Unix 权限方法与 trait 导入 | 8 | Unix 保留原权限；Windows 使用当前用户专属的受保护 DACL |
| 根模块 cfg 排除后仍有公共类型引用 | 5 | 开放可移植问题/历史模块；打开句柄上读取稳定文件身份 |

新增回归包括真实 Socket 往返/错 ID、权限放宽检测与修复、原子替换、目录复用、reparse point 拒绝、追加与同长度替换的文件身份。
Codex 的既有 POSIX relay 未移植；Windows App 本地服务、真实 Agent、完整安全存储及候选包仍须分别验收。

macOS `cargo test --locked -p norma` 为 340 passed、0 failed、1 ignored。
该 ignored 是基线已有的 `real_grok_discovers_installed_skills_and_hooks`，需要已安装 Grok Build；本次没有新增或修改 ignore。
新增 Socket 测试单独复验通过；正常提交 hook 的 workspace check 和 fmt check 通过。

额外 i18n 护栏仍在 `crates/app/src/settings/voice/view/gpt_live.rs:70` 报既有的 GPT-Live 硬编码。
已核对该文件与修复前基线逐字节一致；本批没有修改 App 文案，不把此检查标为通过。

Windows 修复版运行：[41307703559](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559)。
实际源码 `dae96214474474dffe6e0fc96efabb9d2e14990f`；运行已结束，整体为 failure。

| 本批验收 | 结果 | 原始日志 |
|---|---|---|
| 原 14 个 norma 错误 | 已消除，当前 norma 编译错误数为 0 | compile/norma_compile.log |
| norma Windows lib | PASS，退出 0，185.69 秒 | compile/norma_compile.log |
| norma 默认 CLI/helper check | PASS，独立 gate 成功 | workflow.log |
| norma 平台回归 | 43 passed、0 failed、0 ignored，153.86 秒 | compile/norma_platform_tests.log |
| canvas | 52 passed、0 failed、0 ignored，58.67 秒 | compile/canvas_tests.log |
| App | FAIL，退出 101，808.72 秒；5 个 E0433 | compile/app_compile.log |
| 真实 Norma 图形、真实 Agent、候选包 | NOT_RUN | 未执行，不由示例或协议夹具替代 |

43 项包括 lib 的 26 项，以及 client_platform、client_tests、codex_question、cursor_questions、protocol_tests 共 17 项。
没有宣称全部 Windows integration tests 均已运行；该测试范围没有任何忽略项。
新增的 DACL 检测/修复、原子替换、junction/符号链接拒绝、文件身份正反测试均已在 Windows 实际通过。

App 首个失败依赖为 `voice_input`。
`crates/voice_input/src/vad.rs:31`、45、64–66 行无条件引用 sherpa_onnx。
`crates/voice_input/Cargo.toml` 却只在 macOS 目标声明该依赖，产生 5 个 E0433。
本批未修改或禁用 VAD；下一步适配 Windows 原生依赖与构建，再继续暴露其他阻塞。
App 日志另有 workspace 的既有 unused metadata 警告，未用 allow 压制。
这些错误不是下游问题总数，App/release/候选包均未通过。

本批规范入口为 `docs/quality/evidence/windows-feasibility-20261101/41307703559/compiler-blockers.json`。
原始日志、原图与 SHA-256 清单为同目录的 manifest；本地 Mac 测试和正常 hook 记录位于 local 子目录。
本批公开源码和 native OS 夹具未使用模型凭据，模型调用数仍为 0。
Windows Installer 的 /dev/urandom 随机数来源与 Cursor 的 Unix 项目路径编码仍待后续运行适配。

## 实测环境

- runner label：`windows-2025`；实际镜像 `win25-vs2026`，版本 `20261021.278.1`。
- OS：Windows Server 2025 Datacenter x64，10.0.26100。
- Rust 固定 1.91.1，target 为 `x86_64-pc-windows-msvc`；Node 22.23.4。
- UserInteractive=true；进程与活动控制台均为 session 2；显示器为 1024×768。
- 显卡为 Microsoft Hyper-V Video；D3D11 hardware 与 WARP 创建均 HRESULT=0、feature level 11.0。
- GPUI 原图为 640×560；RGB 采样为 100、100、267；固定色样自检通过。
- 仅记录各任务结束时的磁盘余量，未测峰值，也未宣称标准镜像永远保持上述能力。

## 证据布局与校验

`docs/quality/evidence/windows-feasibility-20261101/` 下按 run ID 分目录，当前每轮均为 attempt 1。
每个目录包含以下文件，具体字节数和 SHA-256 以该轮清单为准；本批还增加 local 本机验证日志：

- `manifest.json`：run ID、attempt、源码 SHA、运行链接与逐文件 SHA-256。
- `run.json`、`jobs.json`、`artifacts.json`：GitHub API 原始运行、job 和产物元数据。
- `workflow.log`：完整 Actions 日志；与各任务的独立日志互相补充。
- `environment/report.json`、`environment/native.json`：环境与 DirectX 实测。
- `compile/report.json`：命令、退出码、耗时；同目录保存 canvas、norma、app 的实际日志。
- `graphics/report.json`、`graphics/native.json`：构建、窗口、像素和输入结果；有图像时保存 PNG 原图。
- `agents/report.json`：固定 CLI 版本、无认证检查及明确的 NOT_RUN 项；同目录保存帮助、版本和握手日志。
- `feasibility.json`、`summary.md`：当前源码和 attempt 的汇总结论。
- `compiler-blockers.json`：从该轮日志提取的错误与测试计数，是派生索引；初始为 norma 14 个，本批为 norma 0 个、App 5 个。

上述布局中的短文件名用于说明各 run 目录内部结构。
首轮收束汇总入口为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/feasibility.json`。
首轮收束校验清单为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/manifest.json`。
CI 临时产物有 3–7 天留存期，本目录持久保存小报告、日志与原图；没有上传构建二进制。

## 边界与下一步

未提供模型凭据，未新增 Secrets，真实模型调用数为 0。
未复制本机登录信息，未启用付费 runner、签名、发布或机器采购。
真实 Agent 的后续验收需要受保护环境与已授权调用预算，缺失时继续保持 NOT_RUN。

迁移顺序为 voice_input 的 Sherpa/VAD 编译边界 → App 后续编译阻塞 → storage/workspace 安全语义 → PowerShell/ConPTY/进程树 → 真实 Norma 窗口 → 真实 Agent → 同版本候选包复验。
原 14 个错误只代表已观察到的编译前沿，不能视作全部适配工作，也不能据此下调估算。
G1 尚未完成，两个 Agent 的产品支持矩阵尚未冻结。

本机聚焦验证通过：报告语义 6 项、契约检查 4 项、域契约检查与 actionlint。
Windows environment job 也运行了这两组 Python 测试和契约检查。
首轮未重建本机 App/workspace；本批已按正常 hook 执行 workspace check，未运行真实 App 或 release 构建。

现行计划：`docs/exec-plans/2026-11-01-windows-github-runner-goal.md`。
HTML 展示：`docs/exec-plans/2026-11-01-windows-github-runner-goal.html`。

最新结果入口：`docs/quality/evidence/windows-feasibility-20261101/41307703559/feasibility.json`。
最新校验清单：`docs/quality/evidence/windows-feasibility-20261101/41307703559/manifest.json`。

本机独立任务缓存已按归属回收：8881 个文件，1.8 GiB；清理日志保存于最新运行的 local 子目录。
