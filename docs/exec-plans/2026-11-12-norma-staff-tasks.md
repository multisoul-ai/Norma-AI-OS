# Norma 幕僚模式：任务 / Worker 数据结构与落地方案

状态：草案，经 /grill-me 访谈确认。原型：`docs/prototypes/norma-staff-mode/`。

## 1. 目标与范围

把 Norma 作为「幕僚（master-agent）」：用户开一个会话 = 一个**任务**，任务由多张智能体卡片（**Worker**）协作完成。提供与画布并列、可全屏的「幕僚」一级视图：左侧按状态分组的任务列表，中间与 Norma 的对话，右侧复用现有右抽屉（标签栏 + 文件名子栏 + 渲染内容）。

不做（首版）：Worker 依赖编排；卡片跨任务移动；自动给存量卡片分组；iOS。

## 2. 词汇

| 术语 | 含义 |
| --- | --- |
| 幕僚 / master-agent | 现有 `brain` crate，ACP 后台会话驱动的协调者，不亲自写代码 |
| 任务 Task | 一次与幕僚的会话，有独立 ACP 会话，取代旧的 `Goal` |
| Worker | 任务派出的执行者，对应一张智能体卡片；完成后可回收，之后用绑定的引擎会话再唤起 |
| 回收 | 关闭卡片与进程，保留 Worker 与 `engine_session_id` |
| 未归属卡片 | 没有任务归属的存量卡片 |

## 2.5 整体架构

可视化版本：`docs/design-docs/2026-11-12-norma-staff-architecture.html`（浏览器打开）。

```text
app / ui（GPUI）
  画布 | 幕僚视图（新增：三栏·全屏·⌘E）| 右抽屉（复用）
  brain_host（扩展）
     ↓ 用户消息                      ↑ 任务快照
brain（无 gpui / storage / runtime）
  事件路由（node_id 反查归属）──→ 任务会话池（新增：每任务一条 ACP 会话）
        └────────────────────→ 分诊会话 · 审批会话（现有）
  TaskStore（取代 GoalStore）←── Goal 迁移（新增）
     ↓ 经 norma CLI 行动                ↑ hook 事件 + dispatch_id
norma（协议 + CLI）：Task / Worker 类型 · task · assign --task（新增）
     ↓ 创建 / 回收 / 唤起
Runtime：Worker 卡片（PTY）→ norma-hook ─────────┘

任务会话池 ──ACP──→ 适配器（Claude Code · Codex · OpenCode）
TaskStore ──原子写──→ ~/.norma/brain/projects/<ws-hash>/tasks.json
```

依赖方向不变：`brain` 不依赖 gpui / runtime / storage / ui / agent，只通过 `norma` CLI 行动；写入只有 brain 一个入口。

## 3. 数据结构

```
Task {
  id, workspace_id, project_id, title, created_at, updated_at,
  acp_session_id,            // 幕僚侧独立会话，首次对话时才创建
  phase: Active | Merging | Done,   // Norma 推进，用户可改
  messages[],                // 唯一数据源，会话重建时重放
  workers[]
}
Worker {
  agent, engine, role,
  card_id?,                  // 回收后为空
  dispatch_id,               // 当前一轮工作的编号，事件必须带回
  engine_session_id,         // 再唤起时恢复
  state: Active | Reclaimed | Failed | Interrupted
}
```

- 一个 Worker 只属于一个任务，不跨任务移动；任务可有 0 个 Worker（等待拆解）。
- 每次派活或唤起生成一个 `dispatch_id`，随启动环境变量 `NORMA_DISPATCH_ID` 下发；Worker 记录当前 `dispatch_id`。卡片不自报任务或工作空间，宿主用 hook 的 token + `node_id` 反查 `Task.workers` 得到任务与工作空间（沿用 `BrainScope` 与 `--workspace --route`）。不改 storage；归属以 brain 的 `Task.workers` 为唯一真相。
- 任务数据落在 `~/.norma/brain/projects/<workspace-id-hash>/`，与提议、决策日志同级，沿用原子替换快照与失败重试。

## 3.5 数据关系

```text
WORKSPACE 1─n PROJECT 1─n TASK
TASK 1─n WORKER            派出
TASK 1─n MESSAGE           记录（role: you|norma|event；含 dispatch_id）
TASK 1─0..1 ACP_SESSION    独立会话
WORKER 1─0..1 CARD         当前对应（回收后为空）
WORKER 1─0..1 ENGINE_SESSION  再唤起时恢复
```

- 一个 Worker 只属于一个任务，不跨任务移动。
- 工作空间归属不存在卡片上：宿主用 hook 的 token + `node_id` 反查 `Task.workers`。

## 4. 状态

展示状态由 `phase` 与存活 Worker 推导，已回收 Worker 不参与：

1. `phase = Done` → 已完成；`phase = Merging` → 待合并。
2. 否则有 Worker 等你决定，或有待你处理的建议 → 需要你。
3. 否则有 Worker 在执行 → 处理中；其余 → 等待中。

`phase` 由 Norma 判断并推进（M1 即自动推进），用户可改；用户改过的 `phase` 不被 Norma 立即覆盖。已完成任务里用户发新消息或唤起旧 Worker，自动回到 Active 并在对话里记一条「已重新打开」（用户动作可覆盖用户设置的 phase）。

## 5. 分派、结果回传与恢复

分派：
1. 左栏「把下一件事，交给 Norma」建空任务。
2. 幕僚在对话里给出提议（拆成 N 个 Worker 及 Agent、引擎、目录），沿用现有 Proposal；用户采用后才执行。
3. 执行通过 `norma brain assign --task <id>` 创建或唤起卡片并生成 `dispatch_id`。
4. 派完活这一轮即结束，不阻塞、不轮询；结果作为新消息再唤醒会话。

结果回传（沿用 hook，借鉴 OpenBot 的先落盘再投递）：
1. Worker 完成时，`norma-hook` 上报 `AgentFinished { agent, outcome, excerpt, from_result }`，并带 `dispatch_id`。
2. 宿主用 token + `node_id` 反查任务与工作空间；事件的 `dispatch_id` 与 Worker 当前值不一致则丢弃（只留历史，不触发幕僚）。
3. 事件先写入 `Task.messages`（以 `dispatch_id` + 事件类型为去重键），成功后再唤醒任务会话。
4. 任务会话一次只处理一个事件（串行）；会话忙则排队，队列深度有上限，超出合并成一条摘要；重启后继续处理。
5. `from_result = false` 时视为「未取得结果」，由幕僚追问，不当作完成。
6. 结果与通知事件不再要求回信；每个任务限制自动派活次数（可配置），超限转人工。

失败与中断：Worker 失败、中断或引擎异常退出，都会生成终态消息进入任务对话，Worker 状态记为 Failed / Interrupted，任务显示「需要你」。幕僚提议重派或回收，由用户决定；不自动重试，避免重复有副作用的写操作。

其他：
- 卡片事件无 task 归属时走常驻分诊会话；审批仍走现有审批会话。
- 任务会话已回收（冷）时，事件触发会话重建并重放 `Task.messages`。
- 用户在画布上直接关闭任务内的卡片，视为回收：保留 `engine_session_id`，Worker 变 Reclaimed。
- 完成后 Norma 提议回收与推进状态。

重启对账：启动时对「进行中」的 Worker 先查引擎会话与卡片的真实状态：已结束则补结算；查不到则标记 Interrupted 交给用户。不自动重发可能已执行过的操作。

### 分派与结果回传时序

```text
你 ──新建任务，说目标──→ 任务会话（幕僚）
任务会话 ──提议：N 个 Worker──→ 你
你 ──采用──→ 任务会话
任务会话 ──assign --task（生成 dispatch_id）──→ norma CLI
norma CLI ──创建 / 唤起卡片──→ App / Runtime
App / Runtime ──启动，NORMA_DISPATCH_ID──→ Worker 卡片
Worker 卡片 ──hook：AgentFinished + dispatch_id──→ App / Runtime
App / Runtime：node_id 反查任务与工作空间，核对 dispatch_id
App / Runtime ──先写 Task.messages，再唤醒会话──→ 任务会话
任务会话 ──汇报 / 追问 / 需要你决定──→ 你
你 ──确认回收──→ 任务会话 ──reclaim（保留 engine_session_id）──→ norma CLI
```

### 状态流转

```text
任务 phase
  Active ──Worker 全部待验收──→ Merging ──你验收 / 归档──→ Done
  Done ──新消息或唤起 Worker──→ Active

Worker state
  Active ──回收 / 关闭卡片──→ Reclaimed ──唤起（engine_session_id）──→ Active
  Active ──失败──→ Failed
  Active ──中断 / 重启查不到──→ Interrupted
```

## 5.5 与现有汇报的关系

现有汇报按工作空间统一做：`Router` 去抖、静默池、定时摘要和回来时概览，由常驻主会话播报；`triage` 是纯函数，在交给任何会话之前先判断立即汇报还是静默。任务会话不能破坏这套机制：

1. `Router` 与 `triage` 保持在最前面，规则不改。已核对 `scoped_router.rs`（dev 上存在，PR #190 的 worktree 较旧没有）：`ScopedRouter` 按 `BrainScope{workspace_id, route_id}` 各持一个 `Router`，`triage` 在 `Router::push` 内执行，产出的 `Action` 只有 `Lane::Main | Approval` 两条车道。任务会话池因此要新增车道 `Lane::Task(task_id)`，在 `ScopedRouter` 之后按 `Task.workers` 把 `Action` 改投任务会话；`BrainEventKind::AgentFinished` 目前没有 `node_id` / `dispatch_id` 字段，需扩展。
2. 所有事件先写入 `Task.messages`。只有「立即汇报」的事件，或用户正打开该任务时，才唤醒任务会话；静默事件只落盘，攒到静默池扫描时再合并处理，不逐条唤醒。
3. 任务会话只负责该任务里的对话，不播报。语音汇报、定时摘要、回来时概览仍由主会话负责。
4. 主会话提示里的「最近 5 条 Goal」由各任务摘要（标题、phase、需要你的 Worker）取代。
5. 失败：语音立即汇报（不变），任务页显示「需要你」。
6. Interrupted：已决定保持静默。`triage` 不改规则，任务页显示「需要你」，回来时概览会提到。

## 6. 与现有架构的关系

- `brain`：新增 Task、Worker、任务会话池；仍不依赖 gpui / runtime / storage / ui / agent，只经 `norma` CLI 行动；Task 写入只有 brain 一个入口。
- `norma`：Task / Worker 协议类型；CLI 新增 `task create/list` 与 `assign --task`。
- `app` / `ui`：经现有 `brain_host` 订阅任务快照渲染，用户消息经 `brain_host` 发给 brain；新增「画布 / 幕僚」一级视图；全屏 = 隐藏顶栏最大化；右栏复用现有右抽屉组件。
- 旧 `Goal`：一对一迁移成单 Worker 任务；存量无任务卡片在列表顶部显示「未归属卡片」入口，由用户手动归入任务。
- 用户可见文案走 `ui::i18n`，同步 `crates/ui/locales/` 中英文键。

## 7. 风险与待验证

- OpenBot 仓库使用 PolyForm Noncommercial 许可，只参考其机制（先落盘再投递、回复关联键、失败终态、重启对账），不复用代码。
- 适配器核对结果（只读调研）：brain 现在每个 `AcpSession::start` 起一个新适配器进程、只发一次 `session/new`，从未调用 `session/load` / `session/resume`（`crates/brain/src/acp/client.rs:55-77`）。Codex（codex-acp 2.5.0）多会话并存与按 id 恢复均支持（隔离为代码推断，未实测；brain 固定的是 2.2.2，未核对）；OpenCode 恢复支持（仅二进制 grep，未实测）；Claude Code 未装适配器，未知。结论：采用「每任务一进程」，`initialize` 返回值需保留，`loadSession` / `sessionCapabilities.resume` 为真才恢复，否则新建会话并重放 `Task.messages`。
- 适配器对「一任务一会话」的支持仍需用真实 Claude Code / Codex / OpenCode 实测；热会话要有数量上限与回收策略，避免进程与 token 成本随任务数增长。
- 再唤起依赖各引擎 CLI 的会话恢复能力，需逐个引擎确认；不支持的引擎需降级为「新会话 + 摘要」。
- Norma 自动推进 `phase` 可能误判完成，须与「拿不准转人工」的既有原则一致。
- 已核对 `scoped_router.rs`：需新增 `Lane::Task`，并扩展 `AgentFinished` 携带 `node_id` / `dispatch_id`（见 5.5），实现时注意 `crates/brain/src/` 内 `task_outline.rs` 已有「staff task」的另一含义，类型命名避免冲突。
- 冷会话重建的重放成本，需要消息裁剪或摘要策略。

## 8. 验收标准

域层（`brain` / `norma` 单测，遵循项目测试注释规范）：
- 展示状态推导：各 phase 与 Worker 状态组合的预期结果，含已回收 Worker 不参与。
- 重启后 Task、messages、Worker 绑定完整恢复；快照损坏时保留原件并重试。
- 两个任务的消息与事件互不串扰；同名 Agent 在不同工作空间的事件按 `node_id` 反查归属，不串扰。
- 同一事件重复上报只处理一次；回收或重派后迟到的旧 `dispatch_id` 事件被丢弃。
- Worker 失败 / 中断生成终态消息且任务显示「需要你」，不被伪装成成功。
- 重启后已结束的 Worker 被补结算，查不到的标记 Interrupted，且不重发操作。
- 自动派活次数超限转人工。
- 静默事件只写入 `Task.messages`，不唤醒任务会话；立即汇报的事件只由主会话播报一次。
- 定时摘要与回来时概览读取任务摘要而不是 Goal。
- 冷会话事件触发重建并重放。
- 画布关闭卡片 → Reclaimed 且保留 `engine_session_id`；再唤起使用该会话。
- 用户改 `phase` 后 Norma 不立即覆盖；已完成任务收到新消息自动回到 Active。
- 旧 Goal 迁移为单 Worker 任务，不丢数据。

真实 App（新增 desktop-e2e 模块 `staff`，看图版登记）：三栏布局、「画布 / 幕僚」切换、全屏与 Esc 退出、右抽屉标签与文件列表、中英文、浅色 / 深色。

## 9. MVP 拆分

原则：先跑通「建任务 → 提议 → 采用 → Worker 执行 → 结果回到任务对话 → 你验收」这一条主线，其余后置。

### M1（MVP）：能用的最小闭环
- `norma` / `brain`：`Task`、`Worker` 类型与 `TaskStore`（JSON 原子快照）；Worker 状态只做 Active / Failed / Interrupted。
- 任务会话：先验证适配器「一任务一会话」；若验证不过，降级为复用主会话并按任务重放 `Task.messages`。
- 分派：幕僚提议 → 你采用 → `assign --task` 生成 `dispatch_id`，创建新卡片。
- 结果回传：hook `AgentFinished` + `dispatch_id` 核对；先落盘再唤醒；`from_result=false` 视为未取得结果；失败生成终态消息。
- 汇报：`Router` / `triage` 不改；静默事件只落盘；任务会话不播报。
- 状态：展示状态由 Worker 推导；`phase` 由 Norma 自动推进，用户可改，用户改过的值不被覆盖；已完成任务收到新消息自动回到 Active。
- App：「画布 / 幕僚」切换、全屏、左任务列表（状态分组）、中对话、右抽屉复用；中英文。
- 验收：域层单测 + 新增 desktop-e2e 模块 `staff`。

### M2：回收与恢复
- Worker 回收 / 再唤起（`engine_session_id`），画布关卡片视为回收。
- 重启对账（补结算 / 标 Interrupted）。
- 冷会话重建与热会话上限。

### M3：存量与自动化
- 旧 Goal 迁移、「未归属卡片」入口。
- 自动派活次数上限（默认 8，可配置）与事件队列合并；超限转人工。

### 明确延后
Worker 依赖编排、卡片跨任务移动、自动给旧卡片分组、iOS。

### MVP 前置验证
1. 三个适配器的「一任务一会话」支持（已做只读调研，见第 7 节；Claude Code 与 OpenCode 仍待实测；验证不过则降级为复用主会话并按任务重放 `Task.messages`，已决定）。
2. ~~读 `scoped_router.rs`~~ 已完成，结论见 5.5。
3. hook 事件能否携带 `NORMA_DISPATCH_ID`（`AgentFinished` 需新增字段）。
