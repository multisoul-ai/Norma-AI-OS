# Goal：Norma OS iOS 频道协作高保真原型

状态：**HTML 原型已交付；当前入口已切到工作空间九屏** · 更新于 2026-09-26  
工作分支：`codex/norma-ios-companion`（本轮文档同步分支：`cursor/norma-ios-prototype-sync-de44`）  
Goal：完成「频道协作 + Norma 实时虚拟人」的可交互 iPhone 原型；保留旧版，比较三种信息架构，并验证关键流程与动效。

## 0. 当前真相（相对原计划）

| 事实 | 证据 |
| --- | --- |
| 产品入口已从频道 A/B/C 原型切到工作空间九屏 | [`docs/prototypes/README.md`](../prototypes/README.md) 指向 [`norma-ios-workspace.html`](../prototypes/norma-ios-workspace.html) |
| 频道状态模型与派工/语音/验收规则仍在共享模型中 | [`docs/prototypes/norma-channel-model.js`](../prototypes/norma-channel-model.js)；Node 检查 [`scripts/check-norma-realtime-prototype.cjs`](../../scripts/check-norma-realtime-prototype.cjs) **PASS**（2026-09-26 本环境复跑） |
| 三种 IA（A/B/C）仍可在历史参考入口比较 | [`docs/prototypes/norma-realtime.html`](../prototypes/norma-realtime.html)?variant=A\|B\|C；底部切换器与方向键切换仍在该页 |
| 九屏交互与窄屏/触控/减少动效已由 Chrome Playwright 覆盖 | [`crates/e2e/tests/browser/norma-ios-workspace.cjs`](../../crates/e2e/tests/browser/norma-ios-workspace.cjs) **PASS**（`BROWSER_CHANNEL=chrome`，Chrome 148；2026-09-26 本环境复跑） |
| 浏览器验收截图与边界说明已写 | [`docs/prototypes/assets/norma-ios-workspace/validation.md`](../prototypes/assets/norma-ios-workspace/validation.md) |
| **未完成**：项目要求的原生 Computer Use / 真实 GUI 验收 | 同上 validation.md：`cgWindowNotFound` / `nodeRepl.fetch request failed`；本轮未启动原生 iOS |

本轮只制作**产品原型**；不修改原生 iOS、Mac Runtime、真实录音或生产数据。演示中的 Agent 回复、执行、成果和语音均为本地模拟。

## 1. 本轮交付（对照原目标）

1. ~~入口固定为 `norma-realtime.html`~~ → **已超载**：当前入口为工作空间九屏；`norma-realtime.html` 保留作 A/B/C 方案比较与频道流程参考。
2. 旧版原样归档：[`docs/prototypes/archive/2026-09-25-norma-realtime/`](../prototypes/archive/2026-09-25-norma-realtime/README.md)，含 HTML、检查脚本与浏览器用例副本。
3. 可点击完整演示（建频道/工作空间 → 共享上下文 → 派工 / `@Agent` → 任务卡 → 验收 → 归档/重开；频道内虚拟人通话与转写回执）已在共享模型 + 九屏原型中落地，并由 Node/Playwright 护栏覆盖。
4. 浏览器操作截图与验证记录见 [`assets/norma-ios-workspace/`](../prototypes/assets/norma-ios-workspace/)；明确区分 HTML 模拟与原生能力。

## 2. 已确定的产品规则

| 场景 | 原型应表现的规则 | 护栏证据 |
| --- | --- | --- |
| 想法 | 保存即建频道；只记录或讨论时不启动 Agent。 | `check-norma-realtime-prototype.cjs`「保存想法」；workspace 浏览器测试「保存为工作空间」 |
| 执行 | 明确开始后 Norma 关联项目并派工。 | 同上「开始后派工」断言 |
| 协作 | 跨项目共享目标、硬约束、消息、任务与成果。 | 「共享上下文 / 工作简报」检查与九屏设置流 |
| `@Agent` | 每次提及有明确回应；续聊不复制活跃任务。 | Node `@Codex` 断言；workspace `.chat-updated` 断言 |
| 注意力 | 收件箱只放决定、失败与成果验收。 | `inboxItems` 去重检查；workspace 收件箱搜索 |
| 语音 | 通话绑定当前频道；静音/转写/挂断后回执回频道。 | Node 语音段；workspace W07 通话流 |
| 收口 | 验收后归档，可重开；硬约束不可被 Norma 越过。 | Node accept/archive/reopen；workspace 手机视口归档流 |

## 3. 三种可比较的信息架构

| 方案 | 首屏重心 | 状态 |
| --- | --- | --- |
| A · 频道优先 | 频道列表与每件事的当前状态 | 仍可在 `norma-realtime.html?variant=A` 比较 |
| B · Norma 优先 | 大幅虚拟人与当前频道实时状态 | 仍可在 `?variant=B` 比较 |
| C · 待办优先 | 需要人决定的少量事项 | 仍可在 `?variant=C` 比较 |

**后续产品方向**：九屏工作空间原型（W01–W09 设计图 + 可交互补充状态）成为当前 README 入口；A/B/C 不再是主入口，但未删除，供方案对照。最终原生实现方向仍待用户确认。

## 4. 实施顺序与完成情况

| 阶段 | 具体动作 | 状态 | 完成证据 |
| --- | --- | --- | --- |
| 0 · 保全 | 原样复制旧入口、检查脚本、浏览器用例 | **完成** | [`archive/2026-09-25-norma-realtime/README.md`](../prototypes/archive/2026-09-25-norma-realtime/README.md) |
| 1 · 状态模型 | `Channel` / 消息 / 任务 / 收件箱 / 语音最小演示状态 + 聚焦检查 | **完成** | [`norma-channel-model.js`](../prototypes/norma-channel-model.js) + `check-norma-realtime-prototype.cjs` PASS |
| 2 · 三种高保真首屏 | A/B/C URL 与切换器 | **完成（参考入口）** | `norma-realtime.html` 内 `variants` / `variantBar` / 方向键；检查脚本匹配 `variant=A\|B\|C` |
| 3 · 关键页面 | 频道对话、上下文、派工、任务、收件箱、验收、语音、归档 | **完成** | realtime HTML + workspace 九屏交互；浏览器 e2e PASS |
| 4 · 动效与可访问性 | 状态动效、减少动态效果、触控 ≥44pt | **完成（浏览器级）** | workspace e2e：`prefers-reduced-motion`、390×844 / 375×667、44×44 断言；validation.md |
| 5 · 验收与交付 | 状态检查 + 真实浏览器点击 + 截图 + README | **部分完成** | Node + Chrome Playwright + README/截图已齐；**原生 Computer Use 未完成** |

## 5. Apple 交互与动效规范

依据项目的 `apple-design` 与 `emil-design-eng` 技能设计，并按 `review-animations` 审查动效：以内容层级、即时反馈、空间连续性和可中断性服务操作，不做持续吸引注意的装饰动画。

| 操作 | 动效与目的 | 无动画时的明确反馈 | 原型落地 |
| --- | --- | --- | --- |
| 点频道进入 / 返回 | 轻微位移与淡入 | 标题和返回路径立即更新 | workspace / realtime 换页与状态文案 |
| 打开共享上下文或操作抽屉 | 底部 Sheet，可下滑关闭 | 抽屉立即出现或关闭 | workspace Sheet 把手下滑关闭（e2e） |
| 派工与 `@Agent` 接收 | 按钮即时压下；回执可见 | 文本回执含 Agent 名与状态 | 共享模型 + chat 更新层 |
| 实时聆听 / Norma 回应 | 通话活跃时状态变化 | 聆听/静音/结束文字同步 | W07 + 静音/转写断言 |
| 待验收 → 已验收、归档 / 重开 | 状态文案与列表更新 | 收件箱数量与频道位置更新 | accept/archive/reopen 护栏 |

`prefers-reduced-motion` 下移除位移与循环类动画，保留必要状态反馈（workspace e2e 已断言 animation-name 为 `none`）。

## 6. 验收清单

- [x] 新想法生成一个频道/工作空间，显示“尚未执行”，没有任务卡片；明确开始后才出现具名 Agent 接收回执。  
  证据：`check-norma-realtime-prototype.cjs`；`norma-ios-workspace.cjs`「尚未执行 / 无 Codex 任务」。
- [x] 同一频道关联 iOS App 与 Norma API，并可查看共享目标、硬约束、任务与成果；别的频道不混入消息。  
  证据：Node「共享上下文 / 工作简报」；workspace 项目选择与简报流。
- [x] 在频道输入 `@Codex`、`@Design` 或 `@Norma` 均有回应；活跃任务续聊不复制任务。  
  证据：Node `@Codex` 续聊断言；workspace `@Codex` / `@Norma` 可见回执。
- [x] Norma 语音从当前频道进入；模拟转写、Agent 回执、挂断后记录均回到该频道，任务继续存在。  
  证据：Node 语音段；workspace W07 通话与挂断后 `.chat-updated`。
- [x] 收件箱只展示决定、失败与待验收；成果有证据与验收入口，未验收不得误显示完成。  
  证据：`inboxItems` 去重；workspace 验收后待办消失；未验收不能归档（Node）。
- [x] 用户可归档已收口频道并重开；三种方案的 URL、底部切换器和键盘切换可用（在 `norma-realtime.html`）。  
  证据：Node archive/reopen；workspace 归档/重开；realtime `?variant=` + `variantBar` + ArrowLeft/Right。
- [x] 浏览器实际检查手机窄屏、桌面预览、至少 44pt 主触控目标、减少动态效果。  
  证据：`norma-ios-workspace.cjs` + [`validation.md`](../prototypes/assets/norma-ios-workspace/validation.md)。
- [ ] **原生 Computer Use / 真实 App GUI 验收**（项目 CLAUDE.md 页面验收要求）。  
  证据缺口：validation.md 记录 Computer Use 接口复测失败；本轮仅 Chrome Playwright / Browser Use。

## 7. 文件与运行方式

- **当前入口**：[`docs/prototypes/norma-ios-workspace.html`](../prototypes/norma-ios-workspace.html)  
  启动：`python3 -m http.server 8766 --bind 127.0.0.1 --directory docs`  
  打开：`http://127.0.0.1:8766/prototypes/norma-ios-workspace.html`
- **频道 A/B/C 参考入口**：[`docs/prototypes/norma-realtime.html`](../prototypes/norma-realtime.html)?variant=A
- **旧版快照**：[`docs/prototypes/archive/2026-09-25-norma-realtime/`](../prototypes/archive/2026-09-25-norma-realtime/)
- **静态设计图**：[`docs/product-specs/norma-ios-prototypes-2026-09-26/`](../product-specs/norma-ios-prototypes-2026-09-26/)
- **检查命令**（需先在 `spikes/norma-spatial-prototype` 执行 `npm install`，浏览器测试另需本机 Chrome 或 Playwright Chromium）：

```bash
node scripts/check-norma-realtime-prototype.cjs
BROWSER_CHANNEL=chrome node crates/e2e/tests/browser/norma-ios-workspace.cjs
```

## 8. 建议的下一步（非本轮范围）

1. 用户确认：以九屏工作空间为原生 iOS 实现基线，或先开 PR 合并原型文档/护栏收尾。  
2. 在具备原生 GUI / Computer Use 的环境补做真实 App 验收（当前环境缺口已记录）。  
3. 不要在未确认前大改原生 iOS / Mac Runtime。

本文件已按 2026-09-26 仓库真相与本环境复跑结果修订；勾选项仅在具备上述证据时勾选。
