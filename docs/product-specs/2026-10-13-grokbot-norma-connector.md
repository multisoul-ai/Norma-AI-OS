# Grok Bot 作为 Norma 调度员：本机 MCP 连接器、派工与通知

状态：实现规格（P1），按 2026-10-13 yakami 拍板写入，待工程拆分 · 2026-10-13。代码事实基于 origin/dev `83590301`。本文只定义规格，不改代码；后续 P2–P5 每期一个 PR。

## 0. 已拍板的决定

以下决定由 yakami 确认，原样执行，本文不再重新取舍。

1. **接入方式**：Norma 在 Mac 上的 Norma App 进程内运行一个本机 MCP 服务器（传输为 MCP Streamable HTTP），只绑定 `127.0.0.1`；通过用户自有域名上的 **Cloudflare 命名隧道（named tunnel）** 暴露给 Grok Bot。**不使用 quick tunnel**（`trycloudflare.com` 地址每次变化，无法作为固定连接器地址）。隧道之上再加 **Bearer Token** 鉴权。Token 永不进仓库；生成、存储（macOS 钥匙串）、轮换、吊销见第 6 节。
2. **由 Grok Bot 决定派给谁**：Norma 本身不做路由、不按能力选人。提供六个工具：`directory`、`dispatch_task`、`list_tasks`、`get_task_progress`、`cancel_task`、`resolve_approval`。`resolve_approval`（批准 / 拒绝）**每次都必须由 Grok Bot 先问用户确认**——这是客户端要求；同时服务端 API 也强制要求确认标志 + 一次性 nonce（第 4.6 节）。
3. **通知四类事件**：任务完成、任务失败、等待工具审批、30 分钟无进展。Norma 向用户配置的 **Grok Bot routine webhook URL** 发 POST，唤醒 Grok Bot 给用户发消息。负载 JSON、HMAC 签名、退避重试、去重键、离线行为见第 5 节。
4. **不依赖草稿 PR #97**（分支 `codex/workspace-agent-dispatch`），只借鉴其设计思想（第 2 节）。
5. **分期交付，一期一个 PR**：P1 本规格；P2 本机 MCP 服务器 + Token 鉴权 + 只读工具；P3 Cloudflare 命名隧道 + 在 Grok Bot 添加自定义连接器，只读端到端（Cloudflare 登录由用户完成）；P4 `dispatch_task` + `cancel_task`（幂等与回执）；P5 webhook 通知 + `resolve_approval`。

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

- Norma App 内新增 MCP 服务器（新）、六个工具、Token 管理、命名隧道接入说明、webhook 通知发送器（新）、连接器审计日志（新）、设置页「Grok Bot 连接器」（新）。

### 1.4 范围外

- Norma 内的智能路由 / 按能力选人；新建 Agent 卡片；Agent 提问（非审批）的回答；终端原始输入；文件读写；跨 Mac 汇总（首版只连一台 Mac）。
- 不改手机配对与授权模型（`mobile_commands`、`MobileDeviceGrant`），不改 Unix socket 协议的现有语义。
- 不合入、不依赖 #97 的代码。

## 2. 从 PR #97 借鉴的设计（只借思想，不依赖代码）

来源：`origin/codex/workspace-agent-dispatch` 上的 `docs/product-specs/2026-10-10-workspace-agent-task-dispatch.md` 与 `docs/exec-plans/2026-10-10-workspace-agent-task-dispatch.md`（T01–T09，T06–T09 仍未完成）。

| 借鉴点 | #97 中的含义 | 本规格如何采用 |
| --- | --- | --- |
| 持久卡片身份 | 目标快照含工作空间、区域、持久卡片身份、节点、Runtime 路由代次、Provider 会话指纹；删除后重建是新身份，旧请求拒绝 | `directory` 返回 `card_id`（不透明字符串，见 3.2），`dispatch_task` 只接受 `card_id`，并带 `card_revision`；身份变化 → `target_changed`，**不自动换卡** |
| 唯一角色名称 | 空间内主名称 + 中文语音别名共同查重，大小写不敏感；改名后旧称失效 | `directory` 只展示名称供 Grok Bot 与用户对话；`role_name_unique` 标记名称在该工作空间内是否唯一。Grok Bot 用名称跟用户沟通，**但派工必须用 `card_id`** |
| 精确目标解析 | 不用相似名称、焦点或「唯一剩余卡片」替代明确称呼；补全前零接收零投递 | 服务端不做模糊匹配；名称解析由 Grok Bot 在 `directory` 结果中完成，多候选或零候选时由 Grok Bot 问用户 |
| 带操作账本的幂等投递 | 重发同操作返回原回执、同键不同参数冲突、正文/Enter 写入崩溃标「待核对」不自动重放；提交 ≠ 运行 ≠ 完成 | 新表 `connector_operations`（第 7 节），语义与 `MobileCommandStore::reserve`、`orchestration_task_operations` 一致；状态严格区分 `accepted / submitted / running / done / failed / cancelled / outcome_unknown` |

不借鉴：#97 的 Dock UI、名称修复迁移、`WorkspaceAgentRoleStore` 表结构、工作空间进度汇报 typed tool。若 #97 日后合入，P4 可把 `card_id` 解析切换到其持久身份（开放问题 Q3）。

## 3. 架构

```text
Grok Bot (云端)
  │ HTTPS  Authorization: Bearer <token>
  ▼
https://norma.<用户域名>/mcp   ← Cloudflare 命名隧道（cloudflared，用户登录）
  │
  ▼  http://127.0.0.1:<port>/mcp
Norma App 进程
  ├─ ConnectorServer（新）: Streamable HTTP + MCP JSON-RPC, 鉴权, 限流, 审计
  ├─ ConnectorTools（新）: 六个工具 → 复用 norma::protocol::CardCommand 执行路径 / WorkspaceActivitySummary
  ├─ connector_operations 账本（新, app.sqlite）
  └─ ConnectorNotifier（新）: 事件 → webhook POST(HMAC) → Grok Bot routine
```

### 3.1 MCP 服务器（新）

- 进程内后台线程/tokio 任务，**只绑定 `127.0.0.1`**（不复用 `mobile::server::allowed_address`，它放行私网）。端口默认 `47615`（固定端口便于隧道配置，冲突时设置页报错，不随机），可在设置中改。
- 路径 `POST /mcp`、`GET /mcp`（SSE 流，可选）按 MCP Streamable HTTP 规范；支持 `initialize`、`tools/list`、`tools/call`；`Mcp-Session-Id` 会话头。其他路径 404。
- 实现库：仓库没有 HTTP 框架，P2 需要选定（开放问题 Q1）。要求请求体上限 64 KiB、读超时 10 s、并发连接上限 8。
- 默认关闭；设置页「Grok Bot 连接器」开关打开后才监听。

### 3.2 `card_id`（新，不透明）

格式 `c1.<base64url(desktop_workspace_id | area_id | node_id | definition_id | 创建时间戳)>.<8 字节 HMAC>`。HMAC 用本机密钥（钥匙串）防伪造。`card_revision` = 当前 Runtime 路由代次 + Provider 会话指纹的哈希。解析时逐项复核；任一不符 → `target_changed`。这是 P2 的新增投影，不改现有表。

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
| `task.stalled` | 未终态任务 30 分钟没有新的 Hook / turn / 进度事件 | 新增计时：`last_activity_at` |

范围：默认只通知 `source=grokbot` 的任务 + 全部审批（开放问题 Q5）。`task.stalled` 每个任务每次停滞只发一次，有新活动后重置。

### 5.2 负载

```json
{"version": 1, "event_id": "evt_…", "dedupe_key": "task.completed:<task_id>",
 "type": "task.completed", "occurred_at": "2026-10-13T05:25:00Z",
 "machine": {"machine_id": "…", "name": "yakami 的 MacBook"},
 "workspace": {"workspace_id": "…", "name": "Norma"},
 "card": {"card_id": "c1.…", "role_name": "Alan"},
 "task": {"task_id": "…", "operation_id": "…"|null, "state": "done", "summary": "≤500 字"},
 "approval": null,
 "hint": "调用 get_task_progress 获取详情"}
```
`approval.waiting` 时 `approval = {"approval_id", "tool", "details_summary"}`。负载不含终端全文、文件内容、Token。

去重键：`task.completed:<task_id>`、`task.failed:<task_id>:<run_id>`、`approval.waiting:<approval_id>`、`task.stalled:<task_id>:<last_activity_at>`。

### 5.3 签名

请求头：`X-Norma-Timestamp: <unix>`、`X-Norma-Event-Id`、`X-Norma-Signature: v1=<hex(HMAC-SHA256(secret, timestamp + "." + body))>`。secret 与 Bearer Token **分开**，同样存钥匙串。接收方应拒绝时间差 > 5 分钟的请求（Grok Bot routine 能否校验见 Q4）。

### 5.4 投递、重试、离线

- 新表 `connector_outbox`（app.sqlite）：`event_id PK, dedupe_key UNIQUE, payload_json, attempts, next_attempt_at, state ∈ pending/delivered/dead`。事件先落库再发送。
- 2xx 视为成功；4xx（除 408/429）视为永久失败 → `dead`；5xx / 超时 / 网络错误重试：1 min、2、4、8、16、32 min，此后每 60 min，最长 24 h → `dead`。429 遵守 `Retry-After`。
- Mac 睡眠 / 断网 / App 退出：outbox 持久保留，恢复后按序补发；同一任务已有更新事件（例如 stalled 后 completed）时，过时的 `task.stalled` 直接丢弃不发。审批事件补发前复核仍为 pending，已关闭则丢弃。
- `dead` 事件在设置页可见；不会重复骚扰用户。

## 6. 安全

1. **监听**：只绑 `127.0.0.1`，拒绝非回环 `peer_addr`；`Host` 头必须是配置的隧道域名或 `127.0.0.1:<port>`（防 DNS rebinding）；不开 CORS。
2. **隧道**：Cloudflare 命名隧道，`cloudflared` 配置 `ingress` 只转发 `https://norma.<域名>/mcp` → `http://127.0.0.1:<port>`，其他 404。Cloudflare 登录和 DNS 由用户在 P3 完成；隧道凭据文件在 `~/.cloudflared/`，不进仓库。可选叠加 Cloudflare Access 服务令牌（Q2）。
3. **Token**：
   - 生成：设置页「生成连接令牌」，32 字节 CSPRNG，格式 `nrm_live_<base64url>`；只在生成时显示一次，供用户粘贴进 Grok Bot 自定义连接器。
   - 存储：macOS 钥匙串（service `ai.multisoul.norma.connector`），本地只存 `sha256(token)` 用于常数时间比较；不写日志、不写 SQLite、不进仓库或 E2E 夹具。
   - 轮换：生成新 Token 后旧 Token 保留 24 小时宽限（可设为 0）；设置页显示两者状态。
   - 吊销：「立即吊销」删除钥匙串项并断开所有 MCP 会话；关闭连接器开关等同全部吊销。
4. **限流**：每 Token 读 60 次/分钟，写 10 次/分钟，`resolve_approval` commit 5 次/分钟；连续 10 次鉴权失败 → 封锁 15 分钟并在设置页提示。
5. **审计日志**（新表 `connector_audit`）：时间、工具、参数摘要（任务正文只存哈希 + 前 80 字）、`operation_id`、结果码、`user_quote`（审批）。保存 90 天；设置页可查看。
6. **Grok Bot 永远不能做的事**：写终端原始输入 / 按键；读写文件、读终端全文；创建、删除、改名卡片或工作空间；传 `force_untracked`；会话级或永久放行审批；无 nonce 批准；修改连接器设置、Token、webhook URL；访问手机配对 / `MobileDeviceGrant`；触发 Norma 自主路由。
7. **未知字段**：所有输入 `deny_unknown_fields`（与 `MobileCommandRequest` 一致）。

## 7. 新增存储（全部为新）

- `connector_operations(client_id, operation_id, tool, request_hash, state, result_json, error, created_at_unix, updated_at_unix, PRIMARY KEY(client_id, operation_id))`——语义复制 `MobileCommandStore::reserve`（`New / Existing / Conflict`），不复用 `mobile_commands`（它绑定 `device_id` 与手机授权）。
- `connector_outbox`、`connector_audit`、`connector_nonces`（内存即可，重启作废）。
- 设置：端口、webhook URL、启用开关、通知范围（存 app 设置）；Token 与 HMAC secret 只在钥匙串。

## 8. 分期

| 期 | PR 内容 | 完成标志 |
| --- | --- | --- |
| P1 | 本规格 | 合入 docs |
| P2 | `ConnectorServer` + Token 生成/存储/吊销 + `directory` / `list_tasks` / `get_task_progress` + 审计 + 限流 | 本机 `curl`/MCP Inspector 调用通过 AC-01–AC-12 |
| P3 | 命名隧道配置文档与设置页地址字段；用户登录 Cloudflare 并在 Grok Bot 添加自定义连接器 | Grok Bot 远程调用三个只读工具成功（AC-13–AC-16） |
| P4 | `dispatch_task` + `cancel_task` + `connector_operations` + `card_id` 精确解析 | AC-17–AC-26 |
| P5 | `ConnectorNotifier` + outbox + HMAC + `resolve_approval` 两步确认 | AC-27–AC-40 |

## 9. 验收清单（QA）

- AC-01 连接器默认关闭；关闭时端口无监听。
- AC-02 开启后只监听 `127.0.0.1:<port>`；从局域网 IP 连接失败。
- AC-03 无 / 错误 Token → HTTP 401，审计记录一条失败。
- AC-04 Token 只显示一次；钥匙串可见该项；SQLite 与日志中搜索不到明文 Token。
- AC-05 吊销后旧 Token 立即 401，已有 MCP 会话断开。
- AC-06 轮换后旧 Token 在宽限期内可用、期满 401。
- AC-07 `Host` 头非白名单 → 拒绝。
- AC-08 `tools/list` 在 P2 只列三个只读工具。
- AC-09 `directory` 返回所有工作空间、项目、卡片；未加载区域卡片 `state=unloaded`，调用前后无新 PTY 启动。
- AC-10 两张同名卡片 → 两者 `role_name_unique=false`，`card_id` 不同。
- AC-11 `list_tasks` 默认含全部未终态任务与 24 h 内终态；分页游标稳定。
- AC-12 超过读限流 → `rate_limited` 含 `retry_after_s`；连续 10 次鉴权失败触发 15 分钟封锁。
- AC-13 命名隧道域名稳定，App 重启 / 隧道重启后 URL 不变。
- AC-14 通过隧道访问非 `/mcp` 路径 → 404。
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
- AC-32 停滞 30 分钟只发一次 `task.stalled`；恢复活动后再停滞 30 分钟再发一次。
- AC-33 负载中无终端全文、文件内容、Token。
- AC-34 `resolve_approval commit` 缺 `user_confirmed` 或 nonce → `confirmation_required`。
- AC-35 nonce 复用 / 过期 / 跨 approval 使用 → `nonce_invalid` / `nonce_expired`。
- AC-36 批准后实际只放行该次调用；Provider 无会话级放行选项暴露。
- AC-37 审批已在 Mac 或手机上处理 → `request_closed`；不同决定 → `decision_conflict`。
- AC-38 每次审批审计记录含 `user_quote` 与决定。
- AC-39 Grok Bot 侧：用户未确认时 Grok Bot 不调用 commit（客户端走查用例）。
- AC-40 第 6.6 节列出的禁止能力在 `tools/list` 中不存在，任何参数组合都无法达成。

## 10. 开放问题

- Q1 MCP / HTTP 实现库：仓库没有 HTTP 服务框架。选 `axum`+`rmcp`（新增依赖、二进制体积）还是延续 `agent_hook_ingress.rs` 的手写 `std::net`（自己实现 Streamable HTTP 与 SSE）？
- Q2 是否在 Bearer Token 之外再加 Cloudflare Access（服务令牌 / mTLS）？Grok Bot 自定义连接器能否发送额外头部决定可行性。
- Q3 #97 合入后是否把 `card_id` 改为其持久卡片身份？在此之前 `node_id + definition_id + 创建时间` 的身份是否足够（同目录多画布 node_id 可重复）。
- Q4 Grok Bot routine webhook 是否能校验 HMAC / 时间戳？若不能，签名只作为 URL 内秘密之外的纵深防御。
- Q5 通知范围：只通知 Grok Bot 派出的任务，还是包括桌面 / 手机 / 直接卡片发起的任务？审批是否全部通知？
- Q6 与手机推送（`crates/app/src/mobile/push/activity.rs`）同时触发时是否去重 / 只选一个渠道？
- Q7 端口固定 `47615` 是否接受；Mac 多用户场景是否需要每用户端口。
- Q8 「30 分钟无进展」的「进展」定义：仅 Hook / turn 事件，还是也算终端输出？无跟踪（untracked）任务是否参与。
- Q9 Token 宽限期默认 24 h 还是 0。
- Q10 App 未运行时 MCP 不可用；是否需要登录项 / 后台常驻。

## 11. 风险（代码中让决定变难的地方）

- R1 现有 `AgentTaskAssignment` 只有 `name + task`，派工按名字解析；而 `agent_definitions` 名称唯一索引是定义级，同一定义可能有多张卡片。P4 需要新增按卡片实例派工的内部入口，否则无法满足「精确目标」。
- R2 `node_id` 是区域内局部编号，同目录多画布可重复（#97 T01 正是为此）；dev 上没有持久卡片身份，`card_id` 只能尽力而为。
- R3 审批只支持 `Codex | Claude`（`ApprovalTarget::valid`）；OpenCode 等卡片的审批无法 resolve。
- R4 未加载区域的状态只有持久快照，`busy/idle` 可能过时；必须标注 `unloaded`，不能推断。
- R5 仓库无 HTTP 服务框架、无钥匙串生产代码，P2 工作量与依赖审查比看起来大。
- R6 跨工作空间派工依赖 `orchestration_task_routes`，目标工作空间未打开时的行为（`closed_workspace_response`）需在 P4 明确为错误还是排队。
