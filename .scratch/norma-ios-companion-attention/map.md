# Norma iOS 伴侣：注意力优先指挥面

Labels: `wayfinder:map`  
Tracker note: GitHub issue create 403，本 effort 落在本地 markdown tracker（见 `docs/agents/issue-tracker.md` 不可写时的回退）。

## Destination

交付一份可交互的 Norma iOS 伴侣指挥原型（可点 HTML）+ 薄 SPEC：桌上 Norma OS 须在跑；首页为「Norma 指挥条 + 闸门主舞台」；注意力闸门/批审为主路径；验收写清打断分级、统一审批队列、resume 线索、Agent 只经闸门。完成汇报默认进摘要；场景感知写原则、原型可后置；真机联调与云端会话不绑死本 map。

## Notes

- **Domain**: Norma OS 伴侣面；词汇优先用根目录 `CONTEXT.md`（Norma / Agent 卡片 / Norma 回合）；注意力层对照 [Agent Chief](https://github.com/SmileLikeYe/agent-chief)（`propose` → `Decision` → 服从）。
- **Skills**: 每会话 `grilling` + `domain-modeling`；页面用 `apple-design` / `mobile-native`；原型票用 `prototype`；Chief 对照用 `research`。
- **Standing preferences**（已 grilling 锁定）:
  - Q1=C 交互原型+薄 SPEC · Q2=Norma 伴侣+注意力层 · Q3=注意力一等验收 · Q4=走开推进为主
  - Q5=假数据可点原型优先 · Q6=本机 Norma 须在跑 · Q7=打断分级+队列+resume+闸门协议 · Q8=Destination 如上
  - **Q9=C** Norma 指挥条 + 闸门主舞台 · Q10=C Norma 为主、卡住 zoom · Q11=A HTML 原型 · Q12=B 完成进 digest
- **Plan, don't do**: 本 map 只产决策与原型资产，不实现生产 SwiftUI、不接真联调。

## Decisions so far

<!-- 关闭的票：名称链接 + 一行 gist -->

## Not yet specified

- 配对/信任：手机如何证明可指挥本机 Norma（QR / 中继叙事）
- iOS 通知：silent / 横幅 / 响铃 ↔ Decision 等级与场景
- Shadow / 观察模式是否进首版原型
- 语音是否为指挥条一等公民
- digest 刷新节奏（早晚批 vs 随时开）
- 薄 SPEC 章节目录与验收条目终稿

## Out of scope

- 云端 Agent 会话（桌上可关仍指挥）
- 真机 Norma 联调作为本 map 完成条件
- 完整 SwiftUI 生产实现 / App Store 发布
- 非 Norma 的通用多 CLI 驾驶舱
- 场景引擎完整实现（仅原则）
