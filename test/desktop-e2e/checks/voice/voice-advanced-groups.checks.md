# 语音更多设置分组

运行 `bash scripts/desktop-e2e.sh voice-advanced-groups --visual`。蓝宝石之夜，语音指令模式。不保存 Key、不打开麦克风。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| 00-groups-collapsed.png | 分组标题行与摘要、状态标签，全部收起 | 展开的表单、重复的麦克风权限行、明文 Key |
| 01-recognition-open.png | 识别服务只显示本地 Sherpa 设置，其他分组收起 | Azure 区域/Key、A1 安装项 |
| 02-model-open.png | 理解模型展开，识别服务自动收起 | 回显的 Key |
| 03-recognition-closed.png | 识别服务行收起 | 识别模型内容 |
| 04-groups-closed-again.png | 全部收起 | 展开的表单 |
