# 语音指令识别与 Agent 指派链路分析

> 日期：2026-08-01
> 范围：语音 Final Transcript 到 `reply_agent` / `assign_agent_task` 的生产链路、目标匹配、上下文影响、Agent 数量增长边界、近期真实日志与项目数据库。
> 结论类型：代码事实、日志/数据库事实与推断分开标注。
> 本次只做调查，没有修改产品代码。

## 1. 结论摘要

当前误派不是派发执行层随机把 A 映射成 B。执行层使用唯一名称/别名和精确查找；一旦上游已经选择了一个存在且启用的错误 Agent，执行层无法知道语义上选错了，只会正常提交。

已确认的主要故障链是：

```text
ASR 把目标名转写错或丢失分隔符
  → 确定性直接寻址没有命中
  → 整条指令退回 LLM
  → LLM 结合忙闲、焦点、会话尾部和历史重新选择目标
  → 选择了另一个真实存在的 Agent
  → Adapter 精确校验通过
  → 错误任务被真实派发
```

2026-08-01 的真实日志完整复现了这条链：用户意图是 `Alan`，ASR/模型收到的文本是 `Allen`；直接解析无法命中。模型随后明确认为 Alan 正忙，选择了空闲的 Jasper。项目数据库中的派发记录也确认为 Jasper，而不是 Alan。

当前项目有 19 个启用 Agent，尚未触发模型环境的 32-Agent 截断。因此本次 Jasper 误派的直接原因不是 32-Agent 上限，而是“名称转写错误 + 精确路由失效 + LLM 自主改选目标”。Agent 增多会提供更多可被模型改选的合法候选，并增加上下文噪音；超过 32 个时还会出现确定的名册可见性断层。

## 2. 调查方法与证据

本报告检查了：

- ASR、Dialogue、NormaSession、直接寻址、工具 Schema、App Adapter、协调器与存储代码。
- Agent、Voice、E2E 测试与产品规格。
- 当日日志 `~/.norma/logs/norma-os.2026-08-01.log`。
- 当前项目数据库 `.multisoul/project.sqlite` 中的 `agent_definitions`、`agent_dispatch_tasks` 和 `voice_history`。
- 聚焦验证：`cargo test -p agent --test direct_address_tests --test norma_environment_tests`，14 个测试全部通过。

## 3. 从语音到派发的完整生产链路

### 3.1 音频与 ASR

持续聆听路径把 PCM 先交给 VAD。`SpeechStart` 会重置识别器，语音活动期间把 PCM 推给 ASR，`SpeechEnd` 时补一次 `end_utterance()`：[listening.rs](../../crates/dialogue/src/listening.rs#L18)。

ASR 领域事件只携带字符串：

```rust
Partial(String)
Final(String)
```

接口中没有 confidence、N-best、词级概率或候选名称信息：[recognition.rs](../../crates/asr/src/recognition.rs#L53)、[clients.rs](../../crates/dialogue/src/clients.rs#L6)。

收到 Final 后，Dialogue 只执行 `trim`、空文本校验和状态切换，不做 Agent 名称纠错、候选解析或置信度判断：[listening.rs](../../crates/dialogue/src/listening.rs#L70)、[session.rs](../../crates/dialogue/src/session.rs#L34)。

单次语音指令与持续聆听最终汇入同一 Agent 回合。单次入口把完整 transcript 交给 `begin_single_voice_command`：[dictation.rs](../../crates/app/src/app/voice/dictation.rs#L15)。

### 3.2 Dialogue 到 NormaSession

Dialogue 进入 `Thinking` 后，从 `pending_transcript` 取出原始 Final Transcript，提交：

```rust
AgentCommand::UserTurn {
    input: transcript,
    input_source: AgentInputSource::Voice,
}
```

见 [reply_loop.rs](../../crates/app/src/app/dialogue/reply_loop.rs#L103)。

每次用户回合前，App 都先发送当前 Agent roster 和 Canvas context，再发送 UserTurn：[context_commands.rs](../../crates/app/src/app/agent_runtime/context_commands.rs#L3)、[agent_runtime.rs](../../crates/app/src/app/agent_runtime.rs#L425)。

`AgentInputSource::Voice` 不参与目标选择。它随 `ToolContext` 传到 Adapter，当前主要用于语音派发/续聊后的视觉反馈：[master.rs](../../crates/agent/src/master.rs#L198)、[adapter_pump.rs](../../crates/app/src/app/agent_runtime/adapter_pump.rs#L293)。因此 Final Transcript 之后，语音和文字共享同一套目标路由。

### 3.3 NormaSession 的两条分支

NormaSession 每回合先生成动态 system prompt，再执行确定性直接寻址；只有未被直接处理时才调用模型：[master.rs](../../crates/agent/src/master.rs#L177)。

```text
UserTurn
  ├─ 直接寻址命中
  │    ├─ 活跃且未中断 → reply_agent
  │    ├─ 明确新任务 / Ended → assign_agent_task
  │    ├─ 未激活且任务自包含 → 激活并 assign_agent_task
  │    └─ 未激活且依赖“刚才/继续” → 零副作用拒绝
  └─ 未命中
       → LLM + protocol_history + 动态环境 + typed tools
```

直接分支绕过模型和模型工具循环：[direct_address_turn.rs](../../crates/agent/src/direct_address_turn.rs#L7)。

### 3.4 Adapter 与真正派发

模型或直接解析最终都会生成 `reply_agent` 或 `assign_agent_task`。模型工具的 `agent` 参数只是自由字符串，不是 enum 或 definition ID：[adapter.rs](../../crates/agent/src/adapter.rs#L283)。

App Adapter 会：

1. 按正式名称或语音别名精确解析 Agent。
2. 拒绝未知或禁用目标。
3. 新任务路径自动激活没有卡片的 Agent。
4. 协调器再次按唯一名称/别名查找。
5. 建立派发记录并把任务写入目标 AgentCard。

关键实现见 [state.rs](../../crates/app/src/settings/agents/state.rs#L100)、[commands.rs](../../crates/app/src/app/agent_runtime/commands.rs#L116)、[coordination.rs](../../crates/agent/src/coordination.rs#L251)。

名称与别名冲突会在存储和协调器层被拒绝：[agent_definition_store.rs](../../crates/storage/src/agent_definition_store.rs#L283)、[coordination.rs](../../crates/agent/src/coordination.rs#L328)。所以没有证据表明底层查找在 Agent 变多后随机串线。

但如果上游选择了另一个真实存在的 Agent，例如 Jasper，所有精确校验都会成功。当前没有“最终目标是否仍与用户 transcript 一致”的语义复核。

## 4. 当前如何匹配目标 Agent

### 4.1 确定性直接寻址

直接解析器的事实规则是：

- 只看启用的 Agent。
- 只匹配句首正式名称或 `speech_alias`；另有一个非常窄的“给 `<Agent>` 新任务”结构。
- 英文仅忽略大小写，中文必须原文相同。
- 名称后必须是结束或逗号、冒号、空格等分隔符。
- 多个前置 Agent 最多解析 8 个。
- 多个前缀同时命中时，选择最长字符串，避免 `Alan` 命中 `Alanine`。
- 不做拼音、同音词、编辑距离、发音近似或 LLM 纠错。

见 [direct_address.rs](../../crates/agent/src/direct_address.rs#L38) 和 [direct_address.rs](../../crates/agent/src/direct_address.rs#L130)。

这意味着以下文本会绕过确定性路径：

| transcript | 原因 |
|---|---|
| `Allen，检查日志` | `Allen != Alan` |
| `Grace你再看一下` | `Grace` 后没有允许的分隔符 |
| `请 Grace 看一下` | Agent 不在句首 |
| `把这个任务派给 Henry` | Agent 不在句首，且不是窄化的“给 Agent 新任务”结构 |
| `Agent A，检查项目` | 后备正式名是 `AgentA`，空格会破坏精确匹配 |

产品规格写明“让 Bella”“请 Bella”“交给 Bella”“指派给 Bella”应继续可用，并将其视作自然委派结构：[SPEC](../product-specs/2026-07-18-SPEC-natural-agent-delegation-and-canvas-context.md#L43)。当前纯解析器没有覆盖这些结构，它们依赖模型。这是规格与实现之间的明显缺口。

### 4.2 LLM fallback

直接解析未命中后，模型同时获得：

- 当前 transcript。
- 当前 Agent 名册及名称/别名、运行时、启用、卡片、忙闲和中断状态。
- 当前 Canvas 卡片、焦点和选择状态。
- Agent 会话尾部。
- Norma 持久 protocol history。
- `assign_agent_task` / `reply_agent` 等自由字符串目标工具。

固定提示词解释了何时回复现有卡片、何时新建任务，但没有建立一条硬约束：“如果疑似点名未知或近似名称，不得自行选择另一个 Agent”。提示词还要求目标和动作清楚时直接执行、不询问确认：[master_prompt.rs](../../crates/agent/src/master_prompt.rs#L15)。

因此 ASR 名称错误会把问题从“确定性路由”升级成“模型自由选择”。Agent 越多，可供模型选择的合法替代目标越多。

## 5. 上下文是否会影响匹配

答案是会，但只影响 LLM fallback；精确直接寻址命中时不会进入模型历史。

### 5.1 持久对话历史

`NormaSession` 持有 `protocol_history`。默认模型上下文按 16,384 token 估算，历史预算计算为：

```text
16,384 × 3 / 4 × 2 chars/token = 24,576 serialized chars
```

另外还有 96 条消息硬上限，超限时按旧回合裁剪：[master_history.rs](../../crates/agent/src/master_history.rs#L4)、[master.rs](../../crates/agent/src/master.rs#L260)。

风险点：

- 该预算只计算 protocol history，不包含每轮重新生成的 system prompt、当前 transcript 和工具 schema。
- 当前大量中文会话使用固定 `2 chars/token` 并不稳妥。
- 模型或 Provider 仍可能在最终请求层截断。
- 直接寻址分支在 [master.rs](../../crates/agent/src/master.rs#L208) 提前返回，没有把该用户输入和派发结果写入 `protocol_history`。后续“继续刚才那个”不能可靠依赖 Norma 历史恢复这次直接派发。

### 5.2 Agent 会话尾部

每个尾部最多保留 1,200 字符，所有 Agent 的消息正文共享 8,000 字符预算：[conversation_tail.rs](../../crates/agent/src/conversation_tail.rs#L4)。

排序优先级是：当轮点名 Agent、焦点 Agent、选中 Agent、其他 Agent；同级再按最近更新时间。但生产 `norma_system_prompt` 使用默认 hints，没有先从可能损坏的 transcript 中提取 `named_agents`，实际通常是焦点/选中/最近优先：[environment.rs](../../crates/agent/src/environment.rs#L11)、[conversation_tail.rs](../../crates/agent/src/conversation_tail.rs#L182)。

因此当直接名称匹配失败时，焦点或最近活跃的其他 Agent 内容可能比用户真正想叫的 Agent 更醒目。正文预算耗尽后，后续 Agent 只剩身份信息，消息体被标记为 `omitted_due_to_budget`。

### 5.3 Canvas 焦点和忙闲状态

Canvas 最多提供 64 张卡片，排序为焦点、AgentCard、选中、其他：[coordination.rs](../../crates/app/src/app/agent_runtime/coordination.rs#L7)、[current_context.rs](../../crates/agent/src/current_context.rs#L41)。

焦点不会在代码层自动决定派发目标，但它被明确写进提示词。忙闲也不是执行层的“换人”规则：忙碌 Agent 可以走已有会话回复或队列。然而模型可能把忙闲理解为选择替代 Agent 的依据；真实 Jasper 误派日志正是这样做的。

### 5.4 上下文可能既“过多”又“缺少关键内容”

真实误派回合的日志包含 11 个会话尾部，已有 2 个消息体因 8,000 字符预算被省略；该行中转义后的 system instructions 约 21,687 字符。这不是 token 精确值，但证明 19 个 Agent 时动态上下文已经很大。

误派后的下一轮，用户说“你重新派发给 Alan 去做吧”，模型明确回答它不知道原任务，称上下文已被截断。代码可以确认存在多个裁剪与不入历史边界；仅凭当前日志还不能唯一判定这次缺失是 protocol history 裁剪、Provider 请求截断、Rig 返回的消息集合不完整，还是运行时生命周期变化所致。

所以用户的两个猜测都成立：上下文可能引入错误候选，也可能没有保住真正需要的上一轮任务正文。

## 6. Agent 列表变多后的行为

### 6.1 精确路径本身不会在 19 个 Agent 时随机失准

确定性解析器扫描完整 roster，不受 32-Agent prompt 截断影响；唯一名称/别名避免一个 transcript 同时精确命中两个人。几十个 Agent 的线性扫描不是当前性能问题。

换句话说：只要 ASR 正确产出句首正式名/别名和合法分隔符，Agent 数量增加不会让精确路径自动选错。

### 6.2 超过 32 个 Agent 的确定性断层

模型环境把 Agent 按 `(definition_id, lowercase name)` 排序，只保留前 32 个，剩余只显示计数：[environment.rs](../../crates/agent/src/environment.rs#L7)。它不按当轮疑似名称、焦点、活跃或最近使用优先保留。

因此：

- 第 33 个以后 Agent 若被精确识别，直接解析仍可正确派发。
- 同一个 Agent 只要出现拼写、分词、标点或位置误差，回退模型后可能完全看不到它。
- 模型可能拒绝、幻觉一个名字，或从前 32 个可见 Agent 中挑另一个。

当前数据库只有 19 个启用 Agent，所以这是已确认的代码风险，但不是当前 Jasper 事件的直接原因。

### 6.3 30 个内置名称之后的语音可用性下降

系统提供 30 个带中文语音别名的内置名称。耗尽后生成 `AgentA`、`AgentB` 等后备名：[identity.rs](../../crates/agent/src/identity.rs#L27)、[identity.rs](../../crates/agent/src/identity.rs#L102)。

存储层只为内置正式名自动附加语音别名，后备名没有别名：[agent_definition_store.rs](../../crates/storage/src/agent_definition_store.rs#L271)。

这会形成另一个增长断层：ASR 很可能把 `AgentA` 转成 `Agent A`，但正式名称规则和直接解析都不接受这个空格形式。`AgentA` / `AgentB` 本身也比人名更容易混淆。

### 6.4 更多 Agent 会放大 fallback 的候选与上下文压力

即使没有触发 32 上限，更多 Agent 也会：

- 增加 LLM 可自行改选的合法目标。
- 增加名称的声学近似概率。
- 增加动态 roster、卡片和尾部数据。
- 更快耗尽 8,000 字符尾部正文预算。
- 让忙闲、最近使用和焦点等非目标信息获得更多相对影响。

这是“Agent 越多越容易误派”与当前实现一致的机制解释，但需要更多结构化样本才能量化错误率曲线。

## 7. 近期日志与数据库证据

### 7.1 已确认：`Alan` 被识别为 `Allen`，最终派给 Jasper

当日日志 [line 38664](/Users/alan/.norma/logs/norma-os.2026-08-01.log:38664) 显示 transcript：

```text
Allen，帮我看一下为什么现在……流星的效果……掉帧……
```

同一行模型推理明确写出：

- Alan 当前获得焦点，但正在处理任务。
- Jasper 是空闲 Codex。
- 因此选择 Jasper。

随后模型回复“已经派发给 Jasper”。

项目数据库 `agent_dispatch_tasks`：

- `id=272`：目标 Jasper，原任务为这条流星掉帧分析。
- `id=273`：用户重新说明后，目标才变为 Alan。

用户紧接着明确纠正“应该指派给 Alan，而不是 Jasper”。因此“用户说的是 Alan”不是仅凭拼写做的猜测，而有相邻用户纠正和数据库结果共同支持。

### 7.2 已确认：纠错回合没有恢复上一任务正文

日志 [line 38721](/Users/alan/.norma/logs/norma-os.2026-08-01.log:38721) 中，用户说：

```text
你重新派发给Alan去做吧。
```

模型回答不知道原任务内容，并要求用户重述。之后用户确实重新讲了一遍流星掉帧任务，才生成数据库 `id=273`。

这证明当前“纠正目标但沿用刚才任务”的体验不可靠；具体是哪一层丢失，仍需增加结构化 trace 才能唯一定位。

### 7.3 高可信推断：`浩宇` 被转写为“你有”后，Norma 自己执行

日志 [line 58866](/Users/alan/.norma/logs/norma-os.2026-08-01.log:58866) 中的 transcript 以：

```text
你有你来帮我看一下目前有什么开恩项目……
```

开头。当前 roster 存在中文 Agent `浩宇`。后续用户纠正“不应该自己去做，应该指派给这个 Agent”。模型在该回合自行使用 Browser/Terminal，没有派发 Agent。

结合发音、当前 Agent 名称和后续纠正，`你有` 很可能是 `浩宇` 的 ASR 错误；由于没有原始音频，这一点标为高可信推断，不标为完全证实。

### 7.4 已确认：正确名称缺少分隔符也会绕过直接解析

日志 [line 66625](/Users/alan/.norma/logs/norma-os.2026-08-01.log:66625) 的 transcript 以 `Grace你。你自己再看一下` 开头。`Grace` 正确存在，但后面直接连接汉字 `你`，不满足当前分隔符规则，因此不会走确定性直接路径。用户随后再次纠正“你不应该自己去做，把这个事情派发给 Grace”。

这说明问题不只发生在名字拼错时，ASR 的空格/标点输出差异也足以改变路由分支。

## 8. ASR 侧的具体问题

### 8.1 Agent 名称没有进入识别器词表偏置

本地双语 Zipformer 已支持 hotword，但默认内容只有：

```text
JAVA
KOTLIN
```

见 [sherpa.rs](../../crates/asr/src/providers/local/sherpa.rs#L53)。没有代码把当前项目的正式 Agent 名称和 `speech_alias` 动态注入 recognizer。

文档也明确说明，当前 `start_recording` 仍走默认 builder，业务层单独构造自定义配置不会改变现有录音 session：[asr-hotwords.md](../references/asr-hotwords.md#L127)。热词还是 recognizer 创建参数，现有 session 不能热更新。

另外，Paraformer 和当前部分 Zipformer 模型不支持该 hotword 配置；Azure 路径也没有通过统一 ASR 接口接收项目级 Agent 词表。问题不是简单加两个静态热词即可覆盖。

### 8.2 无法度量和处理歧义

ASR 接口只向上返回 String，路由层看不到：

- `Alan` 与 `Allen` 的候选概率。
- `浩宇` 与 `你有` 的替代候选。
- 名称片段的低置信度。
- Provider 是否已使用 phrase bias。

所以系统只能把一条可能错误的 transcript 当作确定事实。

## 9. 可观测性缺口

数据库已有 `voice_history` 字段：raw/refined transcript、confidence、planner、target node 和状态。但当前成功语音回合没有调用结构化保存；现有生产调用只覆盖空 transcript 和 Provider error：[history.rs](../../crates/app/src/app/voice/history.rs#L34)。`target_node_id` 在保存 helper 中固定为 `None`。

当前误派只能依靠体积很大的 Rig prompt 日志和 `agent_dispatch_tasks` 人工对时。缺少一个可以直接查询的闭环：

```text
raw ASR
→ normalized transcript
→ resolver route
→ candidate Agents
→ selected definition_id
→ model/direct
→ tool target
→ final node_id / dispatch task id
```

这会使“到底是 ASR 错、解析错、模型改选、还是 Adapter 串线”难以快速区分，也无法统计 Agent 数量与误派率的关系。

## 10. 问题优先级

| 优先级 | 问题 | 状态 | 影响 |
|---|---|---|---|
| P0 | ASR 名称轻微错误后，系统允许 LLM 自主换成另一个合法 Agent | 日志与 DB 已证实 | 真实错误副作用 |
| P0 | 目标工具使用自由字符串，最终只验证“存在”，不验证“与用户意图一致” | 代码已证实 | 合法错误目标无法拦截 |
| P1 | Agent 名/别名未动态进入 ASR 偏置，ASR 无 confidence/N-best | 代码已证实 | 名称错误无法提前发现 |
| P1 | 自然委派结构与无分隔符场景大量落入 LLM | 代码、规格、日志已证实 | 确定性覆盖过窄 |
| P1 | 纠正目标时没有结构化保存上一目标和任务正文 | 日志已证实体验；根层待 trace | 用户必须重述，易二次错误 |
| P1 | protocol history 与动态环境分别限额，整体 token 预算不闭合 | 代码已证实 | 截断与上下文污染 |
| P1 | 成功语音回合没有结构化目标审计 | 代码已证实 | 无法量化和快速复盘 |
| P2 | 模型名册只显示前 32 个 definition | 代码已证实，当前 19 未触发 | 第 33 个以后 fallback 失明 |
| P2 | 30 个内置名后使用无语音别名的 AgentA/B | 代码已证实 | 规模增长后 ASR 更脆弱 |
| P2 | 多 Agent 直接寻址最多 8 个 | 代码已证实 | 大批量语音委派截断 |

## 11. 建议的优化方向（供下一步讨论）

本报告不直接修改代码。推荐先讨论以下设计顺序。

### 11.1 P0：把“目标解析”从通用 LLM 决策中剥离

建立独立的 `VoiceAgentTargetResolver` 或同等领域能力：

1. 输入 raw/final transcript 和完整 enabled roster。
2. 输出稳定 `definition_id`、正文切分和解析证据。
3. 精确命中直接确定。
4. 近似命中只能产生候选，不能直接改选另一个 Agent。
5. 高风险或多候选时只确认目标，不要求用户重述整条任务。
6. LLM 只判断 `reply` / `new task` 和清洗任务正文，不再拥有随意替换已识别目标的权力。

最重要的安全规则应是：用户开头疑似点名了一个 Agent，但名称无法可靠解析时，零副作用澄清；绝不能因为另一个 Agent 空闲就换人。

### 11.2 P1：让 ASR 知道当前 Agent 词表

- 对支持 phrase/hotword bias 的 Provider，录音 session 创建时注入当前正式名与语音别名。
- roster 改变后安全重建下一次 recognizer，不在活跃 utterance 中热换。
- 扩展 Provider 中立接口，至少支持置信度或候选列表。
- 对不支持 bias 的 Provider，依赖后置候选解析和确认，不伪装成已可靠识别。

热词只能降低错误率，不能替代目标安全边界。

### 11.3 P1：扩大确定性语法，但保持保守

优先覆盖规格已有结构：

- `请/让/交给/指派给 <Agent> ...`
- `把任务派给 <Agent> ...`
- `<Agent>你...` 这类已知名称后紧邻称呼代词的 ASR 形式。
- `Agent A` 与已登记 `AgentA` 的受控空格归一化。

不要做无阈值的全局模糊匹配。候选相近或可能命中多个 Agent 时必须澄清。

### 11.4 P1：保存结构化“上一派发意图”

单独保存最近一次明确派发的：

- `definition_id`
- 原始任务正文
- dispatch task/receipt id
- 当前可否安全重派

“重新派给 Alan”“刚才那个改给 Grace”应引用这份结构化状态，而不是从 protocol history 或多个 Agent conversation tail 中猜。

### 11.5 P1：补齐结构化诊断

在不长期暴露敏感语音内容的前提下，至少为本地诊断记录：

- trace id、Provider/model、Agent 数量。
- raw/final transcript 的受控留存策略。
- resolver 类型：exact / normalized / candidate / model / clarification。
- 候选及分数、最终 definition ID。
- 工具目标、node ID、dispatch task ID 和结果。

这能直接生成“按 Agent 数量分桶的目标纠正率”，而不是再人工搜索 30 MB 日志。

### 11.6 P2：重做大名册上下文

- 先用确定性/声学 resolver 从完整 roster 检索少量候选，再把候选交给模型。
- 第 33 个以后不能简单按 definition ID 隐藏；疑似目标必须优先进入模型上下文。
- 整体预算应同时计算 system prompt、roster、tails、history、当前输入和工具 schema。
- 30 个内置名耗尽后，不应默认生成没有可说别名的 `AgentA/B`；至少要求或生成稳定的语音称呼。

## 12. 现有测试覆盖与缺口

已通过的直接寻址测试覆盖：

- 活跃 Agent 回复。
- Ended Agent 新任务。
- inactive + context-dependent 拒绝。
- 多 Agent 句首集合。
- 普通提及和状态查询。
- Voice source 向 Adapter 透传。

环境测试覆盖了少量 Agent 的名称/别名、卡片与 prompt 组合。此次聚焦运行共 14 个测试全部通过。

没有找到以下保护：

- `Allen → Alan`、`浩宇 → 你有` 等 ASR 错误候选。
- `Grace你...` 无分隔符。
- `请/让/交给/指派给 Agent` 的确定性真实路由。
- 33 个以上 Agent 时目标是否仍可见。
- 30 个内置名耗尽后的真实语音识别。
- raw ASR → target definition ID → dispatch receipt 的完整 E2E。
- 模型不得因为目标忙碌而改选空闲 Agent。
- 纠错回合稳定复用上一任务正文。
- 成功语音回合的结构化审计完整性。

现有 Voice E2E 主要证明语音 Final 会进入 Agent 回合，或在已知 Adapter 目标下产生回执/动效；它们没有用真实/模拟 ASR 名称误差验证目标解析。

## 13. 最终判断

用户观察“Agent 越多，语音指派越容易错”与当前架构一致，但需要拆成三个阶段：

1. **当前已发生的主因**：ASR 或标点把正式名变成非精确形式，直接解析失效；模型根据忙闲和上下文改选其他 Agent。
2. **当前已存在的放大器**：更多合法候选、更大的动态上下文、会话尾部预算竞争，以及不可靠的纠错上下文。
3. **规模继续增长后的硬断层**：模型只见前 32 个 Agent，30 个内置名称后语音别名消失。

因此第一优先级不是继续调整 Master prompt，而是建立独立、可审计、失败时零副作用的目标解析边界。只要 LLM 仍能在目标名识别失败后自行挑一个空闲 Agent，增加上下文或换更强模型都只能降低概率，不能消除错误派发。
