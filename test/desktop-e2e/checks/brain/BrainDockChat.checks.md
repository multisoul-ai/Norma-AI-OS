# Cmd+K 聊天路由

目标 Agent 的收件以假 claude 的 stdin 记录为准；幕僚回复以 Dock 回复区文字为准。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `dock-chat-unknown.png` | 中文「没有名为“Nobodyxyz”的 Agent」/英文「No agent named “Nobodyxyz”」 | 新卡片、英文/中文混用、无提示 |
| `dock-chat-brain-reply.png` | Dock 回复区显示「幕僚收到：今天进展如何 QQ7」 | 只有「已交给幕僚」、空白、报错 |
