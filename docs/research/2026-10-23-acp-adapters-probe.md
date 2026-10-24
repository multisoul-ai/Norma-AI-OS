# ACP 适配器实测（2026-10-23）

目的：确认 Claude Code、Codex、OpenCode 通过 ACP 作为 Norma 后台大脑的可行性。方法：用 Node 脚本按 ACP 的 JSON-RPC 流程握手、建会话、发 prompt，记录耗时和权限请求。

## 包与启动方式

| 后端 | 启动 | 说明 |
| --- | --- | --- |
| Claude Code | `@agentclientprotocol/claude-agent-acp@0.94.0`，可执行文件 `claude-agent-acp` | `@zed-industries/claude-code-acp` 已改名并弃用 |
| Codex | `@zed-industries/codex-acp@0.16.0`，可执行文件 `codex-acp` | 包已标记为由 `@agentclientprotocol/codex-acp` 取代；本次未实测 |
| OpenCode | `opencode acp` | 自带 ACP，无需适配器 |

## 首字延迟（session/prompt 发出到第一个文字块）

| 后端 | session/new | 首字 |
| --- | --- | --- |
| Claude Code（回答一个词） | 1.7–5 秒 | 3.1 秒；带一次工具调用时 5–8 秒 |
| OpenCode（回答一个词） | 约 1 秒 | 8.7 秒 |

结论：不适合接前台对话，只做后台大脑。这印证了 `NormaSession` 要保留。

## 权限请求

- 用户的 `~/.claude/settings.json` 把 `defaultMode` 设成 `bypassPermissions` 时，适配器**不会**发 `session/request_permission`，Bash 和 Write 都直接执行。
- 在 `session/new` 的 `_meta.claudeCode.options` 里传 `settingSources: []`，并在建会话后调用 `session/set_mode` 设为 `default`，才会收到权限请求。`echo` 这类只读命令仍会被适配器自动放行，`touch` 会触发请求。
- 请求形态：`toolCall.name`（如 `Bash`）、`toolCall.kind`（`execute`）、`toolCall.rawInput.command`、`toolCall.locations[].path`；`options` 里有 `allow-once` 和 `reject`，`kind` 分别是 `allow_once`、`reject_once`。
- `_meta.claudeCode.options.disallowedTools` 可直接禁用 Write、Edit 等工具，作为闸门之外的第二道防线。

## 对设计的影响

1. 闸门必须强制 `settingSources: []` 和 `default` 模式，否则用户的全局设置会让闸门失效。已写入 `brain::backend::session_meta`。
2. ACP client 自己手写（`initialize`、`session/new`、`session/set_mode`、`session/prompt`、`session/request_permission`），不依赖 `agent-client-protocol` crate；用到的方法少，权限处理要完全可控。
3. 现有的审批代理只对带手机控制的卡片生效（`mobile_control`，由 `runtime::mobile_approval_snapshots` 提供），并同时支持 Claude 和 Codex。普通桌面卡片的权限提示仍留在终端里，Norma 无法拦截。

## 未验证

- Codex 适配器的权限请求字段。`PermissionRequest::from_params` 按 `kind` 和 `rawInput.command` 数组做了兼容，但没有用真实 Codex 实测。
- hook 等满 120 秒后 Claude Code 是否退回终端权限提示。

## 2026-10-24 补充：真实适配器发出的更新类型

用 `brain_smoke`（`SMOKE_EVENT=finished`）对 `claude-agent-acp@0.94.0` 跑一轮“Agent 已完成”事件，统计活动窗口收到的记录：

| 更新 | 次数 | 说明 |
|---|---|---|
| `agent_message_chunk` | 1 | 回复，合并后一条 |
| `tool_call` / `tool_call_update` | 3 / 9 | 工具调用与结果；本次的 `norma` 不是真实服务，结果为 failed |
| `session/request_permission` | 3 | 权限请求，闸门全部放行 |
| `usage_update` | 10 | 用量；只保留相邻同类的最新一条 |
| `available_commands_update` | 1 | 可用命令 |
| `agent_thought_chunk`、`plan` | 0 | 这一轮没有出现，不能据此断定适配器不发 |

只测了 Claude Code 适配器的一轮。Codex、OpenCode 适配器没有实测。

## 2026-10-24 补测：Codex（`@agentclientprotocol/codex-acp@2.2.2`）

- 握手、建会话、发 prompt 都正常；会话默认模式是 `agent`（Auto review），执行 `echo hi` **不发** `session/request_permission`，Norma 的闸门不起作用。
- 切到 `read-only` 后：只读命令仍在沙箱里直接执行；写文件（`touch`）和连 unix socket（`norma` 命令要连 App 的 socket）先被沙箱拦下，再发权限请求，选项为 `allow_once` / `allow_always` / `reject_once`，批准后执行成功。
- Codex 发来的 `rawInput.command` 会把整条命令再用双引号包一层，闸门需要拆开一层再检查。
- 结论：幕僚用 Codex 时会话切到 `read-only`；与 Claude Code 不同，Codex 的只读命令（读文件、列目录）不经过 Norma 闸门，只受 Codex 只读沙箱约束，可能读到项目目录之外的文件。
- 真实 App 验收：`NORMA_BRAIN_E2E_BACKEND=codex NORMA_BRAIN_E2E_ADAPTERS=<装有 codex-acp 的目录> scripts/desktop-e2e.sh brain-report` 通过。

## 2026-10-24 补测：OpenCode（`opencode acp`）

- 默认 `build` 模式下，执行命令、写文件、连 socket 都**不发**权限请求，闸门失效；`plan` 模式只是模型自己拒绝，不是强制拦截。
- 启动时注入 `OPENCODE_CONFIG_CONTENT={"permission":{"bash":"ask","edit":"deny","webfetch":"deny"}}` 后，每条命令都会发权限请求（选项 `once` / `always` / `reject`），交由 Norma 闸门判断。
- 模型列表：三个适配器的 `session/new` 回复里都有 `configOptions` 中 id 为 `model` 的一项，可用 `session/set_config_option` 切换；Codex 另有 `reasoning_effort`。
- 真实 App 验收：`NORMA_BRAIN_E2E_BACKEND=opencode scripts/desktop-e2e.sh brain-report` 通过。
