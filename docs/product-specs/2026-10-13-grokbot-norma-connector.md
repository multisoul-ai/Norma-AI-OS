# Grok Bot 作为 Norma 调度员：本机 MCP 连接器、派工与通知

状态：实现规格（P1），按 2026-10-13 yakami 拍板写入，并按 2026-10-14 yakami 决定修订（Token 存储、隧道、锁定），待工程拆分 · 2026-10-14。代码事实基于 origin/dev `83590301`。本文只定义规格，不改代码；后续 P2–P5 每期一个 PR。

## 0. 已拍板的决定

以下决定由 yakami 确认，原样执行，本文不再重新取舍。

1. **接入方式**：Norma 在 Mac 上的 Norma App 进程内运行一个本机 MCP 服务器（传输为 MCP Streamable HTTP），只绑定 `127.0.0.1`；通过 **Tailscale Funnel** 暴露给 Grok Bot（2026-10-14 决定：优先 Funnel，不用 Cloudflare Tunnel）。Funnel 提供固定地址 `https://<machine>.<tailnet>.ts.net`，机器名与 tailnet 不变则地址不变。隧道之上只用 **Bearer Token** 鉴权。Token 永不进仓库；生成、存储（**明文存于 Norma 的 SQLite**，不用 macOS 钥匙串，设置页随时可查看）、轮换、吊销见第 6 节。
2. **由 Grok Bot 决定派给谁**：Norma 本身不做路由、不按能力选人。提供六个工具：`directory`、`dispatch_task`、`list_tasks`、`get_task_progress`、`cancel_task`、`resolve_approval`。`resolve_approval`（批准 / 拒绝）**每次都必须由 Grok Bot 先问用户确认**——这是客户端要求；同时服务端 API 也强制要求确认标志 + 一次性 nonce（第 4.6 节）。
3. **通知四类事件**：任务完成、任务失败、等待工具审批、30 分钟无进展。Norma 向用户配置的 **Grok Bot routine webhook URL** 发 POST，唤醒 Grok Bot 给用户发消息。负载 JSON、HMAC 签名、退避重试、去重键、离线行为见第 5 节。
4. **不依赖草稿 PR #97**（分支 `codex/workspace-agent-dispatch`），只借鉴其设计思想（第 2 节）；`card_id` 由连接器自行定义，不切换到 #97 的身份（Q3）。
5. **鉴权失败锁定（2026-10-14）**：按**来源 IP** 计数鉴权失败，只锁该 IP 的失败尝试；**携带正确 Token 的请求永不被锁**。来源 IP 的确定方式见 6.4。Token 熵 ≥ 128 bit。不设全局失败告警。
6. **分期交付，一期一个 PR**：P1 本规格；P2 本机 MCP 服务器 + Token 鉴权 + 只读工具；P3 Tailscale Funnel + 在 Grok Bot 添加自定义连接器，只读端到端（Tailscale 登录与开启 Funnel 由用户完成）；P4 `dispatch_task` + `cancel_task`（幂等与回执）；P5 webhook 通知 + `resolve_approval`。

## 1. 背景与目标

### 1.1 现状（代码事实）

- **已有的本机编排协议（Unix socket，非 HTTP）**：`crates/app/src/norma/endpoint.rs` 在 `norma::client::default_socket_path()`（`crates/norma/src/client.rs:208`，默认 `$TMPDIR` 下的 socket）上监听，接收 `norma::protocol::CardCommand`（`crates/norma/src/protocol.rs:69` 起）：`ListWorkspaces`、`ListAgents`、`AgentTaskStatus { task_ref }`、`AgentTaskResult { task_ref }`、`AgentTaskCancel { task_ref }`、`DispatchAgentTasks { assignments: Vec<AgentTaskAssignment>, force_untracked }` 等。`AgentTaskAssignment` 只有 `name` + `task`（第 133 行），**按名字寻址**。回执 `AgentTaskReceipt`（第 287 行，含 `task_id`、`operation_id`、`target_definition_id`、`target_node_id`）、状态 `AgentTaskStatus`（第 323 行）与枚举 `AgentTaskReceiptStatus`（第 310 行：`Accepted / Submitted / Queued / Untracked / CancelRequested / Done / Failed / Cancelled / Interrupted`）。分派与取消由 `crates/app/src/norma/endpoint/task_dispatch.rs`、`task_control.rs`、`routing.rs` 执行。工作空间摘要 `WorkspaceSummary`（`crates/norma/src/protocol/workspace.rs:5`：`workspace_id / display_name / active / route_state / agent_count`）。
- **任务持久化**：`agent_dispatch_tasks`（`crates/storage/src/migrations.rs:257`，含 `task_ref / root_task_ref / definition_id / node_id / task_text / state / tracking / result_text`）；跨工作空间路由 `orchestration_task_routes` 与操作幂等表 `orchestration_task_operations(operation_id PK, request_fingerprint, result_json)`（第 75、112 行）。
- **手机命令账本 / 回执**：`mobile_commands`（`crates/storage/src/mobile_command_store.rs:246`，主键 `(device_id, operation_id)`，`request_hash`，`state ∈ accepted/succeeded/rejected/outcome_unknown`）。`MobileCommandStore::reserve` 返回 `MobileCommandReservation::{New, Existing(receipt), Conflict}`（第 46、63 行），注释明确「No recovery path replays a reserved side effect」。手机端命令外壳 `MobileCommandRequest { operation_id, workspace_id, route_id, grant_revision, command }`（`crates/app/src/mobile/command.rs:16`），`CommandLease`（第 124 行）防并发执行。交付快照 `MobileTaskDeliveryStore`（`crates/storage/src/mobile_task_delivery_store.rs`）。
- **审批表示**：`norma::agent_hook::ApprovalDecision { Allow, Deny }`（`crates/norma/src/agent_hook/approval.rs:21`）；`norma::mobile::ApprovalTarget { node_id, profile, run_id, session_id, turn_id, request_id }`（`crates/norma/src/mobile/approval.rs:15`），`valid()` 只接受 `Codex | Claude`，`delivery_state()` 返回 `waiting / submitted / request_closed` 或错误 `approval_changed / decision_conflict / approval_unavailable`；`ApprovalSnapshot`（第 95 行，含 `card_title / tool / details / decisions`）。手机侧审批调用 `crates/app/src/mobile/activity.rs` 的 `Operation::Decide { target, operation_id, decision }`（第 65 行起）与 `DecisionCall`（第 91 行），回执经 `Operation::Receipt` 查询；Mac 侧执行在 `crates/app/src/norma/endpoint/mobile_approval.rs`。
- **活动汇总 / 需要关注**：`WorkspaceActivitySummary::from_facts`（`crates/norma/src/mobile/workspace_activity.rs:106`）产出 `projects`、`agents`（`WorkspaceActivityAgent { node_id, name, state, action, progress: Option<TaskProgress> }`）与 `attention`（`kind ∈ approval / failed / review`，`WorkspaceActivityTarget`）。`activity_event()`（第 448 行）是纯函数生命周期判定，供推送用。推送实现 `crates/app/src/mobile/push/activity.rs`。
- **现有本机 HTTP**：仅 `crates/app/src/settings/agent_hook_ingress.rs:138` 的 Agent Hook 入口（`127.0.0.1:0` 随机端口，手写 `std::net` 处理 `/agent-hooks`）。手机服务器 `crates/app/src/mobile/server.rs` 是 **TLS WebSocket**（`tungstenite`，`norma::mobile::transport`），`allowed_address()`（第 463 行）允许回环 / 私网 / Tailscale。**仓库内没有 HTTP 服务框架（无 axum / hyper server / rmcp）**；依赖里有 `tokio`、`reqwest`、`ureq`、`hmac`、`sha2`（Cargo.lock）。仓库内没有生产代码使用 macOS 钥匙串（`security-framework` 只是传递依赖）。

### 1.2 目标

用户在任何地方对 Grok Bot 说「让 Alan 修登录 bug」「现在谁在忙」「Alan 那个任务怎么样了」，Grok Bot 能看到 Norma 的工作空间与 Agent 卡片、把任务派给**指定**卡片、查询进度、取消任务、在用户确认后处理工具审批；任务完成 / 失败 / 等待审批 / 卡住 30 分钟时，Norma 主动通知 Grok Bot，由 Grok Bot 告诉用户。

### 1.3 范围内

- Norma App 内新增 MCP 服务器（新）、六个工具、Token 管理、Tailscale Funnel 接入说明、webhook 通知发送器（新）、连接器审计日志（新）、设置页「Grok Bot 连接器」（新）。

### 1.4 范围外

- Norma 内的智能路由 / 按能力选人；新建 Agent 卡片；Agent 提问（非审批）的回答；终端原始输入；文件读写；跨 Mac 汇总（首版只连一台 Mac）。
- 不改手机配对与授权模型（`mobile_commands`、`MobileDeviceGrant`），不改 Unix socket 协议的现有语义。
- 不合入、不依赖 #97 的代码。

## 2. 从 PR #97 借鉴的设计（只借思想，不依赖代码）

来源：`origin/codex/workspace-agent-dispatch` 上的 `docs/product-specs/2026-10-10-workspace-agent-task-dispatch.md` 与 `docs/exec-plans/2026-10-10-workspace-agent-task-dispatch.md`（T01–T09，T06–T09 仍未完成）。

| 借鉴点 | #97 中的含义 | 本规格如何采用 |
| --- | --- | --- |
| 持久卡片身份 | 目标快照含工作空间、区域、持久卡片身份、节点、Runtime 路由代次、Provider 会话指纹；删除后重建是新身份，旧请求拒绝 | `directory` 返回 `card_id`（不透明字符串，见 3.2），`dispatch_task` 只接受 `card_id`，并带 `card_revision`（本规格只取持久状态计算，不含 Runtime 路由代次，见 3.2）；身份变化 → `target_changed`，**不自动换卡** |
| 唯一角色名称 | 空间内主名称 + 中文语音别名共同查重，大小写不敏感；改名后旧称失效 | `directory` 只展示名称供 Grok Bot 与用户对话；`role_name_unique` 标记名称在该工作空间内是否唯一。Grok Bot 用名称跟用户沟通，**但派工必须用 `card_id`** |
| 精确目标解析 | 不用相似名称、焦点或「唯一剩余卡片」替代明确称呼；补全前零接收零投递 | 服务端不做模糊匹配；名称解析由 Grok Bot 在 `directory` 结果中完成，多候选或零候选时由 Grok Bot 问用户 |
| 带操作账本的幂等投递 | 重发同操作返回原回执、同键不同参数冲突、正文/Enter 写入崩溃标「待核对」不自动重放；提交 ≠ 运行 ≠ 完成 | 新表 `connector_operations`（第 7 节），语义与 `MobileCommandStore::reserve`、`orchestration_task_operations` 一致；状态严格区分 `accepted / submitted / running / done / failed / cancelled / outcome_unknown` |

不借鉴：#97 的 Dock UI、名称修复迁移、`WorkspaceAgentRoleStore` 表结构、工作空间进度汇报 typed tool。`card_id` 由连接器自行定义并长期保持（Q3 已决定），即使 #97 日后合入也不切换到其身份。

## 3. 架构

```text
Grok Bot (云端)
  │ HTTPS  Authorization: Bearer <token>
  ▼
https://<machine>.<tailnet>.ts.net/mcp   ← Tailscale Funnel（tailscaled 本机反向代理，用户登录）
  │  （tailscaled 覆盖写入 X-Forwarded-For = 公网客户端 IP）
  ▼  http://127.0.0.1:<port>/mcp
Norma App 进程
  ├─ ConnectorServer（新）: Streamable HTTP + MCP JSON-RPC, 鉴权, 限流, 审计
  ├─ ConnectorTools（新）: 六个工具 → 复用 norma::protocol::CardCommand 执行路径 / WorkspaceActivitySummary
  ├─ connector_operations 账本（新, app.sqlite）
  └─ ConnectorNotifier（新）: 事件 → webhook POST(HMAC) → Grok Bot routine
```

### 3.1 MCP 服务器（新）

- 进程内 tokio 任务，**只绑定 `127.0.0.1`**（不复用 `mobile::server::allowed_address`，它放行私网）。端口默认 `47615`（固定端口便于隧道配置，冲突时设置页报错，不随机），可在设置中改；修改端口时设置页提示「需同步修改 Tailscale Funnel 配置的目标端口」，并给出命令 `tailscale funnel --bg --https=443 http://127.0.0.1:<新端口>`（Q7）。
- 无后台常驻进程、不注册登录项（Q10）：App 未运行时连接器不可达，Grok Bot 向用户显示「Norma 未运行」；App 重新打开后 outbox 中积压的事件再补发（5.4）。
- 路径 `POST /mcp`、`GET /mcp`（SSE 流，可选）按 MCP Streamable HTTP 规范；支持 `initialize`、`tools/list`、`tools/call`；`Mcp-Session-Id` 会话头。其他路径 404。
- 实现库：`axum`（HTTP）+ `rmcp`（MCP Streamable HTTP / JSON-RPC），P2 新增这两个依赖（Q1）。要求请求体上限 64 KiB、读超时 10 s、并发连接上限 8。
- 默认关闭；设置页「Grok Bot 连接器」开关打开后才监听。

### 3.2 `card_id`（新，不透明）

`card_id` 由连接器自行定义（Q3），不依赖 #97。格式 `c1.<base64url(desktop_workspace_id | area_id | canvas_id | node_id | definition_id | 创建时间戳)>.<8 字节 HMAC>`。因为同一目录下可有多个画布、`node_id` 只在画布内唯一，编码中必须包含画布标识 `canvas_id`，保证同目录多画布时 `card_id` 仍唯一。HMAC 用本机密钥（存 Norma SQLite，同 6.3）防伪造。`card_revision` = `sha256(card 身份字段 | 项目根目录 | 工作目录 | profile | session_id)` 的截断哈希，**只由持久状态计算**，不含 Runtime 路由代次或任何运行期状态（App 重启、PTY 重连不改变 revision）；无 `session_id` 时以空串参与计算。解析时逐项复核；任一不符 → `target_changed`。这是 P2 的新增投影，不改现有表；若现有存储里没有稳定的画布标识，P2 需新增（见 R2）。

## 4. 工具定义

通用约定：
- 所有工具返回 MCP `CallToolResult`，`structuredContent` 为下列输出 JSON，同时 `content[0].text` 给一句中文摘要供模型阅读。
- 错误：`isError: true`，`structuredContent = {"error": {"code": "...", "message": "...", "retryable": bool}}`。
- 通用错误码：`unauthorized`（401，HTTP 层）、`rate_limited`（含 `retry_after_s`）、`connector_disabled`、`invalid_argument`、`workspace_not_found`、`card_not_found`、`target_changed`、`internal`。
- 文本字段一律有界：任务正文 ≤ 8000 字符；摘要 ≤ 500 字符；列表分页 `limit ≤ 50`。

### 4.1 `directory`（只读，P2）

- 输入：`{"workspace_id"?: string, "include_idle"?: bool = true}`
- 输出：
```json
{"observed_at": 1760000000, "workspaces": [{
  "workspace_id": "…", "name": "Norma", "active": true,
  "projects": [{"project_id": "…", "area_id": "…", "name": "Web"}],
  "cards": [{"card_id": "c1.…", "card_revision": "…", "role_name": "Alan",
    "role_name_unique": true, "project_id": "…", "profile": "Codex",
    "state": "busy|idle|waiting_approval|unloaded|unknown",
    "current_task_id": "…"|null, "loaded": true}]}]}
```
- 错误：`workspace_not_found`。
- 幂等：只读，无副作用；不启动未加载区域的 PTY。
- 基于：`CardCommand::ListWorkspaces` / `ListAgents`（`crates/norma/src/protocol.rs`）、`WorkspaceSummary`、`WorkspaceActivitySummary.projects[].agents[]` 的 `state`（`crates/norma/src/mobile/workspace_activity.rs`）、手机侧取数 `crates/app/src/mobile/activity.rs`（`capture` 快照）。`busy/idle` 映射规则：存在未终态任务或 `WorkspaceActivityStatus::Running` → `busy`；有待审批 → `waiting_approval`；未加载 → `unloaded`（取持久快照，不推断）。`role_name_unique` 现阶段按 `agent_definitions(name COLLATE NOCASE)` 唯一索引与卡片名在工作空间内计算（新）。

### 4.2 `dispatch_task`（写，P4）

- 输入：
```json
{"operation_id": "uuid-v4", "card_id": "c1.…", "card_revision": "…",
 "task": "修复登录页 500", "context"?: "…", "allow_queue"?: true}
```
- 输出（回执）：
```json
{"operation_id": "…", "task_id": "…", "card_id": "c1.…", "role_name": "Alan",
 "state": "accepted|submitted|queued|untracked|outcome_unknown",
 "tracking": "tracked|untracked", "replayed": false, "accepted_at": 1760000000}
```
- 错误：`card_not_found`、`target_changed`、`card_busy`（`allow_queue=false` 且忙）、`approval_pending`（卡片正等审批，不越过）、`input_occupied`（用户正在该卡片输入）、`untracked_requires_confirmation`（对应现有 `force_untracked`，连接器**永不**传 `force_untracked=true`）、`operation_conflict`（同 `operation_id` 不同参数）、`task_too_long`。
- 幂等：`operation_id` 由 Grok Bot 生成；`(client_id, operation_id)` 在 `connector_operations` 中预留；`request_hash = sha256(card_id|card_revision|task|context|allow_queue)`。重放 → 返回原回执且 `replayed=true`；哈希不同 → `operation_conflict`。预留后崩溃 → `outcome_unknown`，**不自动重放**，Grok Bot 只能用 `get_task_progress` 核对。
- 基于：`CardCommand::DispatchAgentTasks` 执行路径（`crates/app/src/norma/endpoint/task_dispatch.rs`）、回执 `AgentTaskReceipt`、`orchestration_task_operations`。**需新增**：按 `node_id`（而不是 `AgentTaskAssignment.name`）派工的内部入口，因为现有 assignment 按名字寻址（风险 R1）。

### 4.3 `list_tasks`（只读，P2）

- 输入：`{"workspace_id"?: string, "card_id"?: string, "states"?: ["running", ...], "since"?: unix, "cursor"?: string, "limit"?: 20}`
- 输出：`{"tasks": [{"task_id", "card_id", "role_name", "workspace_id", "state", "tracking", "summary", "created_at", "updated_at", "source": "grokbot|desktop|mobile|other"}], "next_cursor": string|null}`
- 默认：全部未终态 + 最近 24 小时终态。
- 错误：`invalid_argument`（游标非法）。幂等：只读。
- 基于：`agent_dispatch_tasks`（`crates/storage/src/migrations.rs:257`）、`AgentTaskReceiptStatus`；`source=grokbot` 由 `connector_operations` 关联得出（新）。

### 4.4 `get_task_progress`（只读，P2）

- 输入：`{"task_id": string}` 或 `{"operation_id": string}`
- 输出：`{"task_id", "state", "tracking", "progress": {"completed", "total", "current_step"}|null, "last_activity_at", "pending_approval": {"approval_id", "tool", "summary"}|null, "result_summary": string|null, "error": string|null}`
- `progress` 仅在有可信计划分母时返回（复用 `norma::mobile::TaskProgress`，`crates/norma/src/mobile/snapshot.rs:75`）；不编造百分比。
- 错误：`task_not_found`。幂等：只读。
- 基于：`CardCommand::AgentTaskStatus` / `AgentTaskResult`、`AgentTaskStatus.result`、`WorkspaceActivityAttention`（审批）。

### 4.5 `cancel_task`（写，P4）

- 输入：`{"operation_id": "uuid", "task_id": string}`
- 输出：`{"operation_id", "task_id", "state": "cancel_requested|cancelled|already_terminal", "replayed": bool}`
- 错误：`task_not_found`、`operation_conflict`。只取消该任务，不影响同卡其他任务。
- 幂等：同 4.2；对已终态任务返回 `already_terminal`（非错误）。
- 基于：`CardCommand::AgentTaskCancel { task_ref }`（`crates/app/src/norma/endpoint/task_control.rs`），状态 `CancelRequested → Cancelled`。

### 4.6 `resolve_approval`（写，P5，两步确认）

客户端要求：**Grok Bot 每次都必须先把工具名、参数摘要、所属卡片展示给用户，并得到用户本次明确的「批准」或「拒绝」后才能调用第二步**。不得基于先前授权、规则或推断自动批准。

服务端强制两步：
1. `{"step": "prepare", "approval_id": string}` → 返回 `{"approval_id", "card_id", "role_name", "tool", "details_summary", "decisions": ["allow","deny"], "confirmation_nonce": "…", "nonce_expires_at"}`。nonce 32 字节随机、单次有效、5 分钟过期、绑定 `approval_id`。
2. `{"step": "commit", "operation_id", "approval_id", "decision": "allow|deny", "user_confirmed": true, "confirmation_nonce": "…", "user_quote"?: "用户原话"}` → 返回 `{"operation_id", "state": "waiting|submitted|request_closed", "replayed"}`。
- `approval_id`：对 `ApprovalTarget { node_id, profile, run_id, session_id, turn_id, request_id }` 的不透明编码 + HMAC（新）。
- 错误：`confirmation_required`（`user_confirmed≠true` 或缺 nonce）、`nonce_invalid`、`nonce_expired`、`approval_changed`、`decision_conflict`、`approval_unavailable`、`request_closed`（后四个直接沿用 `ApprovalTarget::delivery_state` 的返回）、`operation_conflict`。
- 幂等：同 `operation_id` 重放返回原回执；nonce 在 commit 成功预留时作废。
- 只提供 Provider 给出的单次决定（`ApprovalSnapshot.decisions`），**永不**提供会话级放行或「总是允许」。
- 基于：`norma::mobile::ApprovalTarget`、`ApprovalDecision`、手机 `Operation::Decide` 与 `crates/app/src/norma/endpoint/mobile_approval.rs` 的执行与回执路径。

## 5. 通知（P5）

### 5.1 事件

| 事件 `type` | 触发 | 依据 |
| --- | --- | --- |
| `task.completed` | 任务进入 `Done` | `AgentTaskReceiptStatus::Done` |
| `task.failed` | 进入 `Failed` / `Interrupted` | 同上 |
| `approval.waiting` | 出现可决定的审批（`decisions` 非空） | `WorkspaceActivityAttention.kind == "approval"` |
| `task.stalled` | 已跟踪（tracked）的未终态任务连续 30 分钟没有新输出、工具调用或状态变化；untracked 任务不参与 | 新增计时：`last_activity_at`（Q8） |

范围（Q5）：默认只通知 Grok Bot 派出的任务（`source=grokbot`），包括这些任务上的审批；设置项「通知全部任务」打开后，桌面 / 手机 / 直接卡片发起的任务及其审批也通知。

与手机推送（`crates/app/src/mobile/push/activity.rs`）的关系（Q6）：两路各自独立发送，互不去重、互不抑制；`dedupe_key` 只在 webhook 通道内部生效。

`task.stalled` 每个任务每次停滞只发一次，有新活动后重置。

### 5.2 负载

```json
{"version": 1, "event_id": "evt_…", "dedupe_key": "task.completed:<task_id>",
 "type": "task.completed", "occurred_at": 1760333100,
 "machine": {"machine_id": "…", "name": "yakami 的 MacBook"},
 "workspace": {"workspace_id": "…", "name": "Norma"},
 "card": {"card_id": "c1.…", "role_name": "Alan"},
 "task": {"task_id": "…", "operation_id": "…"|null, "state": "done", "summary": "≤500 字"},
 "approval": null,
 "hint": "调用 get_task_progress 获取详情"}
```
`occurred_at` 为整数 unix 秒（UTC），与工具输出一致（暂定，待 yakami 确认，2026-10-14）；`X-Norma-Timestamp` 同为 unix 秒。

`approval.waiting` 时 `approval = {"approval_id", "tool", "details_summary"}`。负载不含终端全文、文件内容、Token。

去重键：`task.completed:<task_id>`、`task.failed:<task_id>:<run_id>`、`approval.waiting:<approval_id>`、`task.stalled:<task_id>:<last_activity_at>`。

### 5.3 签名

请求头：`X-Norma-Timestamp: <unix>`、`X-Norma-Event-Id`、`X-Norma-Signature: v1=<hex(HMAC-SHA256(secret, timestamp + "." + body))>`。secret 与 Bearer Token **分开**，同样存 Norma SQLite（不进仓库、不写日志）。Grok Bot routine **必须**校验 HMAC 签名与时间戳（时间差 > 5 分钟视为失败），校验失败直接丢弃事件、不通知用户（Q4）。

**P5 前置条件**：确认 Grok Bot routine 的 webhook 触发器能读到原始请求体（逐字节）和请求头。若不能，需要一个小型中继先校验签名再转发给 routine；是否引入中继待用户确认（风险 R7）。

### 5.4 投递、重试、离线

- 新表 `connector_outbox`（app.sqlite）：`event_id PK, dedupe_key UNIQUE, payload_json, attempts, next_attempt_at, state ∈ pending/delivered/dead`。事件先落库再发送。
- 2xx 视为成功；4xx（除 408/429）视为永久失败 → `dead`；5xx / 超时 / 网络错误重试：1 min、2、4、8、16、32 min，此后每 60 min，最长 24 h → `dead`。429 遵守 `Retry-After`。
- Mac 睡眠 / 断网 / App 退出：outbox 持久保留，恢复（或 App 重新打开）后按序补发；同一任务已有更新事件（例如 stalled 后 completed）时，过时的 `task.stalled` 直接丢弃不发。审批事件补发前复核仍为 pending，已关闭则丢弃。
- `dead` 事件在设置页可见；不会重复骚扰用户。

## 6. 安全

1. **监听**：只绑 `127.0.0.1`，拒绝非回环 `peer_addr`；`Host` 头必须是配置的 Funnel 地址 `<machine>.<tailnet>.ts.net` 或 `127.0.0.1:<port>`（防 DNS rebinding）；不开 CORS。
2. **隧道（Tailscale Funnel）**：P3 由用户在 Mac 上登录 Tailscale、在 tailnet 策略中允许该机器使用 Funnel，然后执行 `tailscale funnel --bg --https=443 http://127.0.0.1:<port>`（默认 47615）。Funnel 以 HTTPS 反向代理模式（不是 TCP 转发）把 `https://<machine>.<tailnet>.ts.net` 转发到本机端口；该地址在机器名与 tailnet 名不变时固定，作为 Grok Bot 自定义连接器地址（`…/mcp`）。非 `/mcp` 路径由 Norma 返回 404。Tailscale 凭据在系统 Tailscale 客户端内，不进仓库。改端口必须同步改 Funnel 配置（3.1）。Cloudflare 命名隧道 / Cloudflare Access 只作为后续可选方案记录，不在 P2–P5 范围内。
3. **Token**：
   - 生成：设置页「生成连接令牌」，32 字节 CSPRNG（熵 256 bit，要求 ≥ 128 bit），格式 `nrm_live_<base64url>`，供用户粘贴进 Grok Bot 自定义连接器。
   - 存储：**明文存于 Norma 的 SQLite（app.sqlite 新表 `connector_tokens`）**，不使用 macOS 钥匙串；设置页**随时可查看**（显示 / 复制）当前 Token 及宽限期内的旧 Token。比较用常数时间比较。不写日志、不进仓库或 E2E 夹具（测试用随机生成的 Token）。
   - 轮换：生成新 Token 后旧 Token 继续可用 24 小时宽限期，期满失效（Q9）；设置页显示两者状态。
   - 吊销：「立即吊销」立即生效（无宽限），删除 SQLite 中的 Token 记录并断开所有 MCP 会话；关闭连接器开关等同全部吊销。
4. **限流与锁定**：
   - 限流：每 Token 读 60 次/分钟，写 10 次/分钟，`resolve_approval` commit 5 次/分钟。
   - 锁定按**来源 IP**：同一来源 IP 15 分钟窗口内连续 10 次鉴权失败 → 该 IP 的**鉴权失败尝试**被拒 15 分钟（返回 429 `rate_limited`，含 `retry_after_s`），并在设置页提示。**携带正确 Token 的请求永不被锁**：先校验 Token，正确则直接放行，不受任何 IP 的锁定状态影响；其他 IP 的错误 Token 不影响正确 Token。
   - 来源 IP 的确定：Funnel 的 HTTPS 反向代理（tailscaled 内的 Go `httputil.ReverseProxy`）对转发请求用 `Header.Set` **覆盖**写入 `X-Forwarded-For = 公网客户端真实 IP`，并设置 `Tailscale-Funnel-Request: ?1`（先删除客户端自带的同名头），因此经 Funnel 进来的请求，公网客户端无法伪造这两个头。但经 Funnel 与本机进程直连本地端口的请求，TCP 对端地址都是 `127.0.0.1`，只看对端地址无法区分。规则：
     - 仅当该 TCP 连接确认来自本机 tailscaled 的 Funnel 代理时（P2/P3 实现：用 macOS libproc 按连接四元组查对端进程，确认是 Tailscale 系统扩展 / tailscaled 进程，并同时带 `Tailscale-Funnel-Request: ?1`），才采用 `X-Forwarded-For`（取唯一值，多值或格式非法视为不可信）作为来源 IP；
     - 其他情况（本机进程直连本地端口）一律用真实对端地址（`127.0.0.1`）作为来源 IP，忽略请求中的 `X-Forwarded-For` / `Tailscale-Funnel-Request`，所以伪造转发头既不能冒充别的 IP、也不能逃脱自己的计数；
     - 若 P3 实测无法可靠识别 tailscaled 对端进程，退化为：一律用对端地址计数（所有 Funnel 流量共享一个 `127.0.0.1` 桶）。因正确 Token 永不被锁，退化只影响错误 Token 的封锁粒度，不影响可用性（R8）。
5. **审计日志**（新表 `connector_audit`）：时间、工具、参数摘要（任务正文只存哈希 + 前 80 字）、`operation_id`、结果码、`user_quote`（审批）。保存 90 天；设置页可查看。
6. **Grok Bot 永远不能做的事**：写终端原始输入 / 按键；读写文件、读终端全文；创建、删除、改名卡片或工作空间；传 `force_untracked`；会话级或永久放行审批；无 nonce 批准；修改连接器设置、Token、webhook URL；访问手机配对 / `MobileDeviceGrant`；触发 Norma 自主路由。
7. **未知字段**：所有输入 `deny_unknown_fields`（与 `MobileCommandRequest` 一致）。

## 7. 新增存储（全部为新）

- `connector_operations(client_id, operation_id, tool, request_hash, state, result_json, error, created_at_unix, updated_at_unix, PRIMARY KEY(client_id, operation_id))`——保存 30 天后清理（暂定，待 yakami 确认）；`client_id` 为连接器调用方身份，Token 轮换 24 小时宽限期内新旧 Token 视为同一 `client_id`，保证跨轮换重放仍幂等（暂定，待 yakami 确认）。语义复制 `MobileCommandStore::reserve`（`New / Existing / Conflict`），不复用 `mobile_commands`（它绑定 `device_id` 与手机授权）。
- `connector_outbox`、`connector_audit`、`connector_nonces`（内存即可，重启作废）。
- `connector_tokens(token, state ∈ active/grace/revoked, created_at_unix, grace_until_unix)`、`connector_secrets`（HMAC secret、`card_id` 密钥）。
- 鉴权失败计数（按来源 IP）：内存即可，重启清零。
- 设置：端口（默认 47615）、webhook URL、启用开关、通知范围（默认仅 Grok Bot 派出的任务，可切换为全部任务）（存 app 设置）；Token 与 HMAC secret 明文存 Norma SQLite（`connector_tokens` / `connector_secrets`），不进仓库、不写日志。

## 8. 分期

| 期 | PR 内容 | 完成标志 |
| --- | --- | --- |
| P1 | 本规格 | 合入 docs |
| P2 | `ConnectorServer`（axum + rmcp） + Token 生成/SQLite 存储/查看/吊销 + 按来源 IP 锁定 + `directory` / `list_tasks` / `get_task_progress` + 审计 + 限流 | 本机 `curl`/MCP Inspector 调用通过 AC-01–AC-12、AC-41–AC-43、AC-50–AC-53 |
| P3 | Tailscale Funnel 配置文档与设置页地址字段；用户登录 Tailscale、开启 Funnel 并在 Grok Bot 添加自定义连接器；确认 Funnel 来源 IP 识别（R8） | Grok Bot 远程调用三个只读工具成功（AC-13–AC-16、AC-44、AC-55） |
| P4 | `dispatch_task` + `cancel_task` + `connector_operations` + `card_id` 精确解析 | AC-17–AC-26 |
| P5 | 前置：确认 routine webhook 能读原始请求体与请求头（否则需中继，待用户确认）；`ConnectorNotifier` + outbox + HMAC + `resolve_approval` 两步确认 | AC-27–AC-40、AC-45–AC-49 |

## 9. 验收清单（QA）

- AC-01 连接器默认关闭；关闭时端口无监听。
- AC-02 开启后只监听 `127.0.0.1:<port>`；从局域网 IP 连接失败。
- AC-03 无 / 错误 Token → HTTP 401，审计记录一条失败。
- AC-04 Token 明文存于 Norma SQLite `connector_tokens`，不在 macOS 钥匙串；Token 熵 ≥ 128 bit；日志与仓库（含 E2E 夹具）中搜索不到该 Token。
- AC-05 吊销立即生效：旧 Token 立即 401，SQLite 中记录删除，已有 MCP 会话断开。
- AC-06 轮换后旧 Token 在 24 小时宽限期内可用、期满 401。
- AC-07 `Host` 头非白名单 → 拒绝。
- AC-08 `tools/list` 在 P2 只列三个只读工具。
- AC-09 `directory` 返回所有工作空间、项目、卡片；未加载区域卡片 `state=unloaded`，调用前后无新 PTY 启动。
- AC-10 两张同名卡片 → 两者 `role_name_unique=false`，`card_id` 不同；同一目录两个画布中 `node_id` 相同的卡片，`card_id` 也不同。
- AC-11 `list_tasks` 默认含全部未终态任务与 24 h 内终态；分页游标稳定。
- AC-12 超过读限流 → `rate_limited` 含 `retry_after_s`；同一来源 IP 连续 10 次鉴权失败 → 该 IP 的错误 Token 请求被拒 15 分钟。
- AC-13 Funnel 地址 `https://<machine>.<tailnet>.ts.net` 稳定，App 重启 / tailscaled 重启后 URL 不变。
- AC-14 通过 Funnel 访问非 `/mcp` 路径 → 404。
- AC-15 Grok Bot 自定义连接器能列出工具并调用 `directory`。
- AC-16 Mac 睡眠时 Grok Bot 调用得到明确的连接失败，不误报为空目录。
- AC-17 `dispatch_task` 到指定 `card_id`，只有该卡片收到输入，其他卡片零输入。
- AC-18 同 `operation_id` 同参数重放 → 原回执，`replayed=true`，卡片不重复输入。
- AC-19 同 `operation_id` 不同参数 → `operation_conflict`。
- AC-20 卡片删除后重建（同名同 node_id）→ 旧 `card_id` 返回 `target_changed`。
- AC-21 卡片改名后旧 `card_id` 仍指向原卡片（身份不随名字）。
- AC-22 卡片等审批时派工 → `approval_pending`，不越过审批；用户正在输入 → `input_occupied`。
- AC-23 无可信跟踪的卡片 → `untracked_requires_confirmation`，不会 `force_untracked`。
- AC-24 写入过程中杀掉 App → 重启后该操作为 `outcome_unknown`，无自动重放。
- AC-25 `cancel_task` 只取消目标任务；对已完成任务返回 `already_terminal`。
- AC-26 回执状态区分 accepted / submitted / running，提交不被报告为完成。
- AC-27 四类事件各自触发一次 webhook，负载符合 5.2 schema。
- AC-28 签名可用 secret 复算；改动 body 一个字节签名不匹配。
- AC-29 同 `dedupe_key` 只投递一次（包括 App 重启后）。
- AC-30 webhook 返回 500 → 按 1/2/4… 分钟退避；返回 410 → 不再重试，标 `dead`。
- AC-31 断网 2 小时后恢复 → 积压事件补发；已关闭的审批事件不补发。
- AC-32 已跟踪任务连续 30 分钟无新输出、工具调用或状态变化 → 只发一次 `task.stalled`；恢复活动后再停滞 30 分钟再发一次；untracked 任务永不发 `task.stalled`。
- AC-33 负载中无终端全文、文件内容、Token。
- AC-34 `resolve_approval commit` 缺 `user_confirmed` 或 nonce → `confirmation_required`。
- AC-35 nonce 复用 / 过期 / 跨 approval 使用 → `nonce_invalid` / `nonce_expired`。
- AC-36 批准后实际只放行该次调用；Provider 无会话级放行选项暴露。
- AC-37 审批已在 Mac 或手机上处理 → `request_closed`；不同决定 → `decision_conflict`。
- AC-38 每次审批审计记录含 `user_quote` 与决定。
- AC-39 Grok Bot 侧：用户未确认时 Grok Bot 不调用 commit（客户端走查用例）。
- AC-40 第 6.6 节列出的禁止能力在 `tools/list` 中不存在，任何参数组合都无法达成。
- AC-41 默认端口为 `47615`；在设置中改端口后按新端口监听，且设置页显示「需同步修改 Tailscale Funnel 配置」提示及对应命令。
- AC-42 App 未运行时无任何 Norma 进程监听端口；未安装登录项或后台常驻进程。
- AC-43 `card_id` 中包含画布标识；同目录多画布场景下 `directory` 返回的 `card_id` 全部唯一。
- AC-44 App 未运行时 Grok Bot 调用得到连接失败，并向用户显示「Norma 未运行」，不误报为空目录。
- AC-45 Grok Bot routine 收到签名错误或时间戳过期的事件 → 丢弃，不给用户发消息。
- AC-46 默认设置下，只有 Grok Bot 派出的任务（及其审批）触发 webhook；桌面 / 手机发起的任务不触发。打开「通知全部任务」后这些任务也触发。
- AC-47 同一事件同时触发手机推送与 webhook 时，两路都发送，互不去重。
- AC-48 App 退出时 outbox 有积压事件 → App 重新打开后补发。
- AC-49 （P5 前置）已记录 routine webhook 能否读取原始请求体与请求头的验证结果；若不能，已获得用户对中继方案的确认后再实现。
- AC-50 设置页随时可查看（显示 / 复制）当前 Token，宽限期内也可查看旧 Token；App 重启后仍可查看。
- AC-51 其他来源 IP 发送大量错误 Token（达到并超过锁定阈值）后，正确 Token 的请求（无论来自哪个 IP，包括同一 IP）仍立即成功。
- AC-52 同一来源 IP 连续 10 次错误 Token → 该 IP 后续错误 Token 请求返回 429 `rate_limited`（含 `retry_after_s`），15 分钟后解除；其他 IP 的失败计数不受影响。
- AC-53 本机进程直连 `127.0.0.1:<port>`，携带伪造的 `X-Forwarded-For` / `Tailscale-Funnel-Request` 头：失败计入 `127.0.0.1` 桶，不计入伪造 IP；无法借此绕过锁定或让其他 IP 被锁。
- AC-54 （已删除，2026-10-14：不做全局失败告警；编号不复用）
- AC-55 （P3）经 Funnel 的请求，来源 IP 取 tailscaled 写入的 `X-Forwarded-For`（公网客户端 IP），客户端自带的 `X-Forwarded-For` 被覆盖无效；实测结果记录在 P3 PR 中。

## 10. 已决定

以下 10 项原开放问题已由 yakami 于 2026-10-13 决定，正文已同步修改；Q2、Q7、Q9 及 Q11–Q13 按 2026-10-14 决定更新。

- Q1 HTTP / MCP 实现：`axum` + `rmcp`（3.1）。
- Q2 P3 使用 Tailscale Funnel（2026-10-14 改定，替代 Cloudflare 命名隧道），只用 Bearer Token；Cloudflare Tunnel / Access 仅作后续可选（6.2）。
- Q3 不依赖 #97。`card_id` 由连接器自行定义，同一目录多画布时也必须唯一（编码含 `canvas_id`）；不再计划切换到 #97 的身份（2、3.2）。
- Q4 Grok Bot routine 必须校验 HMAC 签名，校验失败丢弃事件。P5 前置条件与风险：确认 routine webhook 触发器能读取原始请求体与请求头；若不能，需要小型中继，待用户确认（5.3、R7）。
- Q5 默认只通知 Grok Bot 派出的任务；设置项可打开「通知全部任务」（5.1、7）。
- Q6 手机推送与 Grok Bot 通知各自独立发送，不跨渠道去重（5.1）。
- Q7 默认端口 `47615`，可在设置中修改；修改时提示需同步修改 Tailscale Funnel 配置（3.1）。
- Q8 停滞：连续 30 分钟没有新输出、工具调用或状态变化；untracked 任务不参与（5.1）。
- Q9 Token 轮换后旧 Token 有 24 小时宽限期；吊销立即生效（6.3）。
- Q11 （2026-10-14）Token 明文存 Norma SQLite，不用钥匙串；设置页随时可查看；Token 永不进仓库（6.3）。
- Q12 （2026-10-14）锁定按来源 IP 计数鉴权失败，正确 Token 永不被锁；Funnel 下来源 IP 取 tailscaled 写入的 `X-Forwarded-For`，仅在确认连接来自本机 Funnel 代理时采信（6.4）。
- Q13 （2026-10-14）Token 熵 ≥ 128 bit；不设全局失败告警（6.3、6.4）。
- 暂定（2026-10-14，待 yakami 确认）：`card_revision` 只由持久状态计算（3.2）；webhook `occurred_at` 改为 unix 秒（5.2）；`connector_operations` 保存 30 天、轮换宽限期内新旧 Token 同一 `client_id`（7）。P4 其余暂定项见 `2026-10-14-grokbot-connector-p4-tickets.md`。
- Q10 不做后台常驻。App 未运行时连接器不可达，Grok Bot 显示「Norma 未运行」；outbox 中的事件在 App 重新打开后补发（3.1、5.4）。

## 11. 风险（代码中让决定变难的地方）

- R1 现有 `AgentTaskAssignment` 只有 `name + task`，派工按名字解析；而 `agent_definitions` 名称唯一索引是定义级，同一定义可能有多张卡片。P4 需要新增按卡片实例派工的内部入口，否则无法满足「精确目标」。
- R2 `node_id` 是区域内局部编号，同目录多画布可重复（#97 T01 正是为此）；dev 上没有持久卡片身份；连接器自定义的 `card_id` 必须编码画布标识才能唯一，P2 需确认现有存储中有稳定的画布标识，没有则新增。
- R3 审批只支持 `Codex | Claude`（`ApprovalTarget::valid`）；OpenCode 等卡片的审批无法 resolve。
- R4 未加载区域的状态只有持久快照，`busy/idle` 可能过时；必须标注 `unloaded`，不能推断。
- R5 仓库无 HTTP 服务框架，P2 工作量与依赖审查比看起来大。Token 明文存 SQLite 意味着能读 app.sqlite 的本机进程 / 备份可取得 Token，这是已接受的取舍。
- R6 跨工作空间派工依赖 `orchestration_task_routes`，目标工作空间未打开时的行为（`closed_workspace_response`）需在 P4 明确为错误还是排队。
- R7 Grok Bot routine 的 webhook 触发器可能拿不到原始请求体 / 请求头，导致无法校验 HMAC；此时需要一个小型中继（P5 前置确认，是否引入待用户确认）。
- R8 经 Funnel 与本机直连的请求 TCP 对端都是 `127.0.0.1`；按对端进程识别 tailscaled 依赖 macOS libproc 与 Tailscale 客户端形态（App Store 版为系统扩展），P3 需实测。识别不可靠时退化为按对端地址计数（6.4），只降低错误 Token 封锁粒度。
