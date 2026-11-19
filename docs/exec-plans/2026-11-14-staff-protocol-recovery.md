# 幕僚原生问答与格式纠正回归

范围：PR #240，基准 `769c1d7f`。用户真实 OpenCode 对话中，问答工具未显示卡片；`task phase … && task propose …` 被拒绝后被适配器误报为用户拒绝授权。

## 原因与修复

- OpenCode 2.1.3 原生 question 使用 `q0`/`q0_custom`，此前只识别 Claude `question_0` 和 Codex 元数据。保留完整 description、多选、选项说明和自定义入口；泛化 MCP 表单不纳入。
- Claude 生产幕僚会话仅暴露 Bash/Read。仅 staff 增加 AskUserQuestion；普通与审批会话、逐次权限策略不变。
- 真实 OpenCode 会把 JSON-RPC 权限错误吞为 user declined 并结束一轮。单改错误字符串不可行。宿主保留原组合拒绝，在同一会话补发具体纠正说明，最多两次；纠正只针对每个子命令单独通过当前任务 Gate 的字面量组合。真正越权、其他程序、重定向、管道均不自动续接。
- 纠正列表按解析 argv 排除已经获准的调用，不重放已成功或结果未知的操作。重试继续经过原 Gate。用户取消和原始总期限继续生效。
- 工具活动以结构化标记在展示边界翻译成“格式错误 / Format error”，不继续显示适配器误报；同轮续接清理过时的回复分片。

## 验收矩阵

| 要求 | 证据 |
| --- | --- |
| 原生问答从 ACP 进入卡片、回答只进入下一轮 | `staff_acp_questions`，真实 OpenCode `staff_opencode_live::native_question_roundtrip_through_brain_service` |
| OpenCode 单题/多题、单选/多选/自定义 | 真实抓包 fixtures 与 `acp_questions_tests` |
| 组合拒绝后具体反馈、独立重试 | `staff_command_retry`，真实 OpenCode `staff_opencode_live::command_format_feedback_reaches_real_model` |
| 部分成功不重放，越权不重试，取消/到期/预算有界 | `staff_command_retry` 的 prior-success、partial、natural、unsafe、cancel、timeout、repeat 及截止窗口回归 |
| UI 旧误报清理 | App `staff_activity_tests::command_correction_replaces_stale_adapter_reply` |
| 中英真实窗口与录像 | `staff-protocol-recovery` / `staff-protocol-recovery-en --visual`，最终 2/2 通过，见下方证据目录 |

## 已运行证据

真实问答闭环与两次独立格式恢复均通过，日志保留在 `artifacts/staff-opencode-regression/validation/`。真实格式恢复中，phase/propose 两条独立 CLI 各执行一次；CLI 为隔离临时记录器，未连接用户 App 或触发真实派活。

协议与 UI 范围验证：最终 `cargo test -p brain` 通过（171 项，4 项 ignored）；问答与纠正定向 9/9、App staff activity 5/5、`cargo test -p ui` 23/23、`cargo check -p app`、最终 debug 构建通过。i18n 2315 双语键、行数、no-allow、资源生命周期和 diff 检查通过。独立对抗式审查与补审完成，无剩余 P1/P2。

提交使用仓库配置的原生 pre-commit，包含 staged 护栏、资源所有权 clippy 和格式检查；完整提交输出保留在 `artifacts/staff-opencode-regression/validation/pr240-commit.log`。

旧七步 UI/完整对话录像复用 `docs/exec-plans/2026-11-13-staff-message-cards-acceptance.md`。本轮不宣称重跑完整 staff 模块，也不把抓包 fixture 等同于全部真实模型端到端。

## 真实窗口与录像

最终实现使用同一 debug 二进制，两个定向场景均退出 0、`result=pass`、`verification=model-assisted`，每场三项关键图检查均通过：原生单题表单、纠正工具详情、最终回复与可采用提议。程序另断言一条组合被拒、两条纠正各执行一次、仅一个提议。

| 场景 | 证据目录 | 场景耗时 | 连续录像 |
| --- | --- | --- | --- |
| 中文 | `artifacts/desktop-workspace-e2e/2026-11-14T03-28-16Z/` | 366.77 秒 | `staff-protocol-recovery.mp4`，360.865 秒，604 帧 |
| 英文 | `artifacts/desktop-workspace-e2e/2026-11-14T03-34-46Z/` | 476.15 秒 | `staff-protocol-recovery.mp4`，468.142 秒，701 帧 |

录像均为连续真实窗口 H.264、2560×1600；ffprobe 核验时长和帧数，并解码查看两场录像末尾关键帧，回复与提议均清晰。采集最长间隔中文 2.07 秒、英文 2.81 秒，非恒定 4fps。模型响应等待时间也保留在录像中，不以静态截图拼接。最终回复位于提议之前，最后向上滚动让回复和提议同屏；完整问答、格式反馈及纠正过程已有逐图判据与原图。

## 验证边界

- 本轮真实模型验证 OpenCode 2.1.3；Claude 工具白名单通过 stdio 握手与既有真实表单验证，未重跑真实 Claude/Codex 完整对话。
- 真实模型测试调用隔离 CLI 记录器，真实窗口测试使用生产 CLI/Gate/TaskStore 与抓包适配器；没有对用户项目实际派发 Worker。
- 旧记录的 125 秒真实超时、Worker 原卡先答镜像与浅色主题限制仍适用。

## 失败与修正记录

- 独立审查发现已成功 A 后 A&&B 的重放风险，已加按 argv 去重与非幂等 task reply 回归。
- 接收后截止检查先于待答暂停结算的回归已红测复现；修复后问答与纠正协议测试合计 9/9 通过。取消/超时终态以实际错误为准。
- 真实 OpenCode 两用例并行复跑时，问答通过，格式恢复出现一次 120 秒超时；没有因此放宽期限。独立串行重跑 23.16 秒通过，保留失败日志，不把真实模型调用声明为确定性稳定性测试。
- 桌面首次 `2026-11-14T02-58-30Z` 因本新场景的 Worker 预置依赖 NORMA_NODE_ID 而在原生问答前失败。本场景仅需可寻址名册，现使用独立就绪夹具；不改变生产启动代码，不作为 Worker 派发验收。
- `2026-11-14T03-06-01Z` 在人工看图后提交，超过真实 120 秒问答期限而失败；调整为选项点击后立即捕获并提交，再审核保存的画面。`2026-11-14T03-15-00Z` 在上下文切换期间仍未能及时回答，同样超时。均保留失败记录，没有修改产品期限。
- `2026-11-14T03-22-40Z` 问答、格式恢复已通过，最后截图只看到提议和尾部工具，未包含前面的最终回复，因此整项判失败。检查任务文件确认回复已落盘，修正验收滚动位置后中英最终均通过，未修改产品渲染。
- 按 Apple 反馈原则，错误状态区分格式错误与授权拒绝，具体原因在原工具详情中呈现；沿用现有布局及交互，不增加权限卡。
