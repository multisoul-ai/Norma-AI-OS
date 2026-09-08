# Norma Quest 开发协议 v1

UTF-8 JSON，通过 WebSocket `/session` 传输。服务端为权威状态持有者。

客户端建立连接后首先发送：

```json
{"version":1,"type":"hello"}
```

Mac 返回 `session.snapshot`。字段和初始样例见 [`snapshot.v1.json`](snapshot.v1.json)：

- `version`：必填，当前必须为 1。
- `sessionId`：会话标识；`sequence`：同会话单调递增的快照版本。
- `source`：`mock` 表示脚手架数据，UI 必须明确展示。
- `agents`：Agent 身份、任务身份、状态和当前步骤。
- `decisions`：每项有稳定 ID、关联任务/Agent、问题、背景与可选项。
- `terminal`：当前仅为样例 transcript，不是 ANSI 增量协议。

客户端作出决策：

```json
{"version":1,"type":"decision.resolve","requestId":"唯一请求ID","decisionId":"preview-access","optionId":"keep-private"}
```

服务端返回：

```json
{"version":1,"type":"command.result","requestId":"唯一请求ID","accepted":true,"message":"accepted"}
```

成功后再广播更新后的 `session.snapshot`。客户端应等待确认，不可在发送时直接移除待办。非法选项、已解决决策或重复请求返回 `accepted:false`；不能恢复其他任务。界面以广播快照为准，确认响应本身不代表后续任务已完成。

协议或握手错误返回：

```json
{"version":1,"type":"error","message":"unsupported_version"}
```

同一连接收到同会话的相同或较小 `sequence` 时忽略；重新连接后先清空版本游标，再接收完整快照。旧连接回调不得写入新连接状态。

开发服务最大入站消息 64 KiB；客户端拒绝超过 262144 字符的消息、超过 100 个 Agent/决策或 200 行 transcript。大规模日志/媒体需要新增增量或媒体协议，不得无限扩大此快照。

当前仅支持 `hello` 与 `decision.resolve`；没有 terminal input、shell、文件写入或远程执行命令。测试必须走真实 loopback WebSocket 验证广播和重连。
