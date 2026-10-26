# 真实 Claude 幕僚汇报

本场景验证真实模型 → 权限闸门 → CLI → App → 幕僚活动。隔离数据没有 TTS，明确要求跳过语音及 `queued=false`，不验证实际播音。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `report-identity.png` | 幕僚活动展开主会话，显示 ClaudeCode 与真实 session_id | 空白正文、认证错误、仍在进行中 |
| `report-delivered.png` | 活动里可读的 `brain report --text` 命令；正文汇报登录页完成、询问旧接口；权限“放行” | 正文被遮挡或截断到不可读、report 被拒绝 |
| `report-receipt.png` | report 成功结果含 `ok:true` 与 `queued:false` | CLI 失败、把无 TTS 写成已入队或已播音 |
