# Norma Spatial · VR 交互原型

这是以头显体验为主要对象的一次性 WebXR 原型。核心设计问题：**如何一边指挥多位 Agent，一边持续看见任务进度，并在空间里阅读和操作任何类型的成果？**

- [VR 主入口](http://localhost:3000/?mode=orbit)：独立进度面板 + 当前内容面板 + 按需出现的 Norma 对话。
- [二维辅助编辑](http://localhost:3000/?mode=workspace)：自由画布、CLI 输入、便签、添加内容与标注。
- 旧 A/B/C 仅通过 `?compare=1&variant=A` 进入，作为历史比较；默认不显示变体切换器。

仓库根目录启动：

```sh
npm --prefix spikes/norma-spatial-prototype ci
npm --prefix spikes/norma-spatial-prototype run dev -- --host 0.0.0.0
```

需要 Node 22.13+。当前环境已启动在端口 3000。

## VR 中的完整体验

1. 默认就有正在执行的模拟任务。左侧独立面板持续显示总体百分比、运行人数、Codex / Claude / 验证的状态和当前步骤。
2. 点击 Agent 行，中央切换到对应 CLI 或验证结果；进度面板保持可见。CLI 顶部可暂停或继续该会话，进度面板底部可控制整个团队。
3. 抓住进度面板标题单独移动它；抓住中央标题移动内容窗口。它们是独立的 Three.js 平面，互不绑定。
4. 「··· → 呈现内容」切换 Codex / Claude CLI、diff、审查报告、实时图表、网页与图片、三维对象或自由画布。三维对象可抓取旋转。
5. 「对话」打开空间内 Norma 面板。开始语音后持续识别，回复期间暂停识别避免回声，播报完自动继续聆听；点击可打断播报。没有麦克风时，可点击明确标记的演示指令来开始、暂停、继续和切换成果。
6. 「画笔」进入标注画布，绘制、擦除、撤销，再交给 Norma。标注带上关联内容；提交后恢复选择模式。
7. 「进入 VR」请求 WebXR 头显会话。浏览器指针预演相同的命中和抓取路径，拖动空白模拟环顾。窗口远近、大小、固定与重新居中位于「··· → 摆放与大小」。

## 产品判断

用户反馈已经明确了两点：按钮不能铺满视野，执行进度也不能藏在菜单里。因此常驻的是简洁的任务状态面板和当前阅读内容；低频动作按需展开。自由画布只是内容组织方式之一。

Apple 原则落实在独立空间层级、内容优先、抓取时跟手、明确的按下反馈和渐进展开；没有自动镜头运动。默认四项空间工具，不因成果类型增加整排常驻按钮。

## 模拟与真实能力的边界

- 实际实现：Three.js 三维场景、独立窗口移动、WebXR session/select/grip 输入、模型旋转、画笔和共享画布状态。
- 模拟数据：CLI transcript、任务执行、分支、diff、测试结果、报告、网页和 Agent 进度。不会启动真实 Coding Agent 或修改真实仓库。
- 语音：Web Speech ASR 与 TTS 适配；对话理解来自本地规则。识别和播报交接通过协议测试，真人麦克风与头显佩戴仍未实机验收。
- 内容面板展示多种代表性 mock 类型，不代表已实现任意文件导入、任意网站嵌入或真实数据连接。
- 状态仅在当前页面内存中保留；刷新重置。远程头显需要可信 HTTPS，当前没有在线部署。

## 验证

```sh
npm --prefix spikes/norma-spatial-prototype test
npm --prefix spikes/norma-spatial-prototype run typecheck
npm --prefix spikes/norma-spatial-prototype run build
npm --prefix spikes/norma-spatial-prototype run test:e2e
```

24 项聚焦测试通过。当前 VR 主路径通过 8 项真实浏览器断言，覆盖独立进度、点名暂停、CLI 切换与多种成果呈现；共享 runner 为 `tests/browser/norma_vr.mjs`。另有 20 项二维辅助编辑与 3 项 VR 入口断言，以及旧比较页 21 项断言。当前验收使用 Computer Use 操作实际浏览器；CI 入口需要 Playwright Chromium。

三维实体旋转、进度窗口独立抓取和空间对话中的演示指令另经 Computer Use 操作确认。证据与截图保存在忽略跟踪的 `outputs/`。

当时 Rust `cargo test -p e2e` 被 Xcode Metal Toolchain 缺失阻断；该旧 crate 现已移除，本原型没有修改 Rust/GPUI。

### 玻璃画布与右侧决策

空间画布、进度与工具栏使用环境透光、磨砂与边缘高光，文字保持独立清晰。右侧「需要你决策」显示等待用户的测试范围和成果验收，选择后解锁对应任务；无待办时收成小卡片。运行 `npm test` 可验证 24 项状态与交互测试，VR 浏览器 runner 更新为 12 项决策与执行断言。

### 多任务待决策

默认提供 5 个独立任务待办，每页 4 项。右侧保留列表，点选任意项在中央展开详情；处理后只继续对应任务，其余待办保留。`npm test` 当前 26 项通过；VR 队列 Computer Use runner 完成 13 项断言。完整头显舒适度验收尚未进行。
