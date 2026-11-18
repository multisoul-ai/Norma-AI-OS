# 幕僚派活与响应速度重构 SPEC

> 状态：已定稿（2026-11-18）。按对照代码的对抗式审查补充，审查后的取舍已逐条与用户确认。

## 1. 背景与目标

- 现状数据（主项目 `tasks.json` + `app.sqlite` 的 `brain_turns`，截至 2026-11-18）：
  - 幕僚回复耗时 p50 79.5s、p90 181s、最长 546s。
  - 546s 那一轮有 81 次工具调用（`rg`/`sqlite3`/`python3`/在家目录下搜索），其中两次搜索各卡 97s。
  - 不调工具的轮次只要 3–4s。
- 32 条提议中，28 条是幕僚主动 `propose`，3 条是策略降级。根因有四层：提示词引导它提议、`assign_followups` 默认关闭、第一个 Worker 必须是「后续」、风险词按子串匹配。
- 目标：用户说「让 Grace 做 X」就直接开工；幕僚只负责协调，不自己翻代码；Worker 的状态只保留一份。

## 2. 范围

**本次要做**

- A. 派活规则重写（第 4 节），提示词与 skill 同步。
- B. 幕僚工具收紧：只放行 `norma brain` 命令和项目内 Read；命令速查始终放进提示词。
- C. 会话结构：Claude/Codex 用一条 ACP 连接开多个会话，token 按会话传入；OpenCode 维持每个任务一个进程。Claude 最多保留 4 个活跃会话。
- D. 会话常驻后每轮只发增量；新开会话时发完整快照。
- E. Worker 状态统一：把现有的投递判定（`app::staff` 的 `delivery_action` / `delivery_confirmed`、`brain/src/staff/delivery.rs`）合并成 GPUI 无关的纯状态机，`Task.workers` 是唯一的事实来源。
- F. 修改现有「已派给」行和撤回按钮（`app/staff/messages/card/dispatched_proposal.rs`）：显示条件、撤回语义按第 7 节调整。
- G. `tasks.json` 升到 v3，旧数据备份后清空。

**不做**

- 投递方式：保留模拟键入，不改成通过启动参数传 prompt。
- 主会话 lane、审批 lane 的行为改动（含主会话 `BrainOp::Assign` 的授权，见第 4 节）。
- iOS 端展示。

## 3. 场景

1. 「让 Grace 把这个分支合并到 dev」：点名，即使高风险也直接派给 Grace。
2. 「安排一个 work 修一下任务列表对齐」：没点名，不是高风险，幕僚挑不忙的引擎直接派。
3. 「安排一个 work 删掉旧的迁移代码」：没点名，命中删除类风险，转成提议等用户采用。
4. Grace 完成后幕僚判断还有后续：自动派，同一任务连续最多 5 次，用户任何一次发话都清零。
5. 用户点名 Grace，画布上的 Grace 卡片空闲：在这张卡片里继续，不论卡片是不是用户自己开的；卡片正忙就新开一张 Grace 卡片。

## 4. 派活规则（取代 `decide_assign` / `decide_continue`）

| 触发 | 条件 | 自动派活开启 | 自动派活关闭 |
|---|---|---|---|
| 用户消息触发的轮次 | 点名（宿主核对，定义见下） | 直接派，不看风险 | 直接派（点名本身就是明确授权） |
| 用户消息触发的轮次 | 没点名 | 不是高风险就直接派，高风险转提议 | 转提议（原因 `autonomy_ask`） |
| 事件触发的轮次（Worker 完成等） | 不是高风险且 `auto_dispatches < 5` | 直接派，否则转提议 | 转提议 |

- 删掉「第一个 Worker 必须是后续」这条规则（`NotFollowUp`），连同文案 `staff.downgrade.not_follow_up`。
- 风险词：英文按单词边界匹配，多词短语整体匹配；中文保持子串匹配。
- **开关**：新增幕僚专用字段 `autonomy.staff_auto_dispatch`，默认 `true`，取代幕僚对 `assign_followups` 的读取。`assign_followups` 保留原默认值，只管主会话 `BrainOp::Assign`（`brain_host/ops.rs`），这样不放开主会话。原因：设置写回会显式写出 `assign_followups`（`brain_host/config.rs`），本机现值是 `false`，改代码默认值对老用户无效。面板上的「以后这类先问我」改为写 `staff_auto_dispatch`。
- `AUTO_DISPATCH_LIMIT` 改为 5，清零条件从「用户采用提议」改为「用户任何一次发话」。文案 `staff.downgrade.limit_reached` 里写死的次数改成占位符。
- **点名的定义**：
  - 原话 = 上一轮结束后到本轮开始之间所有 `TaskMessage::User` 的文本，剔除图片路径；不含 `Answered`（回答选项是幕僚自己写的，不能让它借选项给自己授权）。语音输入按转写文本处理。
  - 匹配对象 = 名册里的人名和语音别名；不含引擎 slug 或回退显示名（如 "Claude"），名字少于 2 个字符不参与。
  - 英文按单词边界、大小写不敏感的整词匹配；中文整名包含匹配。
- **提示词与 skill**：
  - 去掉「不要挑具名智能体」（`lane/staff.rs`）和「`<agent>` 是 engine id，不是人名」（`norma/src/orchestration/brain_skill.rs`）。用户点名时 `task assign` 传名册名；没点名传引擎 id，由宿主 `least_loaded` 挑人。
  - 默认用 `task assign`；`propose` 只用于高风险的活，或用户说过「先给我看」的事。
  - skill 里「3 automatic dispatches」「rule 2」等描述同步改。

## 5. 数据与接口

- `Worker` 新增 `phase: WorkerPhase`：
  `Reserved → Spawning → Typed → Submitted{retried} → Running → Waiting → Finished(outcome) | Failed(reason)`。
- 旧字段的去留（`norma/src/protocol/staff.rs`）：

  | 旧字段 | 处理 |
  |---|---|
  | `prompt_delivered` | 删除，由 `phase >= Running` 派生 |
  | `state` | 删除，由 `phase` 派生；`w.chip()` 改读 `phase` |
  | `outcome` | 并入 `Finished(outcome)`，保留 `outcome()` 访问器，`check_continuable` 和界面改用它 |
  | `needs_input` | 并入 `Waiting` |
  | App `staff.pending` / `staff.unconfirmed` | 删除 |

- `Runtime.staff_dispatches` 只作为 node → dispatch 的索引，启动时从 `TaskStore` 重建，不再单独保存状态。另记每个节点进程启动时的 `NORMA_DISPATCH_ID`（`spawn_dispatch`），用于第 6 节的卡片复用改绑。
- `TaskStore` 按 `(card_id, dispatch_id)` 定位任务，替代现在「第一个含该 `card_id` 的任务」（`staff.rs`、`staff/delivery.rs`），避免同一张卡跨任务复用时事件落错或被丢。
- `TaskStore` 快照升到 v3；加载到旧版本时备份为 `tasks.v2.bak.json`，从空库开始。回滚：降回旧版本 App 前，手动把 `tasks.v2.bak.json` 改名回 `tasks.json`（v3 文件旧版本读不了），写进 PR 说明。
- 会话参数：
  - Claude：`_meta.claudeCode.options.env.NORMA_BRAIN_TOKEN`，同时设 `tools` / `disallowedTools`。
  - Codex：`_meta.norma.config.shell_environment_policy.set`，由 `codex_loader.mjs` 的补丁合并进 `thread/start`。
  - 两者都要先做实验（第 10 节）。

## 6. 技术方案

### 6.1 Worker 状态机

- 放在 `brain::staff::worker_life`，由现有 `brain/src/staff/delivery.rs` 演进而来，是纯函数：`(WorkerPhase, Input) -> (WorkerPhase, Vec<Effect>)`。
  - `Input`：Spawned、OutputQuiet、PromptSubmitHook、Stop、Exited、Tick、Withdraw、Restarted。
  - `Effect`：CreateCard、Spawn、Type、PressEnter、Interrupt、CloseCard、WakeStaff。
- App 只负责执行 `Effect`，并把 hook 和 runtime 事件转成 `Input`。`brain_host/staff/dispatch.rs`、`input.rs`、`resume.rs` 收敛成一个执行器；`app::staff` 里的 `delivery_action` / `delivery_confirmed` 移进状态机后删除。
- **App 重启**：`restore_canvas_snapshot` 会 `kill_all` 并清空 `staff_dispatches`，重新拉起的进程不带 dispatch id。加载 `TaskStore` 时，处于 Reserved/Spawning/Typed/Submitted/Running/Waiting 的 Worker 一律转 `Failed(Restarted)`，并唤醒幕僚一次说明情况。这一次唤醒不计入自动次数。

### 6.2 卡片复用与 hook 认领

- 进程的 `NORMA_DISPATCH_ID` 在启动时就固定了，`accepts_staff_hook` 遇到不一致会直接拒绝（`runtime/staff.rs`）。
- **复用改绑**：复用空闲卡片时，Runtime 把该节点的「当前 dispatch」改成新的 dispatch。hook 的认领规则改为：`_norma_dispatch_id` 等于该节点**进程启动时的 dispatch id**，或缺失且 `control_run_id` 命中，就算接受，再路由到当前 dispatch。
- 用户自己开的卡（进程没带 dispatch id）走 `control_run_id` 分支；认领失败时退回新开卡片。

### 6.3 会话管理（共用 ACP 连接）

- `TaskRunners` 改为每个后端一条 ACP 连接，内部按任务开会话。同一任务的轮次仍然串行；不同任务的轮次可以并发。
- **按会话分发**：现在 `prompt_observed` 独占 `conn.recv`，会丢掉不属于本轮的通知（`acp/client.rs`），问题回调也只绑定一个。改为：一个独立读线程按 `sessionId` 把 `session/update`、`session/request_permission`、`elicitation/create` 分发给各自会话；每个会话有自己的权限上下文和问题回调。
- **取消**：现在 `abort()` 先 `session/cancel` 再 `conn.kill()`。共用连接后，单轮取消或超时只发 `session/cancel`，不杀进程；只有连接本身失效才整体重建。
- **LRU**：Claude 会话超过 4 个时，只驱逐**空闲**会话里最久没动的那个；4 个都在跑时，新任务的轮次排队。任务 Done 或归档时关闭它的会话。
- **token**：每个会话持有自己的 `SessionToken`。驱逐、任务 Done、连接重建时撤销；轮换后旧 token 返回 `staff.invalid_source`。
- **回收后恢复**：幕僚会话设置了 `persistSession: false`（`crates/brain/src/backend.rs`），回收后无法 `session/load` 接回。重新开会话时发完整任务快照，对话历史以 `tasks.json` 为准。
- OpenCode 维持现有的每个任务一个进程。
- `lane/worker.rs` 已有 499 行，先按职责拆分（会话生命周期 / 轮次执行 / 增量提示），再做本节改造。

### 6.4 增量提示

- 不按 `messages` 下标做增量。原因：`discard_empty_reply` 会删消息导致下标错位，而且 proposals、workers、phase、questions 的变化不产生新消息。
- 改用 `Task.revision`（每次 `TaskStore` 写入加 1）。会话记录上次发出时的 revision，以及当时的结构化状态摘要；每轮发「新增消息 + 结构化状态的差异」。
- 凡是新建会话一律重置，发完整快照。新建的情况包括：LRU 回收、`max_turns` 到期、出错重建、临时重试。

### 6.5 工具收紧

- **前提先验证**：Claude 的 session mode 是 `default`，Read 这类只读工具可能根本不走 `session/request_permission`。先用 `brain_trace_items` 对比「工具调用数」和「权限请求数」，确认 rg、sqlite3、Read 哪些经过了回调。
- **Claude**：
  - 在 `_meta` 里用 `tools` 白名单只留 Bash 和 Read。
  - Read 的范围用 `_meta` 的工作目录或额外目录配置限制，不靠回调。
- **Bash 判定**：权限回调（`acp/permission.rs`）现在只读 `rawInput.command` 字符串，规则改为：
  1. 先解包 Codex 的 `{normaShellCommand}` 对象和 `zsh -lc "…"` 外壳。
  2. 拆成 argv 后，要求第一个参数是 `norma_bin` 的绝对路径、第二个是 `brain`。
  3. 出现 `;`、`&`、`|`、`$(`、反引号或重定向的一律拒绝。
  4. 拒绝时返回「请派 Worker 去做」，不计为失败。
- **Codex**：幕僚已经是 `collaboration_mode=plan`。读命令是否走审批要实验确认（第 10 节）；如果不走，Codex 幕僚只靠提示词约束。
- 去掉幕僚 lane 的 `skill_ready` 分支，命令速查始终放进提示词。审批 lane 的 `skill_ready`（`router.rs`）不动。

## 7. UI/UX

- 修改现有的「已派给 Grace · <标题>」行。撤回按钮的显示条件从「自动派发且没有结果」改为 `phase < Running`（卡片确认收到 prompt 之前）。点整行跳到画布上的那张卡片。
- **撤回语义**：
  - 宿主为这次派活新开的卡：关闭卡片。
  - 复用的卡（含用户自己开的）：只发中断（Ctrl-C），不关闭。
  - Worker 记为 `Finished(Interrupted)`。
  - 撤回**不唤醒幕僚**，只在任务里追加一条系统消息「用户撤回了派给 Grace 的 <标题>」，下次用户发话时幕僚能看到。这样避免事件轮次里马上把同一件事再派一遍。
- 转成提议时，面板显示原因（复用 `staff.downgrade.*` 文案）。
- 新增和修改的 i18n key 都要有中英文：撤回系统消息、`limit_reached` 占位符、重启失败原因等。新增和改动的视觉用 desktop-e2e 的 `--visual` 模式验收。

## 8. 状态、错误与边界

- 点名的人不在名册里：回执报 `staff.agent_unavailable`，并附上可派的名单，幕僚照实告诉用户。
- 点名的卡片正忙：新开同名卡片（场景 5）。
- 会话被回收后又有事件：新开会话并发完整快照，多等约 5 秒。
- 4 个 Claude 会话都在跑：第 5 个任务排队，面板显示「排队中」。
- 连接异常（例如 2026-11-18 的 `Invalid params`）：重建连接，期间排队的轮次不丢；连续失败 3 次后面板显示错误。
- 幕僚调用了被禁的工具：拒绝，并在同一轮里返回提示，不计为失败。
- 撤回和送达确认同时到达：状态机按到达顺序处理。`Withdraw` 到达时如果已经是 `Running`，忽略撤回，并在界面上收起按钮。
- App 重启时有投递中的 Worker：见 6.1。

## 9. 非功能

- 性能：不设硬指标，要求用 `brain_turns` 列出改动前后的数字。
- 安全：
  - 点名授权只认第 4 节定义的用户原话。
  - token 按会话隔离，轮换即撤销。
  - Bash 按 argv 判定，拒绝复合命令。
  - OpenCode 不共用进程。
- 内存：Claude 幕僚会话最多 4 个，约 1.5GB 封顶。
- 文件行数：所有改动后的 Rust 文件不超过 500 行。

## 10. 风险与未决（实施前先做的实验）

- **Claude 按会话传 env**：确认 `_meta.claudeCode.options.env` 是合并进进程环境还是整体替换（整体替换会丢 PATH）。会话建好后先跑一次 `norma brain agents` 自检，失败就退回每个任务一个进程；工具收紧、增量提示、派活规则照常交付，只是内存和首次启动不做优化。
- **Codex 补丁**：确认 `shell_environment_policy.set` 按线程生效，shell 命令能看到 token。不成立就退回每个任务一个进程，其余同上。
- **工具权限可见性**：确认 Claude `default` 模式下哪些工具会走权限回调，以及 Codex plan 模式下读命令是否走审批（见 6.5）。Codex 收不紧时只靠提示词约束，第 11 节「只出现 `norma brain` 命令和 Read」这一条只对 Claude 幕僚强制。
- 风险词仍然是关键词匹配，不是安全边界，代码注释里已经这样写明。点名授权后，高风险操作的最后一道防线是 Worker 自己在终端里的权限审批。
- 2026-11-18 11:27–11:30 的 8 次「会话创建失败 Invalid params (-32602)」原因未查，需单独排查。

## 11. 验收

**行为**

- [ ] 点名派活不出提议，高风险的活也一样。在真实 App 里用「让 <名字> …」验证。
- [ ] 幕僚的轮次在 `brain_trace_items` 里只出现 `norma brain` 命令和项目内 Read 两种工具调用。
- [ ] 用 `brain_turns` 列出改动前后幕僚每轮耗时的 p50/p90。
- [ ] Worker 的投递状态只存在 `Task.workers` 里，`app.staff.pending/unconfirmed` 和 `prompt_delivered` 已删除。
- [ ] 撤回后幕僚不会自动重派同一件事。

**单元测试**

- [ ] 第 4 节表格的每一格，包括开关关闭的情况。
- [ ] 点名的负例：英文短名嵌在单词里、引擎名、图片路径、`Answered` 选项文本。
- [ ] 状态机的每个转换，包括撤回与确认竞态、重启转 `Failed(Restarted)`。
- [ ] Bash 判定：复合命令、`$(...)`、Codex 对象格式、`zsh -lc` 外壳。
- [ ] 共用连接：取消一个会话不影响其他会话；权限请求按 `sessionId` 路由；驱逐后旧 token 失效。
- [ ] 增量提示：删消息后游标正确，结构化状态变化能发出，新会话重置。
- [ ] `(card_id, dispatch_id)` 定位，以及复用卡片改绑后 hook 被接受。

**需要同步修改的现有测试与资源**

- [ ] `brain/tests/staff_policy.rs` 里 `NotFollowUp` 的断言。
- [ ] desktop-e2e：`StaffOpenCodeDispatch.swift`（读 `prompt_delivered`）、`StaffAutoDispatch.swift`（auto 语义）、`StaffFixture` / `BrainFixture`（写了 `assign_followups`）。
- [ ] desktop-e2e 新增：「已派给」这一行、撤回（新开卡关闭、复用卡只中断）。
- [ ] i18n：`staff.downgrade.limit_reached` 占位符、删除 `not_follow_up`、新增撤回和重启提示；运行 `python3 scripts/check-i18n.py`。
- [ ] `norma-brain` skill 文本与提示词同步。
