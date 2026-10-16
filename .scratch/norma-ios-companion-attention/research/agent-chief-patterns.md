# Agent Chief → Norma iOS 伴侣：可复用模式对照

Sources (primary):

- https://raw.githubusercontent.com/SmileLikeYe/agent-chief/main/docs/protocol.md
- https://raw.githubusercontent.com/SmileLikeYe/agent-chief/main/docs/architecture.md
- https://raw.githubusercontent.com/SmileLikeYe/agent-chief/main/skills/openclaw/SKILL.md
- https://raw.githubusercontent.com/SmileLikeYe/agent-chief/main/README.md
- Local: `/workspace/CONTEXT.md`（Norma iOS 伴侣草案词汇）
- Local map decisions: home C、Q12=B、resume 票 03、词汇票 04

## Executive takeaways

1. **抄协议，不抄产品壳**：`propose(event) → Decision(route)` + Agent 必须服从；伴侣 App 是 **delivery/反馈面**，不是第二套判断脑（首版假数据原型可先写死 route）。
2. **五路由里先落地三用户面**：`interrupt`→**需要你**；`digest`→**摘要**；`drop` 对用户不可见。`dispatch` 对用户呈现为「带着方案的需要你/摘要」；`curate` 首版可不出 UI。
3. **事件最小字段**：`source` / `summary` / `topic?` / `suggested_action` / `evidence` / `claimed_urgency`（不可盲信）。手机卡：summary + 方案行 + 徽标 + 来源 Agent。
4. **强制不直推**：OpenClaw skill 铁律 — MUST NOT message user；只 `propose`。Norma 侧应对 Agent 卡片/运行时施加同等公民规则。
5. **可丢/后置**：完整场景引擎、Shadow 毕业、EMA/`POLICY.md` 学习、Composio、任意 shell dispatch — 写进原则或后续票，不堵 HTML 原型。

## Protocol mapping

| Chief | Norma iOS 伴侣 | 首版原型 |
|-------|----------------|----------|
| Candidate event | 桌上 Norma/Agent 产出的候选注意力 | 假数据 JSON |
| `suggested_action` | 决策卡灰字「下一步」/锁屏短方案（票 03） | 必填示例 |
| `route=interrupt` | **需要你** + 可选 Live Activity | 主列表 |
| `route=digest` | **摘要** | 折叠区 |
| `route=dispatch` | 先后台核验，再以 interrupt/digest 投递且带 `plan` | 假数据直接带方案 |
| `route=drop` | 不展示（含 all-clear） | 不出现在列表 |
| `route=curate` | 无独立 UI | 省略 |
| Delivery levels | 映射通知强度；锁屏用 interrupt 子集 | 一帧示意即可 |
| Feedback 👍/👎 | 可选；原型可先不做 | 后置 |
| Shadow mode | 观察模式 | Not yet specified |

## Recommended defaults（给票 02 用）

| Norma 事件类型（草案） | 默认 route | 类型徽标 |
|------------------------|------------|----------|
| Agent 明确提问 / 需选方案 | interrupt | 提问 |
| 权限 / 不可逆外发·合并·删除 | interrupt | 权限 |
| 任务失败需人接手 | interrupt | 失败 |
| 任务成功完成 | digest | （摘要内，无需要你徽标） |
| 进度心跳 / all clear | drop | — |
| 可自动预备的修复（有验收） | dispatch→再投递 | 视结果 |

卡片必填：`summary`、`suggested_action`（或模板下一步）、`source`（Agent 呼叫名）、`badge`、可选 `evidence`。

## UX mapping → home C

- 指挥条 = 用户 → Norma（入站指令），不是 Agent 出站。
- 需要你 = interrupt 队列；条目徽标提问/权限/失败。
- 摘要 = digest；完成默认进此。
- 全部 Agent = 二级监视，非 Chief 职责。
- Resume：见票 03（卡/条/锁屏）。

## Do not copy wholesale

- 本地 `127.0.0.1` console 当手机壳
- 把「闸门/Chief」做成用户品牌（词汇 Avoid）
- 场景引擎七态全做进首版原型
- 信任声明的 urgency 字段

## Answer（票 01 gist）

采用 Chief 的 **propose→Decision→服从** 与 **interrupt/digest/drop（+dispatch 带 plan）**；手机做投递与批审面。事件字段对齐 protocol；完成→摘要，提问/权限/失败→需要你；Agent 禁止直推。学习/Shadow/场景引擎后置。详细对照见本文件。
