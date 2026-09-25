# 可点 HTML 原型：首页 C + 摘要 + 卡住 zoom + 空状态

Type: prototype  
Status: claimed  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

用假数据做出可点 HTML 原型，是否足以让人走通：指挥条发一句 → 决策卡带方案批一次 → 打开摘要看完成项 → 从卡住卡 zoom 进 Agent 上下文再返回 → 空状态「闸门之后」？交互是否符合 apple-design / 减切换损耗（不默认进长会话）？

视觉与交互参照：[`../references/README.md`](../references/README.md)（Cursor Mobile 简洁风）。原型须看得出：pill 指挥条、Needs Attention 式主队列、舰队非首页主舞台、可选锁屏级极简状态文案。

## Prototype (in progress)

- **File**: [`../prototype/index.html`](../prototype/index.html)
- **Serve**: `cd .scratch/norma-ios-companion-attention/prototype && python3 -m http.server 8765` → http://127.0.0.1:8765/
- **Picker**: Quiet / Stacked / Hub（顶栏；`1–3` / `←→` / `R`）
- **Flows**: 需要你批处理、摘要、看上下文 zoom、全部 Agent、锁屏 Live Activity、指挥条发送（短澄清内联 / 复杂进队列）、空状态
- **Archive**: 旧草图在 [`../assets/archive/`](../assets/archive/)

等待用户选定变体后再 `keep` promote 并关票。
