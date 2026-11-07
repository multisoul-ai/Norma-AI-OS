# Norma：五种 Agent 指挥形态

本轮探索的问题：在保留画布与 Agent 原卡片的前提下，用户通过什么布局和操作方式，才能更轻松地指挥并行工作？

五个方案使用同一组场景与身份数据，独立保存本次页面会话中的操作状态。每次只展示一种完整布局，便于直接比较。尚未选择正式产品方向，也未修改 Rust App。

## 打开与切换

- 总入口：http://127.0.0.1:8745/docs/prototypes/norma-command-center/?v=1
- 也可以直接打开本目录的 `docs/prototypes/norma-command-center/index.html`。
- 底部切换器、数字键 1–5、左右方向键切换方案；输入时不抢占按键。
- 右上角“对比方案”查看取舍；“喜欢这个方向”把偏好保存到本机浏览器。
- EN / 中切换界面语言；用户输入保留原文。
- Esc 关闭当前弹层；⌘/Ctrl + Enter 提交当前输入表单。
- 页脚可暂停视频、重置当前方案。重置不会清空其他方案或已选偏好。

服务器停掉后，在 worktree 根目录运行：

```sh
python3 -m http.server 8745 --bind 127.0.0.1
```

## 五种形态

| 形态 | 布局与核心交互 | 适合 | 代价 |
| --- | --- | --- | --- |
| 1 指挥台 | 中间按项目组织 Agent 卡片，右侧待决定；勾选多人后派工 | 多项目并行、频繁调整方向 | 需要扫描较多信息 |
| 2 任务地图 | 目标树与阶段节点、右侧详情；查看上游依赖，明确把上下文交给助手 | 任务存在依赖、需要安排接力 | 工作很多时需要整理关系 |
| 3 决策台 | 左侧待决策、中间单件简报、右侧其他工作；采纳、修改、延期、重新打开 | 希望 Norma 先整理问题，自己只拍板 | 多件工作对照需要切换 |
| 4 调度表 | 每个 Agent 一行；当前执行、接下来、稍后分列；批量暂停和继续 | Agent 较多、需要高效调度 | 长内容需要打开会话阅读 |
| 5 协作房间 | 每个目标一个房间；目标说明、多人讨论、成员状态；发给全组或一位 | 多目标都需要持续讨论 | 全局进展分布在多个房间 |

用简单比喻理解：1 是看全班再点名，2 是看谁在等谁，3 是老师递给你待签字的作业，4 是给每个人排课表，5 是每个小组各有一张讨论桌。

## 每个方案可以怎么试

1. 指挥台：勾选 Grace 与 Kevin → 输入方向 → 预览接收者 → 下达指令 → 看两人的独立回执。实验室中同名 Grace 不会收到指令。
2. 任务地图：选 Echo → 显示上游依赖 → 请另一位 Agent 协助 → 指定 Jasper → 确认后打开 Jasper 原会话，能看到来源摘要与指令。
3. 决策台：先延期 Grace，再延期 Jasper；稍后列表可重新打开任意一条。标记 Jasper 已验收只进入“待合并”，不自动完成或合并。
4. 调度表：同时选择运行中的 Kevin 与等待中的 Iris → 暂停所选。Kevin 暂停，Iris 跳过并反馈。点击计划可编辑；同一 Agent 的计划可在“接下来 / 稍后”之间拖动。
5. 协作房间：在桌面验收房间写草稿 → 切到官网房间写另一条 → 返回。草稿各自保留；选择 Jasper 后发送，只向他的原卡片会话派发。

所有方案都可以打开原卡片对话、定位画布上的对应卡片、返回继续输入。定位期间草稿保留。

## 与当前产品的关系

- Workspace 决定上下文范围，Project 表示工作所在项目，Agent 卡片承载实际会话与执行状态。
- Norma 是协调层：汇总状态、整理决定、安排协助、把明确指令传给指定卡片。
- 会话键使用工作空间、项目、卡片、session 四项标识，显示名不是路由依据。
- 房间按目标聚合原卡片，不重新创建执行者；地图交接写入来源引用与摘要，不挪走源会话。
- 调度计划和当前运行状态分开，移动计划不会伪造任务已执行或完成。
- 五种视图拥有独立的模拟状态，便于来回比较；刷新重置演示数据，仅偏好长期保存。

## 设计原则的落地

沿用 Apple 设计技能的内容优先、直接操作与空间连续性：保留 App 默认视频、系统字体、明确的对话与画布返回路径；高频方案切换即时完成，减少动态效果时自动暂停视频。

采用 Material 的清晰层级和状态反馈：统一间距、柔和语义色、明确的接收者预览、逐目标回执；按钮具有 hover、pressed、focus、selected、disabled 等适用状态。待决定、运行中、等待、已完成分开呈现。

## 验证与边界

2026-11-05：本目录的 13 项 Playwright 交互测试通过，包括五形态渲染、输入不抢键、身份隔离、上下文交接、筛选后目标、延期恢复、执行与计划分离、房间草稿隔离、定位返回、重置隔离、视频状态、偏好持久化及 900px 英文布局。

已逐页检查 1600×1050 中文截图与 900×900 英文截图。截图在 `docs/prototypes/norma-command-center/assets/`；捕获脚本为 `docs/prototypes/norma-command-center/tests/capture.cjs`。仓库 `scripts/check-i18n.py` 通过；该脚本校验正式 App 的翻译键，原型双语另外通过浏览器验证。

本机复现测试（在 worktree 根目录）：

```sh
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules node /Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules/playwright/cli.js test docs/prototypes/norma-command-center/tests/flows.spec.cjs --workers=1 --reporter=line --timeout=30000 --output=/tmp/norma-command-verified
```

本轮是独立 HTML 交互原型：进度、依赖、回复和回执均为本地场景数据，不调用真实 Agent、后台调度器、PR 或发布系统；画布定位也在原型内模拟。真实 App 的全局快捷键和入口尚未接入，因此不运行桌面 App E2E。

源场景复用 `docs/prototypes/norma-bottom-drawer/data.js`；背景复用 `crates/app/assets/wallpapers/norma-awake.mp4`。打开文件时需保留这些相对目录关系。
