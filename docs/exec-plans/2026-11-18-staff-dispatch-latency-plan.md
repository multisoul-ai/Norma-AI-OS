# 幕僚派活与响应速度重构 实施计划

- SPEC：[`2026-11-18-staff-dispatch-latency.md`](2026-11-18-staff-dispatch-latency.md)（下称 SPEC，章节号沿用 SPEC）。
- 分支：`feat/staff-dispatch-refactor-20261118`，worktree `../norma-staff-dispatch-impl`。不得在主目录 `dev` 上改代码。
- 交付分三个 PR，每个 PR 合入 `dev` 前 CI 必须全绿：
  - PR-1：任务 0–3，派活规则与工具收紧。用户最先感知到的改善在这一组。
  - PR-2：任务 4–7，Worker 状态统一。
  - PR-3：任务 8–10，会话常驻与验收。
- 每个任务都按 TDD 来：先写失败的测试，再实现。测试注释、断言说明、魔数推导按 `docs/quality/test-requirements.md` 写。
- 每个任务完成后跑：`cargo test -p brain`（涉及 app 时加 `cargo check -p app`）、`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`。改了文案的再跑 `python3 scripts/check-i18n.py`。
- 禁止 `--no-verify`、禁止 `#[allow]`、单个 Rust 文件不超过 500 行。

---

## 任务 0：三个前置实验（不进生产代码，结论写回 SPEC 第 10 节）

实验代码放 `spikes/staff-acp-multisession/`，跑完写结论，代码不合入。

1. **Claude 按会话传 env**
   - 起一个 `claude-agent-acp` 进程，开两个会话，分别在 `_meta.claudeCode.options.env` 里传不同的 `NORMA_BRAIN_TOKEN`。
   - 每个会话里让模型跑 `echo $NORMA_BRAIN_TOKEN $PATH`，确认两边各自看到自己的 token，且 PATH 没丢。
   - 同时验证 `_meta.claudeCode.options.tools` 白名单（只留 Bash、Read）是否生效。
2. **Codex 补丁**
   - 在 `crates/brain/src/acp/codex_loader.mjs` 的副本里，把 `_meta.norma.config` 合并进 `thread/start` 的 config。
   - 两个线程分别设 `shell_environment_policy.set.NORMA_BRAIN_TOKEN`，确认 shell 命令各自看到自己的值。
3. **权限可见性**
   - 用 `app.sqlite` 的 `brain_trace_items` 统计历史幕僚轮次里「工具调用」和「`session/request_permission`」的对应关系。
   - 在实验进程里分别调 Read、Grep、Bash(`rg`)，记录哪些会触发权限回调（Claude `default` 模式、Codex `plan` 模式各测一次）。

**结论分支**

- 实验 1 失败：Claude 维持每个任务一个进程。任务 9 只做「按会话分发 + 不杀进程 + LRU」中仍适用的部分。
- 实验 2 失败：Codex 维持每个任务一个进程。
- 实验 3 显示 Read 不走回调：Read 的范围改用 `_meta` 工作目录或额外目录配置限制。
- Codex 收不紧：Codex 只靠提示词约束。

---

## 任务 1：派活策略重写（PR-1）

文件：
- `crates/brain/src/staff/policy.rs`
- `crates/brain/src/staff/assign.rs`
- `crates/brain/tests/staff_policy.rs`
- `crates/app/src/app/brain_host/config.rs`
- 设置界面的开关来源，以及 `panel_rows.rs` 的「以后这类先问我」

1. **配置**
   - 新增 `autonomy.staff_auto_dispatch: bool`，默认 `true`，写回和读取都要支持。
   - `assign_followups` 不动，仍只管主会话 `BrainOp::Assign`（`brain_host/ops.rs`）。
   - 「以后这类先问我」改为写 `staff_auto_dispatch = false`。
2. **规则**
   - 用一个纯函数 `decide_dispatch(ctx) -> Decision` 取代 `decide_assign` / `decide_continue`。
   - 输入：触发来源（用户轮 / 事件轮）、是否点名、是否高风险、开关、`auto_dispatches`。
   - 按 SPEC 第 4 节的表格逐格实现。
   - 删除 `NotFollowUp` 及其文案 key `staff.downgrade.not_follow_up`。
3. **计数**
   - `AUTO_DISPATCH_LIMIT = 5`。
   - 清零时机改到 `TaskStore` 记录用户消息时（任何 `TaskMessage::User`），不再在采用提议时清零。
   - `staff.downgrade.limit_reached` 改为带 `{limit}` 占位符，中英文都改。
4. **风险词**：英文按单词边界匹配，多词短语整体匹配；中文仍按子串匹配。
5. **测试**
   - 表格每一格一个用例，共 3 行 × 开关开/关。
   - 风险词：`merged` 不命中 `merge`，`git push` 命中，「删除」命中。
   - 计数：第 5 次自动派活之后转提议；用户发话后清零。
   - 同步修改 `staff_policy.rs` 里原来 `NotFollowUp` 的断言。

## 任务 2：点名判定（PR-1）

文件：
- `crates/brain/src/staff.rs` 或新建 `crates/brain/src/staff/naming.rs`
- `crates/brain/src/staff/assign.rs`
- `crates/app/src/app/brain_host/staff.rs`（`resolve_staff_entry`）

1. **记下原话**
   - `begin_turn` 时收集「上一轮结束后到本轮开始」之间所有 `TaskMessage::User` 的文本，存为本轮的 `user_words`。
   - 剔除图片路径（复用 `staff_images` 的解析），排除 `Answered`。
2. **匹配**：纯函数 `named_in(words, roster_entry) -> bool`。
   - 匹配名册里的人名和语音别名。
   - 英文按单词边界、大小写不敏感的整词匹配；中文整名包含。
   - 少于 2 个字符的名字不参与；引擎 slug 和回退显示名不参与。
3. **接入**
   - `task assign <agent>`：`<agent>` 是名册名时，宿主用它选人，并用 `named_in` 判定是否点名。
   - `<agent>` 是引擎 id 时，走 `least_loaded`，按没点名处理。
   - 名册里没有这个人：回执 `staff.agent_unavailable`，并附上可派的名单。
4. **测试**
   - 正例：「让 Grace 合并」「grace, merge it」「让小王删一下」。
   - 负例：
     - `Ed` 不命中 `merged`。
     - 「用 claude 做」不算点名。
     - 图片路径里出现的名字不算。
     - `Answered` 选项里出现的名字不算。
     - 上上轮的消息不算。

## 任务 3：提示词、skill 与工具收紧（PR-1）

文件：
- `crates/brain/src/lane/staff.rs`
- `crates/brain/src/lane/staff_prompt.rs`
- `crates/norma/src/orchestration/brain_skill.rs`
- `crates/brain/src/acp/permission.rs`
- `crates/brain/src/backend.rs`（`session_meta`）

1. **提示词**
   - 删除「不要挑具名智能体」，改为：用户点名时 `assign` 传名册名，否则传引擎 id。
   - 默认用 `task assign`；`propose` 只用于高风险的活，或用户说过「先给我看」的事。
   - 删掉幕僚 lane 的 `skill_ready` 分支，命令速查始终内联。审批 lane（`router.rs`）的 `skill_ready` 不动。
2. **skill**：同步改 engine-id 说明、「3 automatic dispatches」「rule 2」等描述。
3. **Bash 判定**（`permission.rs`）
   - 新增纯函数 `staff_command_allowed(raw_input, norma_bin) -> bool`。
   - 先解包 `rawInput.command` 字符串和 Codex 的 `{normaShellCommand}` 对象，再剥掉 `zsh -lc "…"` 外壳。
   - 拆成 argv 后，要求第一个参数等于 `norma_bin` 的绝对路径、第二个是 `brain`。
   - 出现 `;`、`&`、`|`、`` ` ``、`$(`、`>`、`<` 的一律拒绝。
   - 拒绝时返回「请派 Worker 去做」。
4. **Read 与其他工具**
   - 按任务 0 的实验 3 结论：在 `session_meta` 里加 `tools: ["Bash","Read"]`。
   - Read 只放行项目目录：回调能看到 Read 时在回调里限制，看不到时用 `_meta` 目录配置限制。
5. **测试**
   - `staff_command_allowed`：合法命令、`; rg ~`、`$(…)`、`| sh`、Codex 对象、`zsh -lc` 外壳、非 norma 的绝对路径。
   - `session_meta` 快照里包含工具白名单。

**PR-1 验收**

- 真实 App 里说「让 Grace …一件高风险的事」，直接派出，没有提议（desktop-e2e `staff` 模块新增场景，或用 `StaffAutoDispatch` 改写）。
- 跑一轮需要查资料的请求，`brain_trace_items` 里只出现 `norma brain` 命令和 Read。

---

## 任务 4：数据模型 v3（PR-2）

文件：
- `crates/norma/src/protocol/staff.rs`（`Worker`）
- `crates/brain/src/staff.rs`（`SNAPSHOT_VERSION`、加载）
- `crates/brain/src/staff/delivery.rs`

1. 新增 `WorkerPhase`，按 SPEC 第 5 节的对照表处理旧字段：
   - 删除 `prompt_delivered`、`state`、`needs_input`。
   - `outcome` 并入 `Finished(outcome)`，保留 `outcome()` 访问器。
   - `chip()` 改读 `phase`。
2. `SNAPSHOT_VERSION = 3`。加载到旧版本时，把原文件改名为 `tasks.v2.bak.json`，从空库开始，并写一条日志。
3. `TaskStore` 按 `(card_id, dispatch_id)` 定位任务，替代「第一个含该 card_id 的任务」：改 `record_event`、`record_delivery`。
4. 新增 `Task.revision`：每次落盘写入加 1。只改内存的 `update_live` 不加。
5. **测试**
   - v2 文件加载后，备份文件存在，store 为空。
   - 同一 card_id 挂在两个任务、dispatch 不同时，事件各自落到正确的任务。
   - 旧字段的派生值正确。

## 任务 5：Worker 状态机（PR-2）

文件：
- 新建 `crates/brain/src/staff/worker_life.rs`，由 `delivery.rs` 演进而来
- `crates/app/src/app/brain_host/staff/{dispatch,input,resume,events}.rs`
- `crates/app/src/app/staff/` 下的 `delivery_action` / `delivery_confirmed`

1. 纯函数 `step(phase, input) -> (phase, Vec<Effect>)`：
   - `Input`：Spawned、OutputQuiet、PromptSubmitHook、Stop、Exited、Tick、Withdraw、Restarted。
   - `Effect`：CreateCard、Spawn、Type、PressEnter、Interrupt、CloseCard、WakeStaff。
2. 现有超时、重试、晚到确认（`recover_late_delivery`）等语义要全部搬进状态机，不能丢。
3. App 侧收敛成一个执行器：
   - 把 hook 和 runtime 事件翻译成 `Input`，然后执行 `Effect`。
   - 删除 `app.staff.pending` / `staff.unconfirmed`，以及 app 层的 `delivery_action` / `delivery_confirmed`。
4. **重启**：加载 `TaskStore` 时，所有未结束的 Worker 走 `Restarted`，转为 `Failed(Restarted)`，并唤醒幕僚一次（不计入 `auto_dispatches`）。
5. **测试**
   - 状态机的每个转换一个用例。
   - 撤回和确认竞态：`Withdraw` 晚于 `PromptSubmitHook` 时忽略撤回。
   - 超时转 NeedsAttention 后晚到确认能恢复。
   - 重启转换，以及唤醒不计数。

## 任务 6：卡片复用改绑（PR-2）

文件：
- `crates/runtime/src/runtime/staff.rs`
- `crates/app/src/app/brain_host/staff/pick.rs`

1. Runtime 对每个节点记两样：`spawn_dispatch`（进程启动时注入的 `NORMA_DISPATCH_ID`）和 `current_dispatch`。
2. `accepts_staff_hook` 的接受条件：`_norma_dispatch_id == spawn_dispatch`，或者该字段缺失且 `control_run_id` 命中。接受后按 `current_dispatch` 路由。
3. **选卡**
   - 点名时，优先找同名的空闲卡（不论是否用户自己开的），复用前改绑。
   - 卡片正忙就新开。
   - 用户自己开的卡，如果 `control_run_id` 认领不了，也新开。
4. `staff_dispatches` 启动时从 `TaskStore` 重建。
5. **测试**
   - 复用别的任务留下的卡并改绑后，hook 被接受，并路由到新 dispatch。
   - 进程的 dispatch id 不匹配时拒绝。
   - 卡片正忙时新开卡片。

## 任务 7：撤回与面板（PR-2）

文件：
- `crates/app/src/app/staff/messages/card/dispatched_proposal.rs`
- `crates/brain/src/staff/assign.rs`（`cancel` 路径）
- `crates/ui/locales/`

1. 撤回按钮的显示条件改为 `phase < Running`。点整行跳到画布上的那张卡片。
2. **撤回语义**
   - 宿主为这次派活新开的卡：`CloseCard`。
   - 复用的卡：`Interrupt`（Ctrl-C），不关闭。
   - Worker 记为 `Finished(Interrupted)`。
   - 不唤醒幕僚，只追加一条系统消息「用户撤回了派给 {name} 的 {title}」。
3. 新增中英文 key：撤回系统消息、`Failed(Restarted)` 的原因、「排队中」。
4. **测试**
   - 撤回后 `wake == false`。
   - 复用的卡撤回后仍然存在。
   - desktop-e2e 新增两个场景：新开卡撤回后卡片消失；复用卡撤回后卡片保留（含 `--visual` 看图版）。
   - 同步修改 `StaffOpenCodeDispatch.swift`（原来读 `prompt_delivered`）、`StaffFixture` / `BrainFixture`（原来写 `assign_followups`）。

**PR-2 验收**

- `Task.workers` 是唯一的投递状态，app 里已经没有 pending/unconfirmed。
- 真实 App 重启时有投递中的 Worker：面板显示失败，幕僚说明一次。

---

## 任务 8：拆分 `lane/worker.rs` 并做增量提示（PR-3）

1. 先做纯重构（行为不变、测试不改）：把 `lane/worker.rs`（499 行）拆成会话生命周期、轮次执行、提示构建三块。
2. **增量提示**
   - 会话记录 `last_revision` 和上次发出的结构化状态摘要（proposals、workers、phase、questions）。
   - 每轮发「新增消息 + 状态差异」。
   - 凡是新建会话一律发完整快照：LRU 回收、`max_turns` 到期、出错重建、临时重试。
3. **测试**
   - `discard_empty_reply` 删消息之后，增量里不会漏消息也不会重复。
   - 只有 proposal 状态变化时，增量里包含这个变化。
   - 新会话发完整快照。

## 任务 9：共用 ACP 连接与多会话（PR-3，范围取决于任务 0）

文件：
- `crates/brain/src/acp/client.rs`
- `crates/brain/src/service/runners.rs`
- `crates/brain/src/lane/session.rs`

1. **按会话分发**
   - 连接上起一个独立读线程，按 `sessionId` 把 `session/update`、`session/request_permission`、`elicitation/create` 分发到各会话的 channel。
   - 每个会话有自己的权限上下文和问题回调。
2. **取消**：单轮取消或超时只发 `session/cancel`；只有连接失效时才 kill 并整体重建，重建期间排队的轮次不丢；连续失败 3 次后面板报错。
3. **调度**
   - `TaskRunners` 按后端持有一条连接。同一任务的轮次串行，不同任务可以并发。
   - Claude 最多 4 个会话，只驱逐空闲会话里最久没动的；全部在忙时新轮次排队，面板显示「排队中」。
   - 任务 Done 或归档时关闭它的会话。
   - OpenCode 维持每个任务一个进程。
4. **token**
   - 每个会话持有自己的 `SessionToken`，经 `_meta` 下发（Claude 用 `options.env`，Codex 用 `norma.config`）。
   - 驱逐、任务 Done、连接重建时撤销 token。
   - 会话建好后先跑一次 `norma brain agents` 自检，失败就退回每个任务一个进程。
5. **测试**（用假的 ACP 对端，GPUI 无关）
   - 取消会话 A 不影响会话 B。
   - B 的权限请求不会进 A。
   - 第 5 个任务排队，A 空闲后被驱逐，第 5 个开始。
   - 驱逐后旧 token 返回 `staff.invalid_source`。

## 任务 10：收尾验收（PR-3）

1. 在 `docs/exec-plans/2026-11-18-staff-dispatch-latency.md` 末尾附「改动前后」：用 `brain_turns` 列出幕僚每轮的 p50/p90/max。改动前取 2026-11-18 之前的数据，改动后取真实使用至少 20 轮的数据。
2. 逐条勾选 SPEC 第 11 节的验收清单，给每条附上证据（测试名、e2e 场景名或日志查询语句）。
3. 跑 `cargo test --workspace`、`cargo check -p app`、项目里全部的 `scripts/check-*`，以及 desktop-e2e `staff` 模块。
4. 代码审查用独立 subagent 做对抗式审查，发现的问题修复后再合并。
