# 可语音呼叫 Agent 实施计划

- 日期：2026-07-14
- 分支：`feat/voice-addressable-agents`
- 固定起点：`430a88f686d4fd226f576f15d75726a43c424ad6`
- 对应规格：[`docs/product-specs/2026-07-14-SPEC-voice-addressable-agents.md`](../product-specs/2026-07-14-SPEC-voice-addressable-agents.md)

## 实施原则

- 采用垂直 TDD：每次只增加一个可观察行为，先看到聚焦测试失败，再做最小实现并复测。
- Agent 名称、定义、单卡、队列与回执收口到零 GPUI 的协调模块；应用层只负责适配 Runtime、Storage 与 UI。
- 单次语音与持续聆听共享同一组高层 Agent 工具；模型提示词不承担确定性不变量。
- 每条用户可观察验收都映射到自动化测试；真实 CLI/Hook 环境使用 ignored L3，避免把 headless 测试误称为真实应用测试。

## 垂直切片

### 1. 名称目录与定义契约

- 在 `agent` 域层建立中文/英文名称校验、规范化键、30 个固定名称与中文语音别名、确定性分配器。
- 扩展 Storage 的 Agent 定义接口：Workflow Description 可空、正式名与预设别名共享唯一命名空间、稳定列出与重命名。
- 聚焦验证：`cargo test -p agent`、`cargo test -p storage`。

### 2. 卡片身份与协调模块

- Agent 卡片快照持久化 definition id，兼容旧 alias-only snapshot。
- 建立公开协调命令/结果接口，注入定义仓库、任务仓库与活跃卡片 host。
- 实现定义创建、激活幂等、单 Agent 单卡、重命名同步、活跃保护、旧卡迁移与部分激活失败。
- 聚焦验证：`cargo test -p agent -p canvas -p runtime -p storage`。

### 3. 委派、队列与真实回执

- 增加按 Agent 隔离的 FIFO、批量原子预检、managed/untracked 跟踪、force 降级、终态推进和 Interrupted 恢复。
- 区分 created、activated、submitted、queued、untracked、completed、failed、interrupted 与 no-effect rejection。
- 聚焦验证：协调模块公开 seam 与 Storage 恢复测试。

### 4. 高层工具、提示词与路由

- 为 CardTask 增加 `list_agents`、`create_agent`、`activate_agent`、`dispatch_agent_tasks`。
- 禁止通用 `create_card(kind=agent)` 和 `send_text` 绕过协调器。
- Dialogue/CardTask 每轮注入紧凑动态名册，补充工具选择、普通提及保护和真实回执规则。
- 为明确呼叫增加确定性 fallback；未知、禁用和含糊分工保持零副作用。
- 聚焦验证：完整 `ProviderRequest`、公开 AgentEvent 与结构化工具结果。

### 5. 应用入口与交互

- Settings Add Agent 使用未保存草稿：自动名、显式 Codex 默认、可空 Workflow Description，Save 只保存定义。
- 工具栏、菜单、快捷键创建定义并立即打开唯一卡片；已有定义通过 Open Card 激活。
- 单次语音与持续聆听接入共享工具；按模式区分完成 UI 通知与 TTS。
- 忙碌关闭、活跃编辑保护和 CLI 启动部分失败使用统一协调结果。

### 6. 自动化验收

- Headless e2e 覆盖 Settings 保存、快速创建、两种语音模式、未知/禁用零副作用、managed FIFO、untracked force 与旧卡迁移。
- ignored L3 使用真实 PTY/CLI/Hook；环境缺失时输出明确跳过原因。
- 根据受影响范围运行 crate 测试、`cargo test -p e2e`、`cargo test --workspace`、`cargo check -p app`、release build 和仓库护栏脚本。

### 7. 审查与提交

- 以规格符合性和工程质量两个轴进行本地审查，修复发现的问题后重跑对应验证。
- 检查只包含本 worktree 的目标变更，使用正常 hooks 提交到 `feat/voice-addressable-agents`。
