# Grok Bot 连接器 P4 实现边界

本期以 PR #106（b2025578）的 P4-1–P4-7 工单与 PR #105（bb4692a8）的修订规格为依据。P2/P3 的令牌明文存储、轮换宽限、Funnel 配置与鉴权决定保持原状。

四个工具为 `dispatch_task`、`cancel_task`、`reply_agent`、`create_card`，分别构造 `AssignAgentTask`、`CancelAgentTask`、`ReplyAgent`、`CreateAgent`。经 `execute_norma_adapter_command_with_source`、`AgentInputSource::Connector` 和 `window=None` 执行。内部只增加可选精确目标元数据；既有按名入口继续按名寻址。连接器没有独立派工管线，不执行任何 `force_untracked=true` 分支。

节点和 revision 校验位于 Adapter 执行层。签名中的 workspace/area/canvas/node/definition/birth 被逐项核对，改名不改变身份；目标变化拒绝且零输入。任务取消增加内部 `expected_task_ref` 校验，避免运行实例内的 u64 编号在重启后撞号。对外任务标识沿用 P2 的持久 `task_ref`，不把协调器局部编号当成全局标识。

连接器只查找指定区域已经保留的项目实例，不切换当前可见区域，不自动加载项目；不存在区域为 `area_not_found`，缺失实例为 `workspace_closed`，均在预留前拒绝。回复只投递已有卡片的会话，不创建任务；未加载卡片返回 `card_not_loaded`。建卡限定 Claude/Codex、自动命名、自动启用，可在一次操作中附带任务，不需要额外确认，不设数量限额。

## 命令账本

复用 `MobileCommandStore` 与原 `mobile_commands`，未新增 `connector_operations`。专用入口验证 `device_id=connector:<client_epoch>`、`owner_id=grokbot`，路由保留原 card/task/area 标识。手机仍执行原身份和 grant 校验；手机查询明确排除 `connector:` 行。

指纹包括工具名与规范化参数，省略 `allow_queue` 与显式 `true` 等价。纯参数/签名错误不占键；执行期拒绝不可变，解除条件后使用新 `operation_id`。未完成预留只返回 `outcome_unknown`，不自动重新执行。令牌轮换共享持久 epoch，吊销后新授权获得独立 epoch。连接器行超过 30 天清理，后台每小时维护，提交前也清理；手机行不清理。

默认只允许当前连接器授权的任务取消。持久设置 `ConnectorConfig.allow_cancel_other_tasks` 默认为 false，显式开启后可取消其它来源任务；P4 未新增设置 UI 控件。

## 回执与时间

MCP 输出时间统一为整数 UTC unix 秒，未知为 `null`。目录卡片的展示创建时间来自定义记录；无定义时 `null`。身份编码的 birth 计算保持原状。

派工 `accepted_at`、取消 `requested_at`、建卡 `created_at` 取预留时刻，重复调用不变。已接收/排队/提交不表示任务完成。进度包含 `created_at`、`updated_at`、`state_changed_at|null` 与最多 20 条降序事件；状态迁移由画布库中的真实 INSERT/UPDATE 触发记录，旧库不伪造历史。`last_activity_at` 在 P4 仍取任务更新时刻。只有未完成回执而无任务记录时，进度返回 `outcome_unknown` 与已知预留事件，未知任务时间为 null。

每次 MCP 写调用仅一条审计，包括来源、operation_id、参数哈希、正文哈希与前 80 字符；Adapter 内部拒绝原文只进该审计，响应使用稳定错误码。

## 验证边界

临时 SQLite、随机 Token、记录/echo PTY 与 GPUI 测试上下文验证实际工具→账本→Adapter 路径。真实 macOS App、Funnel、Grok Bot 文案与用户确认行为需在 Mac 上验收。取消不会调用 allow/deny；真实 Claude/Codex 在取消后关闭挂起审批的时机仍需 Mac/CLI 验证。
