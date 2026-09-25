# 对照 Agent Chief：抽出可复用的注意力协议与 UX 模式

Type: research  
Status: resolved  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

从 [Agent Chief](https://github.com/SmileLikeYe/agent-chief) 中，哪些**协议契约、路由语义、投递等级、学习/影子模式、以及「带着方案来」**可直接映射到 Norma iOS 伴侣（首页 Q9=C），哪些必须改写或丢弃？产出一份给后续 grilling/prototype 用的对照表与推荐默认值（含：事件字段、Decision route、digest vs interrupt 边界、Agent 禁止直推的强制点）。

## Answer

采用 Chief 的 **propose → Decision → 服从** 与路由 **interrupt / digest / drop**（**dispatch** 对用户呈现为带 `plan` 的投递）；伴侣 App 是投递与批审面。事件最小字段：`summary`、`suggested_action`、`source`、可选 `evidence`；`claimed_urgency` 不可盲信。完成→**摘要**；提问/权限/失败→**需要你**；心跳/all-clear→drop。Agent MUST NOT 直推用户。学习、Shadow、完整场景引擎后置。

全文：[research/agent-chief-patterns.md](../research/agent-chief-patterns.md)
