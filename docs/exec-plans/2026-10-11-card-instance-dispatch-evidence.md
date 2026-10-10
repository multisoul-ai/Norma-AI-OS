# T05 卡片实例级派工验收记录

日期：2026-10-11。关联：工作空间派工规格 issue #96 的 T05。状态：T05 实施与范围内验证完成。

## 实施边界

新空间派工使用 `agent::card_instance` 与 `MultisoulApp` 的实例入口，保留完整 `WorkspaceCardTarget`。
`definition_id` 只检查可选配置，不用来寻找卡片；旧自主委派保持现有入口。
正式多区域授权、冷启动准备、持久幂等和整组原子接收由 T06 接入。

App 公开 `preflight_card_instance_assignment`、`accept_guarded_card_instance_assignment`、回执与事件查询、
提交前取消、原 turn 中断、授权撤销以及精确终态证据核对。前台不查询 SQLite，不启动其他卡片或 PTY。

同一卡片仅串行传输正文和 Enter，沿用 100 ms 间隔；A 未结束时可合法提交 B。
B 在自身可信 Provider start 前保持 Submitted；无可靠 Hook 时保持 Untracked，不能推断运行或完成。
无跟踪且可能忙时，`force_untracked` 只能来自用户明确授权，仍不得绕过草稿、审批或未知输入保护。

## 验收映射

| 要求 | 证据 |
| --- | --- |
| 无 definition 卡片、两卡共享 definition 仍投指定实例；非目标零输入、无新 PTY | `crates/app/tests/unit/card_instance_delivery_tests.rs` |
| A/B 输入与 run/session/turn 结果独立，迟到 A 不结束 B | `crates/agent/tests/card_instance_coordination.rs`、App evidence tests |
| 重复操作不重排队；同 assignment 改参冲突 | Agent coordination tests、App delivery tests |
| 未绑定未知任务不凭正文猜恢复；已绑定未知任务仅原 exact turn 核对 | Agent coordination tests |
| Untracked 显式授权；纯预检与接收规则一致 | `crates/agent/tests/card_instance_untracked.rs`、App input tests |
| 正文与 Enter 独占；新旧自动路径、人类草稿、审批及手机控制不能插入 | `crates/runtime/tests/workspace_card_input.rs`、App input tests |
| 正文后会话替换或撤权不 Enter、不清行；部分 IO 和 RPC 回执丢失未知，不盲目重发 | Runtime input tests、App input tests |
| 原 turn 取消不误中断排队 B 或较新直接任务 | App evidence tests |
| 重复旧直接 start 不回退最新 turn；同 session 新 run 不绕过去重 | `crates/runtime/tests/workspace_card_hook_identity.rs`、App evidence tests |
| 同批旧、新任务按 Hook 身份认领；同批终态后切会话仍保留原证据 | App evidence tests，经生产 `AgentHookServer::drain_into_runtime` 与 frame 共用核对入口 |

旧审批测试原先用同一个 turn 模拟三轮提交；现改为各轮独立 Provider turn ID。
重复 start 的旧测试保留计时与输入保护断言，同时明确 Runtime 在状态更新前拒绝重复事件。

## 验证与限制

已通过：

- `cargo test -p agent -p runtime --quiet`：受影响两 crate 完整测试通过；保留原有忽略项。
- `cargo test -p app --lib card_instance --quiet`：10 项通过，包含同批 Stop 后 SessionStart 的回归。
- `cargo test -p app --lib agent_hook --quiet`：11 项通过，覆盖受影响 Hook 接收路径。
- `cargo check -p app --quiet`：生产 App 编译检查通过。
- `cargo fmt --all -- --check`、`git diff --check` 与源码行数护栏通过。
- 存储/性能护栏、no-allow 与域层无 GPUI 检查通过。

独立对抗式审查已关闭 4 项发现，并复核事件接收时的私有身份快照、Runtime start 去重、
共享预检和精确事件过滤；最终只读复核未发现新明确反例。

录制 PTY 仅证明输送和零额外输入，不证明真实 Provider 完成了用户任务。
本任务未改页面或样式；真实 App、真实 Provider、Dock 回执/导航、跨项目加载闭环由 T07/T08 集成验收。
自然 RPC 终态需 T08 用可信原 run/session/turn 结果回调核对接口；不以卡片 Done 完成下一任务。
不安全输入隔离不因一条结果摘要而释放，未知正文不自动重放。未从历史账本恢复旧输入队列。
