# 手动终止后语音续发

- `voice-resume-interrupted.png`：同一卡片显示 TASK INTERRUPTED，CLI 仍存活。
- `voice-resume-old-queued.png`：审批等待期间旧消息已有 queued 回执，终端没有执行旧方向。
- `voice-resume-executed.png`：同一卡片显示 EXECUTED 2；任务说明、背景、用户原话单层排列，没有英文交接模板嵌套。

硬断言：真实 PTY 恰好执行两条指令，第二条完整保留本轮原话、独有背景与约束；不新建卡片。旧消息先取得生产 queued 回执，中断后观察 3 秒仍未执行；新消息取得 sent 回执。保留 received-messages.json、cli-identity.json 与 model-requests.json 供核验。
仅使用本机脚本化模型与 CLI；ASR 最终转写经测试控制通道注入，不覆盖麦克风、真实云端模型与 TTS。
参考 apple-design：内容优先，分隔线区分任务信息，移除重复说明；复用终端原有中断反馈，不新增动效。
