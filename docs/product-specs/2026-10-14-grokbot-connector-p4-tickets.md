# Grok Bot 连接器 P4：派工、取消、回复与（待定）建卡——工单拆分

状态：工单拆分（仅文档，不改代码）· 2026-10-14 按 yakami 新方向重写。叠在 PR #105（`docs/grokbot-connector-spec-1014`）之上。
唯一规格来源：`docs/product-specs/2026-10-13-grokbot-norma-connector.md`（下称「主规格」）。AC 编号接续主规格；AC-54 已删除不复用；本文中被新方向淘汰的 AC 标「已删除，编号不复用」，新增从 AC-85 起。

代码依据：origin/dev（下列 file:line 均在 dev 上核对）；P2 分支 `origin/feat/grokbot-connector-p2`（PR #104，未合入）。**【假设】** 表示未在代码中核实。

## 0. 新方向（yakami 2026-10-14）与不得改动事项

新方向：
1. 写工具是 **AdapterCommand 的薄映射**：经 `execute_norma_adapter_command_with_source` 执行，新增 `AgentInputSource::Connector`。
2. 幂等 / 回执复用 **`MobileCommandStore` reserve + receipt**（或抽成共享模块），**不建 `connector_operations`**。
3. `AssignAgentTask` 增加可选 `node_id`（由 `card_id` 解析），`card_revision` 校验放在 AdapterCommand 层；**不再另建按节点派工入口**。
4. 新增工具 `reply_agent`。
5. 建卡改为映射 `create_agent(profile, task?)`；仍待 Y-1。
6. 对外仍只暴露 `card_id`、`operation_id` 与结构化错误码（不暴露 node_id、Agent 名寻址或 AdapterError 原文）。
7. 明确范围外：`send_terminal_input`、`delete_canvas_card`、`focus_card`、记忆写入。

不得改动（沿用主规格）：Token 明文存 Norma SQLite；优先 Tailscale Funnel；AC-54 已删除；Norma 未运行时 Grok Bot 显示「Norma 未运行」；连接器永不 `force_untracked`；`card_revision` 只由持久状态（身份 | 项目根目录 | 工作目录 | profile | session_id）计算；`resolve_approval` 属 P5 且每次先问 yakami；主规格 1.4 / 6.6 的「不新建卡片」保持，标「待 Y-1」。

## 1. 代码现状（file:line）

| 事实 | 位置 |
| --- | --- |
| `AdapterCommand` 枚举：`CreateAgent { profile, task: Option<String> }`、`AssignAgentTask { agent, task }`、`CancelAgentTask { task_id: u64 }`、`ReplyAgent { agent, message }`、`CancelAgentReply { reply_id?, agent? }` 等 | `crates/agent/src/adapter.rs:32–61` |
| 工具定义 `create_agent`（profile ∈ codex/claude/opencode/cursor/grok，可附 task「创建后立即入队」）`:312`；`assign_agent_task` `:323`；`cancel_agent_task` `:334`；`reply_agent`（「向已有卡片发会话消息…不创建新任务」）`:342`；`focus_card` `:388`；`send_terminal_input` `:396`；`delete_canvas_card` `:408`；记忆工具 `:413–414` | `crates/agent/src/adapter.rs` |
| 参数结构 `CreateAgentArgs{profile, task?}` `:18`、`AssignAgentTaskArgs{agent, task}` `:25`、`ReplyAgentArgs{agent, message}` `:32`、`TaskIdArgs{task_id: u64}` `:46`，均 `deny_unknown_fields`；解析入口 `parse_adapter_command` `:107`（`create_agent` `:160`、`assign_agent_task` `:179`、`cancel_agent_task` `:185`、`reply_agent` `:192`） | `crates/agent/src/adapter/commands.rs` |
| 执行入口 `execute_norma_adapter_command_with_source(app, window: Option<&mut Window>, cx, command, input_source)` `:60`；记忆命令直接拒绝 `:68`；`CreateAgent` → `CardCommand::CreateAgent { name: None, profile, activate: true }`，有 task 时再 `dispatch_agent_task` `:108–147`；`AssignAgentTask` → `adapter_task::dispatch_agent_task(…, agent, task, input_source == DoubaoVoice)` `:149–156`（最后一个参数即 force_untracked 语义【假设：需核对 `adapter_task` 形参名】）；`CancelAgentTask` 区分未提交 / 运行中，`cancel_unsubmitted_task` / `cancel_task` + `persist_cancellation` `:157–250`；`ReplyAgent` → `reply_adapter::execute_reply_adapter_command` `:275–295`（仅 Voice/DoubaoVoice 触发视觉效果） | `crates/app/src/app/agent_runtime/adapter_pump.rs` |
| `AgentInputSource { Voice, DoubaoVoice, MobileDiscussion, MobileExecution, Other(默认) }` | `crates/agent/src/model.rs:4–14` |
| 手机实时工具：`RealtimeCall{device_id, call_id, turn_id, workspace_id, operation_id, request_hash = sha256([project_id, name, arguments])}` `:14–23`；鉴权后 `MobileCommandStore::reserve_realtime` `:36–40`；`Existing` 且非 unknown 直接返回原值，Pending/unknown「只读不重提交」`:51–55` | `crates/app/src/mobile/server/connection/realtime/policy.rs:8–55` |
| 回执轮询：`succeeded` 取结果、`outcome_unknown` →「不能重复派工」、`rejected` 返回错误、`accepted` 等待；35 s 截止（派工 30 s lease + 5 s）`:59–105`；之前要求 `realtime_command_authorized`（按用户原话判定执行意图）`:33` | `crates/app/src/mobile/server/connection/realtime/execution.rs` |
| `MobileCommandStore`：`reserve` `:63`、`finish` `:101`；`mobile_commands(device_id, operation_id, workspace_id, route_id, owner_id, request_hash, state ∈ accepted/succeeded/rejected/outcome_unknown, result_json, error, created_at_unix, PK(device_id, operation_id))` `:246–258`；实时表 `mobile_realtime_calls … UNIQUE(device_id, operation_id)` | `crates/storage/src/mobile_command_store.rs`、`…/realtime.rs:232–236` |
| `mobile_commands` 被多处读取（投递、计划、todo、thread、feedback） | `crates/storage/src/mobile_task_delivery_store.rs`、`project_plan_store.rs`、`mobile_command_store/{delivery,todo,thread,feedback}.rs` |
| P2 只读工具与输出（`Task.created_at/updated_at: i64`、`observed_at` unix 秒）；`card_revision` 已由持久目标状态派生（提交 `87ac5a71`） | P2 `crates/norma/src/connector/tools/{model,schema}.rs` |

## 2. 设计

### 2.1 执行路径（P4-1）

```
MCP tools/call → 参数校验(deny_unknown_fields) → card_id 验签解析 → MobileCommandStore reserve
  → AdapterCommand{…, node_id, expected_revision} → execute_norma_adapter_command_with_source(app, None, cx, cmd, AgentInputSource::Connector)
  → AdapterResult / AdapterError → 结构化回执 / 错误码 → finish → 审计
```

- 新增 `AgentInputSource::Connector`（`model.rs:4`）。规则：不等于 `DoubaoVoice` → 永不 force_untracked；不触发 Voice 视觉效果；`window = None`（连接器无窗口）【假设：`None` 路径对四个命令都可用，需核对 `execute_agent_card_command_with_optional_window`】。
- 不调用 `realtime_command_authorized`（它按手机原话判断意图）；连接器的意图确认在 Grok Bot 侧完成【假设，见 §6 R-a】。
- `AssignAgentTask { agent, task, node_id: Option<u64>, expected_revision: Option<String> }`、`ReplyAgent { agent, message, node_id, expected_revision }`：`node_id` 存在时**按节点**寻址，`agent` 只用于展示；节点不存在 → `card_not_found`；revision 不符 → `target_changed`。`node_id = None` 时现有按名字行为不变（语音 / 手机 / Agent 工具回归）。删除原 P4 设计中的「按节点派工内部入口」。
- `card_id` 解析出 `workspace / area / canvas / node_id`；连接器选取对应工作空间的 `MultisoulApp` 执行【假设：`app` 与工作空间 / 画布的对应关系需核对；目标未打开 → `workspace_closed`】。
- `AdapterError::Rejected(String)` 映射为稳定错误码（§2.6）；未识别的映射为 `agent_rejected`，原文只进审计，不回给 Grok Bot。

### 2.2 幂等与回执（P4-2）

- 复用 `MobileCommandStore::reserve / finish / receipt`（或把这三者与状态机抽成 `command_ledger` 共享模块，手机与连接器共用；二选一在 P4-2 内决定，外部语义一致）。
- 键映射：`device_id = "connector:<client_epoch>"`，`operation_id` 原样；`workspace_id` 取 card 所属工作空间；`route_id` = `card_id` 或 `task_id`；`owner_id = "grokbot"`；`request_hash = sha256(json([tool, 规范化参数]))`（同 `policy.rs:20` 含工具名）。
- `client_epoch`：轮换 24 h 宽限期内新旧 Token 同 epoch（同 `client_id`，AC-69）；吊销后重新生成的 Token 新 epoch，旧 `operation_id` 不返回旧回执、为独立命名空间（AC-72）。epoch 存 `connector_secrets`【假设：P2 表可加一行，不改 Token 明文存储决定】。
- 状态：`accepted → succeeded / rejected / outcome_unknown`。同键同参 → 原回执（`replayed=true`）；同键异参或异工具 → `operation_conflict`；崩溃 / 超时 → `outcome_unknown`，**只读核对、永不重提交**（`policy.rs:53–55`、`execution.rs:68–71` 同义）。
- 因 revision 校验在 AdapterCommand 层（预留之后），`target_changed` / `approval_pending` 等执行期拒绝会记为 `rejected` 并在重放时原样返回；**条件消除后需用新 `operation_id` 重试**（AC-59 改义）。纯参数校验与 `card_id` 验签失败在预留前返回，不占键。
- 保存期：连接器行 30 天后清理（AC-70，暂定，待 yakami 确认）。
- 隔离：所有读 `mobile_commands` 的手机侧查询排除 `connector:` 前缀；连接器行不经 `MobileDeviceGrant` 鉴权（AC-91）。若排除改动面过大，则改走共享模块 + 独立表（仍不叫、也不按原设计建 `connector_operations`），由 P4-2 评估【假设】。

### 2.3 `dispatch_task`（P4-3）

- 输入：`{"operation_id", "card_id", "card_revision", "task"(≤8000), "context"?, "allow_queue"?=true}`；`context` 拼入 task 正文【假设】。不加 `workspace_id`（暂定，待 yakami 确认）。
- 映射：`AssignAgentTask{agent: role_name, task, node_id, expected_revision}`。
- 输出：`{"operation_id", "task_id", "card_id", "role_name", "workspace_id", "state": "accepted|submitted|queued|untracked|outcome_unknown", "tracking", "replayed", "accepted_at"}`。`task_id` 为 `AdapterResult` 中的协调器任务号，以十进制字符串对外【假设：与 P2 `list_tasks.task_id` 同源，V-4】。提交 ≠ 运行 ≠ 完成（AC-26）。
- 错误：`invalid_argument`、`task_too_long`、`card_not_found`、`target_changed`、`workspace_not_found`、`workspace_closed`（暂定）、`card_busy`、`approval_pending`、`input_occupied`、`untracked_requires_confirmation`、`operation_conflict`、`agent_rejected`、`rate_limited`、`internal`；HTTP 401。`card_busy / approval_pending / input_occupied` 是否能从 `AdapterError` 区分出来【假设，V-5】；不能区分的归 `agent_rejected`。

### 2.4 `cancel_task`（P4-3）

- 输入：`{"operation_id", "task_id"}`；映射 `CancelAgentTask{task_id}`（`adapter_pump.rs:157`）。
- 输出：`{"operation_id", "task_id", "state": "cancel_requested|cancelled|already_terminal", "terminal_state"?, "requested_at", "replayed"}`。
- 只取消 Grok Bot 派出的任务（同 `connector:` 命名空间回执关联），直接取消无额外确认，可设置放开（暂定，待 yakami 确认）；其他 → `not_connector_task`。
- 未提交任务走 `cancel_unsubmitted_task`；`AlreadySubmitted` 类拒绝（`adapter_pump.rs:270`）映射为 `cancel_requested` 或 `agent_rejected`【假设，V-5】。
- 等待审批时取消：照常取消，不替用户作审批决定；审批随任务关闭 → `request_closed`（开发期验证 V-1）。

### 2.5 `reply_agent`（P4-4，新增）

- 用途：向画布上**已有**卡片发送会话消息，不创建任务（`adapter.rs:342`）。
- 输入：`{"operation_id", "card_id", "card_revision", "message"(1..=8000)}`；映射 `ReplyAgent{agent, message, node_id, expected_revision}`。
- 输出：`{"operation_id", "card_id", "reply_id", "reply_status", "replayed"}`（取自 `AdapterResult.reply_id / reply_status`，`adapter_pump.rs:286–288`）。
- 错误：`invalid_argument`、`card_not_found`、`target_changed`、`card_not_loaded`（新，暂定：卡片未在画布加载，不自动加载）、`workspace_closed`、`operation_conflict`、`agent_rejected`、`rate_limited`。
- 确认：同 `dispatch_task`，用户说「跟 Alan 说…」即可直接发，名称有歧义时先问。
- 不提供 `cancel_agent_reply`（撤回）【建议，R-c】。

### 2.6 `create_card`（P4-6，待 Y-1）

- 映射 `AdapterCommand::CreateAgent { profile, task? }`（`adapter.rs:312`，执行 `adapter_pump.rs:108–147`：`CardCommand::CreateAgent{name: None, activate: true}`，有 task 时创建后立即入队）。原 P4-5 复用手机 `creation.rs` 的设计作废，仅作参考（其账本恢复与回滚思路）。
- 输入：`{"operation_id", "workspace_id", "area_id", "profile": "claude|codex", "task"?}`。无 `role_name`（现路径 `name: None` 自动取名）、无 `working_dir`（卡片在区域根运行）、无 `start`（`activate: true` 固定）。
- 输出：`{"operation_id", "card_id", "card_revision", "role_name", "profile", "workspace_id", "area_id", "created_at"(unix 秒), "state": "loaded|outcome_unknown", "created_by": "grokbot", "task_id"?: string, "replayed"}`。带 task 时一次操作完成建卡 + 派工，Grok Bot 不再调用 `dispatch_task`。
- 幂等：同 2.2（含 AC-72：吊销后不得重放旧建卡回执）。
- 错误：`invalid_argument`、`workspace_not_found`、`workspace_closed`、`area_not_found`（新）、`profile_unavailable`（新）、`card_limit_reached`（新）、`daily_create_limit_reached`（新，含 `retry_after_s`）、`card_create_failed`（新：`AgentCreated` 回执缺 `node_id` 或意外回执，`adapter_pump.rs:141–146`）、`operation_conflict`、`agent_rejected`、`rate_limited`。
- 目录：`directory.cards[].created_by = "grokbot" | null`（由连接器命名空间回执关联）。审计一条。
- `CreateAgent` 在哪个区域建卡由 `app` 决定，现路径无 `area_id` 参数【假设：需让连接器选定目标 app / 区域，否则 Y-3】。

### 2.7 错误码汇总（新增均需进 #105 通用表，暂定）

已加入 #105：`card_not_loaded`、`agent_rejected`、`not_connector_task`。本文另提（P4-6，待 Y-1 后加入 #105）：`area_not_found`、`profile_unavailable`、`card_limit_reached`、`daily_create_limit_reached`、`card_create_failed`、`workspace_closed`（P4-3 起使用，暂定）。原 P4-5 提出的 `role_name_taken`、`role_name_invalid`、`project_changed`、`working_dir_unsupported`、`working_dir_outside_project`、`working_dir_not_found`、`card_start_failed` 随设计作废，**撤回**。

### 2.8 明确范围外

`send_terminal_input`、`delete_canvas_card`、`focus_card`、记忆写入（`Memory` / `WorkspaceMemory` 写操作），以及 `create_terminal`、`create_browser`、`create_preview`、`cancel_agent_reply`、`find_agent_tasks`（只读用 P2 的 `list_tasks`）。连接器的 AdapterCommand 映射只构造四种命令，其他变体从 `Connector` 来源到达时一律拒绝（AC-92）。

## 3. 审批与取消

- P4 不提供 `resolve_approval`；派出任务中的工具审批在 P5 经 `approval.waiting` 通知，Grok Bot **每次**向 yakami 展示并获本次明确批准 / 拒绝后才 commit（主规格 4.6）。
- 等待审批时 `cancel_task` 照常取消，不代为决定（V-1 实测）。
- 建卡（P4-6）建议每次先问 yakami（Y-2）。

## 4. 时间字段（补充条款，沿用）

- MCP 工具输出一律整数 unix 秒（UTC）；webhook `occurred_at` 亦为 unix 秒（主规格 5.2，暂定，待 yakami 确认，P5 实现）；未知为 `null`，不伪造。
- `directory.cards[].created_at`：`agent_definitions.created_at_unix`，无定义记录 → `null`（暂定，待 yakami 确认）；不改 `CardIdentity.created_at` 计算。
- 回执：`accepted_at`（dispatch）、`requested_at`（cancel）、`created_at`（create_card）为预留时刻，重放不变。
- `get_task_progress`：`created_at`、`updated_at`、`state_changed_at|null`、`events[{type, occurred_at, state?}]`（≤20，新→旧，来源为连接器回执与任务状态迁移）；`last_activity_at` P4 仍取 `updated_at_unix`，真实来源移到 P5（暂定，待 yakami 确认）。

## 5. 验收

### 5.1 主规格 P4 AC（AC-17–AC-26）

全部保留：AC-17、AC-18、AC-19、AC-20、AC-21、AC-22、AC-23、AC-24、AC-25、AC-26。其中 AC-22 依赖 V-5（AdapterError 可区分），AC-24 的 `outcome_unknown` 由 `MobileCommandStore` 提供。

### 5.2 本文 AC（AC-56 起）

- AC-56 `dispatch_task` / `cancel_task` / `reply_agent` 出现在 `tools/list`，注解非只读；P2 三个只读工具不变。（改：增 `reply_agent`）
- AC-57 同一 `operation_id` 跨工具使用 → `operation_conflict`（`request_hash` 含工具名）。（保留）
- AC-58 缺省字段规范化：显式 `allow_queue=true` 与省略视为同一请求。（保留）
- AC-59 参数校验 / `card_id` 验签失败不占用 `operation_id`；执行期拒绝（`target_changed`、`approval_pending`、`card_busy`、`input_occupied`）记为 `rejected`，同键重放返回同一拒绝，需新 `operation_id` 重试。（改）
- AC-60 `cancel_task` 对已终态任务 → `already_terminal` + `terminal_state`；重放相同。（保留）
- AC-61 等待审批时取消 → 任务取消、审批未被决定、随后 `request_closed`。（保留）
- AC-62 `directory.cards[].created_at` 为 unix 秒或 `null`，不以区域时间冒充。（保留）
- AC-63 `get_task_progress` 时间字段与 `events` 规则。（保留）
- AC-64 回执时间（`accepted_at` / `requested_at` / `created_at`）重放不变。（改：增 create_card）
- AC-65 App 未运行时写工具 → Grok Bot 显示「Norma 未运行」。（保留）
- AC-66 每次写工具调用在 `connector_audit` 记一条（正文仅哈希 + 前 80 字）。（改：覆盖全部写工具）
- AC-67 目标工作空间未打开 → `workspace_closed`，不打开工作空间、不启动 PTY、不占键。（保留）
- AC-68 默认只能取消 Grok Bot 派出的任务，其他 → `not_connector_task`；设置放开后可取消。（保留）
- AC-69 轮换宽限期内新 Token 重放旧 Token 的 `operation_id` → 原回执（同 `client_epoch`）。（保留，映射到 `device_id` 键）
- AC-70 连接器命名空间的 `MobileCommandStore` 行 30 天后清理；清理前重放幂等。（改：不再指 `connector_operations`）
- AC-71 `card_revision` 只随持久状态变化。（保留）
- AC-72 吊销 + 重新生成后旧 `operation_id` 不返回旧回执（新 `client_epoch`）。（保留）
- AC-73 `create_card` 仅在 Y-1 获批后出现在 `tools/list`。（保留）
- AC-74 `create_card` 成功 → `card_id / card_revision / created_at / state=loaded / created_by=grokbot`，`directory` 中可见并可立即派工。（改：无 start 参数）
- AC-75 （已删除，编号不复用：`start=false` 不再存在）
- AC-76 `create_card` 同键同参重放 → 原回执、不新增卡片；同键异参 → `operation_conflict`。（保留）
- AC-77 吊销后用旧 `operation_id` 调用 `create_card` 不返回旧回执。（保留）
- AC-78 （已删除，编号不复用：无 `role_name` 输入）
- AC-79 `profile` 不在开放列表 → `profile_unavailable`。（改：去掉 working_dir）
- AC-80 `workspace_closed` / `area_not_found`，均无副作用。（保留）
- AC-81 达每工作空间 / 每日上限 → `card_limit_reached` / `daily_create_limit_reached`。（保留）
- AC-82 `CreateAgent` 意外回执或无 `node_id` → `card_create_failed`；中途杀 App → `outcome_unknown`，无重复卡片、无自动重放。（改：不再承诺 creation.rs 式回滚）
- AC-83 每次 `create_card` 审计一条。（保留）
- AC-84 （Grok Bot 侧）未获 yakami 本次同意不调用 `create_card`。（保留，依赖 Y-2）

### 5.3 新增（AC-85 起）

- AC-85 四个写工具都经 `execute_norma_adapter_command_with_source(…, AgentInputSource::Connector)` 执行；审计记录来源 `connector`。
- AC-86 `AssignAgentTask` / `ReplyAgent` 带 `node_id` 时只作用于该节点，即使同名卡片存在；不带 `node_id` 时语音、手机、Agent 工具的按名行为回归不变。
- AC-87 `expected_revision` 不符 → `target_changed`，卡片零输入；校验发生在 AdapterCommand 层。
- AC-88 `Connector` 来源下 `dispatch_agent_task` 的 force_untracked 参数恒为 false；无可信跟踪卡片 → `untracked_requires_confirmation`。
- AC-89 `reply_agent` 把消息投到 `card_id` 对应卡片会话，不创建任务（`list_tasks` 无新增）；同键重放不重复发送；卡片未加载 → `card_not_loaded`，不自动加载。
- AC-90 未识别的 `AdapterError::Rejected` → `agent_rejected`，`message` 不含内部原文；原文仅在审计中。
- AC-91 连接器行（`device_id` 以 `connector:` 开头）不出现在任何手机投递 / 计划 / 回执查询中；手机 `MobileDeviceGrant` 与配对不受影响。
- AC-92 `tools/list` 中无 `send_terminal_input`、`delete_canvas_card`、`focus_card`、记忆写入及 2.8 所列工具；任何输入都无法从 `Connector` 来源构造这些 AdapterCommand。
- AC-93 `create_card` 带 `task` → 一次操作建卡并入队，回执含 `task_id`；重放不重复建卡或派工。
- AC-94 `create_card` 只在 `area_id` 指定的区域建卡；无法定位到该区域的 app → `workspace_closed` 或 `area_not_found`，不在当前聚焦区域建卡。

## 6. 工单

模板：任务｜仓库｜Done when｜不要做。均在 `multisoul-ai/Norma-AI-OS`，叠在 P2（#104）之上，P2 合入前不开工。

### P4-1 AdapterCommand 层扩展

- **任务**：新增 `AgentInputSource::Connector`；`AssignAgentTask` / `ReplyAgent` 增加可选 `node_id` 与 `expected_revision`，在 AdapterCommand 层按节点寻址与校验 revision；连接器来源的 force_untracked 恒 false、`window=None`；`AdapterError` → 结构化错误码映射表；`card_id` → 目标 app / 节点解析。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/agent/src/{adapter.rs,adapter/commands.rs,model.rs}`、`crates/app/src/app/agent_runtime/adapter_pump.rs`、`adapter_task`、`reply_adapter`、`crates/norma/src/connector`）
- **Done when**：AC-17、AC-20、AC-21、AC-23、AC-85、AC-86、AC-87、AC-88、AC-90 通过；V-5 结论记录在 PR。
- **不要做**：不另建按节点派工入口；不改 `node_id=None` 时的按名行为；不给 `Connector` 开放 2.8 的命令；不调用 `realtime_command_authorized` 之外再造意图判定；不改 Unix socket 协议。

### P4-2 幂等与回执复用 `MobileCommandStore`

- **任务**：连接器写工具接入 `MobileCommandStore` reserve / finish / receipt（或抽共享 `command_ledger` 模块）；`device_id = connector:<client_epoch>` 键映射与 epoch 维护（轮换同 epoch、吊销换 epoch）；`outcome_unknown` 只读核对；30 天清理；手机侧读者排除 `connector:` 行。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/storage/src/mobile_command_store*`、读 `mobile_commands` 的各 store、`crates/norma/src/connector`）
- **Done when**：AC-18、AC-19、AC-24、AC-57、AC-58、AC-59、AC-64、AC-69、AC-70、AC-72、AC-91 通过。
- **不要做**：不建 `connector_operations`；不让连接器行经过 `MobileDeviceGrant`；不自动重放；不改 Token 明文存储与吊销语义。

### P4-3 `dispatch_task` + `cancel_task`

- **任务**：按 2.3 / 2.4 的薄映射与输入输出；`not_connector_task`、`workspace_closed`、`already_terminal`；审计。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector/tools*`）
- **Done when**：AC-22、AC-25、AC-26、AC-56、AC-60、AC-61、AC-65、AC-66、AC-67、AC-68 通过；V-1、V-4 结论记录在 PR。
- **不要做**：不替用户作审批决定；不取消同卡其他任务；不自动打开工作空间；不暴露 `node_id` 或 Agent 名寻址。

### P4-4 `reply_agent`

- **任务**：按 2.5 映射 `ReplyAgent`；`card_not_loaded`；审计。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector/tools*`、`reply_adapter` 调用侧）
- **Done when**：AC-56（reply 部分）、AC-66、AC-89 通过。
- **不要做**：不创建任务；不自动加载卡片；不提供 `cancel_agent_reply`；不用于 Terminal 卡片。

### P4-5 时间字段

- **任务**：§4 的字段；必要时新增状态迁移时间记录（只增不改）；同步主规格。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector/tools`、`crates/storage`、`docs/product-specs`）
- **Done when**：AC-62、AC-63、AC-64 通过。
- **不要做**：不改 `card_id` 编码中的 `created_at`；不伪造未知时间；不在 P4 实现 webhook 与真实 `last_activity_at`（P5）。

### P4-6 `create_card`（Y-1 获批前不开工）

- **任务**：按 2.6 映射 `CreateAgent{profile, task?}`；区域定位；限额；`created_by`；新错误码；在 #105 修订 1.4 / 6.6 与错误码表。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector`、`adapter_pump.rs` CreateAgent 分支、`docs/product-specs`）
- **Done when**：AC-73、AC-74、AC-76、AC-77、AC-79、AC-80、AC-81、AC-82、AC-83、AC-84、AC-93、AC-94 通过；Y-1、Y-2、Y-3、Y-5、Y-6 已有决定。
- **不要做**：不复用手机 `creation.rs` 流程或其账本；不做删除 / 改名卡片、不建工作空间 / 区域；不做 `load_card`；不开放 claude / codex 以外 profile（Y-6 前）。

### P4-7 测试与端到端

- **任务**：AdapterCommand 层单测（按节点 / 按名回归）、账本与隔离测试、工具集成测试（P2 测试风格）、经 Funnel 的 Grok Bot 走查（含「Norma 未运行」）。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/*/tests`，E2E 记录写在 PR）
- **Done when**：AC-17–AC-26、AC-56–AC-74、AC-76、AC-77、AC-79–AC-94（P4-6 部分仅在 Y-1 获批后）有自动化或记录在案的手工结果；夹具 Token 随机生成。
- **不要做**：不把真实 Token 写进夹具 / 日志；不在用户真实工作空间做破坏性测试；不合并 PR。

## 7. 暂定决定（均「暂定，待 yakami 确认」）

- `dispatch_task` 不加 `workspace_id`。
- 目标工作空间未打开 → `workspace_closed`。
- `cancel_task` 只取消 Grok Bot 派出的任务，直接取消，可设置放开。
- 轮换宽限期内新旧 Token 同 `client_id`（同 `client_epoch`）；吊销后新 epoch（2026-10-14 已决定）。
- 连接器回执保存 30 天。
- webhook `occurred_at` 改 unix 秒。
- 无 `agent_definitions` 记录卡片 `created_at = null`。
- 真实 `last_activity_at` 来源移到 P5。
- `reply_agent` 不需额外确认，歧义时先问；不提供撤回。

## 8. 开发期验证项

- V-1（P4-3）等待审批时取消，Codex / Claude 是否关闭挂起审批、是否发出任何 allow / deny。
- V-2（P4-6）`CreateAgent` 后 `session_id` 写入时机，回执 `card_revision` 与之后 `directory` 是否一致。
- V-3（P4-2）列出所有读 `mobile_commands` / `mobile_realtime_calls` 的查询，确认排除 `connector:` 行的改动面；过大则改走共享模块 + 独立表。
- V-4（P4-3）协调器 `task_id: u64`（`adapter.rs:52`）与 P2 `list_tasks.task_id`（`agent_dispatch_tasks`）是否同源。
- V-5（P4-1）`AdapterError` 能否区分 `card_busy` / `approval_pending` / `input_occupied` / `AlreadySubmitted`。

## 9. 开放问题

### 9.1 需要 yakami 决定（附建议）

- Y-1 是否开放建卡（经 `create_agent` 映射），并修订主规格 1.4 / 6.6？建议：开放，仅新建；删除 / 改名仍禁止。
- Y-2 每次 `create_card` 前 Grok Bot 是否都要先问 yakami？建议：每次都问，服务端不加 nonce。
- Y-3 `create_agent` 现路径不带区域参数（`name: None`、`activate: true`）：是否要求连接器必须指定 `area_id` 并只在该区域建卡？建议：必须指定，定位不到即报错，不落到当前聚焦区域。
- Y-4 是否需要独立 `load_card`（加载已有未加载卡片）？建议：不做；`reply_agent` 遇未加载返回 `card_not_loaded`。
- Y-5 建卡限额？建议：每工作空间 Grok Bot 所建卡片 ≤ 5，每日 ≤ 10（Asia/Shanghai 自然日）。
- Y-6 开放哪些 profile？`create_agent` 支持 codex / claude / opencode / cursor / grok（`adapter.rs:316`）；建议首版只开 claude / codex。

### 9.2 其他（附建议，不需 yakami）

- R-a 连接器来源是否需要类似 `realtime_command_authorized` 的意图判定？建议：不需要，Grok Bot 侧确认 + 结构化参数已足够。
- R-b `MobileCommandStore` 直接复用还是抽共享模块？建议：先按 V-3 评估，倾向抽共享模块以避免污染手机读者。
- R-c 是否开放 `cancel_agent_reply`？建议：首版不开放。
