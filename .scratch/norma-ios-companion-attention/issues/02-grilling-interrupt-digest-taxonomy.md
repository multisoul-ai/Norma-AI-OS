# Norma 事件的打断 / 摘要分类法

Type: grilling  
Status: resolved  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

在 Q12=B（完成进 digest；权限/不可逆、Agent 明确提问、任务失败才 interrupt）前提下，桌上 Norma / Agent 卡片会产生哪些**具体事件类型**，各自默认 route 是 `interrupt` / `digest` / `drop`，以及首页「需要你」卡片必须带哪些字段（summary、方案、证据、来源 Agent）？

## Answer

### 默认路由表（T1）

| 事件 | 用户面 | 徽标 |
|------|--------|------|
| Agent 明确提问 / 要你选方案 | **需要你** | 提问 |
| 权限 / 不可逆（外发、合并、删除等） | **需要你** | 权限 |
| 任务失败、需人接手 | **需要你** | 失败 |
| 任务成功完成 | **摘要** | — |
| 进度心跳 / all clear | **丢掉**（UI 不展示） | — |
| 可自动预备且能核验的修复 | 核验后带方案进入需要你或摘要 | 视结果 |

### 卡片字段（T2=C）

必填：`summary`、下一步/方案（`suggested_action` 或模板）、来源（Agent 呼叫名或 Norma）、徽标。  
可选：证据（有则显示）。

### Norma 澄清（T3=C）

- **极短二选一**：指挥条附近内联，不进「需要你」。  
- **复杂澄清**：进「需要你」，来源 **Norma**，徽标 **提问**。  
- Agent 卡片提问：一律「需要你」，来源为该 Agent。

### 原型假数据覆盖（T4）

必须有：提问、权限、失败各至少一条「需要你」+ 一条「摘要」完成 + 一条带方案的核验后事项。  
「丢掉」只写在说明里，不出现在 UI。

对照底稿：[research/agent-chief-patterns.md](../research/agent-chief-patterns.md)
