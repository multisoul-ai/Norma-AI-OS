# 幕僚代答实施计划

状态：实施中；Claude/Codex 原生纵向已通过，OpenCode 生产路径已通过真实 CLI + 确定性模型验证；Cursor 原生前台归属仍阻塞，四家整体未交付。日期：2026-10-27。
能力依据与官方引用：`docs/exec-plans/2026-10-27-agent-question-answering.md`。

## 当前能力与证据边界

| 组合 | 当前结果 | 剩余限制 |
| --- | --- | --- |
| Codex 0.162.1 原 TUI / 同 daemon | 默认 Runtime 原题消费、原模型确认、持久回执、重复提交与进程回收已验证 | 未将此证据计作真实 Norma 窗口验收 |
| Claude Code 2.1.296 原 TUI / Hook | 默认 App 授权到原 tool_use_id 消费、原模型继续、持久回执已验证 | 需启动前安装 Hook，不能接管任意迟装 Hook 的既有题 |
| OpenCode 1.19.3 原 TUI / serve+attach | 正式 TUI plugin 跟随前台 session；真实 CLI + 本地确定性模型单选/多选/自定义通过；切换、失联、Unknown 不重发已验证 | 远程真实模型额度耗尽，未完成该项；不覆盖未知版本或用户自定义 TUI_CONFIG/PURE 启动 |
| Cursor 2026.10.18-ea085cb 原 TUI | 保留原始启动与人工操作 | 启动 session/store/题面不能证明当前前台 call；生产自动代答已禁用，未完成适配 |
| Norma 问答 UI | 真实 App 四操作通过：采用、编辑、明确委托、含空格自定义；每题仅消费一次，无普通任务输入 | sapphire-night / paper-blue 四操作与640px短面板均通过并实际看图；原有Ideas浅色提示对比偏低单独记录。脚本夹具不替代真实 Provider 验收 |

验证摘要：最新debug构建与App check通过；norma/runtime/storage/brain/ui共992项通过。`cargo test --workspace` 的App部分1645通过/1失败/10忽略，失败来自另一组A1语音测试的AgentProvider就绪条件；本任务问答记录通过。release构建退出0（26m25s）。详细记录 `artifacts/agent-question-adapters-20261027/final-validation.md`。

同一题的建议、按钮和明确文字/语音委托使用同一 QuestionRef 与提交入口；普通事件只生成建议。以下记录按时间保留，过往中间失败和实验能力不能替代本矩阵。

## 实施记录（2026-10-27）

- M0 Claude 原生 TUI：PreToolUse 回填与同 tool_use_id 的 PostToolUse 消费确认通过；onFailure continue 的 exit 1 / timeout 均恢复原生人工问答。记录 `artifacts/agent-question-adapters-20261027/m0-claude-native.md`。
- M0 Codex：同 server/live thread pending 重放、原 TUI 继续、人工先答竞争通过；透明 relay 给 TUI 自己的 thread/start 打开 raw 来源事件，未增加 bootstrap turn。按 threadId/turnId/call_id 分类。记录 `artifacts/agent-question-adapters-20261027/m0-codex-native.md`。
- M0 Cursor：真实原生同一工具的单选、多选和自由文本已有消费回执；按键必须逐步读回。已知本机插件同步会阻塞启动，探针有单独受限配置；不将探针配置直接带入产品默认启动。
- M1/M2：CLI、独占提交、WaitingQuestion、Claude 原生 Hook 等待、后台持久化与精确消费对账已接通；采用/编辑/原卡片接管及单题一次性委托已实现。Runtime 全量与 App 问答聚焦测试已通过既有版本，后续改动需增量验证。
- M3：默认 Runtime::spawn_agent 的 Codex 0.162.1 原生纵向测试通过：原题→持久化意图→Green→同 call_id 消费→原模型 NATIVE_REPLY: Green→保存回执。修复真实大帧与 ephemeral 辅助线程错误切换会话；长会话容量与两个实际 PID 退出回收验证已通过。证据 `artifacts/agent-question-adapters-20261027/host-question-delivery-progress.md`。
- M4：OpenCode 1.19.3 原 binary + 确定性模型的单选/多选/custom 探针通过，真实模型额度耗尽。生产默认 Runtime 卡片 host/serve+attach 已接通；真实 binary + 本地确定性模型的单选/多选/custom 生产纵向通过，真实模型仍受额度限制；正式 TUI plugin 的当前会话报告与真实启动/两 PID 回收已通过（17.22 秒）；A→B→home、失联不放行任务、切回不重发未知 POST 的回归已收绿。HTTP 恢复与旧消费对账已接通；当前会话未结算发送、GET 期间切换及稳态查询挂起均有红→绿回归，独立增量审查未发现新阻塞。新增 plugin 后真实 binary + 本地确定性模型三题再次通过（10.91 秒），不代表实际模型额度已恢复。
- M5：Cursor 已确认新会话/子 Agent 共享控件会使启动身份与题面匹配失真。生产启动已停用逐键代答宿主，保留原 TUI；回归红→绿，独立审查确认正常生产入口不可达。保留的 store/engine 属实验实现，Cursor 适配仍未完成，不能把人工回退或 M0 探针计作交付。
- M6：明确文字/语音 UserTurn 已接入单题一次性授权；旧 Unknown 会话过滤与 Thinking 阶段静态确认回归通过。四类非我方/取消终态持久化通过；准备尚未发送的意图终态丢失已复现并修复，新增回归已收绿（App question_ 23 passed / 1 ignored）；真实 Claude 默认原 TUI/App 授权/持久回执/原模型确认纵向也已通过（13.07 秒）。
- M7：真实 `brain-questions-ui` 在 sapphire-night / paper-blue 均退出0（65.68秒 / 69.28秒）；四题各消费一次，无新任务输入；640px短窗口可滚到Ideas输入框，恢复后继续提交原题。各6张关键图已读取，问答区域与布局符合；浅色既有Ideas提示对比偏低单独记录。证据 `artifacts/desktop-workspace-e2e/2026-10-28T02-46-36Z/` 与 `artifacts/desktop-workspace-e2e/2026-10-28T02-48-15Z/`。虚拟显示器透明截图的旧失败保留，成功轮使用后台停放模式。
- OpenCode 未知版本现在在原 TUI shell 先核验，回退保留原新建/resume 命令，不启动 serve/attach；回归红→绿。改动后真实 1.19.3 启动与两个 PID 回收 7.31 秒通过。

上述是阶段进展，不代表四家产品功能已完成。后续阶段与整体完成清单仍按下文验收。

## 1. 交付目标与初始基线

用户在 Codex、Claude Code、OpenCode、Cursor Agent CLI 卡片遇到问答时，幕僚给出建议，用户点击“采用此回答”，答案提交给**当前那次问答**，原 Agent 收到答案并继续。用户明确说“这题你直接回答”时复用同一执行入口。

完成标准包含四家的原生卡片。仅新建协议会话能回答，不算该 Provider 的默认卡片已经适配。Cursor 编辑器内聊天、自动回答所有未来问题、替换 Agent 原生终端渲染，不在本期范围。

以下是实施前的初始基线，最新进展见顶部能力矩阵。

| 初始证据 | 可复用范围 | 不能推导的结论 |
| --- | --- | --- |
| Codex 0.162.1 真实隔离单选往返成功 | app-server 原 RPC 回答与模型消费答案 | 同 daemon 原 TUI 双客户端已经可用 |
| Claude Code 2.1.296 真实 Hook 回填成功 | AskUserQuestion + updatedInput + tool_result | 已显示的原 TUI 问题能被迟装 Hook 接管 |
| OpenCode 1.19.3 question 路由可用 | 当前二进制 HTTP 接口存在 | 真实回答成功、任意 TUI 都能被另一个 server 接管 |
| Cursor 2026.10.18-ea085cb ACP 与处理器映射验证 | 单/多选协议存在；无自由文本字段 | 真实模型消费成功、ACP 能接管旧 TUI |

此前工作区的“下一步阻塞提示/防连点”改动作为临时保护保留，实施时复用反馈，不据此标记代答完成。保留其它并行工作的改动，不整体还原工作区。

## 2. 固定实施约束

1. `AnswerQuestion`、`DecideApproval`、`DispatchTask` 分开路由；旧提议默认仍是 DispatchTask。不能靠模型文字猜某条旧提议是在答哪道题。
2. 一个问题仅有一个可提交的当前版本。操作绑定 workspace/project/node、run 代数、provider 连接、session、请求 ID、题目指纹；没有上游 ID 的字段保留缺省，另建本地调用代数。
3. 问题登记独立于 Agent 状态投影。`WaitingQuestion` 由未决问题驱动；迟到的普通 Hook 不能把它覆盖成空闲。问答结束后根据最新 Provider 状态恢复，不硬设 Done 或 Ready。
4. 回答按钮与直接代答复用同一执行服务；只传规范化答案，不能让幕僚发送任意 PTY 字节。问题、模型建议和调用方自报的 origin 都不是执行授权。
5. 提交开始即占有请求；重复点击、模型重试、桌面/手机竞争只能产生一次有效提交。部分写入或结果不明不能自动重试。
6. 原生问题识别失败时保留原卡片入口；不得 Escape 取消问题后再粘贴新任务，也不得另开 resume 会话回答旧题。
7. 网络、进程、Hook 等待、transcript 与数据库 IO 全部后台执行；前台只更新状态和执行已批准的短输入。保持现有 crate 依赖方向。

## 3. 统一契约

### 对外操作

| 拟新增命令 | 用途与约束 |
| --- | --- |
| `norma brain questions` | 列出当前授权项目的未决问题、能力与精确 QuestionRef；不返回其它项目的问题 |
| `norma brain propose-answer --target <QuestionRef JSON> --answers-json <JSON> --reason <理由>` | 保存建议，不执行；迟到建议必须重新核对问题版本 |
| `norma brain answer --target <QuestionRef JSON> --answers-json <JSON>` | 只消费宿主已记录的当前问题授权；返回提交状态与来源引用，不能靠命令参数自行宣称用户已授权 |

UI 点击由宿主创建本次授权并直接调用同一服务。语音“这题你回答”经现有上下文交接绑定一个确切问题；存在多题且指代不明时只准备建议，不猜目标。首版不增加全局自动代答开关。

建议答案使用本地子题 key、option key 与自由文本；Provider adapter 负责映射到 RPC ID、题目全文、label 或数组位置。保留原始输入用于映射，不对 answer 文本执行 shell 插值。

### 生命周期与回执

```text
Pending → Submitting → Submitted → Answered
                ↘ Failed       ↘ Unknown
未结束请求 → AnsweredElsewhere / Cancelled / Expired
```

- `Submitting`：当前问题已被本次操作占有；先记录意图，再执行 Provider 副作用。
- `Submitted`：已向原请求提交，尚未有对应答案的消费证据。
- `Answered`：收到可关联的工具结果/答案回执与继续执行证据。仅 UI 消失、进入忙碌或 RPC 写入成功均不足以判定。
- `Failed`：确认尚未提交或 Provider 明确拒绝；只有重新检查原题仍待答才允许重试。
- `Unknown`：可能已经提交，但无法确认结果；先对账，不重发。用户明确接管前展示当前已知状态。
- `AnsweredElsewhere`：人工或其它已授权入口先处理；保留记录，关闭旧建议。

历史记录可以持久化，活连接/Hook promise/输入租约不能持久化复用。重启后只通过原 Provider 对账恢复可操作状态；否则旧建议过期。数据库写失败不能变成自动重试 Provider 操作的理由。

## 4. 实施阶段与依赖

顺序：M0 接入验证 → M1 最小契约 → M2 Claude 纵向闭环 → M3 Codex → M4 OpenCode → M5 Cursor/终端兼容 → M6 幕僚与完整交互 → M7 综合验收。

每个阶段采用“一条失败测试 → 最小实现 → 通过 → 下一条”的循环；下表列的是用例清单，不要求先批量写完所有测试。新增文件路径为计划位置，实施时先核对实际模块长度和已有职责。

### M0：补足原生接入证据

**产物**：每家一份版本、启动模式、原请求身份、提交步骤、消费证据与退出回收记录；把可重放的非敏感消息放入 crate 测试夹具。

| Provider | 必做试验 | 通过条件 / 失败处理 |
| --- | --- | --- |
| Claude | 在新隔离原生 TUI 启动前安装 AskUserQuestion Hook，回填一个答案；另测 Hook 退出/超时 | 原终端继续，工具结果与原 tool_use_id 对应；验证该启动模式的实际人工入口，不能假定一定回原问题 |
| Codex | 原 TUI 与 Norma 探针连接同 daemon/live thread；重放 pending 并回答；人工先答一次 | 同一个运行实例接收答案；无额外 turn；能辨认调用来源。不能核验来源的 connector 审批不得进入答题路径 |
| OpenCode | 新隔离 TUI 显式开放 loopback endpoint，或受控 serve + attach；产生真 question 后 reply | 同 server/instance/session 的 question.replied、答案一致、原 TUI 同步继续；记录真实可用的 endpoint 注册方法 |
| Cursor | 原生 TUI 单选、多选、自由输入逐步操作；另测 ACP 单/多选真问题 | 原生卡片保持原进程且收到答案；ACP 只有原连接回包。不能用 ACP 成功代替原生 TUI 验收 |

两种 OpenCode 启动方式只选实测较小且可可靠登记 endpoint 的一种进入首版，不同时实现两套。Codex 共享 daemon 验证不通过时采用已验证的原 PTY 路径，不能以恢复历史替代。Cursor 原生路径不通过则该家仍未完成，不能用“转人工”计作适配成功。

仅使用隔离进程与数据，不操作用户现有问题。已通过的 Codex/Claude 协议单选试验直接复用，除非依赖版本或协议变更。

### M1：最小 Question 契约与阻塞规则

**代码范围**：新增 `crates/norma/src/protocol/question.rs`；扩展 `crates/norma/src/protocol/brain.rs`、`crates/norma/src/cli/brain_command.rs`；新增 `crates/runtime/src/runtime/questions.rs` 与按职责拆分的子模块；挂到 `crates/runtime/src/runtime.rs`。

**先失败的用例**：

- 同一 request ID 在两张卡片或两代连接中重用，旧答案不能命中新题；数值 RPC ID 与字符串 ID 不混淆。
- 首次提交占有请求，第二次提交返回同一进行中状态或已处理；无第二次 Provider 调用。
- 问题 pending 时，新任务与普通消息提交被阻止；迟到 Thinking/Stop 不能凭空证明问题已经回答。
- 题目顺序或选项变化后旧建议失效；不能静默丢失多题中的一题。

**联动**：在 `crates/runtime/src/events.rs` 增加 WaitingQuestion；更新 task-state 的忙碌判断、消息队列、状态文案和所有穷尽匹配。重点入口为 `crates/app/src/app/agent_runtime/coordination.rs`、`crates/app/src/app/agent_runtime/session_reply_queue.rs`、`crates/app/src/cards/agent/card.rs`。状态投影不是未决问题的唯一存储。

**退出条件**：通过公开 Runtime/CLI 边界验证注册、读取、占有、失效；给出的答案尚未接 Provider 时不能返回 Answered。

### M2：Claude 原生 Hook 纵向闭环

**代码范围**：新增 `crates/norma/src/agent_hook/question_forward.rs`；在 Hook 安装/CLI 路由中增加 AskUserQuestion 专用分支；新增 `crates/app/src/app/brain_host/questions.rs` 与 `crates/app/src/norma/endpoint/brain_question.rs`；Runtime 保存可撤销的本次等待句柄。生产入口复用已有鉴权和项目路由，不修改通用权限 Hook 的语义。

**最小流程**：原生卡片发题 → Hook 注册问题并后台等待 → `questions` 查询 → 宿主批准本次 `answer` → 返回保留全部原输入的 updatedInput → tool_result 确认。先用该纵向流程打通核心服务，再扩其它 Provider。

**先失败的用例**：只回 allow 不应判答题完成；完整输入字段被保留；多选与自由文本原样映射；重复问题全文导致歧义时拒绝；人工接管、Hook 超时/进程退出后迟到答案无效。SDK canUseTool adapter 只接已有受控进程，不为旧 TUI 另起进程。

**退出条件**：真实原生卡片成功消费答案；坏 JSON、断连与超时都不把答案写到下一条普通 prompt。不同 onFailure/启动模式的后继状态有真实记录。

### M3：Codex 同会话问答

**代码范围**：在 `crates/norma/src/codex.rs` 既有模块旁新增 `crates/norma/src/codex/questions.rs`；扩展 `crates/runtime/src/runtime/mobile_cards/codex/control.rs` 的相关入口，通用问答登记进入 M1 registry；同 daemon endpoint 与 live thread 的连接由后台宿主维护。

**先失败的用例**：数字/字符串 RPC ID 原样响应；一个 pending 重放两次不产生两份问题；人工先答后本地 answer 被拒绝；同方法承载 connector 审批时按调用来源分流；只有 resolved/turn cancelled 时不标 Answered；响应后断连进入 Unknown，不重复提交。

**原生要求**：TUI 与代答方持有同一活请求，双客户端只允许一个答案生效；不能另起 app-server 恢复历史。工具来源无法可靠对应 itemId 时禁用代答并保留原卡片入口。

**退出条件**：M0 验证过的原生模式可在 Norma 中回答；取消、重放和人工竞争有回归。当前无 multiSelect 能力时不承诺多选。

### M4：OpenCode 同 instance 问答

**代码范围**：新增 `crates/norma/src/opencode/questions.rs` 及必要模块入口；扩展 `crates/norma/src/agent_hook/opencode.rs` 的 question 事件转发；在 Runtime 的原生启动与会话登记中加入 OpenCode，复用 M1 registry。

**先失败的用例**：同 sessionID 位于另一 endpoint/instance 时不能提交；answers 按原题序映射；单选拒绝多个值、关闭 custom 时拒绝自由文本、重复 label 不猜测；事件与 GET 对账重复出现只留一题；已处理 ID/404 不自动提交其它题。

**生命周期**：首版每张卡片/run 拥有独立的受控服务实例或 TUI 内服务，记录 endpoint 归属和进程代数；按 M0 结果选择一种启动模式。loopback 服务使用独立鉴权，后台等待就绪。App 退出/项目释放回收所拥有的 server/TUI 子进程；不关闭外部用户服务。

**退出条件**：真实问题完成单选、多选、自定义、reject 与取消验证；原 TUI 同步更新；未开手机服务时仍可代答。

### M5：通用终端交互与 Cursor 原生适配

**代码范围**：从 `crates/runtime/src/runtime/mobile_cards/terminal_control.rs`、`crates/runtime/src/runtime/mobile_cards/terminal_view.rs` 提取不依赖手机开关的输入租约与快照校验，落入 `crates/runtime/src/runtime/terminal_interaction.rs`；各 Provider 的控件识别/步骤放入 questions 子模块。通用协调服务与 Cursor adapter 一起形成可运行切片，不先重构全部终端。

**先失败的用例**：识别到的只是 scrollback 旧题时不输入；帧/选项/焦点变更后停止旧步骤；用户按键抢占后不再发下一键；多选 Space 是按键而非 paste；半段输入后断连不补发整串；不同 Provider 相似文字不能套用同一按键脚本。

**Cursor 原生路径**：在同一 PTY 读取当前题目 → 定位选项/自定义输入 → 逐步操作并读回 → 提交前核对选中结果 → 收集原问题的消费证据。先覆盖 M0 录制的本机版本；未知布局保留人工入口，不把它计入该版本的通过覆盖。

**Cursor ACP 路径**：如接入明确的托管会话，Provider transport 放在 norma/runtime 相关模块，不能直接复用“幕僚自身模型连接”充当 Agent 卡片连接。每卡片/run 独占一个 connection/session，回答 `cursor/ask_question` 原 RPC；单/多选按 option ID 提交。纯 ACP 无自由文本字段时明确不可用，不另起 TUI。此路径不替代原生验收。

**退出条件**：Cursor 原生单选、多选、自定义有真实消费证据；Codex/Claude/OpenCode 无结构化入口的已有会话按各自录制路径验证。手机现有输入租约与人工控制回归不退化。

### M6：幕僚建议、上下文与卡片交互

**代码范围**：扩展 `crates/brain/src/event.rs`、`crates/brain/src/router.rs`、`crates/brain/src/lane/prompt.rs`；建议执行放入 app brain_host 的 questions 模块；新 UI 放在 `crates/app/src/workspace/desktop/drawers/overview_questions.rs`，复用组件库与现有项目定位；扩展中英文本地化。

**上下文与授权**：新增明确的 AnswerQuestion 上下文操作，接入 `crates/norma/src/protocol/context.rs` 和 `crates/agent/src/context/` 的限制检查。采用答案需要当前问题的有效宿主授权；终端路径同时服从既有“不要向终端输入”约束。AnswerQuestion 不借用 DispatchTask 或 ApproveOperation 的授权。

**UI 行为**：同一问题展示原题、选项、建议及理由；采用、编辑、原卡片处理都操作同一 QuestionRef；只有支持的题型可提交。提交中显示进度并防连点；未确认显示“已提交，等待确认”；失败原因行内可见。`在原卡片处理` 先释放/撤销代答控制，再定位原交互。

**先失败的用例**：没有用户批准只保存建议；语音明确指向当前问题后才直接回答；上下文被更新为禁止输入时旧授权失效；旧模型结果不能覆盖新问题；采用答案不调用 DispatchAgentTasks；后续任务不在回答完成后自动派发；一次问题只计一个待处理项。

**持久化**：复用 `crates/storage/src/context_store.rs`、`crates/storage/src/brain_activity_store.rs` 的来源/活动基础；新增 `crates/storage/src/question_delivery_store.rs` 保存 durable intent 与 receipt，migration 编号按实施时 HEAD 分配。记录 delivery ID、完整目标身份、题目版本、答案摘要/受控正文、状态和 Provider 证据；同一目标与版本的占有必须幂等，安全重试复用原 delivery ID。后台持久化意图后重新校验活请求与授权，再发送；写入失败时不发送。存储层不依赖 brain/runtime，用明确 DTO 保持边界。提交意图与外部响应没有跨进程事务，恢复时查询原请求，绝不由数据库 Pending 推导“应当再发一次”；敏感答案不进入普通活动摘要。

**退出条件**：用户可在真实 App 完成采用与编辑；直接代答复用同一入口；浅/深色的 hover、pressed、focus、disabled、提交/失败状态可见。加载 apple-design 与适用 gpui-* skill 后实现 UI，钉死 API 与已有代码优先。

### M7：四家验收与灰度交付

按 Provider + 版本 + 启动模式登记能力，只有该组合通过真实验收才开放采用入口。可以分阶段开放已通过的组合，但四家默认卡片均通过前，不宣布整体完成。

- 断开某 Provider adapter 时，该 Provider 停止接管新题；其它 Provider 和原卡片继续可用。
- 关闭能力不强杀用户 CLI；对在途 Hook/请求按对应启动模式释放、对账或标记待确认，不把未发送答案伪装为成功。
- 不主动替换用户运行中的 App，也不重启其会话；发布与安装作为独立交付操作。
- 针对取消、跨会话、重复提交、权限分流和持久化恢复安排独立对抗式 review；主 Agent 核验具体反例及证据。

## 5. 测试落点与完成检查

下列新增测试文件和场景名为计划名称，落地前先登记；不要把尚未实现的命令写为已通过。

| 边界 | 计划位置 / 可观察结果 |
| --- | --- |
| CLI/协议与 Provider 转换 | `crates/norma/tests/question_protocol.rs`、`crates/norma/tests/question_cli.rs`；准确目标、完整答案、错误分类与兼容反序列化 |
| Runtime 生命周期与原生输入 | `crates/runtime/tests/unit/question_lifecycle_tests.rs`、`crates/runtime/tests/unit/question_terminal_tests.rs`；调用公开 Runtime 边界，检查状态与真实输出，不镜像私有实现 |
| 真实 Provider 协议 | `crates/runtime/tests/agent_questions_real.rs`；helpers 在 `crates/runtime/tests/support/questions/`，逐 Provider 显式启用 ignored 场景 |
| 幕僚事件与建议 | `crates/brain/tests/question_flow.rs`；建议不等于执行、取消使迟到结果失效 |
| App 上下文与按钮行为 | `crates/app/tests/unit/brain_question_actions_tests.rs`、`crates/app/tests/unit/brain_question_context_tests.rs`；一题一次提交、正确项目、输入限制、无新任务 |
| 存储恢复 | `crates/storage/tests/question_delivery.rs`；durable intent/receipt、同目标幂等占有、崩溃对账、旧句柄不重发 |
| 真实 Norma App | `test/desktop-e2e/Scenarios/brain/BrainQuestions.swift`；夹具与看图清单分别在该模块 Fixtures/checks；只在 `BrainModule.swift` 注册场景，不集中改 Catalog |

所有测试注明场景、数据构造、执行和预期，assert 带具体说明；大型单测按 crate tests/unit 挂载，不把私有函数改公开只为测试。

拟新增真实 App 场景：`brain-questions-claude`、`brain-questions-codex`、`brain-questions-opencode`、`brain-questions-cursor`、`brain-questions-conflict`。分别记录工具结果/实际输入、问题身份、最终消费证据和截图；假的 WaitingApproval 事件只能验证投影，不能替代这些真实问答。

实施中的最小检查：修改 norma/runtime/brain/agent/storage/ui 时运行对应 crate 测试；app 修改跑 `cargo check -p app` 与相关聚焦测试。最终跨 crate 集成跑 `cargo test --workspace`；GPUI 相关跑 `cargo build -p app --release`。真实 App 用对应最小场景，关键图实际查看；无需因场景额外产图扩大视觉范围。

同时按变化运行行数、禁止 allow、存储边界、性能护栏；新文件保持 ≤500 行。测试证据可按不变的代码范围复用，失败定位根因后才扩大范围。

**整体完成清单**：

- [ ] 四家本机目标版本在原生卡片里都能回答真实单选，原 Agent 消费答案后继续。
- [ ] 各家声明支持的多选、多题、自由文本均已逐项验收；不支持的能力不会暴露可执行入口。
- [ ] 采用答案不会派发新任务，也不会替代权限审批；Codex connector 同方法负例通过。
- [ ] 双击、人工先答、桌面/手机竞争、跨工作空间与同 ID 重用不会重复或错投。
- [ ] 断连、超时、关闭项目、重启与写库失败不重发不明结果；无遗留子进程。
- [ ] 问题、建议、提交和完成记录能追溯到同一目标；UI 与真实执行状态一致。
- [ ] 必需测试、四家真实 App 证据、关键视觉检查、独立审查和构建完成。

## 6. 从哪里开始

第一项实施任务是 M0 的 **Claude 原生 TUI Hook** 与 **Cursor 原生问答操作**，确认最直接路径和最不确定路径；随后完成 Codex 同 daemon 与 OpenCode 同服务验证。将结果登记后进入 M1，并以 M2 的 Claude 路径完成第一条产品纵向闭环。

以上“从哪里开始”保留原计划顺序；当前实施已经进入综合验收，继续工作应从本页顶部矩阵和最新证据记录接续，不重复已完成阶段。
