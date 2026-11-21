# Norma 全局任务收件箱

2026-11-05：根据用户选定的两张参考图，将指挥入口收敛为一个安静的任务页面。保持深灰背景、居中输入、平面任务列表；选中任务后，左侧保留任务，右侧展开对应 Agent 卡片的原会话。

当前进一步收敛为双窗口：Norma 任务窗口与画布窗口独立显示、关闭和导航，共享任务状态、原会话消息及草稿。任务窗口不叠在画布上，也不加载背景视频；画布继续保留当前视频背景。

## 体验入口

- **推荐入口，双窗口画布**：http://127.0.0.1:8745/docs/prototypes/norma-inbox/?window=canvas
- 独立 Norma 窗口：http://127.0.0.1:8745/docs/prototypes/norma-inbox/?window=tasks
- 旧版嵌入式首页：http://127.0.0.1:8745/docs/prototypes/norma-inbox/
- 直接体验对话：http://127.0.0.1:8745/docs/prototypes/norma-inbox/?task=login
- 从画布开始：http://127.0.0.1:8745/docs/prototypes/norma-inbox/?scene=canvas

在双窗口入口点击画布底部 Norma，或者按 `⌘⇧Space`（或 Ctrl+Shift+Space），会真正打开另一个浏览器窗口。再次呼出复用已有窗口。选择任务不会移动画布；点击定位才唤起画布并居中显示目标卡片。任务窗口右上角的“画布”入口只切换窗口，不改变画布位置。

Esc 依次退出选择器、任务会话、任务窗口。关闭任务窗口保留画布，重新打开恢复所选任务、筛选和草稿。⌘/Ctrl+Enter 提交当前表单。浏览器可能按用户设置将新窗口打开为标签页。

在 worktree 根目录运行静态服务器：

```sh
python3 -m http.server 8745 --bind 127.0.0.1
```

旧版嵌入原型也可直接打开 `docs/prototypes/norma-inbox/index.html`。双窗口共享数据需要使用上述同源 HTTP 地址。

## 两个页面状态

**未选择任务**：标题与输入框居中，下面按“需要你 / 处理中 / 等待中 / 已归档”组织工作。任务行同时给出标题、工作空间、项目、最新进展及 Agent 身份。已回答结果在需要你分组中等待查看；归档操作可撤销，不删除原会话。

**选择任务后**：大标题收起，左侧成为任务列表，右侧显示原对话、Norma 的建议、可展开的执行步骤和持续沟通输入框。切换任务时，草稿、消息及阅读位置分别保存。

## 工作空间、项目与 Agent

- 顶部的查看范围可以跨全部工作空间，也可以缩小到某一工作空间或具体项目。
- 新输入框中的项目是新工作的归属，独立于查看范围；筛选不会偷偷修改派工目标。
- 选择 Norma：新建项目内的工作，先等待拆解。尚未创建执行卡片，因此定位入口不可用。
- 选择已有 Agent：指令直接进入指定原卡片的会话，不额外创建同名工作或会话。
- 会话使用 `workspace / project / card / session` 的完整标识。实验室与 Norma OS 中同名 Grace 的消息和草稿互相隔离。
- 已打开的对话不因筛选变化而消失；若不在范围内，显示可恢复范围的提示。

用简单比喻理解：画布是大家工作的桌子，这个页面是收件箱。Norma 帮你收集答案和需要决定的问题，点开一封就能继续和原来那张桌子上的同事沟通。

## 已实现的交互

1. 快捷键和 Dock 呼出 / 收起，保留任务上下文。
2. 搜索工作、筛选工作空间和项目；菜单支持搜索、方向键、Enter、Esc，键盘浏览与已选值分离。
3. 给 Norma 创建工作，或直接给指定 Agent 卡片下指令。
4. 左侧切换任务、右侧继续对话；各任务独立草稿与消息。
5. 定位原卡片，在画布内继续输入 / 发送，再返回任务页；两个入口共用会话。
6. 采用建议后继续执行，验收只进入待合并，避免伪造最终合并。
7. 归档与移回收件箱；已归档会话仍可继续。
8. 模拟新进展只更新对应任务，不抢走当前任务或草稿。
9. 添加本地附件名称与移除；仅保存文件名和大小，不读取内容、不上传。
10. 中英文、900/600px 小窗口、减少动态效果、背景视频暂停，以及画布平移与缩放。

## 设计落地

沿用 Apple Design 的内容优先和连续上下文：同一任务从列表进入对话，再定位卡片；关闭页面回到原画布。页面使用参考图的纯深灰内容面，原视频保留在画布和页面边缘，不穿透正文。

选择器遵循项目 Material 规范：紧凑触发器、按需搜索浮层、单行操作和双行信息、独立的键盘浏览与已选勾选，以及明确的 hover / pressed / focus / disabled 反馈。高频任务切换即时完成，页面入场 160ms，减少动态效果时取消动画并暂停视频。

## 验证

12 项 Playwright 流程测试通过：原有 8 项交互回归，以及独立双窗口、双向消息与草稿同步、关闭重开与复用窗口、两窗口同时向独立会话写入的 4 项测试。

已查看首页、对话、工作空间菜单、画布定位、已回答任务和小窗口截图。截图在 `docs/prototypes/norma-inbox/assets/`。仓库 `scripts/check-i18n.py` 通过；原型双语另经浏览器验证。

独立窗口截图为 `docs/prototypes/norma-inbox/assets/standalone-tasks.png` 和 `docs/prototypes/norma-inbox/assets/standalone-canvas.png`，由 `docs/prototypes/norma-inbox/tests/capture-windows.cjs` 从真实的两个浏览器页面对象捕获。

测试与截图命令（在 worktree 根目录）：

```sh
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules node /Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules/playwright/cli.js test docs/prototypes/norma-inbox/tests --workers=1 --reporter=line --timeout=30000 --output=/tmp/norma-inbox-all-verified
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules node docs/prototypes/norma-inbox/tests/capture.cjs
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules node docs/prototypes/norma-inbox/tests/capture-windows.cjs
```

## 实现边界

这是独立 HTML 原型，消息、进展、Agent 回执与画布均为本地模拟。没有修改正式 Rust App，也没有连接真实 Agent 或 PTY。

双窗口模式将演示数据保存在本机浏览器，并通过同源存储事件同步；字段增量与独立消息 ID 在 Web Lock 内合并，避免把另一窗口的新消息覆盖掉。视角、当前选择、筛选和阅读位置属于各自窗口；不会因为另一边切换任务而跟着跳转。URL 的 `demo` 参数可隔离不同演示数据，默认是 `shared`。旧版嵌入模式依然在刷新后重置。

快捷键当前只在浏览器页面获得焦点时生效。真实 macOS 全局呼出需在正式 App 中实现窗口激活与系统快捷键注册，并在 Settings → Keyboard Shortcuts 提供录制和恢复。当前仓库的 `⌘E` 属于语音交互，因此本轮原型另用 `⌘⇧Space`，不覆盖已有习惯。

复用的场景来源是 `docs/prototypes/norma-bottom-drawer/data.js`；当前背景视频是 `crates/app/assets/wallpapers/norma-awake.mp4`。本轮未改原生页面，因此未运行无关 Rust 构建和 desktop-e2e。
