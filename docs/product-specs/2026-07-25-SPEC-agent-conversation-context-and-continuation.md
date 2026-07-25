# Agent 会话感知与跨卡片续聊 SPEC

- 日期：2026-07-25
- 状态：Draft
- 范围：Norma Master Agent、Agent 卡片会话上下文、跨卡片续聊、回复排队与持久化
- 相关规格：
  - [`2026-07-18-SPEC-natural-agent-delegation-and-canvas-context.md`](2026-07-18-SPEC-natural-agent-delegation-and-canvas-context.md)
  - [`2026-07-18-SPEC-deterministic-agent-delegation-dispatch.md`](2026-07-18-SPEC-deterministic-agent-delegation-dispatch.md)
  - [`2026-07-18-SPEC-norma-master-agent-runtime.md`](2026-07-18-SPEC-norma-master-agent-runtime.md)

## 1. 背景与目标

### 1.1 背景

Norma 当前每轮能获得当前项目、Canvas 卡片标题、选中/聚焦状态，以及 Agent 名册中的启用、活跃、跟踪和忙碌状态，但不知道各 Agent 卡片最近完整的对话内容。

典型失败场景：

1. 用户直接在 Alan Agent 卡片中讨论需求。
2. Alan 给出 A/B 两个方案并等待用户选择。
3. 用户转而对 Norma 说：“Alan，按照 A 方案执行。”
4. Norma 不知道 Alan 刚刚说了什么，可能追问“A 方案是什么”、生成普通回复，或错误地把这句话作为一项新任务排到 Alan 当前工作之后。

这里存在两个不同的产品缺口：

- **会话感知缺口**：Norma 不知道 Agent 卡片最近完整的用户输入和 Agent 回复。
- **动作语义缺口**：现有 `assign_agent_task` 表示“创建或排队一项新任务”，不能表达“回复 Agent 当前会话”。

仅把最后一条消息塞进 Prompt 不能完整解决问题。如果 Alan 正在等待用户决策，而回复又进入新任务队列，就会形成“Alan 等回复、回复等 Alan 当前任务结束”的错误时序。

### 1.2 产品目标

建立“结构化会话尾部上下文 + 明确的会话回复动作”，使 Norma 能够：

- 感知所有活跃 Agent 卡片的最近完整回合。
- 理解“Alan 刚才建议什么”“谁在等我决定”等状态问题。
- 将“Alan，按照 A 方案执行”发送到 Alan 当前会话，而不是创建新任务。
- 在 Agent 忙碌时把回复排为同一会话的下一条输入，并允许提交前撤回。
- 对已发送、已排队、失败、取消和中断给出真实反馈。
- 在 Claude、Codex、Cursor、Grok、OpenCode 五种 Agent Provider 上使用统一领域契约。

### 1.3 成功定义

本功能成功时，用户可以先直接与任一 Agent 卡片讨论，再通过 Norma 对该会话做决策或追问，无需重复背景；Norma 不会把上下文依赖的回复误当成独立新任务，也不会把“已排队”误报为“已发送”或“已执行”。

## 2. 范围

### 2.1 In Scope

- 所有活跃 Agent 卡片的有界结构化会话尾部。
- 最近一次用户输入和最近一条完整 Agent 回复。
- 用户手动输入、Norma 会话回复和任务派发产生的 Agent 对话。
- 五种 Provider 的消息采集与统一归一化。
- 项目级有界持久化和重启恢复。
- 明确点名 Agent 时的确定性会话回复。
- 基于会话尾部的只读状态问答。
- 单 Agent 和多 Agent 会话回复。
- 忙碌状态下的同会话回复队列。
- 排队回复的自然语言撤回。
- 真实、逐目标的结构化回执。
- 上下文预算、截断、不可用和隐私边界。

### 2.2 Out of Scope

- 完整对话历史浏览或搜索。
- 把完整终端输出、工具结果、文件内容、网页正文或 diff 注入 Norma。
- 自动恢复已经关闭且没有活跃会话的 Agent 卡片以继续旧对话。
- 跨 Agent 工作依赖、等待接力或自动协作计划。
- 让一个 Agent 读取另一个 Agent 的完整会话。
- 向正在运行的 Agent 立即注入 Steering 指令。
- 把普通自然语言解释成工具权限批准。
- 自动回滚已经送入 Agent CLI 的消息。
- 新增 Settings、页面、弹窗或专用会话管理 UI。

## 3. 领域语言与用户场景

### 3.1 领域语言

#### Agent 新任务

一项具有独立任务身份、状态和队列生命周期的工作，继续通过 `assign_agent_task` 与 `AgentTaskCoordinator` 管理。

#### Agent 会话回复

发送到一个已经活跃的 Agent 会话中的下一条用户消息。它沿用目标 Agent 的现有 session，不创建 Agent task，也不占用 Agent task FIFO。

#### 结构化会话尾部

Norma 可见的有界上下文，包含 Agent 身份、会话状态、最近用户输入、最近完整 Agent 回复、更新时间、可用性和截断信息。它不是完整对话，也不是执行真相源。

#### 最近完整 Agent 回复

Provider 已明确结束生成的最近一条 Agent 文本回复。正在流式生成的部分文本不属于“完整回复”，不得进入 Norma 的默认上下文。

### 3.2 关键用户故事

1. 作为用户，我想对 Norma 说“Alan，按照 A 方案执行”，让这句话进入 Alan 当前会话。
2. 作为用户，我想问“Alan 刚才建议什么”，让 Norma 根据 Alan 最近完整回复回答。
3. 作为用户，我想问“谁在等我决定”，让 Norma根据所有活跃 Agent 的尾部上下文回答。
4. 作为用户，我想在 Alan 正忙时继续补充要求，使消息在同一会话中排为下一条，而不是创建新任务。
5. 作为用户，我想在回复尚未提交时说“别发给 Alan 了”，撤回排队消息。
6. 作为用户，我想说“Alan 和 Bella，都按 A 方案执行”，让系统分别向两个活跃会话发送同一句话，并逐项回报。
7. 作为用户，我想说“给 Alan 一个新任务：检查支付模块”，使它继续走现有 Agent 任务队列。
8. 作为用户，我想在 Alan 不活跃时说“Alan，检查 README”，使自包含请求继续自动激活并派发为新任务。
9. 作为用户，我想在 Alan 不活跃时说“Alan，继续刚才那个”，让系统明确拒绝，而不是向空白会话猜测发送。
10. 作为用户，我希望 App 重启并恢复同一活跃 Agent session 后，Norma 仍能立即看到最近完整回合。

## 4. 业务流程与路由规则

### 4.1 主流程：续接活跃会话

```text
Agent Hook / Provider adapter
  → 捕获用户输入
  → 捕获完整 Agent 回复
  → 更新内存中的结构化尾部
  → 后台持久化项目级尾部

用户对 Norma 输入
  → 解析句首 Agent 名称/语音别名
  → 判断是否明确要求“新任务”
  → 校验目标是否存在活跃 session
  → 立即发送，或排为同一 session 的下一条
  → 返回 sent / queued / failed 的真实回执
```

### 4.2 会话回复与新任务的优先级

| 输入与目标状态 | 行为 |
|---|---|
| Alan 活跃；“Alan，按照 A 方案执行” | 回复 Alan 当前会话 |
| Alan 活跃；“Alan，再补充一个约束……” | 回复 Alan 当前会话 |
| Alan 活跃；“给 Alan 一个新任务：检查支付模块” | 进入 Agent 新任务队列 |
| Alan 不活跃；“Alan，检查 README” | 自包含请求，自动激活并作为新任务派发 |
| Alan 不活跃；“Alan，按照 A 方案执行” | 拒绝；说明没有可续接会话 |
| Alan 不活跃；“Alan，继续刚才那个” | 拒绝；不得猜测或创建空白新任务 |
| “Alan 刚才建议什么？” | 只读问答，不发送消息 |
| “我觉得 Alan 这个名字不错” | 普通对话，不发送消息 |

“新任务”“另一个任务”“单独派发”“排一个任务”等明确措辞优先于默认续聊语义。

对于不活跃 Agent，自包含性判断必须保守。包含“继续、接着、刚才、之前、上面、这个、那个、照此、按 A/B 方案、同意、不同意、就这样”或等价英文指代的表达，不得自动转成新任务。模型认为不确定时只允许失败或提出一个最小问题，不得猜测执行。

### 4.3 多 Agent 回复

- 多个句首精确名称或语音别名可以形成回复目标集合。
- 同一句回复按用户表达顺序分别发送给每个 Agent。
- 每个目标独立预检和执行，不要求原子成功。
- 一个目标失败不阻止其他有效目标发送。
- 最终回执必须逐项说明已发送、已排队或失败，不得用“都已发送”掩盖部分失败。
- 多 Agent 回复不代表 Agent 之间共享上下文、实时协作或互相等待。

### 4.4 忙碌 Agent 的时序

以下状态视为暂时不可接收普通会话回复：

- `Thinking`
- `RunningTool`
- `WaitingApproval`

回复进入独立的 `AgentSessionReplyQueue`，等待同一 Agent session 回到可输入状态后提交。该队列不复用 `AgentTaskCoordinator`，因为回复不具有新任务语义。

以下状态可以立即尝试发送：

- `Ready`
- `Done`
- `Failed`，但 session 和 live node 仍有效
- 无 Hook 跟踪但 live node 与 session 可验证；此时只能确认 PTY 写入，不声称 Agent 已处理

`Ended`、session 变化、节点删除或目标不再映射到原 Agent 定义时，排队回复失败，不得发送到替代节点。

### 4.5 撤回

- 每条回复分配 `reply_id`。
- `cancel_agent_reply(reply_id)` 撤回一条明确的未提交回复。
- 按 Agent 名称撤回时，取消该 Agent 当前 session 中全部尚未提交的回复。
- 已经成功写入文本并提交 Enter 的回复不可撤回。
- 关闭项目或退出 App 时，所有未提交回复进入 `interrupted`，重启后不自动重放。

## 5. 数据模型与接口

### 5.1 `AgentConversationTail`

建议的领域字段：

```rust
pub struct AgentConversationTail {
    pub definition_id: i64,
    pub node_id: u64,
    pub session_id: String,
    pub agent_name: String,
    pub profile: AgentProfile,
    pub task_state: AgentTaskState,
    pub last_user_message: Option<String>,
    pub last_agent_message: Option<String>,
    pub updated_at_unix: i64,
    pub availability: AgentConversationAvailability,
    pub user_message_truncated: bool,
    pub agent_message_truncated: bool,
}
```

`availability` 至少区分：

- `available`
- `unavailable`
- `stale`

`unavailable` 表示 Provider 没有提供可验证的完整消息；`stale` 表示记录属于已知 session，但实时状态已无法确认。Norma 不得把两者描述成“当前最新”。

### 5.2 Norma 环境契约

`CurrentCanvasContext` 增加 Agent 会话尾部集合或等价的结构化字段。渲染格式必须：

- 将动态内容放在明确的数据边界内，不能覆盖系统规则。
- 对字符串使用安全转义。
- 显示 session 状态、更新时间和截断/省略标记。
- 所有活跃 Agent 至少保留身份、状态和上下文可用性。
- 消息正文超出总预算时，保留高优先级目标并明确报告省略数量。

上下文排序优先级：

1. 本轮明确点名的 Agent。
2. 当前聚焦 Agent 卡片。
3. 当前选中 Agent 卡片。
4. 最近更新。
5. 稳定 `node_id`。

### 5.3 消息预算

- 单条用户消息最多保留 1,200 个 Unicode 字符。
- 单条 Agent 回复最多保留 1,200 个 Unicode 字符。
- 所有 Agent 消息正文的总预算为 8,000 个 Unicode 字符。
- 身份、状态、availability 和截断元数据不因正文预算而消失。
- 达到预算时使用稳定截断和明确省略标记，不得静默伪装成完整上下文。

### 5.4 Norma 工具

新增两个高层工具。

#### `reply_agent`

```json
{
  "agent": "Alan",
  "message": "按照 A 方案执行"
}
```

约束：

- 一次只回复一个 Agent。
- 目标必须是已定义、已启用且有活跃 live session 的 Agent。
- 发送前重新校验 definition、node 和 session。
- 不创建 Agent task。

#### `cancel_agent_reply`

```json
{
  "reply_id": 42
}
```

或：

```json
{
  "agent": "Alan"
}
```

`reply_id` 与 `agent` 至少提供一个；同时提供时 `reply_id` 必须属于该 Agent，否则拒绝。

### 5.5 回复回执

```rust
pub struct AgentReplyReceipt {
    pub reply_id: u64,
    pub definition_id: i64,
    pub name: String,
    pub node_id: u64,
    pub session_id: String,
    pub message: String,
    pub status: AgentReplyStatus,
}
```

`AgentReplyStatus`：

- `Sent`
- `Queued`
- `Failed`
- `Cancelled`
- `Interrupted`

`Sent` 只证明文本与 Enter 已被 Runtime/PTY 接受，不证明 Agent 已理解、处理或完成。

### 5.6 持久化

项目 SQLite 保存每个活跃 session 的最近完整回合，推荐主键：

```text
(definition_id, session_id)
```

至少包含：

- definition/node/session 身份
- Provider profile
- 最近用户消息
- 最近完整 Agent 回复
- 更新时间
- availability
- 两侧截断标记

生命周期：

- 新完整回合覆盖同 session 的旧尾部。
- App 恢复同一 Agent session 时加载尾部。
- 卡片删除或 session 明确结束时清理。
- 不保存完整历史。
- 不持久化等待中的回复以供重启重放。

## 6. 技术实现概览

### 6.1 分层边界

#### `crates/norma`

- 在 Provider Hook 边界识别用户消息和完整 Agent 回复。
- 将五种 Provider 的差异归一化为中立会话观察事件。
- 不持有项目 SQLite 或 GPUI 状态。

#### `crates/agent`

- 定义结构化尾部、上下文预算和渲染规则。
- 定义 `reply_agent`、`cancel_agent_reply` 工具及 Adapter 命令。
- 实现确定性句首点名路由和 reply/task 语义优先级。
- 保持零 GPUI、零 Storage 依赖。

#### `crates/runtime`

- 继续提供 live node、Agent task state、session id 和输入发送能力。
- 不读取 Provider 历史目录，不依赖 `agent` 或 `app`。

#### `crates/storage`

- 提供项目级尾部 Store、migration 和后台写入 seam。
- 不依赖 `app`、`runtime` 或 `ui`。

#### `crates/app`

- 将 Hook 观察映射为当前项目的会话尾部。
- 维护 `AgentSessionReplyQueue`。
- 在 GPUI 前台只更新内存状态和执行轻量 Runtime 输入副作用。
- 通过后台线程/worker 完成 SQLite 写入和必要的 Provider transcript 读取。
- 在发送前重新校验 Agent 定义、node 和 session。

### 6.2 Provider 采集

首版承诺覆盖：

- Claude
- Codex
- Cursor
- Grok
- OpenCode

采集优先级：

1. Provider 完成事件直接携带的最终 Agent 文本。
2. Provider 提供明确 transcript/session 定位时，在后台读取当前回合的有界尾部。
3. 无法可靠得到完整文本时标记 `unavailable`。

禁止：

- 从终端可见 grid 猜测最后一条消息。
- 在 GPUI 前台扫描 Provider 会话目录。
- 读取无界完整 transcript 后再截断。
- 把工具输出误识别为 Agent 最终回复。
- 把流式中间片段标记为完整回复。

每个 Provider 必须拥有固定输入 fixture 和契约测试，不能以一个 Provider 的字段结构类推其他 Provider。

### 6.3 确定性回复路由

模型调用前先执行轻量、纯逻辑的直接寻址解析：

- 只接受当前项目中精确 Agent 名称或已登记语音别名。
- 允许句首逗号、冒号、空格和常见 ASR 标点差异。
- 目标后必须存在非空消息。
- 明确的新任务措辞转入任务语义。
- 活跃目标的普通直接寻址转入会话回复语义。
- 普通提及、说明性问题和状态查询不得触发副作用。

确定性路由保证“Alan，按照 A 方案执行”不依赖模型是否愿意调用工具。模型仍负责非确定性状态问答、总结和必要的自然回复。

### 6.4 安全发送

会话回复复用现有 Agent CLI 文本输入和 Enter 提交能力，但必须与 Agent task 提交状态分离。

执行顺序：

1. 校验 Agent 定义仍存在且启用。
2. 校验 node 仍属于该 definition。
3. 校验 node 仍为 live Agent 节点。
4. 校验 session id 与排队时一致。
5. 输入文本。
6. 在既有安全延迟/就绪边界后提交 Enter。
7. 任一步失败时返回结构化失败或 unknown，不自动重复可能已发生的副作用。

如果输入成功但 Enter 失败，必须沿用安全清空与未知副作用保护，不能直接重试整条文本。

### 6.5 后台与性能边界

- Hook drain、PTY output chunk、`Runtime::drain_all`、`send_node_input` 和 Canvas mouse move 中不得同步写 SQLite。
- transcript 定位和读取必须后台化、按 session 精确读取且先限制字节范围。
- 同一 session 高频 Hook 事件可以合并后台写入，只持久化最近完整回合。
- Context 构建只读取内存快照，不在每次 Norma turn 查询 SQLite。

## 7. UI/UX 需求

### 7.1 交互原则

- 遵循 macOS 直接操作与即时反馈原则。
- 目标和动作明确时不增加确认弹窗。
- 不新增专用 UI；反馈复用 Norma 当前文字/语音回复。
- 反馈使用用户能理解的 Agent 名称，不暴露内部 definition/session id，除非诊断需要。
- 多目标结果按用户表达顺序汇报。

### 7.2 回执文案语义

允许的语义示例：

- `Sent`：“已经发给 Alan 了。”
- `Queued`：“Alan 还在处理当前内容，我已经把这句话排为下一条。”
- 部分成功：“已发给 Alan；Bella 没有可续接的会话，所以没有发送。”
- `Cancelled`：“还没发出的 Alan 回复已经撤回。”
- `Failed`：“Alan 当前没有可续接的会话，所以我没有发送‘按 A 方案执行’。”

禁止：

- 把 `Queued` 说成“已经发给”。
- 把 `Sent` 说成“Alan 已经开始执行”或“已经完成”。
- 上下文 `unavailable` 时编造 Alan 的建议。
- 多目标部分失败时说“都处理好了”。

### 7.3 状态问答

Norma 可以根据有界尾部回答：

- “Alan 刚才建议什么？”
- “谁在等我决定？”
- “Bella 最后是在问我问题吗？”

Norma 必须把推断与事实区分开：

- “Alan 的最后一条完整回复是……”
- “从这条回复看，他似乎在等你选择 A/B。”

“是否等待用户决策”不是 Provider 保证提供的事实字段时，不能当作确定状态陈述。

## 8. 状态、错误与边界情况

### 8.1 上下文不可用

- 活跃 Agent 有 live session，但 Provider 无法提供完整尾部：
  - 明确点名回复仍可发送。
  - Norma 回答历史问题时说明上下文不可用。
- 记录属于旧 session：
  - 标记 `stale`。
  - 不用于声称当前最新消息。
  - 不把回复发送到新 session。

### 8.2 Agent 不活跃

- 自包含请求可以沿用现有自动激活和新任务派发。
- 上下文依赖请求明确失败。
- 不自动恢复旧 session 来尝试理解“A 方案”。
- 不从持久化尾部推断已经关闭的会话仍可安全续接。

### 8.3 Agent 忙碌

- 回复进入会话回复队列。
- `WaitingApproval` 不自动解释为业务决策等待，也不自动输入批准。
- 队列等待期间目标 session 变化则失败。
- 队列可被自然语言取消。

### 8.4 多目标部分失败

- 每个目标独立产生回执。
- 有效目标继续执行。
- 最终反馈列出失败目标与原因。
- 不提供事务性回滚。

### 8.5 重复与幂等

- 同一 Norma tool `operation_id` 只能产生一次回复或一次排队记录。
- 重复收到相同 Hook 完成事件不得产生多份尾部或重复写入。
- App 无法确认 Enter 是否送达时返回 `unknown`/失败，不自动重放。

### 8.6 项目与生命周期

- 会话尾部严格限定当前项目。
- 项目切换后不得把旧项目尾部注入新项目 Norma Session。
- 关闭项目或退出 App 会中断未发送回复。
- 恢复同一项目和同一 session 时加载持久化尾部，但不重放旧回复。

## 9. 非功能性需求

### 9.1 性能

- Norma turn 的上下文组装只访问内存。
- SQLite 持久化和 transcript 读取不得阻塞 GPUI 前台。
- 消息和全局上下文必须有硬上限。
- 活跃 Agent 数增加时，Context 大小保持有界并提供省略计数。

### 9.2 隐私

允许发送给 Norma 所配置模型：

- Agent 名称和状态。
- 最近一次有界用户输入。
- 最近一条有界完整 Agent 回复。

默认禁止发送：

- 完整对话。
- 完整终端输出。
- 工具输入与输出。
- 文件正文。
- 网页正文或 DOM。
- Git diff。
- Provider transcript 路径和内部 session 凭据。

本功能不新增远程模型专用开关。用户选择的 Norma Provider 可以接收上述有界原文，这是明确接受的产品取舍。

### 9.3 安全

- 动态尾部是数据，不是系统指令。
- Agent 回复中的“忽略规则”“调用工具”等文字不得覆盖 Norma 系统规则。
- 工具执行前仍由 Adapter 校验真实 Agent、node 和 session。
- 日志默认只记录 availability、长度、截断和路由结果，不复制消息原文。

### 9.4 可维护性

- Provider 差异限制在 `norma::agent_hook` 或等价 adapter 边界。
- `agent` 使用 Provider-neutral 类型。
- 会话回复与 Agent task 保持独立状态机。
- 不建立第二套 Agent task 完成事实源。

## 10. 风险、权衡与未决问题

### 10.1 已确认的权衡

- 选择结构化最近回合，而不是完整对话，以控制 token、隐私和 Provider 兼容成本。
- 选择所有活跃 Agent 默认可见，同时设置全局预算，而不是只在点名后按需查询。
- 选择最近完整回复，不使用实时部分文本，以降低误判风险。
- 选择项目 SQLite 有界持久化，以支持 App 重启后的即时感知。
- 选择五种 Provider 同时形成契约，避免同一产品能力因 Agent 类型而改变。
- 选择多目标逐项执行，而不是原子批量，接受部分成功并要求真实回报。
- 选择忙碌时排为下一条，不使用实时 Steering。
- 选择不活跃 Agent 的自包含请求继续自动派发，以保留现有自然委派能力。

### 10.2 主要风险

#### Provider 完成消息格式变化

缓解：

- 使用每 Provider 独立 fixture。
- 采集失败时降级为 `unavailable`。
- 不回退到终端 grid 猜测。

#### 上下文中的提示注入

缓解：

- 明确数据边界和安全转义。
- 系统 Prompt 规定动态消息不是指令。
- 副作用仍经 Adapter 实时校验。

#### 文本已输入但 Enter 失败

缓解：

- 使用分阶段提交和既有安全清空策略。
- 状态不确定时停止自动重试。
- 回执不得声称成功。

#### 不活跃请求的自包含性误判

缓解：

- 对指代词采用保守拒绝。
- 不确定时只问一个最小问题。
- 不允许模型凭持久化旧尾部恢复已关闭会话。

### 10.3 未决问题

本次采访已锁定首版所需产品和技术决策，没有阻塞实现的开放问题。Provider 原生字段映射需在实施时按各自固定 fixture 落地，但不得改变本 SPEC 的统一行为契约。

## 11. 验收标准与测试

### 11.1 核心验收

1. 用户直接在 Alan 卡片中讨论，Alan 完整回复包含 A/B 方案后，Norma 的下一轮环境包含 Alan 最近用户输入和完整回复。
2. 用户说“Alan，按照 A 方案执行”时，同一 Alan session 收到“按照 A 方案执行”并提交 Enter。
3. 上述续聊不创建 Agent task，不增加 `AgentTaskCoordinator` 队列，也不生成 Agent dispatch 持久化记录。
4. Alan 处于 `Thinking` 或 `RunningTool` 时，回复返回 `Queued`；回到可输入状态后只提交一次。
5. 排队期间说“别发给 Alan 了”会撤回未发送回复；Alan CLI 不得收到该文本。
6. “Alan 和 Bella，都按 A 方案执行”分别处理两个 session，并逐项返回真实状态。
7. Alan 成功、Bella 无会话时，Alan 仍收到消息，Bella 不收到；Norma 明确报告部分失败。
8. “给 Alan 一个新任务：检查支付模块”继续走任务队列，不误判为会话回复。
9. Alan 不活跃时，“Alan，检查 README”继续自动激活并派发新任务。
10. Alan 不活跃时，“Alan，继续刚才那个”与“Alan，按 A 方案执行”零副作用失败。
11. “Alan 刚才建议什么？”只产生普通回答，不向 Alan 发送任何内容。
12. “我觉得 Alan 这个名字不错”不得产生会话回复或 Agent task。
13. App 重启并恢复同一项目、同一 Alan session 后，Norma 能读取持久化的最近完整回合。
14. 删除 Alan 卡片或结束其 session 后，对应尾部被清理且不能用于续聊。
15. 正在生成的部分回复不进入 Norma Context；只在 Provider 完成事件后替换最近完整回复。
16. Context 不包含构造的终端秘密、工具输出、文件正文、网页正文或 diff 标记。
17. 超出单条或总预算时输出稳定截断/省略标记，并始终保留所有活跃 Agent 的身份、状态和 availability。
18. Claude、Codex、Cursor、Grok、OpenCode 的固定样本均通过相同会话尾部契约测试。

### 11.2 单元测试

- `norma`：
  - 五种 Provider 的用户消息与完整 Agent 回复解析。
  - 部分消息、工具输出和无效 payload 不得成为完整回复。
  - 重复完成事件幂等。
- `agent`：
  - 句首点名、别名、多目标、新任务优先级和普通提及负例。
  - Context 排序、单条预算、总预算和安全转义。
  - `reply_agent` / `cancel_agent_reply` schema 与 Adapter 命令解析。
- `runtime`：
  - session/node 身份和输入发送边界。
- `storage`：
  - migration、覆盖写入、项目隔离、重启恢复和删除清理。
- `app`：
  - 立即发送、排队、释放、取消、中断、部分失败和 session 变化拒绝。
  - SQLite 写入不发生在 GPUI 前台热路径。

所有测试必须遵循仓库强制规范：

- 测试函数上方包含场景、数据构造、执行过程和预期结果。
- 数值预算写明推导过程。
- 每个断言带失败说明字符串。
- 关键状态逐项写具体正断言和负断言，不用循环掩盖验收点。

### 11.3 e2e

在 `crates/e2e` 增加用户可观察闭环：

```text
进入项目
→ 创建并激活 Alan Agent 卡片
→ 注入用户输入与 Alan 完整 A/B 回复 Hook
→ 通过 Norma 提交“Alan，按照 A 方案执行”
→ 验证 Alan 同一 PTY/session 收到文本与 Enter
→ 验证没有新增 Agent task
→ 验证 Norma 回执为 sent
```

补充 e2e：

- 忙碌 Agent 排队并在终态后发送。
- 排队回复自然语言取消。
- 多 Agent 部分失败。
- 项目重启恢复尾部。
- 不活跃自包含请求与上下文依赖请求的分流。
- 状态问答零副作用。

Headless GPUI e2e 使用可控 PTY 和 Hook 注入验证主行为；五种真实 CLI 的 smoke 可作为环境受控的 ignored L3 测试存在，并与 headless 证据明确区分。

### 11.4 必跑验证

```bash
cargo test -p norma
cargo test -p agent
cargo test -p runtime
cargo test -p storage
cargo test -p e2e
cargo check -p app
cargo test --workspace
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```

## 12. 实施约束

- 实现必须从公开 seam 开始 TDD，不先写生产逻辑。
- 不允许用 `#[allow(...)]` 压制诊断。
- Rust 单文件不得超过 500 行。
- 不得让 `agent` 依赖 Storage、Runtime、App 或 GPUI。
- 不得让 `runtime` 依赖 `agent` 或 `app`。
- 不得在 PTY、Canvas 输入或 GPUI 前台热路径同步读写 SQLite。
- 本 SPEC 只定义后续实现，不授权把完整 Agent 对话扩大为 Norma 的默认上下文。
