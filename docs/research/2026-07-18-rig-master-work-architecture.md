# Rig 驱动的 Master + Work 架构调研

> 调研日期：2026-07-18
> 范围：只判断 Rig 适合承担哪些职责，以及 Norma OS 仍需保留哪些持久领域事实；不包含实现。

## 后续讨论决定

首期不建设 `WorkTask`、`WorkTaskStore` 或通用 `WorkerExecutor`。`MasterAgent` 通过 Rig typed tool 直接把任务分配给 AgentCard，并复用现有每 Agent FIFO、Runtime 与 Hook 回执链路；`Work` 只保留为未来需要独立持久生命周期时的扩展概念。`MasterAgent` 同时取代当前 LLM 层的 Dialogue 与 CardTask 双循环，`crates/dialogue` 只保留语音会话状态。下文关于持久 Work 的分析用于说明未来启用该扩展时 Rig 不能替代的职责，不代表首期范围。

Master 的对话上下文首期只存在于当前项目与当前应用运行周期，语音和文字共享；项目关闭、切换或应用重启后清空，不接 SQLite 持久会话存储。

`NormaSession` 对同一份会话维护两个视图：用户可见历史只呈现用户输入、Norma 最终答复和应用生成的确定性状态通知；Rig 协议历史保留完整的 assistant tool call 与 tool result 配对及 `task_id`，用于合法的多轮工具上下文和后续自然语言指代。上下文裁剪以完整回合及 tool-call/result 配对为原子单位，不留下孤立工具消息。首期不增加摘要记忆、向量记忆或新的持久化 Memory 抽象。

Master 首期保留现有 Canvas 卡片查询、创建、聚焦、删除、发送文本，以及 Agent 管理和任务分配工具；迁移不引入新的工具类别。

Provider 首期只保留现有 OpenAI-compatible 与 Ollama，设置页面和配置语义不变；Rig 只替换底层请求、streaming 与 tool calling，不开放其他 Rig Provider。

Provider 实现收口到 Rig 原生 OpenAI-compatible 与 Ollama Provider。Norma 只保留一个薄 `ModelFactory`，负责把现有 Settings 中的模型名、Base URL、API Key 等配置映射为 Rig model；旧的自写消息协议、HTTP streaming 解析与 tool-call 解析不再作为第二套请求链路保留。若隔离 spike 证明 Rig 缺失某项现有配置能力，只为该缺口增加薄适配，不恢复完整旧 Provider 循环。

Master 的固定身份名为 Norma。她的身份跨项目保持不变，但不属于用户 Agent 定义，不出现在 Agent 列表或画布，也不允许用户删除或重命名；切换项目只重建绑定当前项目的运行会话。她继续通过现有对话文本、TTS 与任务通知路径交互。

`Norma` 是系统保留呼叫名，用户可以显式点名她，但普通输入无需先呼叫；用户 Agent 不能使用该名称，显式呼叫 Norma 也不会把她加入 Agent 卡片委派目标。

用户按 Agent 呼叫名分配任务，不要求 AgentCard 已预先存在；已启用但未激活的 Agent 由 Norma 自动激活唯一卡片并排队，未知或禁用目标失败且不隐式创建 Agent 定义。

每项目最多一个活跃 Norma 回合；新的语音或文字输入取消当前生成与 TTS 后开始新回合，已经取得可信派发回执的 AgentCard 任务继续执行。工具副作用无法确认时停止并报告，不自动重放。

AgentCard 后台终态首期采用确定性通知：Hook 更新状态并由模板驱动 UI/TTS，不自动唤醒 Norma 或调用 LLM；只有用户追问、要求总结或安排下一步时才开始新的 Norma 回合。

Rig 首期使用高层 `AgentRunner` / `stream_prompt` 驱动模型、工具循环与 streaming；不手写 `AgentRun` driver，也不持久化 `AgentRun` 快照。只有出现“崩溃后从一次 LLM 工具调用中间恢复”的明确需求时才下沉到 sans-I/O 状态机。

异步执行采用一个应用级 Tokio runtime，当前项目对应一个可取消的 `NormaSession` async task；GPUI 前台通过有界 channel 收发命令与事件，只更新状态，不等待网络、模型或工具执行。切换项目时取消旧 session 并创建新 session。

Master 的模型可见工具采用多个明确的 Rig typed tools，而不是单个 `workspace_action` / `card_task` 大工具。首期工具分别表达卡片查询、创建、聚焦、删除、发送文本，以及 Agent 管理、任务分配、状态查询和取消；它们共享同一个 App/Runtime adapter，不复制执行逻辑。

首期 Norma 工具调用采用串行执行（Rig `tool_concurrency(1)`）：查询、Canvas 变更与 AgentCard 任务派发均按模型输出顺序执行，避免 `create -> focus -> send_text` 以及连续派发之间出现竞态。这个约束只作用于 Norma 当前回合的工具编排；任务成功派发后，不同 AgentCard 仍由既有的独立队列并行执行。后续若有明确收益，可仅为无副作用的查询工具开放并发。

Norma 采用“先执行、后表态”的工具交互：一旦当前生成包含工具调用，工具调用前的模型文本不作为最终答复、不写入最终对话历史，也不进入 TTS。执行期间 UI 只展示由应用生成的确定性状态（例如“正在分配给 Bella…”）；工具结束后，Norma 根据真实结果生成并播报最终答复，避免在副作用实际完成前作出承诺。

AgentCard 任务采用异步接收语义：`assign_agent_task` 在任务被目标 AgentCard 的 Runtime 队列接受后即返回成功，并携带可追踪的任务标识，不等待 Agent 执行完成。Norma 可在同一回合按顺序派发多个 AgentCard；每次派发独立提交，允许部分成功且不回滚已经接受的任务，最终答复必须逐项反映真实接收结果。执行终态继续通过确定性 UI/TTS 通知呈现，不自动唤醒 Norma。

`assign_agent_task(agent, task)` 是单目标 Rig typed tool。多 Agent 请求由 Norma 为每个目标顺序调用一次，每次独立返回 `task_id` 与接收结果；首期不向模型暴露批量原子派发工具。底层继续复用现有 Agent 协调器与每 Agent FIFO 队列，单目标工具不是新的队列实现。

`assign_agent_task` 是直接执行型工具：只要当前用户请求中的目标 Agent 与任务意图可识别，Norma 必须在同一回合立即调用，不得询问执行确认，也不得为补充实现细节或验收标准而阻塞派发。已启用但未打开的 Agent 自动打开唯一 AgentCard 后入队；只有目标未知、禁用或确实存在指代歧义时才失败或提出一次最小澄清。工具返回后的“已入队”属于事实回执，不是执行前确认。

模型工具集中不提供独立 `open_agent`。没有任务的“只打开 AgentCard”不属于 Norma 首期能力；任务派发需要卡片时，由 `assign_agent_task` 在 Adapter 内部自动打开目标 Agent 的唯一 AgentCard。已经存在的卡片仍可通过通用聚焦能力处理。

Norma 的固定系统提示词必须声明直接呼叫路由：当输入以已知 Agent 的正式名称或语音别名开头，后接逗号、冒号等称呼标点与可执行请求时，即使没有“分配/委派”关键词，也必须立即调用 `assign_agent_task`。例如“alan，请你检查一下当前项目有哪些技术栈”应生成 `agent="alan"`、`task="检查当前项目有哪些技术栈"`，不确认、不由 Norma 代为分析。任务清洗只移除句首称呼、客套和无语义填充，必须保留目标、范围、顺序、约束、交付物与显式命令。Agent 名仅出现在句中、关于 Agent 的说明性提问或没有可执行请求时，不得误判为委派；最终目标仍由 Adapter 按当前项目 Agent roster 确定性校验。

Norma 的 prompt assembly 由固定 system prompt 与动态 `<environment>` 区域组成。每个用户回合开始前重新生成环境快照，首期默认包含当前项目显示名、Agent roster，以及当前画布卡片的 `node_id`、类型、标题、焦点和 Agent 所属关系；不注入 Terminal 内容、密钥或其他非必要数据。环境区不使用 JSON 或需要另行记忆的压缩 DSL，而采用简短、自解释的 Markdown：以“当前项目 / 当前 Agent / 当前画布卡片”为标题，每个对象用一条自然语言短句表达，省略无值字段但不使用含义不明的裸状态码。所有动态值必须单行转义、截断并视为不可信数据，不能被解释为指令；每个 section 有数量/长度上限。后续选择状态、能力或任务摘要可新增明确命名的 section，而不修改 Norma 固定身份。

由于每回合默认注入最新的工作区/画布环境快照，首期不再向模型暴露 `inspect_workspace`；工具执行返回的结构化结果负责说明本回合产生的状态变化，真正执行副作用前 Adapter 仍需重新校验目标，避免依赖过期快照。

`create_agent` 的模型入参只包含 `profile`，不接受 `name` 或 `activate`。用户通常只需说“创建一个 Codex / Claude Code / OpenCode / Cursor Agent”；Norma 将常用说法映射为受支持的 profile 并直接调用，Adapter 从当前内置 Agent 名称池中确定性选择未占用名称，创建持久定义、启用并打开唯一 AgentCard。多 Agent 创建请求拆成多次顺序调用，允许部分成功且不确认；名称池耗尽或 profile 不支持时返回明确错误，禁止模型临时发明名称。

首期最终模型工具集收敛为 8 个：查询任务 `find_agent_tasks`；Agent 操作 `create_agent`、`assign_agent_task`、`cancel_agent_task`；Canvas/Terminal 操作 `create_terminal`、`focus_card`、`send_terminal_input`、`delete_canvas_card`。当前 Agent 与画布卡片由每回合 `<environment>` 默认注入，因此不提供 list/inspect 工具；不提供 `open_agent`，也不提供通用 `card_task` / `workspace_action`。

AgentCard 任务始终以稳定 `task_id` 精确寻址，但用户不必记忆该标识。Norma 可先按 Agent 与未结束状态查询：只有一个匹配任务时直接查询或取消；存在多个匹配任务时列出简短摘要并要求用户明确选择；用户显式提供 `task_id` 时直接精确操作。已结束任务不可取消，排队任务可立即转为 `cancelled`，运行中任务只能先转为 `cancel_requested`，收到 AgentCard 的终止确认后才可报告 `cancelled`。

首期持久化的是 AgentCard 任务审计记录，而不是可恢复的工作生命周期。继续复用项目 SQLite 中已有的 `agent_tasks` 等审计表，用于历史、状态查询与诊断；项目切换、窗口关闭或应用重启时，不恢复排队或运行中的任务，未完成任务统一转为 `interrupted`。跨重启恢复、租约和重试属于未来持久化 Work 扩展。

最终 crate 边界不新增 `master_agent` crate：`crates/agent` 内聚 Norma/MasterAgent、`NormaSession`、Rig `ModelFactory`、typed tools、`AgentCardAdapter` trait 以及任务状态/审计 DTO，并继续保持零 GPUI、零 Storage；`crates/app` 实现 `AgentCardAdapter`，负责通过有界 channel 连接 GPUI/Runtime 与 UI/TTS 事件；`crates/dialogue` 只保留 VAD/ASR/TTS/语音会话状态；`crates/storage` 实现任务审计存储，`agent` 不反向依赖 `storage`。Rig 具体类型不得扩散到 `app`、`runtime` 或 `storage`。

目标状态不长期维护兼容的双架构：`crates/llm` 最终由 `crates/agent` 内的 Rig Provider/`ModelFactory` 取代并移出 workspace，`crates/card_task` 因 Work 延后而移出 workspace，`crates/dialogue` 收缩为纯语音状态机。迁移期适配仅是短暂施工手段；完成验收要求生产依赖不再引用 `llm` 或 `card_task`。

迁移采用“渐进施工、单点切换、功能内清理”：先在隔离测试中完成 Rig Provider、`NormaSession` 与 typed tools，旧生产入口期间仍只走旧链路；新链路完整后一次性把文字和语音入口切到 Norma，不提供用户可见的双 Runtime 开关。切换验证通过后，必须在本次功能的最后阶段删除旧 crate、旧 wiring 与只服务旧路径的测试替身，并完成全仓验证；清理结束前功能不得标记完成，也不得把删除工作延期到未来需求。发布回退依靠版本控制或版本回滚，而不是永久兼容代码。

所有变更类 typed tool 都携带模型不可见的 `operation_id`，由 `NormaSession` 根据 `turn_id + tool_call_id` 生成并通过 Rig 扩展上下文传给 Adapter。同一 `operation_id` 在当前应用运行周期内重复提交时返回第一次的回执，不再次执行副作用；明确失败可进入正常错误处理，超时且无法确认副作用时必须返回 `unknown`、终止当前工具链且禁止自动重试。首期不要求跨重启幂等，持久幂等属于未来 Work。

`delete_card` 属于受限破坏性工具：只有当前用户请求明确表达删除意图且目标可确定时才能执行；“清理画布”等宽泛请求必须先列出候选并要求用户明确目标，Norma 也不能为了完成其他目标自行推断应先删除卡片。该权限规则由 Rig hook/tool validation 在工具边界确定性校验，不能只依赖 system prompt。创建、聚焦、发送文本与 AgentCard 任务派发首期无需额外确认。

Rig 首期沿用现有 `CardTaskBudget` 的行为基线：单个 Norma 回合最多 12 次模型轮次、20 次工具调用、3 次连续工具失败、60 秒总墙钟时间，单次 Provider 请求超时 30 秒；用户取消始终拥有更高优先级。任一预算耗尽后停止后续模型与工具调用，不回滚已经成功的副作用，无法确认的副作用保持 `unknown`，并向 UI/TTS 输出明确的超时或预算错误。默认值在获得新链路真实观测数据后再调整。

Provider/模型配置采用会话内热切换：保存新配置时先取消当前 Norma 回合与 TTS，`ModelFactory` 成功创建新 Rig model 后原子替换旧实例，同时保留当前项目的用户历史与 Rig 协议历史，并按新模型上下文上限重新裁剪。新配置初始化失败时保留旧 model 并报告错误。Norma 使用的模型必须支持 structured tool calling；不支持时明确拒绝切换，不允许静默降级为无法执行工具的聊天模式。

迁移按六个阶段推进且不得跳过最终清理：① 隔离验证 Rig 的两类 Provider、streaming、structured tools、取消与热切换；② 在 `crates/agent` 建立 `NormaSession`、身份、历史、预算与 `ModelFactory`；③ 以 fake adapter 验证 typed tools、幂等、权限与错误语义；④ 实现 App/Runtime `AgentCardAdapter` 并补跨 crate/e2e；⑤ 同时切换文字与语音入口并验证完整用户流程；⑥ 删除 `crates/llm`、`crates/card_task`、旧循环/wiring/依赖/测试替身并运行全仓与架构护栏。只有六阶段全部通过，功能才算完成。

## 结论

建议采用 `rig-core 0.40.x` 作为 `MasterAgent` 的 Agent 内核，替换 Norma 当前自写的 Provider 消息模型、流式解析、tool schema、模型/工具多轮循环与轮次预算。

Norma 仍需保留一个很薄的持久 `WorkTask` 模型和 Agent 卡片协调逻辑。Rig 的可序列化 `AgentRun` 是“一次 Agent 推理运行”的技术状态，不是用户可查询、取消和长期追踪的任务记录；Rig 官方资料也未提供持久任务队列、任务 lease、跨进程调度、任务级幂等或外部 Agent 生命周期协调。

首阶段不建议自建通用 `WorkerExecutor`。现有 worker 只有 AgentCard 一种真实执行方式，应直接实现一个 Rig typed tool / adapter；等第二种生产 worker 出现后再抽取通用执行 seam。

## Rig 可以直接承担的职责

- `Agent` / `AgentBuilder` 组合模型、preamble、context 与 tools；Agent 本身也能作为另一个 Agent 的 tool，用于 Rig 的 manager-worker 模式。
- blocking 与 streaming prompt/chat；多轮工具调用由 `AgentRunner` 驱动。
- typed tools、并行 tool execution、tool-call 参数与结果处理。
- `max_turns` 与无效工具调用重试预算。
- `AgentHook` 对模型调用和工具调用进行观察、限制、参数修正或终止。
- `ToolCallExtensions` 携带 `project_id`、`trace_id`、权限和会话信息，不暴露给模型。
- `ConversationMemory` 加载与追加按 conversation id 隔离的历史；Norma 可实现自己的 SQLite backend。
- `AgentRun` 是 sans-I/O、可逐步推进且实现 `Serialize + Deserialize` 的 Agent 循环状态机。

来源：

- [Rig 0.40 AgentRun 文档](https://docs.rs/rig-core/0.40.0/rig_core/agent/run/index.html)
- [Rig 0.40 Agent 文档](https://docs.rs/rig-core/0.40.0/rig_core/agent/struct.Agent.html)
- [Rig 0.40 StreamingPromptRequest](https://docs.rs/rig-core/0.40.0/rig_core/agent/struct.StreamingPromptRequest.html)
- [Rig manager-worker 官方说明](https://book.rig.rs/playbook/multi-agent-systems.html)
- [Rig ConversationMemory 说明](https://docs.rs/rig-core/0.40.0/rig_core/)
- [Rig 0.40 changelog](https://docs.rs/crate/rig-core/0.40.0/source/CHANGELOG.md)

## Rig 不替 Norma 解决的职责

官方 `AgentRun` 协议只定义 `CallModel`、`CallTools` 与 `Done`。它允许驱动方在步骤间序列化状态，但没有定义：

- 持久 Work 队列与每 Agent FIFO；
- task claim / lease / heartbeat；
- AgentCard 激活、PTY 输入提交和 Hook 终态协调；
- 用户级取消、卡片关闭与项目关闭语义；
- `TraceId + assignment_id` 的副作用幂等；
- 多 Agent 原子接收与逐 Agent 回执；
- UI/TTS 所需的可信派发和完成事实。

因此，应把 `WorkTask` 保持为 Norma 自有 SQLite 记录。Rig tool 负责调用 Norma 的工作接口，返回真实的 `Submitted / Queued / Untracked / Failed` 回执；长期 `Done / Failed / Interrupted` 继续由 Agent Hook 和 Runtime 证据更新。

## `AgentRun` 持久化的限制

Rig 明确说明：序列化状态包含完整对话，具有相同的敏感性；序列化格式尚无跨版本稳定保证，恢复时必须使用挂起该 run 的同一 Rig 版本。

另一个风险是副作用窗口：若 AgentCard 已接收任务，但进程在 `tool_results` 被写回并持久化之前崩溃，直接重放 pending tool call 可能重复提交。因此即便使用 `AgentRun` checkpoint，AgentCard tool 仍必须基于 Norma 的 `TraceId + assignment_id` 幂等。

建议首阶段只持久化 conversation history 与 `WorkTask`，Master 的进程内 run 崩溃后终止该轮；已经成功创建的 Work 由幂等键查回。只有确认需要“恢复到一轮 LLM 的中间 tool step”后，才增加带 `rig_version` 的不透明 `AgentRun` checkpoint。

## 推荐的最小架构

```text
User text / ASR final
        │
        ▼
MasterAgent (Rig Agent)
  ├─ Rig ConversationMemory adapter → SQLite
  ├─ Canvas typed tools
  ├─ Agent management typed tools
  └─ SubmitWorkTool / QueryWorkTool / CancelWorkTool
                         │
                         ▼
                  WorkTaskStore
                         │
                         ▼
                 AgentCardAdapter
                         │
                         ▼
             Runtime / PTY / Agent Hooks
```

`MasterAgent` 内保留现有确定性寻址和目标校验规则，并可通过 Rig hook / tool validation 强制执行；不再保留第二个 CardTask 模型循环。`AgentCardAdapter` 是 app/runtime seam 上的异步 adapter，通过 channel 把后台 Rig tool 调用转交给 GPUI 壳层。

## 版本与接入风险

- `rig-core 0.40.0` 在 Agent run、hook 和预算接口上包含 breaking changes；必须精确 pin 版本，并把 Rig 类型限制在 Agent 实现内部。
- `rig-sqlite` 是向量存储集成，不是开箱即用的 SQLite `ConversationMemory` backend；Norma 仍需用现有 storage 设施实现该 trait。
- Rig 的 Agent-as-Tool 会在同一调用链中等待子 Agent 响应；Norma 的长期外部 AgentCard 不应直接建模为这种进程内子 Agent。
- Rig 是 async-first；需要在后台 runtime 驱动，不能在 GPUI 前台 `block_on`。

## 建议验证点

在决定迁移前只做一个隔离 spike：一个 Rig `MasterAgent`，一个流式普通回复，以及一个幂等 `SubmitWorkTool`。验证动态 Provider、取消、tool-call 参数、背压、GPUI channel seam、SQLite conversation memory 与依赖增量；不接全量 Canvas tools，也不迁移旧状态机。
