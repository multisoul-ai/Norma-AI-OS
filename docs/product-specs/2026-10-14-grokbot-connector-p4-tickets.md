# Grok Bot 连接器 P4：派工与取消——工单拆分

状态：工单拆分（仅文档，不改代码）· 2026-10-14。叠在 PR #105（`docs/grokbot-connector-spec-1014`）之上。
唯一规格来源：`docs/product-specs/2026-10-13-grokbot-norma-connector.md`（下称「主规格」）。本文不改主规格已拍板事项，新增内容以「补充条款」形式提出，AC 编号从 AC-56 起续编（AC-54 已删除，不复用）。

代码事实依据：
- origin/dev（主规格 1.1 节所列文件）；
- P2 分支 `origin/feat/grokbot-connector-p2`（PR #104，**未合入，open**，头 `bc669a0e`）。凡引用 P2 代码处，均以该分支为准，合入时可能变化。
- 标记 **【假设】** 的内容未在代码中核实，实现前需确认。

## 0. 不得改动的已决定事项

以下沿用主规格，P4 任何工单都不得改变：

1. Token **明文存 Norma SQLite**（`connector_tokens`），不用钥匙串（主规格 Q11、6.3）。
2. 隧道**优先 Tailscale Funnel**（Q2、6.2）。
3. **AC-54 已删除**，不设全局失败告警，编号不复用（Q13）。
4. Norma App 未运行时连接器不可达，Grok Bot 向用户显示 **「Norma 未运行」**（Q10、AC-44）。
5. 连接器永不传 `force_untracked=true`；不做 Norma 内智能路由；`card_id` 自定义、不依赖 #97（Q3）。
6. `resolve_approval` 属 P5，每次批准 / 拒绝前 Grok Bot 都必须先问 yakami 确认（0.2、4.6）。

## 1. 代码现状（与 P4 相关）

| 事实 | 位置 | 对 P4 的意义 |
| --- | --- | --- |
| P2 只注册三个只读工具 `directory / list_tasks / get_task_progress`，注解 `read_only(true)` | P2 `crates/norma/src/connector/tools/schema.rs` | P4 新增两个写工具，注解需为 `read_only(false)`，`destructive`：dispatch=false、cancel=true【假设：按 MCP 注解语义取值】 |
| 工具分发 `ConnectorTools::execute(name, input)`，未知工具返回 `invalid_argument`；错误结构 `schema::error(code, retry_after)` | P2 `crates/norma/src/connector/tools.rs` | 新工具接入同一分发与错误结构 |
| 输入结构体全部 `#[serde(deny_unknown_fields)]`，反序列化失败 → `invalid_argument` | P2 `tools/model.rs` | 新输入同样处理 |
| `Task` 输出：`task_id, card_id, role_name, workspace_id, state, tracking, summary, created_at: i64, updated_at: i64, source`；`directory` 输出 `observed_at = auth.now()`（unix 秒） | P2 `tools/model.rs`、`tools.rs` | **连接器现有时间字段全部是整数 unix 秒** |
| `Card` 输出**没有**创建时间；但内部 `CardIdentity.created_at: i64` 已存在（来自 `agent_definitions.created_at_unix`，取不到时退化为 `desktop_project_areas.added_at_unix_ms / 1000`） | P2 `connector/identity.rs`、`storage/src/connector_read.rs` | P4-3 直接暴露，见 §4 |
| `get_task_progress.last_activity_at` 目前直接取 `agent_dispatch_tasks.updated_at_unix` | P2 `tools.rs` | 不是真正「最后活动」，见 §4 与开放问题 |
| P2 已建 `connector_tokens / connector_secrets / connector_audit(occurred_at_unix …)`；**尚无 `connector_operations`** | P2 `storage/src/connector_store.rs` | P4-1 新建 |
| `MobileCommandStore::reserve` → `New / Existing(receipt) / Conflict`；「No recovery path replays a reserved side effect」 | dev `crates/storage/src/mobile_command_store.rs` | 账本语义照抄 |
| `orchestration_task_operations(operation_id PK, request_fingerprint, result_json)` | dev `crates/storage/src/migrations.rs` | 下层派工本身的幂等，连接器账本叠在其上 |
| `CardCommand::DispatchAgentTasks { assignments: Vec<AgentTaskAssignment{name, task}>, force_untracked }`、`AgentTaskCancel { task_ref }`；回执 `AgentTaskReceipt{task_id, operation_id, target_definition_id, target_node_id}`、`AgentTaskReceiptStatus` 九态 | dev `crates/norma/src/protocol.rs`；执行 `crates/app/src/norma/endpoint/task_dispatch.rs`、`task_control.rs` | 派工按**名字**寻址（R1），P4 必须新增按卡片实例寻址的内部入口 |
| 审批目标 `ApprovalTarget { node_id, profile, run_id, session_id, turn_id, request_id }`，`delivery_state()` → `waiting / submitted / request_closed` | dev `crates/norma/src/mobile/approval.rs` | 取消与审批交互（§3） |

## 2. 工具定义

命名沿用主规格：`dispatch_task`、`cancel_task`。字段名与错误码以主规格为准；用户需求中的别名映射如下：

| 需求中的叫法 | 本文 / 主规格的名称 |
| --- | --- |
| prompt / input | `task`（正文）+ `context`（可选补充） |
| idempotency key | `operation_id` |
| invalid_request | `invalid_argument`（主规格通用错误码，P2 已实现） |
| already_finished / already_cancelled | `already_terminal`（**成功结果**的 `state` 值，非错误，主规格 4.5） |
| target_missing | `card_not_found`（卡片不存在 / 已删除 / 签名无效）与 `target_changed`（身份或 revision 不符）——不新增 `target_missing` |
| norma_not_running | **不是工具错误码**：App 不运行时根本无服务端可返回（Q10）。由 Grok Bot 侧在连接失败时显示「Norma 未运行」 |

### 2.1 `dispatch_task`

输入（`deny_unknown_fields`）：

```json
{"operation_id": "uuid-v4",            // 必填，Grok Bot 生成
 "card_id": "c1.…",                     // 必填，来自 directory
 "card_revision": "…",                  // 必填，来自 directory
 "task": "修复登录页 500",               // 必填，1..=8000 字符
 "context": "…",                        // 可选，计入 8000 上限【假设：与 task 合计】
 "allow_queue": true}                   // 可选，默认 true
```

不提供 `workspace_id` 参数：工作空间由 `card_id` 内编码的 `workspace_id` 唯一确定，额外参数只会制造不一致（开放问题 OQ-1）。

输出（回执）：

```json
{"operation_id": "…", "task_id": "…", "card_id": "c1.…", "role_name": "Alan",
 "workspace_id": "…",
 "state": "accepted|submitted|queued|untracked|outcome_unknown",
 "tracking": "tracked|untracked", "replayed": false,
 "accepted_at": 1760000000}
```

- `workspace_id` 为本文补充（便于 Grok Bot 汇报），其余字段原样来自主规格 4.2。
- `state` 映射 `AgentTaskReceiptStatus`：`Accepted→accepted`、`Submitted→submitted`、`Queued→queued`、`Untracked→untracked`；预留后崩溃 → `outcome_unknown`。**提交 ≠ 运行 ≠ 完成**（AC-26）：回执里永远不会出现 `running/done`，运行状态只能通过 `get_task_progress` 查询。

错误码（`isError: true`，结构同 P2 `schema::error`）：

| code | 含义 | retryable |
| --- | --- | --- |
| `invalid_argument` | 缺字段、未知字段、`operation_id` 非 UUID、`task` 为空 | false |
| `task_too_long` | `task`(+`context`) > 8000 字符 | false |
| `card_not_found` | `card_id` 签名无效或卡片已不存在 | false |
| `target_changed` | 身份字段或 `card_revision` 与当前不符（含删除后同名重建，AC-20） | false（需重新 `directory`） |
| `workspace_not_found` | `card_id` 指向的工作空间已删除 | false |
| `card_busy` | `allow_queue=false` 且卡片忙 | true |
| `approval_pending` | 卡片正等工具审批，不越过 | true |
| `input_occupied` | 用户正在该卡片输入 | true |
| `untracked_requires_confirmation` | 卡片无可信跟踪；连接器不传 `force_untracked` | false |
| `operation_conflict` | 同 `operation_id` 不同 `request_hash`（或不同工具） | false |
| `rate_limited` | 写限流 10 次/分钟（主规格 6.4），含 `retry_after_s` | true |
| `connector_disabled` / `internal` | 通用 | — |
| HTTP 401 `unauthorized` | Token 缺失 / 错误（HTTP 层） | false |

`workspace_closed`：目标工作空间未打开时（R6，`closed_workspace_response`）是报错还是排队，主规格未定。本文建议**报错** `workspace_closed`（retryable=true），不自动打开工作空间、不启动 PTY——作为开放问题 OQ-2，待 yakami 确认后才加入错误码表。

### 2.2 `cancel_task`

输入：`{"operation_id": "uuid-v4", "task_id": string}`（`deny_unknown_fields`）

输出：

```json
{"operation_id": "…", "task_id": "…",
 "state": "cancel_requested|cancelled|already_terminal",
 "terminal_state": "done|failed|cancelled|interrupted"|null,
 "replayed": false}
```

- `terminal_state` 为本文补充：仅 `state=already_terminal` 时非空，告诉 Grok Bot 任务是「已完成」还是「已取消」（对应需求中的 already_finished / already_cancelled），不新增错误码。
- 错误：`invalid_argument`、`task_not_found`、`operation_conflict`、`rate_limited`、`internal`。
- 只取消该 `task_id`，不影响同卡其他任务（AC-25）。可取消任何来源（`source` 不限 grokbot）的任务【假设：主规格未限制；见 OQ-3】。
- 基于 `CardCommand::AgentTaskCancel { task_ref }`，`CancelRequested → Cancelled`。

### 2.3 幂等规则（两工具共用）

1. 账本 `connector_operations(client_id, operation_id, tool, request_hash, state, result_json, error, created_at_unix, updated_at_unix, PK(client_id, operation_id))`（主规格第 7 节）。`client_id`：P2 鉴权后的调用方身份【假设：按 Token 记录区分；轮换宽限期内新旧 Token 必须映射到同一 `client_id`，否则重放会被当作新操作——见 OQ-4】。
2. `request_hash`：
   - dispatch：`sha256(tool | card_id | card_revision | task | context | allow_queue)`，缺省值先规范化（`allow_queue` 缺省视为 `true`，`context` 缺省视为空串）。
   - cancel：`sha256(tool | task_id)`。
3. 预留结果：
   - `New` → 执行副作用，写回 `result_json`；
   - `Existing` 且哈希相同 → 返回原回执，`replayed=true`，**不再写卡片**（AC-18）；
   - 哈希不同或 `tool` 不同 → `operation_conflict`（AC-19）。
4. 失败也入账：校验类错误（`card_not_found`、`target_changed`、`approval_pending` 等）**不预留**，同 `operation_id` 可在情况改变后重试；一旦进入写卡片阶段，结果（含错误）固化，重放返回同一结果【假设：与 `MobileCommandStore` 的 `rejected` 语义一致，实现前核对】。
5. 崩溃：预留后、确认写入前崩溃 → 重启后标 `outcome_unknown`，**不自动重放**（AC-24）；Grok Bot 只能用 `get_task_progress {operation_id}` 核对。
6. 下层：连接器把同一 `operation_id` 透传给派工路径，使 `orchestration_task_operations` 的指纹幂等同时生效【假设：需确认按卡片实例的新入口是否走该表】。
7. 账本保存期：建议 30 天后清理【假设，OQ-5】。

## 3. 审批与取消

### 3.1 派出任务中出现工具审批

- P4 不提供 `resolve_approval`，也不发 webhook。P4 期间审批只能在 Mac / 手机上处理；Grok Bot 可通过 `get_task_progress.pending_approval` 和 `directory.state=waiting_approval` 看到。
- P5 起：Grok Bot 派出的任务（`source=grokbot`）中出现的审批，经 `approval.waiting` webhook 通知（主规格 5.1，Q5）；Grok Bot **每一次**都要把工具名、参数摘要、卡片展示给 yakami，得到本次明确的「批准 / 拒绝」后才调 `resolve_approval commit`；不得基于先前授权或推断自动批准（主规格 4.6、AC-39）。
- 派工本身不需要额外审批：用户对 Grok Bot 说「让 Alan 做 X」即是派工意图。但若 Grok Bot 解析名称出现多候选 / 零候选，必须先问用户（主规格 2）。

### 3.2 等待审批时取消

- `cancel_task` 对处于等待审批的任务**照常生效**：走 `AgentTaskCancel`，返回 `cancel_requested` 或 `cancelled`。
- 取消**不会**替用户对审批作出 allow / deny 决定；审批请求随任务结束而关闭，后续 `ApprovalTarget::delivery_state` 返回 `request_closed`【假设：需实测 Codex / Claude 在取消时是否关闭挂起审批】。
- P5 衔接：outbox 中尚未投递的该审批 `approval.waiting` 事件，补发前复核为已关闭即丢弃（主规格 5.4）；之后对该 `approval_id` 的 `resolve_approval` 返回 `request_closed`。
- 取消是否需要 Grok Bot 先问 yakami：本文建议**用户明确说「取消某任务」即可直接调用**，但当按名字匹配到多个未终态任务时必须先问（OQ-3）。

## 4. 补充条款：时间字段（主规格缺失）

### 4.1 格式选择

- **MCP 工具输出：整数 unix 秒（UTC）**。理由：主规格工具输出已用 `observed_at: 1760000000`、`accepted_at: 1760000000`，P2 代码 `Task.created_at/updated_at: i64`、`observed_at = auth.now()` 均为秒；存储列为 `*_unix`。不引入第二种格式。
- **webhook 负载**：保持主规格 5.2 已写的 RFC3339 UTC（`"2026-10-13T05:25:00Z"`），P4 不改；是否统一为 unix 秒列为 OQ-6。
- 字段名后缀统一 `_at`；值为 `null` 表示「未知」，**不得**用 0 或当前时间伪造。

### 4.2 新增字段

| 位置 | 字段 | 来源与回退 |
| --- | --- | --- |
| `directory.cards[]` | `created_at: integer\|null` | 取 `CardIdentity.created_at`（P2 已算）：优先 `agent_definitions.created_at_unix`；无定义记录时 P2 用区域 `added_at_unix_ms/1000`——这是**区域**时间而非卡片时间，输出时应置 `null` 并加 `created_at_source: "definition"\|"area_fallback"`，或直接 `null`（OQ-7）。注意：该值也参与 `card_id` 编码，不能为输出而改其计算 |
| `dispatch_task` 回执 | `accepted_at`（主规格已有） | 连接器预留成功时刻；重放返回原值 |
| `cancel_task` 回执 | `requested_at: integer` | 预留成功时刻；重放返回原值 |
| `get_task_progress` | `created_at`、`updated_at`、`state_changed_at: integer\|null` | `agent_dispatch_tasks.created_at_unix / updated_at_unix`；`state_changed_at` 需新记录，旧任务无记录 → `null` |
| `get_task_progress` | `events: [{"type", "occurred_at": integer, "state"?}]`，最近 ≤ 20 条，新→旧 | 来源：连接器账本 + 任务状态迁移（`accepted/submitted/running/cancel_requested/done/failed/cancelled/interrupted/approval_waiting/approval_closed`）。无可信来源的事件不生成 |
| `get_task_progress` | `last_activity_at`（已有） | 语义修正：应为最后一次输出 / 工具调用 / 状态变化（主规格 Q8）；P2 现取 `updated_at_unix`，在真实活动时间可用前保持该回退，并在文档中注明 |

时区：所有值为 UTC 绝对时刻；展示时由 Grok Bot 转为用户时区（Asia/Shanghai）。

### 4.3 新增验收（续编，AC-54 不复用）

- AC-56 `dispatch_task` / `cancel_task` 出现在 `tools/list`，注解非只读；P2 的三个只读工具不变。
- AC-57 同一 `operation_id` 先用于 `dispatch_task`、再用于 `cancel_task` → `operation_conflict`。
- AC-58 `dispatch_task` 缺省字段规范化：显式 `allow_queue=true` 与省略视为同一请求，重放不冲突。
- AC-59 校验类错误（`card_not_found`/`target_changed`/`approval_pending`/`card_busy`/`input_occupied`）不占用 `operation_id`，条件消除后同 `operation_id` 可成功派工。
- AC-60 `cancel_task` 对已完成 / 已取消任务返回 `state=already_terminal` 且 `terminal_state` 分别为 `done` / `cancelled`，非错误；重放返回相同结果。
- AC-61 任务等待审批时 `cancel_task` → 任务取消，审批未被 allow 或 deny，随后该审批 `delivery_state` 为 `request_closed`。
- AC-62 `directory.cards[].created_at` 为整数 unix 秒或 `null`；卡片无定义记录时不得输出区域时间冒充卡片时间。
- AC-63 `get_task_progress` 返回 `created_at`、`updated_at`、`state_changed_at`、`events[].occurred_at`，均为整数 unix 秒或 `null`；`events` 按时间新→旧、≤ 20 条、无伪造事件。
- AC-64 dispatch / cancel 的回执时间（`accepted_at` / `requested_at`）在重放时与首次一致。
- AC-65 App 未运行时调用 `dispatch_task` / `cancel_task`：Grok Bot 得到连接失败并显示「Norma 未运行」，不报告为派工失败或已取消（AC-44 在写工具上的延伸）。
- AC-66 审计 `connector_audit` 对每次 dispatch / cancel 记一条：`operation_id`、结果码、任务正文仅哈希 + 前 80 字（主规格 6.5）。

## 5. 工单

模板：任务｜仓库｜Done when｜不要做。全部在 `multisoul-ai/Norma-AI-OS`，叠在 P2（#104）之上，**P2 合入前不开工**。

### P4-1 `dispatch_task` 工具 + 操作账本 + 按卡片实例派工

- **任务**：新建 `connector_operations` 表与 `reserve(New/Existing/Conflict)` 存储（语义照 `MobileCommandStore::reserve`）；`card_id` 验签 + 逐项复核 + `card_revision` 比对；新增按 `(workspace, area, canvas, node_id)` 派工的内部入口（解决 R1，不改 `AgentTaskAssignment` 现有按名语义）；`dispatch_task` 输入/输出/错误码按 §2.1；启动时把未确认写入的预留标为 `outcome_unknown`；审计记录。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/storage`、`crates/norma/src/connector`、`crates/app/src/norma/endpoint`）
- **Done when**：AC-17、AC-18、AC-19、AC-20、AC-21、AC-22、AC-23、AC-24、AC-26、AC-56、AC-57、AC-58、AC-59、AC-64（accepted_at）、AC-66 通过。
- **不要做**：不传 `force_untracked`；不做模糊名称匹配或自动换卡；不复用 `mobile_commands`；不自动重放 `outcome_unknown`；不改 Unix socket 协议现有语义；不自动打开未打开的工作空间（OQ-2 未定前）；不碰 Token 存储 / Funnel / 锁定逻辑。

### P4-2 `cancel_task` 工具

- **任务**：`cancel_task` 按 §2.2，复用 P4-1 账本；对接 `AgentTaskCancel`；`already_terminal` + `terminal_state`；等待审批时取消的行为按 §3.2 并实测 Codex / Claude。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector`、`crates/app/src/norma/endpoint/task_control.rs` 调用侧）
- **Done when**：AC-25、AC-60、AC-61、AC-64（requested_at）、AC-66 通过。
- **不要做**：不替用户作审批决定；不取消同卡其他任务；不实现 `resolve_approval` 或 webhook（P5）。

### P4-3 时间字段补充

- **任务**：`directory.cards[].created_at`；`get_task_progress` 的 `created_at / updated_at / state_changed_at / events[]`；`cancel_task.requested_at`；必要时新增状态迁移时间记录（新列或新表，只增不改）。同步把 §4 补充条款合入主规格（另起文档 PR 或本 PR 后续提交）。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/norma/src/connector/tools`、`crates/storage`、`docs/product-specs`）
- **Done when**：AC-62、AC-63 通过；主规格已含 §4 字段与 AC-56–AC-66。
- **不要做**：不改 `card_id` 编码中的 `created_at` 计算（会使已发出的 `card_id` 失效）；不用 0 / 当前时间填未知值；不改 webhook `occurred_at` 格式（OQ-6 未定前）。

### P4-4 测试与端到端

- **任务**：存储单测（reserve 三态、崩溃恢复）；工具集成测试（沿用 P2 `crates/norma/tests/connector_tools.rs`、`connector_http.rs` 风格）；经 Funnel 由 Grok Bot 真实派工 / 取消的走查，含 App 未运行场景；Grok Bot 侧走查：派工多候选时先问用户。
- **仓库**：multisoul-ai/Norma-AI-OS（`crates/*/tests`、E2E 记录写在 P4 PR）
- **Done when**：AC-17–AC-26、AC-56–AC-66 全部有自动化或记录在案的手工结果；E2E 夹具使用随机生成 Token（AC-04）。
- **不要做**：不把真实 Token 写进夹具 / 日志；不在用户真实工作空间上跑破坏性测试；不合并 PR。

## 6. 开放问题

- OQ-1 `dispatch_task` 是否需要可选 `workspace_id`（仅用于与 `card_id` 交叉校验）？本文建议不加。
- OQ-2 目标工作空间未打开（R6）：报错 `workspace_closed` 还是排队？本文建议报错、不自动打开。
- OQ-3 `cancel_task` 是否允许取消非 Grok Bot 派出的任务？Grok Bot 取消前是否一律先问 yakami，还是仅歧义时问？
- OQ-4 `client_id` 的定义：Token 轮换宽限期内新旧 Token 是否同一 `client_id`（影响重放幂等）？
- OQ-5 `connector_operations` 保存期（建议 30 天）。
- OQ-6 webhook `occurred_at` 保持 RFC3339，还是改为与工具输出一致的 unix 秒（或两者并存）？
- OQ-7 无 `agent_definitions` 记录的卡片：`created_at` 输出 `null`，还是带 `created_at_source` 标注的区域时间？
- OQ-8 `last_activity_at` 的真实来源（输出 / 工具调用）在 P4 还是 P5（与 `task.stalled` 一起）实现？
- OQ-9 等待审批时取消，Codex / Claude 是否真的关闭挂起审批（§3.2 假设），需 P4-2 实测。
