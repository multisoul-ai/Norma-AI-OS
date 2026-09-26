# iOS 工作空间原型验收记录

日期：2026-09-26。入口：../../norma-ios-workspace.html。

| 验收项 | 结果与证据 |
| --- | --- |
| W01–W09 主状态 | 九张 1024×1536 设计图全部加载；主状态直接以原图呈现。 |
| 完整交互 | Chrome 浏览器测试通过：创建工作空间、关联项目、工作简报、@Agent、通话转写与工作空间切换、扫码模拟、收件箱搜索、验收、归档及重开。 |
| 手机尺寸 | 390×844 与 375×667 触控视口通过；短屏首页的状态筛选、创建和收件箱入口均完整位于视口内，触控目标至少 44×44 CSS px。 |
| iOS 反馈 | Sheet 可从把手下滑关闭；换页时清除旧提示；减少动态效果时停用页面和 Sheet 动画。 |
| Browser Use | 操作了设置→项目选择，以及扫码→注册两条流程；均无控制台或页面错误。 |
| Google Chrome | 使用本机 Google Chrome 的 Playwright channel 运行完整浏览器测试，结果 PASS。 |

运行命令：BROWSER_CHANNEL=chrome node crates/e2e/tests/browser/norma-ios-workspace.cjs

前置：在 `spikes/norma-spatial-prototype` 执行 `npm install`（测试通过相对路径加载该目录下的 `playwright`）。

2026-09-26 复跑（Linux / Google Chrome 148.0.7778.96，`BROWSER_CHANNEL=chrome`）：`norma-ios-workspace.cjs` **PASS**；同日 `node scripts/check-norma-realtime-prototype.cjs` **PASS**。频道 Goal 状态已同步至 [`docs/exec-plans/2026-09-25-norma-ios-channel-prototype-goal.md`](../../../exec-plans/2026-09-25-norma-ios-channel-prototype-goal.md)。

截图：W01-mobile.png 至 W09-mobile.png 九张主状态，以及 W01-compact-mobile.png、W08-compact-mobile.png 两张短屏状态；W04-updated-mobile.png、settings-mobile.png、W02-scan-mobile.png、new-workspace-mobile.png 为补充状态。

能力边界：电脑配对、Agent 执行和通话均为本地模拟，没有连接真实电脑、麦克风或仓库。Chrome 原生 Computer Use 接口复测仍返回 cgWindowNotFound／nodeRepl.fetch request failed；已用 Browser Use 和 Google Chrome 浏览器测试验证，但项目要求的 Computer Use 原生 GUI 验收仍未完成。
