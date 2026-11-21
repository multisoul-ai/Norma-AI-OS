# Norma：三种轻量画布协作原型

这是对 Slack 式协作页面的轻量探索。三套都以现有画布为主，移除左侧常驻抽屉，复用当前默认的 `norma-awake.mp4` 视频；不引入频道页或管理首页。

| 方案 | 交互轴 | 适合的使用习惯 | 代价 |
| --- | --- | --- | --- |
| 胶囊＋任务浮窗 | 按需出现 | 平时看画布，需要决定时再展开 | 查看全体进度要多点一次；浮窗会覆盖部分卡片 |
| 底部任务条 | 随时切换 | 多项任务来回讨论，每项有独立草稿 | 常驻占用底部空间；任务很多时需要横向滚动 |
| 卡片聚焦 | 单项深入 | 想同时看 Agent 现场和这件事的讨论 | 全局工作需要返回全景，或打开状态胶囊查看 |

## 像五岁一样理解关系

画布是工作室，卡片是每位同事的桌子，Work 是桌上的一件事。Norma 是帮你记住目标、追踪进展和整理建议的搭档。你不用巡回问每位同事“做到哪了”；需要决定时，Norma 带上问题和推荐方案来找你。想亲自看看，就点定位去那张桌子。

三种入口改变的是“你怎么找到这件事”。任务本身、执行 Agent、讨论、草稿、进度和人工待办共用一份状态。全局讨论有单独的上下文。

## 可体验的交互

- 右上状态胶囊：切换“需要你 / 全部工作”，查看阶段和推荐方案，直接采用或定位。
- 卡片：拖动标题栏；打开该项讨论；模拟接管 / 交还。
- 讨论：独立草稿与阅读位置，发送消息、展开步骤、调整建议、查看依据。
- 底部任务条：点击切换，再次点击收起；草稿以小点提示。
- 聚焦：上一项 / 下一项，返回原画布视角。
- 画布：空白处拖动、滚轮平移、⌘ / Ctrl + 滚轮缩放、工具栏放大缩小和适配全体卡片。
- 比较器：点击切换；非输入状态可用 `1 / 2 / 3` 或左右方向键。URL 中的 `?v=1/2/3` 保留方案选择。
- 中英文切换、视频播放 / 暂停、键盘焦点、Escape、减少动态效果。

## 业务边界

人工待办只包括回答、授权、验收。采用答复或授权后，工作进入“继续执行”；通过验收后进入“待合并”。它们不会直接成为“已完成”，也不会自动合并或发布。

页面只用示例数据和本地回执，无网络请求到真实 Agent，不会派工、部署或接管实际 PTY。刷新页面会重置示例任务，方案选择保留在 URL。正式 Rust App 未改动。

## 设计原则落地

依据项目的 Apple Design 与 Emil Design Engineering 技能，画布保持稳定的空间关系，浮层按需出现；拖动直接跟手，放大以视点为中心，退出聚焦精确恢复视角。文字与内容优先于装饰，状态使用柔和语义色。按钮有 hover、pressed、focus、selected 和 disabled 反馈。原型比较器沿用项目标准实现，因任务条占用底部，比较器放在顶部。

## 打开

从 worktree 根目录启动：

```bash
python3 -m http.server 8745 --bind 127.0.0.1
```

访问 `http://127.0.0.1:8745/docs/prototypes/norma-light-canvas/?v=1`。也可直接用浏览器打开本目录的 `index.html`；相对视频路径依赖本仓库目录结构。

## 验证

原型测试放在自身目录。使用仓库已安装的 Playwright 与本机 Chrome：

```bash
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules \
node /Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules/playwright/cli.js \
test docs/prototypes/norma-light-canvas/tests/flows.spec.cjs \
--workers=1 --reporter=line --timeout=45000 --output=/tmp/norma-light-canvas-tests

python3 scripts/check-i18n.py
```

9 项浏览器测试覆盖三套方案、任务隔离、跨方案状态、验收语义、定位精度、接管、编辑、视频、键盘、直接操作、视角返回、阅读位置、减少动态效果，以及 1440 / 1280 / 1024 / 768 / 390px 的双语布局。

截图在 `assets/`。复现截图：

```bash
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules \
node docs/prototypes/norma-light-canvas/tests/capture.cjs
```

截图将视频暂停于第 3 秒，仅用于可重复的视觉比较；原型默认循环播放，减少动态效果时默认暂停。此轮只改变独立 HTML 原型，因此未运行与改动无关的 Rust 构建或真实 App desktop-e2e。
