# 实时对话 Agent 与卡片 Subagent 统一架构规格

- 日期：2026-07-10
- 状态：Superseded
- 取代者：[`2026-07-18-SPEC-norma-master-agent-runtime.md`](2026-07-18-SPEC-norma-master-agent-runtime.md)。本文的 Dialogue/CardTask 双模型架构不再实施，仅保留历史背景。
- 范围：实时语音对话、一次性语音指令、卡片任务派发、Card Subagent、模型 Provider 与任务审计
- 2026-07-15 对齐说明：持久 Agent 身份、Cursor runtime profile 与高层委派规则以后续 [`2026-07-14-SPEC-voice-addressable-agents.md`](2026-07-14-SPEC-voice-addressable-agents.md) 为准。

## Problem Statement

Norma OS 当前的 Always Listening 实时对话可以完成本地语音识别、模型回复和语音合成，但画布操作依赖主聊天模型直接选择底层卡片工具。真实本地模型测试表明，同一类“创建卡片”表达经常只得到口头确认，而没有产生工具调用，因此用户会听到“已经创建”，画布却没有任何变化。

现有实现还把模型通信、实时对话、一次性语音规划和卡片任务分散在多个模块中。主对话、卡片执行和任务回调需要由 app 壳层手工连接，模型 Provider、消息协议和工具调用存在重复实现。Mock 测试能够证明工具调用之后的桥接逻辑，却不能证明真实模型会可靠派发任务。

用户需要一种实时、连续且可信的交互方式：主对话 Agent 只负责自然对话和派发一个高层卡片任务；每个卡片任务由独立 Card Subagent 使用完整白名单工具在后台执行；实际结果再回灌主对话。任何成功播报都必须来自真实工具结果，不能来自模型的无依据声明。

## Solution

建立项目级 `AgentSystem`，统一承载模型 Provider、主对话 Agent、全局串行 CardTask 队列、每任务独立 Card Subagent、任务结果回调和 Agent 事件流。

主对话 Agent 只暴露一个高层 `card_task` 工具，支持启动、取消和查询任务。启动操作只传递自然语言意图，不包含底层卡片步骤。工具处理器立即返回 queued receipt，使主对话可以继续监听和回复，不等待卡片任务完成。

CardTask 进入项目级串行队列后，为该任务创建一个短生命周期 Card Subagent。Subagent 拥有 `list_cards`、`create_card`、`focus_card`、`send_text` 和 `delete_card` 白名单工具，可以执行提交文本和终端命令，也可以删除卡片。所有工具调用都必须经过 Rust 参数校验、项目与任务身份校验、幂等校验和 GPUI 前台 adapter，最终转换为统一 `CardCommand` 后修改 Runtime。

主对话使用流式模型输出和分句 TTS；Card Subagent 第一版使用非流式 tool loop。任务派发和最终结果主动播报，中间进度只更新状态，用户主动查询时才播报。任务取消只停止后续工具调用，不回滚已发生的副作用。

模型相关实现统一收敛到 `agent` 模块，包括 Ollama 与 OpenAI 兼容 Provider、消息协议、流式解析、tool loop、上下文管理和预算控制。原有独立的 LLM、Dialogue、CardTask 和 Voice 指令规划模块最终由 `agent` 替代；音频采集、VAD、ASR 和 TTS 继续由 `voice_sherpa` 负责。

一次性语音和实时对话共用同一个 CardTask 引擎。实时对话由主 Agent 判断并调用 `card_task`；一次性语音直接把最终转写作为 CardTask 意图派发，不再维护第二套 VoicePlan 执行路径。

## User Stories

1. 作为使用实时语音的用户，我希望自然聊天保持低延迟，以便像正常对话一样连续交流。
2. 作为使用实时语音的用户，我希望可以直接说“创建一个终端”，以便在不使用鼠标和键盘的情况下操作画布。
3. 作为使用实时语音的用户，我希望可以创建 Codex、Claude、Cursor 或 OpenCode Agent 卡片，以便快速派发工作。
4. 作为使用实时语音的用户，我希望可以让新建的 Agent 卡片立即接收并提交任务，以便一句话完成创建和派发。
5. 作为使用实时语音的用户，我希望可以给现有卡片发送文本，以便继续推进已有工作。
6. 作为使用实时语音的用户，我希望可以向终端提交命令，以便完成完整的语音终端操作。
7. 作为使用实时语音的用户，我希望可以聚焦某张卡片，以便后续指令围绕该卡片进行。
8. 作为使用实时语音的用户，我希望可以删除卡片，以便通过语音整理画布。
9. 作为使用实时语音的用户，我希望普通聊天不会误触发卡片任务，以免画布出现意外副作用。
10. 作为使用实时语音的用户，我希望模型只有在真实任务进入队列后才说“已派发”，以免把语言承诺误认为执行结果。
11. 作为使用实时语音的用户，我希望模型只有在真实工具执行成功后才说“已完成”，以免收到虚假的成功反馈。
12. 作为使用实时语音的用户，我希望卡片任务在后台运行时仍能继续聊天，以免较慢的任务阻塞实时对话。
13. 作为使用实时语音的用户，我希望任务派发后立即得到简短确认，以便知道系统已经开始处理。
14. 作为使用实时语音的用户，我希望任务成功、失败或取消后得到准确摘要，以便了解实际发生了什么。
15. 作为使用实时语音的用户，我希望中间工具步骤不会逐条打断对话，以免语音反馈过于嘈杂。
16. 作为使用实时语音的用户，我希望可以询问“刚才任务进行到哪了”，以便按需获取进度。
17. 作为使用实时语音的用户，我希望可以说“取消刚才任务”，以便停止尚未执行的后续步骤。
18. 作为取消任务的用户，我希望系统明确说明已经产生的副作用不会回滚，以便正确理解当前画布状态。
19. 作为连续派发多个任务的用户，我希望任务按派发顺序串行执行，以免多个 Subagent 相互干扰。
20. 作为正在手动操作画布的用户，我希望后台任务不会锁住整个画布，以便继续创建、移动或删除卡片。
21. 作为正在手动操作画布的用户，我希望 Subagent 遇到目标变化时重新校验并有限恢复，以免使用陈旧快照错误操作。
22. 作为关闭 Always Listening 的用户，我希望已经派发的 CardTask 继续运行，以免关闭麦克风意外取消工作。
23. 作为重新开启 Always Listening 的用户，我希望听到尚未播报的任务最终结果，以便恢复对任务状态的了解。
24. 作为关闭项目的用户，我希望该项目尚未完成的任务被停止，以免任务跨项目修改状态。
25. 作为 App 异常退出后的用户，我希望未完成任务被标记为 Interrupted，以免系统错误地假装任务仍在运行或已经完成。
26. 作为 App 重启后的用户，我希望旧任务不会自动续跑，以免无法判断崩溃前的副作用是否已经发生。
27. 作为需要审计的用户，我希望任务原始意图、状态和每次工具结果被持久化，以便回看系统实际做过什么。
28. 作为注重上下文的用户，我希望对话上下文在当前项目打开期间跨监听开关保留，以便继续自然追问。
29. 作为切换项目的用户，我希望每个项目拥有独立对话上下文和任务队列，以免卡片引用串到其他项目。
30. 作为重启 App 的用户，我希望主对话使用新上下文，以免陈旧指令和历史引用污染新会话。
31. 作为本地优先用户，我希望默认使用本地模型、ASR 和 TTS，以便减少云端依赖。
32. 作为需要更强模型的用户，我希望 Dialogue Agent 和 Card Subagent 可以分别配置 OpenAI 兼容 Provider，以便独立优化延迟和工具能力。
33. 作为高级用户，我希望两套 Provider 配置互不继承、互不隐式回退，以便明确知道每条链路使用哪个模型。
34. 作为使用本地 Ollama 的用户，我希望 API Key 可以为空，以便直接连接无鉴权本地服务。
35. 作为配置云端 Provider 的用户，我希望 API Key 在 UI 和日志中脱敏，以免凭据意外泄漏。
36. 作为现有一次性语音用户，我希望快捷键语音指令继续工作，并拥有与实时对话相同的卡片能力。
37. 作为现有一次性语音用户，我希望指令直接进入 CardTask，而不必先进行一轮自然聊天，以保持明确的命令语义。
38. 作为等待 Agent 卡片完成内部工作的用户，我希望系统区分“请求已发送”和“卡片内部工作已完成”，以免把派发成功误报为工作完成。
39. 作为要求完成后通知的用户，我希望 Agent 卡片完成事件可以独立回灌主对话，以便收到后续通知而不占用 CardTask 队列。
40. 作为维护者，我希望所有模型 Provider、消息和工具循环集中在一个 Agent 模块，以便一次修复适用于主对话和 Card Subagent。
41. 作为维护者，我希望 app 只处理 GPUI、音频和 Runtime adapter，以便智能决策不散落在 UI 壳层。
42. 作为维护者，我希望真实 Provider 合规测试能够发现工具漏调用，以免 Mock 测试继续掩盖生产失败。
43. 作为维护者，我希望每个任务都有硬预算，以免模型循环耗尽时间或资源。
44. 作为维护者，我希望每个 tool call 具有稳定幂等标识，以免重试或重复事件造成重复副作用。
45. 作为维护者，我希望 AgentSystem 可以使用内存 Provider、ToolHost 和 Journal adapter 测试，以便在不启动真实 UI 和网络的情况下验证完整状态流。

## Implementation Decisions

### 统一 Agent 模块

- 新增 `agent` workspace crate，作为 Norma OS 唯一的模型 Agent Runtime。
- 最终移除独立的 LLM、Dialogue、CardTask 和 Voice 指令规划 crate；迁移期间允许短期兼容，但最终架构不得保留两套生产执行路径。
- `agent` 内部分为 Provider Runtime、Dialogue Agent 和 CardTask 三个职责明确的子模块。
- `agent` 可以依赖统一卡片命令契约，但不得依赖 app、GPUI、Runtime、Storage 或音频模块。
- 领域 prompt 与工具定义分别归 Dialogue 和 CardTask 子模块；通用 Provider、消息、流式协议、tool loop 和预算归 Agent Runtime。

### AgentSystem

- 每个打开项目拥有一个项目级 AgentSystem。
- AgentSystem 统一拥有持久 Dialogue Agent 会话、项目级全局串行 CardTask 队列、Card Subagent factory、任务结果 callback queue 和 Agent 事件流。
- app 只向 AgentSystem 发送用户转写、播放结束、用户打断、监听状态、CardActivity 和项目关闭等命令。
- app 只消费文本 delta、播放停止、任务状态、最终结果和错误等 Agent 事件。
- AgentSystem 必须通过注入的 CardToolHost 执行卡片工具，并通过注入的 CardTaskJournal 持久化审计。

### Dialogue Agent

- Dialogue Agent 在项目打开期间保持对话上下文；关闭再开启 Always Listening 不清空上下文。
- App 重启或项目关闭后不恢复旧模型上下文。
- Dialogue Agent 使用流式模型输出，并向 app 发送文本 delta，供分句 TTS 尽早播放。
- Dialogue Agent 只暴露一个名为 `card_task` 的高层工具。
- `card_task` 支持 start、cancel 和 status 动作。
- start 只携带自然语言 intent；不得携带底层卡片步骤或未经验证的 node id 计划。
- start 必须立即返回 task id 与 queued 状态，不等待 Card Subagent 完成。
- Dialogue Agent 使用自动工具选择；不要求普通聊天每轮强制调用工具。
- queued 结果只允许生成“已派发”语义，最终成功语义必须来自 CardTask 终态事件。

### CardTask 与 Card Subagent

- 所有 CardTask 在项目内全局串行执行；第一版不并行运行多个 Card Subagent。
- 每个 CardTask 创建一个独立、短生命周期 Card Subagent；任务结束后销毁完整模型上下文，只保留结构化审计与摘要。
- Card Subagent 第一版使用非流式 tool loop。
- Card Subagent 拥有 list、create、focus、send text 和 delete 五类白名单卡片工具。
- send text 必须支持 submit=true，包括向终端提交命令。
- 第一版不要求用户二次确认；任务派发即授权 Subagent 完成自然语言 intent。
- Card Subagent 不得访问任意 shell、文件系统、SQLite、GPUI 或 Runtime，只能调用注入的卡片工具。
- 工具调用必须携带 project id、task id 和 tool call id，并在执行副作用前校验。
- 同一 tool call id 最多执行一次。
- 用户可以在 CardTask 执行期间继续操作画布；每次工具执行必须重新校验目标状态。
- 冲突后允许 Subagent重新 list cards 并有限恢复；连续失败达到预算后停止任务。

### 任务状态与取消

- 对外终态包括 Succeeded、Failed、CancelledWithPartialEffects 和 Interrupted。
- 运行状态至少包括 Queued 与 Running；更细的规划和工具步骤状态可以保留为内部审计事件。
- 取消只阻止尚未开始的后续工具调用，不回滚已经发生的创建、删除、提交或其他副作用。
- 取消结果必须记录已完成和未执行的步骤，并明确标记 partial effects。
- 关闭 Always Listening 不取消 CardTask。
- 关闭项目或退出 App 时停止对应任务。
- App 异常退出后，重启时把遗留 Queued 或 Running 记录标记为 Interrupted，不自动续跑。

### 任务反馈与 CardActivity

- 主动语音反馈只包含任务派发和最终成功、失败、取消摘要。
- 中间工具步骤只更新内部状态和 UI；用户通过 status 动作主动查询时才生成语音反馈。
- 用户说话或 Assistant 播放时，最终结果进入 callback queue；回到空闲 Listening 后再播报。
- 多个待播报结果可以合并为一次自然回复。
- CardTask 的成功表示卡片命令或请求已成功派发，不表示 Agent 卡片内部工作已经完成。
- Agent 卡片内部工作完成通过独立 CardActivity 事件进入 AgentSystem。
- 用户要求“完成后告诉我”时建立 CardActivity 观察关系，不让 CardTask 长期占用串行队列。

### Provider 与模型配置

- Agent Runtime 支持 Ollama 原生协议和 OpenAI 兼容接口。
- Dialogue Agent 与 Card Subagent 使用完全独立的 Provider 配置，不继承、不隐式回退。
- 每套配置独立包含 protocol、endpoint、model 和可选 API Key。
- 所有配置统一存入 app SQLite，包括明文 API Key。
- API Key 必须在 Settings UI、Debug、tracing、错误消息和设置导出中脱敏。
- 本地无鉴权 Provider 允许 API Key 为空。
- 任一 Provider 配置不可用时，只禁用对应能力并显示明确错误；不得静默切换另一套 Provider。

### Agent 执行预算

- 每个 Card Subagent 默认最多执行 12 个模型轮次。
- 每个 Card Subagent 默认最多执行 20 次工具调用。
- 每个 Card Subagent 默认最多允许 3 次连续工具失败。
- 每个 Card Subagent 默认最大墙钟时间为 60 秒。
- 达到任一预算后任务进入 Failed，保留已经产生的副作用并回报 BudgetExceeded。
- 第一版预算不进入普通 Settings，但测试可以注入更小预算。

### 持久化

- 任务审计属于当前项目，保存到项目级 SQLite。
- 至少保存 task id、原始 intent、tool call id、状态、时间、最终摘要、错误、每次工具调用和工具结果。
- 持久化不得阻塞 GPUI 前台、Runtime 路由或卡片工具热路径。
- CardTaskJournal 是 AgentSystem 的可替换 seam；生产使用 SQLite adapter，测试使用内存 adapter。
- SQLite 文件与目录依赖当前 macOS 用户权限保护明文 API Key。

### 一次性语音统一

- 一次性语音的最终转写直接作为 DirectCardTask 发送给 AgentSystem。
- DirectCardTask 不经过 Dialogue Agent，直接进入同一个 CardTaskQueue。
- 一次性语音和实时对话必须共用 Card Subagent、工具、预算、持久化、取消和 Runtime adapter。
- 旧 VoicePlan 生产执行链路在迁移完成后删除，避免双重行为来源。

## Acceptance Criteria

1. Always Listening 中说出明确创建终端指令后，主 Dialogue Agent 产生且只产生一个 card_task start 调用，并立即获得 queued receipt。
2. queued receipt 产生后，主对话可以继续接收新的用户 turn，不等待 CardTask 完成。
3. Card Subagent 使用真实卡片工具创建节点后，Runtime 中的节点数量增加，task id 与工具审计一致。
4. “创建 Codex 并发送分析 README”能够在同一 CardTask 中完成 create 与 send text submit，且 send text 指向本任务新建节点。
5. 明确终端命令能够以 submit=true 发送并进入对应终端节点。
6. 明确删除指令能够删除目标卡片，并记录不可回滚副作用。
7. 普通聊天语料不得创建 CardTask 或修改画布。
8. 主模型未产生 card_task 工具调用时，不得有任何画布副作用。
9. 主模型获得 queued receipt 后不得播报任务已经完成。
10. 只有 Succeeded 终态事件可以触发成功完成摘要。
11. 任务失败、预算耗尽和部分取消必须播报与审计中的真实结果一致的摘要。
12. 用户取消 Running 任务后，不再开始新的工具调用；已经完成的副作用保持不变。
13. 连续派发两个任务时，第二个任务在第一个任务结束前保持 Queued，不得并行调用底层卡片工具。
14. 用户在任务执行期间手动删除目标卡片时，工具执行返回结构化冲突，Subagent 可以重新查询或最终失败，不得 panic。
15. 关闭 Always Listening 后 Running CardTask 继续执行，且不再采集麦克风或播放 Dialogue 流。
16. 关闭项目后，对应 CardTask 停止；不得在其他项目执行剩余工具。
17. 重启 App 后遗留未完成任务变为 Interrupted，且不会自动调用任何卡片工具。
18. Dialogue Agent 与 Card Subagent 使用各自明确配置的 Provider；其中一个失败不得切换到另一个。
19. OpenAI 兼容 Provider 可以完成流式 Dialogue tool call 和非流式 Card Subagent tool loop。
20. Ollama Provider 可以完成相同领域契约，协议差异不得泄漏到 Dialogue 或 CardTask 子模块。
21. 主对话首个完整短句可以在整个模型回复完成前进入 TTS。
22. 用户打断时，当前模型流、待合成文本和播放被停止；已派发 CardTask 不受影响。
23. CardActivity 完成事件可以在 CardTask 已结束后独立进入主对话并生成后续通知。
24. 一次性语音与实时对话对相同任务意图产生等价的 CardTask 工具执行结果。
25. API Key 不出现在 Debug、tracing、错误消息、测试输出或未掩码的 Settings 文本中。
26. 真实 Dialogue Provider 合规测试覆盖任务表达、普通聊天、取消、状态查询和含糊表达，并能够报告任务召回率和误派发率。
27. 真实 `.app` L3 流程可以从麦克风语音开始，经过 ASR、主模型派发、Card Subagent 和 Runtime 修改，最终通过 TTS 回报真实结果。

## Testing Decisions

- 最高测试 seam 是项目级 AgentSystem。测试通过发送 AgentCommand 并观察 AgentEvent、CardToolHost 调用和 CardTaskJournal 记录来验证外部行为，不断言内部队列字段、prompt 拼接细节或私有 Provider 类型。
- AgentSystem 测试使用 Scripted Provider、Recording CardToolHost 和 Memory Journal adapter，覆盖普通聊天、任务派发、多轮工具循环、结果回灌、取消、预算、冲突恢复和幂等行为。
- Provider adapter 使用协议级测试验证 Ollama 与 OpenAI 兼容请求、流式 text delta、tool-call delta、完整 tool arguments、超时、取消和错误归一化。
- Dialogue 测试必须断言 queued 与 succeeded 语义分离，禁止在没有终态事件时生成完成声明。
- CardTask 测试必须从自然语言 intent 开始，经过独立 Subagent 的真实 tool loop，到达记录式 ToolHost；不以预先构造完整 CardCommand 列表替代核心路径。
- Storage 测试验证任务、工具结果和 Interrupted 恢复规则；测试使用临时 SQLite，不读取真实用户配置或 API Key。
- app 集成测试使用真实 Runtime adapter 验证 create、focus、send text submit 和 delete 确实修改节点状态，并验证所有模型和 I/O 工作位于后台。
- e2e 测试覆盖一次性语音与 Always Listening 两个入口，断言最终画布状态和任务审计，而不只断言函数返回成功。
- 真实 Provider 测试作为 ignored 或受环境约束测试存在，但必须能由开发者和 CI 指定环境运行；Mock 测试不能替代真实工具选择验证。
- L3 测试必须启动真实 `.app`，使用真实音频输入或可验证音频注入，覆盖 ASR、Dialogue Agent、Card Subagent、Runtime 和 TTS 完整闭环，并在报告中与 headless GPUI 测试明确区分。
- 现有本地语音工具链、后台 channel bridge、真实 Ollama 和真实 TTS 测试可作为迁移先例，但迁移后应通过 AgentSystem 接口重写，避免保留测试过去浅模块内部实现的重复用例。

## Out of Scope

- 第一版不实现用户二次确认或 AwaitingConfirmation 状态。
- 第一版不自动回滚已完成的创建、删除、终端提交或 Agent 消息。
- 第一版不并行运行多个 Card Subagent。
- 第一版不在 App 重启后自动续跑未完成任务。
- 第一版不恢复 App 重启前的主模型对话上下文。
- 第一版不使用 macOS Keychain 或对 SQLite 中的 API Key 加密。
- 第一版不允许模型绕过 CardToolHost 访问任意 shell、文件系统、SQLite、GPUI 或 Runtime。
- 第一版不要求 CardTask 等待 Agent 卡片内部工作完成。
- 第一版不把 Subagent 预算暴露为普通用户设置。
- 第一版不承诺 Browser 或 Preview 卡片的创建与内容操作；工具范围聚焦 Terminal 与 Agent 卡片及通用 focus/delete/list。
- 第一版不实现跨项目任务迁移、跨设备同步或多 App 实例协调。

## Further Notes

- “实时”在本规格中表示 Always Listening、低首响流式主对话和不阻塞聊天的后台卡片任务，不表示 Card Subagent 必须使用流式模型输出。
- “CardTask 成功”只证明 Norma 卡片操作或请求派发成功；Agent 卡片内部工作的终态必须由 CardActivity 单独表达。
- Dialogue Provider 和 Card Subagent Provider 都应提供合规检查。合规检查是能力证据，不是模型质量的永久保证，因此真实 e2e 仍然必要。
- API Key 明文 SQLite 存储是明确接受的产品取舍；实现不得通过无意义的同库密钥“加密”制造错误安全感。
- 模块迁移应采用替换而非叠加策略。新 AgentSystem 验收完成后删除旧生产路径和测试，避免同一用户意图存在两个执行来源。
- 本规格不授权当前实现直接修改；后续应基于本规格拆分实施计划或 tracer-bullet tickets，并遵循 TDD、GPUI 后台执行和 e2e 分层要求。
