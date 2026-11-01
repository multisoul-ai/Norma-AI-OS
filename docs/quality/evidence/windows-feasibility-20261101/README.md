# Windows runner 最小可行性验证证据

本轮最小验证已完成，完整 Windows 迁移未通过。
范围是编译前置、GPUI 基础图形、无认证 CLI 条件；未执行真实模型任务和产品候选包验收。
标准 runner 的实测 OS 为 Windows Server 2025，不能视作 Windows 11 设备验收。

## 实际运行与版本

| 运行 / attempt | 实际源码 SHA | 汇总结论 | 图形结果与调整原因 |
|---|---|---|---|
| [41295234651 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41295234651) | `cbd146d02e1d98ac51d3d8e9deb8879b01e0f9b4` | failure | ENV_UNSUPPORTED：未取得前台窗口控制 |
| [41296333985 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41296333985) | `366b47182f12c7ba68b589cc7bed8148fecdca11` | failure | FAIL：取得窗口原图，但探针误排除了 CSS green；键盘场景未执行 |
| [41297206819 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819) | `58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4` | failure | PASS：固定色样校准后，窗口原图与 OS Alt+F4 关闭通过 |

三次运行之间均有探针代码修正，不以无改动重跑覆盖断言失败。
各轮原始报告均保留，没有把前两轮状态改成 PASS。
每轮结果来自其自身 SHA、run ID、attempt，不跨版本拼接成通过。

## 最后一次运行的结果

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
模型已读取最后一次原图，核验记录见 `docs/quality/evidence/windows-feasibility-20261101/41297206819/visual-observation.md`。

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
每个目录包含以下文件，具体字节数和 SHA-256 以该轮清单为准：

- `manifest.json`：run ID、attempt、源码 SHA、运行链接与逐文件 SHA-256。
- `run.json`、`jobs.json`、`artifacts.json`：GitHub API 原始运行、job 和产物元数据。
- `workflow.log`：完整 Actions 日志；与各任务的独立日志互相补充。
- `environment/report.json`、`environment/native.json`：环境与 DirectX 实测。
- `compile/report.json`：命令、退出码、耗时；同目录保存 canvas、norma、app 的实际日志。
- `graphics/report.json`、`graphics/native.json`：构建、窗口、像素和输入结果；有图像时保存 PNG 原图。
- `agents/report.json`：固定 CLI 版本、无认证检查及明确的 NOT_RUN 项；同目录保存帮助、版本和握手日志。
- `feasibility.json`、`summary.md`：当前源码和 attempt 的汇总结论。
- `compiler-blockers.json`：从该轮日志提取的 14 个错误与测试计数，是派生索引。

上述布局中的短文件名用于说明各 run 目录内部结构。
最后一次汇总入口为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/feasibility.json`。
最后一次校验清单为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/manifest.json`。
CI 临时产物有 3–7 天留存期，本目录持久保存小报告、日志与原图；没有上传构建二进制。

## 边界与下一步

未提供模型凭据，未新增 Secrets，真实模型调用数为 0。
未复制本机登录信息，未启用付费 runner、签名、发布或机器采购。
真实 Agent 的后续验收需要受保护环境与已授权调用预算，缺失时继续保持 NOT_RUN。

迁移顺序为 norma 平台边界 → storage/workspace 安全语义 → PowerShell/ConPTY/进程树 → 真实 Norma 窗口 → 真实 Agent → 同版本候选包复验。
14 个错误只是当前编译前沿，不能视作剩余全部适配工作，也不能据此下调估算。
G1 尚未完成，两个 Agent 的产品支持矩阵尚未冻结。

本机聚焦验证通过：报告语义 6 项、契约检查 4 项、域契约检查与 actionlint。
Windows environment job 也运行了这两组 Python 测试和契约检查。
本轮没有重建本机 App/workspace；macOS 产品回归仍未执行。

现行计划：`docs/exec-plans/2026-11-01-windows-github-runner-goal.md`。
HTML 展示：`docs/exec-plans/2026-11-01-windows-github-runner-goal.html`。
