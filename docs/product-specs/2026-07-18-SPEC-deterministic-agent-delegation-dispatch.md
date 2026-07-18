# 确定性 Agent 委派快路径与真实回执 SPEC

- 日期：2026-07-18
- 状态：Superseded
- 取代者：[`2026-07-18-SPEC-norma-master-agent-runtime.md`](2026-07-18-SPEC-norma-master-agent-runtime.md)。确定性真实回执原则继续有效，旧 Dialogue/CardTask 快路径不再保留。
- 范围：Dialogue 委派结构、CardTask 委派快路径、多 Agent 原子接收、真实回执、TraceId、现有语音反馈
- 交付原则：只收紧委派分支，不改造普通对话和流式 TTS

## Problem Statement

用户希望直接说“Bella，搜索一下当前项目”，或者在一条指令中分别安排 Bella 与 Alan，而不必使用“指派一个任务给……”这类命令式表达。当前自然寻址和 Dialogue 兜底已经能把明确指令排入 CardTask，但排入 CardTask 不等于任务已经进入目标 Agent。

目前 CardTask 仍让第二个模型自由决定调用哪些工具。真实故障中，Card Subagent 只成功调用了 `list_cards` 和 `list_agents`，随后停止输出；系统只检查“是否曾有任意成功工具”，因此把任务标记成功，却从未调用真正的 Agent 批量派发能力。App 随后又把内部 `TaskQueued` 翻译成“任务已派发”，造成用户听见成功播报但 Bella 实际没有收到任务。

这个问题在多 Agent 场景中更严重：系统必须保留每个 Agent 的独立任务正文、一次性验证全部目标、避免 Agent Loop 重试造成重复任务，并在部分目标派发失败时逐 Agent 反馈真实结果。用户还要求通过同一 TraceId 跟踪 Dialogue、CardTask、逐 Agent 派发、真实回执和语音反馈，以便测试与排错。

修复不能以重构普通语音链路为代价。普通对话当前已经使用流式模型输出和增量 TTS；新增通用语音事件层、额外路由模型调用或完整回复缓冲都会扩大改动并可能增加首音延迟。性能底线是普通对话和现有流式 TTS 不增加新阶段，委派路径应因为删除第二次模型决策而更短。

## Solution

保留现有 Dialogue、普通回复流和 TTS 管线，只为委派任务建立一条确定性快路径。

Dialogue 在识别委派时产生简约、逐 Agent 的 `assignments`。每项 assignment 只包含目标呼叫名、轻量整理后的任务正文和必要的卡片引用；真实 Agent 身份、TraceId、assignment_id 与固定完成契约由代码补全并验证。普通聊天继续流式输出，不进入新的结构化回复协议。

CardTask 收到已验证的 assignments 后，不再调用 Card Subagent 模型决定是否派发，而是由代码状态机直接调用现有高层 Agent 批量派发能力。全部目标任务必须一次性进入 Norma 管理的 Agent 队列或全部不进入。每项任务以 `TraceId + assignment_id` 幂等，重复调用只返回已有状态，不会再次入队。

委派完成由确定性代码依据真实回执判断。只有全部 assignment 都取得 Submitted、Queued、Untracked 或明确 Failed 派发结果，批次才结束等待；全成功、部分成功和全失败必须保持不同语义。`Accepted` 和 CardTask `TaskQueued` 仅表示 Norma 正在处理，不是目标 Agent 回执，不能显示或播报“已派发”。语音来源的委派在整批真实派发结果齐全后，复用现有回复与 TTS 路径汇总播报一次；文字来源只显示文字。普通助手回复、Agent 终态通知和当前 TTS 播放优先级保持现状。

任务引用其他卡片时只携带稳定 card_id，不复制完整卡片内容。未来目标 Agent 可通过当前项目内的只读工具查询最新结构化工作摘要；本次快路径不为此改造普通 Dialogue 或复制终端输出。

## User Stories

1. 作为语音用户，我想说“Bella，搜索一下当前项目”，以便不用说“指派一个任务给 Bella”。
2. 作为文字用户，我想使用相同的自然寻址表达，以便语音与文字拥有一致的委派语义。
3. 作为多 Agent 用户，我想说“Bella 和 Alan，都分析当前项目”，以便一个共同任务可以分别进入两个 Agent 的队列。
4. 作为多 Agent 用户，我想说“Bella 搜索前端，Alan 检查后端”，以便一条指令能产生两个不同的 assignment。
5. 作为用户，我想让“Bella，检查 Alan 写的代码”只把 Bella 作为接收者，以便任务正文中的 Alan 不会被错误派发。
6. 作为用户，我想让跨卡片任务携带 Alan 的稳定引用，以便 Bella 后续可以定位正确的工作来源。
7. 作为用户，我想让任务正文保持简短，以便系统不会把卡片快照、完整对话或大段终端输出塞给 Bella。
8. 作为用户，我想让当前项目自动成为“当前项目”“这个项目”的工作范围，以便无需重复提供路径。
9. 作为用户，我想让普通提及“Bella 这个名字不错”保持普通对话，以便名称出现不会误触发委派。
10. 作为用户，我想让说明性问题“怎么让 Bella 搜索项目”只得到说明，以便系统不会把教程问题当成执行授权。
11. 作为用户，我想让未知、禁用或重复目标在任何副作用前被发现，以便不会派发到错误 Agent。
12. 作为多 Agent 用户，我想让任一目标预检失败时整批都不进入队列，以便不会出现无法解释的部分接收。
13. 作为多 Agent 用户，我想让成功接收后的各 Agent 独立投递和运行，以便一个 Agent 后续失败不会回滚其他 Agent 已发生的事实。
14. 作为用户，我想在任务刚进入 Norma 时只看到“处理中”，以便内部入队不会被误解为目标 Agent 已收到。
15. 作为语音用户，我想只在 Bella 返回真实派发回执后听到“已派发”，以便播报与实际状态一致。
16. 作为文字用户，我想让派发结果只显示文字，以便主动输入任务不会意外触发语音。
17. 作为多 Agent 语音用户，我想等全部目标都有派发结果后只听一次汇总，以便不会收到多段相互打断的播报。
18. 作为多 Agent 用户，我想在 Bella 成功、Alan 失败时看到“Bella 已派发；Alan 派发失败”，以便部分结果不会被笼统描述为成功或失败。
19. 作为用户，我想区分 Submitted、Queued 与 Untracked，以便知道任务是已提交、等待中还是无法跟踪完成。
20. 作为用户，我想让目标 Agent 的 Done/Failed 继续使用现有终态通知，以便委派回执不会被误解为工作已经完成。
21. 作为用户，我想让 CardTask 在委派目标明确后直接派发，以便第二个模型不能只查询卡片却忘记执行。
22. 作为用户，我想让 `list_cards` 或 `list_agents` 成功不能单独完成委派，以便只读查询不会伪装成副作用成功。
23. 作为用户，我想让模型停止输出不能自动完成委派，以便成功必须由真实回执证明。
24. 作为用户，我想让同一个 assignment 的重复派发调用只产生一个 Agent 任务，以便自动重试不会让 Bella 收到重复工作。
25. 作为用户，我想让系统仅在确认任务尚未写入 Agent 时自动重试一次，以便临时错误可以恢复而不制造重复提交。
26. 作为用户，我想在系统无法确认是否已提交时看到明确失败，以便系统不会盲目重复写入 Agent 输入区。
27. 作为用户，我想在应用退出、崩溃或重启后清空尚未提交的队列，以便旧任务不会自动恢复执行。
28. 作为排错用户，我想让被清空的任务历史保留并标记 Interrupted，以便我仍能通过原 TraceId 理解发生了什么。
29. 作为多 Agent 用户，我想在说“停掉 Bella 的任务”时只取消 Bella，以便同批 Alan 的任务继续运行。
30. 作为用户，我想在取消运行中的 Agent 时等待真实中断确认，以便系统不会假装任务已经停止。
31. 作为普通语音问答用户，我想让现有模型流与增量 TTS 完全不变，以便修复委派不会增加首音延迟。
32. 作为委派用户，我想减少一次 Card Subagent 模型决策，以便派发路径比当前实现更快、更稳定。
33. 作为维护者，我想为一次完整用户意图使用一个 TraceId，以便 Dialogue、CardTask、多 Agent 子任务、回执和播报可以串联排查。
34. 作为隐私敏感用户，我想让默认日志只记录 TraceId、稳定 ID、状态、阶段、数量和错误类型，以便任务正文与终端内容不会进入 INFO 日志。
35. 作为维护者，我想让每个 assignment 拥有稳定 assignment_id，以便可以精确去重、记录和断言逐 Agent 结果。
36. 作为维护者，我想在公开 AgentSystem 管线上复现“模型只查询但不派发”的回归场景，以便测试能证明确定性快路径绕过了错误模型决策。
37. 作为维护者，我想在 App 语音边界验证 TaskQueued 不播报、真实回执才播报，以便状态文案与音频行为受到自动保护。
38. 作为维护者，我想验证普通 Dialogue 仍只调用原有 Provider 并保持流式 delta，以便性能底线有自动化证据。

## Implementation Decisions

### 最小改动边界

- 普通 Dialogue 回复、流式文本 delta、分句 TTS、播放后端和现有 Agent 终态通知保持不变。
- 不新增通用 SpeechEvent、语音决策锁、第二套路由模型、完整回复缓冲或新的普通回复队列。
- 只修改委派任务的数据契约、CardTask 委派执行分支、真实回执完成规则、任务状态文案和 TraceId 关联。
- 现有通用 CardTask 工具循环继续服务创建、聚焦、删除等非委派任务；本规格不把所有 CardTask 重写成新的状态机。

### Dialogue 与 assignments

- Dialogue 的普通文本分支继续直接流式输出；只有委派分支通过现有高层 CardTask 工具携带结构化 assignments。
- 每项模型可见 assignment 只包含目标呼叫名、轻量整理后的 task_text 与可选 card_refs。模型不生成 TraceId、assignment_id、真实 agent_id 或自由形式完成契约。
- 代码使用当前项目 Agent 名册解析目标并补全稳定身份。未知、禁用、重复或歧义目标在 CardTask 入队前失败，不隐式创建或启用其他身份。
- 明确共同任务为每个目标复制同一轻量任务正文；明确分工为每个目标保留各自任务正文。任务正文中的其他 Agent 名称只形成 card_refs 或普通文本，不自动加入目标集合。
- 确定性直接寻址兜底继续存在。兜底已有明确目标但没有独立分工时，为所有目标构造同一任务正文；不得因为模型漏调工具而回退到普通回复。
- assignments 结构校验失败时自动纠正一次；仍失败则整批不创建，也不要求用户重复完整指令。

### CardTask 委派快路径

- CardTaskRequest 能明确区分普通 intent 与已验证 delegation assignments。
- 携带 assignments 的 CardTask 启动后直接调用现有高层 Agent 批量派发能力，不请求 Card Subagent Provider 决定工具路径。
- 委派快路径允许代码执行必要的目标预检、Agent 激活、队列接收和回执收集，但模型不能决定省略原子派发。
- 非委派 CardTask 保留现有模型轮次、工具预算、失败恢复和取消机制。
- CardTask 的委派完成契约固定为 AllAssignmentsReceipted，不由模型自由生成或修改。

### 原子接收与幂等

- 全部 assignment 必须在一次协调操作中全部进入 Norma 管理的逐 Agent FIFO 队列，或全部不进入。
- 原子接收后的内部初始状态为 Accepted。Accepted 只表示当前应用会话接管任务，不表示目标 Agent 已收到。
- 每项 assignment 由代码生成稳定 assignment_id，并以 TraceId 与 assignment_id 的组合作为幂等键。
- 对同一幂等键的重复派发返回已有任务与状态，不创建第二条 Agent 任务。
- 原子接收之后，各 Agent 的提交、排队、跟踪与运行独立推进；单个后续失败不撤销其他已发生结果。
- 应用退出、崩溃或重启时不恢复尚未提交的 Accepted/Queued 执行项；启动清理保留历史并标记 Interrupted。

### 真实回执与完成语义

- Submitted 表示任务已真实提交到目标 Agent；Queued 表示任务已进入该 Agent 的等待队列；Untracked 表示已提交但无法自动观察终态；Failed 表示该目标派发失败。
- 委派完成评估必须逐 assignment 核对回执，不能使用“至少一个工具成功”作为成功条件。
- 全部 assignment 为 Submitted、Queued 或 Untracked 时，批次结果为 Dispatched。
- 同批同时存在成功接收与 Failed 时，批次结果为 PartiallyDispatched，并保留逐 Agent 结果。
- 全部 assignment 都 Failed 时，批次结果为 DispatchFailed。
- Dispatched、PartiallyDispatched 与 DispatchFailed 都是派发阶段的终结结果，不表示目标 Agent 已完成工作。
- 任意 list、activate 或其他辅助结果不能替代 dispatch receipts；模型无工具调用、模型空回复或只读查询成功均不能完成委派。

### 安全重试与取消

- 只有底层明确证明 assignment 尚未写入目标 Agent 时，派发才自动重试一次。
- 输入已写入、Enter 是否生效不确定或目标输入区状态未知时停止自动重试，返回无法确认的失败结果。
- 取消未提交任务时从执行队列移除并保留历史；取消运行任务时发送中断，先进入 InterruptRequested。
- 只有运行时确认后才进入 Interrupted；超时未确认时明确显示无法确认是否停止。
- 多 Agent 批次中，点名取消只影响对应 assignment；明确“全部”才取消整批。

### 文字与语音反馈

- 内部 CardTask TaskQueued 和 Accepted 统一表达为“处理中”，不得生成“任务已派发”。
- 语音来源任务在全部 assignment 获得派发结果后，复用现有回复与 TTS 路径生成一次简短汇总。
- 文字来源任务只显示结果，不因派发回执主动播放 TTS。
- PartiallyDispatched 按 Agent 分别陈述成功与失败；不得用整批“成功”覆盖失败，也不得用整批“失败”覆盖已发生的成功派发。
- TaskIncomplete 或派发失败的语音来源任务播报一句简短失败结果；详细缺失回执和错误保留在文字与诊断日志。
- 目标 Agent 后续可信 Done/Failed 继续由现有 Agent task announcements 处理，不进入 CardTask 生命周期。
- 普通助手回复的流式生成、隐藏内容过滤、分句时机、TTS 请求与播放优先级不在本规格中改变。

### 跨卡片引用与查询边界

- assignment 可携带当前项目内稳定 card_refs，但不内嵌卡片内容快照。
- 卡片引用不改变委派目标集合；“Bella 检查 Alan 写的代码”仍只有 Bella 一个接收目标。
- 当前画布上下文继续只提供卡片身份与基本状态，不默认包含完整终端、网页或对话内容。
- 未来只读查询工具限定当前项目，返回带 updated_at 的结构化工作摘要；状态不可用时返回 Unavailable，不以旧快照冒充实时状态。
- 完整终端输出、完整对话、文件正文和 diff 不通过该查询工具默认复制；目标 Agent 使用自身文件与 Git 能力读取代码。

### TraceId 与诊断日志

- 一次完整用户意图使用一个 TraceId，必要澄清、自动纠错、多 Agent assignments、真实回执、终态通知和语音结果共享该标识。
- 新的独立用户指令或用户主动重新发起产生新 TraceId。
- TraceId 与任务历史持久化；重启后虽然不恢复执行，Interrupted 清理记录和诊断日志仍使用原 TraceId。
- 关键阶段至少覆盖 Dialogue 路由、CardTask 入队、assignment 原子接收、逐 Agent 提交、回执聚合、完成判断、文字结果与语音请求。
- 默认 INFO 日志只包含 TraceId、task_id、assignment_id、agent_id、阶段、状态、计数、耗时和错误类型。
- 完整 ASR 文本、任务正文和模型可见回复只在用户显式开启完整诊断且处于 DEBUG 时记录；内部 think、密钥和 Authorization 信息永不记录。
- 高频 PCM chunk、fingerprint 和逐块播放细节保留在 TRACE，不淹没关键任务链路。

### 性能约束

- 普通 Dialogue 不新增模型调用、结构化封装、同步等待、文本缓冲或 TTS 中间队列。
- 明确委派不再请求 Card Subagent 模型决定是否调用 dispatch，至少删除一次不必要的模型决策机会。
- TraceId 与 assignment_id 的生成、校验和日志字段不得在 GPUI 前台执行阻塞 IO。
- Agent 队列持久化和原子接收不得进入现有 GPUI、PTY、画布输入或音频 chunk 热路径。

## Testing Decisions

### 测试原则与主 seam

- 测试只验证用户可观察行为和公开领域结果，不断言私有函数调用顺序、完整 Prompt 字符串或内部集合布局。
- 使用两个现有高层 seam，这是覆盖跨 crate 用户闭环所需的最小数量：公开 AgentSystem 管线验证自然输入到高层派发回执；App 语音适配边界验证事件到文字/TTS 的可观察结果。
- AgentSystem seam 使用可编程 Dialogue Provider、可编程 Card Provider、记录型 CardToolHost 与内存 Journal，延续现有 natural-agent-delegation、agent-system-pipeline 和 agent-system 单测风格。
- App seam 使用可控事件输入、假的本地 TTS 与可观察播放请求，延续现有 Agent task announcements 和 Voice reply 测试风格。
- 因验收涉及语音、Agent、Canvas 与持久化闭环，必须更新 crates/e2e 的自动化用例；若没有启动真实 `.app`，报告必须明确属于 headless/L2，不得声称完成 L3 真机语音验收。

### 必须覆盖的 AgentSystem 行为

- 单 Agent 自然委派产生唯一 assignment，目标为 Bella，任务正文不包含多余呼叫词。
- 多 Agent 共同任务产生两个独立 assignment，正文语义相同且 assignment_id 不同。
- 多 Agent 独立分工产生 Bella 与 Alan 各自不同的任务正文。
- “Bella 检查 Alan 写的代码”只生成 Bella assignment，并携带 Alan card_ref；Alan 不成为目标。
- 普通提及、说明性问题、未知名称和禁用 Agent 不产生派发副作用。
- 任一目标预检失败时，所有目标均不存在 Accepted/Queued 任务；必须同时断言失败目标未入队和有效目标也未入队。
- 回归场景中 Card Provider 被配置为只返回 list_cards/list_agents 后停止；委派快路径仍只执行一次 DispatchAgentTasks，且不依赖 Card Provider 完成派发。
- 委派任务不得因 Listed、AgentsListed、Activated 或模型空回复而成功。
- DispatchAgentTasks 返回的回执数少于 assignments 数时，任务不得进入 Dispatched，并明确列出缺失 assignment。
- 全部正向回执产生 Dispatched；正向与 Failed 混合产生 PartiallyDispatched；全部 Failed 产生 DispatchFailed。
- 对相同 TraceId 与 assignment_id 重复执行只产生一条 Agent 任务和一个首次接收事实。
- 安全的提交前失败最多重试一次；不确定提交失败不自动重试。
- 应用重建时不恢复 Accepted/Queued 执行项，历史记录进入 Interrupted 且保留原 TraceId。
- 点名取消只影响对应 assignment；同批其他 assignment 状态不变。
- 每个测试遵守项目测试注释结构，明确数据构造、执行步骤、正断言、负断言及所有预算数值推导。

### 必须覆盖的文字与语音行为

- TaskQueued/Accepted 只能产生“处理中”文字，不产生“任务已派发”文字或 TTS 请求。
- 语音来源的单 Agent 真实回执产生一次派发结果播报。
- 多 Agent 回执分批到达时界面逐个更新，但全部结果齐全前不产生汇总 TTS；齐全后只产生一次。
- PartiallyDispatched 的文字与语音同时包含成功 Agent 和失败 Agent，不允许只出现一侧。
- 文字来源的派发结果不产生 TTS 请求。
- TaskIncomplete/DispatchFailed 的语音来源任务产生简短失败播报，详细错误不进入播报正文。
- 目标 Agent 后续 Done/Failed 仍按现有开关和优先级播报，不被派发回执重复消费。
- 用户说话、录音或普通助手回复期间，任务通知保持等待；用户主动打断当前通知后不重播。

### 性能与回归证据

- 普通 ModelOnly Dialogue 仍以现有方式向 live sink 发送文本 delta，并且不会创建 delegation assignments 或启动 CardTask 快路径。
- 普通语音回复的 Provider 调用次数、首个可播报 segment 形成条件和 TTS 请求路径与改动前相同。
- 明确委派路径不调用 Card Provider 规划 dispatch；测试必须负断言 Card Provider 请求数为零或保持在非委派所需范围。
- 关键日志测试或可控诊断 sink 验证同一 TraceId 出现在路由、assignment、回执和播报阶段，同时默认日志不包含构造的任务秘密文本。
- 修改 Agent 领域后运行 agent 测试；修改 App 语音适配后运行 app 检查；更新用户闭环后运行 e2e。跨 crate 状态或 schema 变化完成后运行 workspace 测试与项目机械化约束。

## Out of Scope

- 重构普通 Dialogue、模型流、分句 TTS、播放后端或语音窗口动画。
- 新增通用 SpeechEvent 总线、语音决策锁、第二套路由模型或完整回复缓冲。
- 让目标 Agent 的工作完成等同于 CardTask 派发完成。
- Agent 间实时通信、共享模型上下文、主从协作或共同编辑协议。
- 应用重启后自动恢复尚未提交的 Agent 队列。
- 在 assignment 中复制完整终端、完整对话、文件正文或 diff。
- 本次交付完整实现目标 Agent 可调用的跨卡片只读查询工具；本规格只固定 card_refs 与未来查询边界。
- 重写所有非委派 CardTask 的 Agent Loop、预算或工具选择策略。
- 修改 Agent Provider、ASR、TTS 模型配置或 Voice Settings。
- 发布外部 issue 或创建远端 tracker 条目；本次按用户要求仅保存本地 Markdown。

## Further Notes

- 本规格使用根领域词汇表中的 Dialogue 路由、CardTask 完成循环、多 Agent 委派、Agent 委派完成、指令 TraceId、Agent 任务回执和 Agent 任务语音通知作为 canonical 语言。
- 本规格以“只收紧委派分支”的最小方案覆盖较早自然委派规格中让 Card Subagent 模型继续决定是否派发的冲突部分。
- 本规格也覆盖早期“TaskQueued 可以表达任务已派发”的旧语义：TaskQueued 与 Accepted 只能表达内部处理中，真实 Agent 回执才允许表达派发结果。
- 原子接收、TraceId、默认日志隐私与 Dialogue/CardTask 边界分别遵循现有相关 ADR；已放弃的通用语音事件 ADR 不属于本规格。
- 当前故障日志已经证明 ASR、Dialogue 路由、CardTask 入队和 TTS 播放可正常工作，缺失环节是 CardTask 未执行 DispatchAgentTasks。因此实现和验证应优先围绕确定性委派快路径，而不是改造语音播放架构。
