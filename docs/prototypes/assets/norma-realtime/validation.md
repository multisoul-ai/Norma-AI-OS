# Norma 对话与多仓协作原型验证

2026-09-22 · Chrome / Computer Use · 本地 HTML 模拟流程。

入口：http://127.0.0.1:8766/prototypes/norma-realtime.html
启动：`python3 -m http.server 8766 --bind 127.0.0.1 --directory docs`

| 检查 | 结果 |
|---|---|
| Norma 首页只保留大尺寸人物为主内容；对话、想法与 Mission 入口收进底部抽屉 | 通过 |
| 执行空间使用 iOS 底部 Sheet，不打开网页式居中弹窗 | 通过 |
| 扫码器 → Studio Mini 身份与权限确认 → 用户允许 → 已注册电脑从 2 台增至 3 台 | 通过 |
| 输入想法 → 只记录为 Thought，不创建任务、不启动 Agent | 通过 |
| “先放着”后想法仍可找回；明确升级后才创建 Mission | 通过 |
| 派工前显示 Agent Card 路由：Daisy / Codex、Ethan / Claude、Felix / Codex，并明确标记新建 | 通过 |
| Work 默认汇总所有项目的 Agent Card，项目筛选只改变视图、不删除任务 | 通过 |
| Work 收起桌面式 Mission 仪表盘，只主动突出需要决定或验收的事项 | 通过 |
| 每张卡片显示稳定名称和 Norma 概括的当前任务标题 | 通过 |
| 点击卡片使用手机层级导航进入全屏工作空间；想法、派工与成果也使用手机 Sheet | 通过 |
| Agent Card 可查看目标、三步方案、当前步骤与过滤后的关键动态 | 通过 |
| 向 Alan 定向补充后回到原卡片，未复制任务；返回 Work 保留卡片集合 | 通过 |
| Inbox 决定返回原 Agent Run，普通进展保持安静 | 通过 |
| 桌面预览 402×874（iPhone 17 Pro）；720 窗口等比缩到 273×594，比例仍为 402/874 | 通过；[满高](iphone17-frame-1100.png) · [720](iphone17-frame-720.png) |
| Mac 离线保留想法，重连不自动执行，明确派工后显示接收回执 | 通过 |
| Web 成果验收进入“已经确认” | 通过 |
| 桌面成果构建说明与稍后查看 | 通过；构建和录屏为说明入口 |
| 桌面 1280×720：手机外框、导航及陪伴页主文案完整可见 | 修复后通过 |
| 手机讨论/发送按钮不小于 44×44 CSS px | 修复后通过 |
| Work / Inbox 不显示 Norma 通话 Dock；Agent Card 不暴露原型模拟按钮 | 通过 |
| 浏览器控制台错误 | 0 条 |
| Node 状态逻辑检查、全部内联脚本及浏览器用例语法 | 通过 |

当前设计目标是让一个人跨仓库指挥多个 Agent，同时减少项目、终端和会话之间的注意力切换。产品流程收敛为 `Thought → Mission → Agent Card → Plan → Run → Artifact`；Work 汇总全局，Inbox 只承接例外，Agent Card 是可查看方案、定向对话和随时接管的手机工作空间。

实际检查的是 HTML 原型。没有验证真实麦克风、实时模型、Mac 派工、安装包、APNs 或原生锁屏音频。

历史截图仅用于对照旧方向：[陪伴](companion-desktop.png) · [对话](conversation-desktop.png) · [调度](dispatch-desktop.png) · [手机通话](companion-mobile.png) · [锁屏示意](lock-mobile.png)。
