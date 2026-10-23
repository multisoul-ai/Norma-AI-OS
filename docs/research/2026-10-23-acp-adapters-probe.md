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
