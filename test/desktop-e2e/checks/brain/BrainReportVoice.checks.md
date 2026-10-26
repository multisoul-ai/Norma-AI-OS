# 真实 Claude 幕僚语音汇报

前置：本机已安装 Matcha Chinese-English。本场景只写隔离数据库的 Legacy、本地 TTS、default 音色及关闭固定任务播报，不读取云端 TTS 凭据、不改变系统输出设备。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `report-identity.png` | 幕僚活动中的 ClaudeCode 真实会话，主会话已结束 | 认证错误、仍在进行中 |
| `report-delivered.png` | report 权限放行，正文汇报登录页完成、询问旧接口 | report 被拒绝、正文不可读 |
| `report-receipt.png` | report 成功回执 `ok:true / queued:true` | CLI 失败或 `queued:false` |

播音硬断言另外检查同一隔离 App 的 `node_id=0 / outcome=Report`：先收到 playback started，再收到 playback finished 且 `completed=true`。后者来自 macOS AVAudioPlayerNode 的 DataPlayedBack 回调，不以入队或固定等待代替播完。

此场景的绿灯覆盖到原生播放完成回调。交付若要求可回放的声音证据，还必须保存仅来自被测 PID 的非静音录音；静音录音或缺少系统采集权限时，声音核验仍为未通过，不能用本场景的退出码替代。
