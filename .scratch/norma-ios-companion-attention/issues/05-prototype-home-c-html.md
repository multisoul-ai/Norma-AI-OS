# 可点 HTML 原型：首页 C + 摘要 + 卡住 zoom + 空状态

Type: prototype  
Status: resolved  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

用假数据做出可点 HTML 原型，是否足以让人走通：指挥条发一句 → 决策卡带方案批一次 → 打开摘要看完成项 → 从卡住卡 zoom 进 Agent 上下文再返回 → 空状态「闸门之后」？交互是否符合 apple-design / 减切换损耗（不默认进长会话）？

视觉与交互参照：[`../references/README.md`](../references/README.md)（Cursor Mobile 简洁风）。原型须看得出：pill 指挥条、Needs Attention 式主队列、舰队非首页主舞台、可选锁屏级极简状态文案。

## Prototype

- **File**: [`../prototype/index.html`](../prototype/index.html)
- **Serve**: `cd .scratch/norma-ios-companion-attention/prototype && python3 -m http.server 8765` → http://127.0.0.1:8765/
- **Picker**: Quiet / Stacked / Hub（顶栏；`1–3` / `←→` / `R`）
- **Deep links**: `?v=1|2|3&demo=lock|empty|zoom|agents|digest`（证据截图用）
- **Archive**: 旧草图在 [`../assets/archive/`](../assets/archive/)
- **Shots**: [`../prototype/shots/`](../prototype/shots/)（`v1–v3` 变体首页 + `lock|empty|zoom|agents|digest`）

## Answer

是。假数据 HTML iPhone 壳已覆盖首页 C 主路径；三变体轴清晰；主队列不默认进长会话（zoom 才进上下文）。Phase 6 `keep <variant>` 仍待用户选定后 promote。

### 变体对照

| # | Variant | Axis | 何时选 | 代价 |
| --- | --- | --- | --- | --- |
| 1 | Quiet | Cursor 式安静列表 + 底栏指挥条 | 日常指挥、要熟悉感 | 摘要默认折叠，完成项不显眼 |
| 2 | Stacked | 顶栏指挥条 + 双区叠放（需要你 / 摘要） | 想一眼看到 digest | 首屏更高，指挥条占顶 |
| 3 | Hub | 「今日」磁贴分流再进列表 | 想先选入口再干活 | 多一步点选；勿称 Inbox |

### 验收矩阵

| 验收项 | 怎么走 | 证据 |
| --- | --- | --- |
| pill 指挥条 | 首页底（Quiet/Hub）或顶（Stacked）；placeholder 含 resume「继续：…」 | `shots/v1.png` `v2.png` `v3.png` |
| 需要你主队列 | 首页默认决策卡：提问 / 权限 / 失败 / 已核验；批「处理/批准」离队 | `v1–v3`；点批后 toast |
| 指挥条发一句 | 输入「修登录」→ 短澄清内联；含「支付/清理」→ 复杂澄清进队列；其它 →「已交给 Norma」 | 交互；`empty` 后可再发 |
| 摘要看完成 | Quiet 展开「摘要 · N」；Stacked 双区；Hub `demo=digest` 或点「摘要」磁贴 | `shots/digest.png` |
| 卡住 zoom | 卡上「看上下文」或 `?demo=zoom` → Ada 会话 + 预填下一步 →「‹ 返回」 | `shots/zoom.png` |
| 空状态 | 批空全部卡，或 `?demo=empty` →「暂时没有需要你的事」 | `shots/empty.png` |
| 全部 Agent 二级 | 导航或 `?demo=agents`；舰队非首页主舞台 | `shots/agents.png` |
| 锁屏 Live Activity | 「锁屏示意」或 `?demo=lock`；权限/失败带方案，提问短标题 | `shots/lock.png` |
| 减切换 / 不默认长会话 | 首页只有批审+指挥；会话在 zoom；Agent 列表二级 | 结构自证 |
| 词汇合规 | 需要你 / 摘要 / 指挥条 / 徽标；无 Inbox、闸门品牌、编排 | Hub 标题「今日」；源码无 Inbox |

### 未进本票（map Not yet）

配对信任、通知等级、Shadow、语音一等公民、digest 刷新节奏、薄 SPEC 终稿、真机联调。

### 下一步

用户回复 `keep Quiet` / `keep Stacked` / `keep Hub`（或 `riff <variant>`）后进入 Phase 6 promote。
