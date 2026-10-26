---
title: 上下文融合，会怎样影响三种对话模式
subtitle: Legacy · 豆包 2.0 · GPT-Live · 兼容性核对 · 2026-10-25
lang: zh
template: doc
theme: shadcn
mode: light
---

## A 结论与范围

```callout info 三种模式都需要接入，但接入位置不同
当前只修改方案文档，未因本任务改变三模式的运行行为。
上一版对 Legacy、Live 后台委派和模式切换的覆盖不够明确。
修订后，首批交付与验收必须包含全部三种模式。
```

**共享目标、约束、记忆和幕僚事实；每种模式继续使用自己的对话管线。** 七组上下文是语义分组，不能要求三个通道发送相同的 messages。

设置中的三种模式由 DesktopVoiceMode 明确定义。单次指令、持续聆听是模式内的交互选项，不是另外两种模型。

| 模式 | 当前代码路径 | 融合的接入点 | 需要保持的行为 |
|---|---|---|---|
| 语音指令 Legacy | 所选 ASR → NormaSession → 所选 TTS | NormaSession 请求前的上下文装配；播报入口记录事实 | 单次／持续聆听、确定性直呼、工具执行、已选模型与音色 |
| 豆包 2.0 | 豆包 ASR → ChatRuntime → 豆包 TTS | 每次 Chat 请求；Report 播报入口；直呼执行校验 | 逐请求刷新环境、用户开口优先、常驻播报与收音分离 |
| GPT-Live | 原生音频会话；需要工具时 client 委派后台 NormaSession | Live 启动与增量上下文；后台委派的独立装配入口 | 原生双工、按住／持续输入、委派校验、持久任务回执、独立静音与停止播音 |

Legacy 的 NormaSession 还被 Dock 文字入口使用，修改公共装配逻辑时必须回归文字对话。GPT-Live 的后台规划也复用 NormaSession，但采用自己的配置和执行 Adapter。

## B 七组内容，三种不同放法

| 分组 | Legacy | 豆包 2.0 | GPT-Live |
|---|---|---|---|
| ① 系统规则 + 个性 | 随本次模型请求装配 | 随本次 Chat 请求装配 | 通话 instructions；后台规划另装配适用规则 |
| ② 环境 + 卡片 | 保留提交回合时的环境更新 | 保留每次模型请求前刷新，含工具后续请求 | 启动快照 + 可用增量；当前状态仍后台核验 |
| ③ 当前目标 + 有效约束 | 本轮小快照；执行入口再次检查 | 本轮小快照；直呼也检查 | Live 接收摘要，后台规划和执行读取完整相关约束 |
| ④ 相关记忆 | 按本轮问题选择 | 按本轮问题选择 | 按需委派读取；返回精简资料，避免重复追加 |
| ⑤ 幕僚决定 + 播报 | Report 记录独立于 TTS；下轮按需取用 | Report 记录独立于播报队列；下轮按需取用 | 通过 Live 通道适配；保持旧 TTS 禁用；ACK 与播放分开 |
| ⑥ 近期历史 | 保留 NormaSession 的原生历史 | 保留 ChatRuntime 的原生历史 | 保留供应商音频会话；后台使用本地时间线快照 |
| ⑦ 当前输入 | ASR 最终文本作为 user 输入 | ASR 最终文本作为 user 输入 | 继续传音频；转写供本地状态与委派，不能重复发成第二个用户请求 |

每次模型请求固定自己的 snapshot，工具后续请求可以取得新 snapshot。用 turn_id 关联这些请求，不把“本轮一致”解释成整轮永远读取旧环境。

预算分别计算：Legacy 采用所选模型的窗口；豆包保留自己的请求预算；Live 同时考虑单条追加限制和会话累计量。后台规划有自己的窗口，不能套用 Live 的单条追加上限。

## C 共享层与模式适配

```flow LR
目标约束与事件 -> 共享上下文装配
记忆与幕僚记录 -> 共享上下文装配
共享上下文装配 -> Legacy请求适配
共享上下文装配 -> 豆包请求适配
共享上下文装配 -> Live通话适配
共享上下文装配 -> Live后台规划适配
```

共享装配 Module 负责范围过滤、相关片段、版本、去重、预算和来源清单。各 Adapter 负责协议、原生历史、输入来源与交付时机。

### Live 必须接入两处

只改通话 instructions，后台可能仍不知道用户新加的约束。只改后台 NormaSession，实时对话又可能接不上幕僚汇报。两处需要同源事实，各自保留职责和预算。

### 直呼路径必须覆盖

Legacy 与豆包都有代码直接识别 Agent 名称的路径，可以跳过模型。GPT-Live 的后台 NormaSession 也可能使用该逻辑。因此，执行限制必须由宿主校验；仅修改 prompt 不足以覆盖这些操作。

### 权限不能随上下文迁移

保留 Voice、DoubaoVoice、GptLiveVoice 的来源身份。共享记录不能改写当前 user_input、operation_id 或审批身份。无项目、工作空间总览和切换中的工具过滤仍按原 Adapter 执行。

## D 最容易发生的兼容性回归

| 反例 | 会影响什么 | 方案约束 |
|---|---|---|
| 只给豆包与 Live 接入 | Legacy 仍无法追问幕僚决定 | 三模式均列入首批；“普通会话”改为具名入口 |
| 所有历史合成一个数组 | Live 重复收到转写；三方重复回答或重复触发工具 | 原生历史各自维护；共享事件按 event_id 去重；跨模式只按需携带摘要 |
| 给所有模型套一份 system | Live 的委派要求、豆包的工具限制被覆盖 | 公共规则与模式规则分层；consumer 明确区分 Live 对话和后台规划 |
| 把环境缓存到通话结束 | 豆包创建卡片后，下一次模型请求仍看旧卡片 | 保留逐请求环境刷新；工具后续请求更新 snapshot |
| 共享层统一停止旧项目通道 | 后台豆包完成通知被切断 | 切模式与切项目分别处理；保留原后台播报生命周期 |
| Report 借用任务播报开关 | 关任务播报时，幕僚汇报也被误关 | 幕僚汇报开关、任务播报开关各自判断；静音收音与停止播音也分开 |
| 切模式后搬运旧工具调用 | 新通道复用旧授权、回调或执行身份 | 新建通道 generation；保留事实和必要摘要，旧调用身份失效 |
| 后台项目播报归到当前焦点 | 用户问原因时查错项目 | 记录原 workspace_id、area_id、task_id；焦点只是指代线索 |

**同一个目录可以对应两张项目画布。** ContextScope 需要 area_id，不能仅用目录字符串区分会话。跨项目总览按稳定地址选择相关事实，不把可见性等同于执行权限。

切模式后的摘要可带“此前在豆包中讨论过”及来源引用，但不能伪装成新通道已经播过。已播放的汇报不自动重播；后台任务事实不因通话结束而删除。

GPT-Live 未连接时，记录幕僚事实与未播状态。首批不因后台事件自动开通话或打开麦克风；用户建立会话后按需取用相关记录。

## E 三模式验收矩阵

以下是实施后的验收要求，尚未运行新实现的测试。本次是源码与方案核对。

| 场景 | Legacy | 豆包 2.0 | GPT-Live |
|---|---|---|---|
| 基础对话与操作 | 单次和持续聆听；原模型与 TTS；直呼 | 按住和持续聆听；原模型与音色；直呼 | 原生音频；按住和持续输入；client 委派 |
| 幕僚汇报后追问 | TTS 后能查关联决定 | Report 后能查关联决定 | Live 收到相关事实，追问能引用；旧 TTS 保持禁用 |
| 用户修订约束 | 后续模型与直呼均遵守 | 后续 Chat 与直呼均遵守 | 对话知晓；后台规划与执行也遵守 |
| 输入或输出被打断 | 保留现有停止、回声抑制和轮次状态 | 用户开口优先；取消旧回复不误停新回复 | 插话失效、输出控制、已接受任务状态保持正确 |
| 工具执行后再查询 | 按既有刷新和核验入口取得事实 | 后续请求读取新环境 | 后台核验当前状态；旧快照不冒充最新 |
| 麦克风未开启 | 语音入口不被上下文同步打开 | 常驻通道可播报，收音保持关闭 | 不自动建立通话；保留未播状态 |
| 开关组合 | 幕僚汇报与任务播报独立 | 同左，且关闭收音不关闭常驻播报 | 同左，且收音静音与输出停止独立 |
| 作用域与恢复 | 同目录双画布、无项目、重启 | 后台项目播报、总览工具过滤、重连 | 过期 generation、旧授权拒绝、持久任务恢复 |

三模式之间有六个有方向的切换，均需覆盖。每个方向检查：旧采集停止、旧音频不补播、旧回调不能驱动新通道、有效事实可追溯。额外回归 Legacy 共用的 Dock 文字入口。

### 可复用的现有测试入口

| 范围 | 现有文件 |
|---|---|
| 模式与生命周期 | `crates/app/tests/unit/desktop_voice_mode_tests.rs`；`crates/app/tests/unit/desktop_voice_lifecycle_tests.rs` |
| Legacy 对话与回声 | `crates/app/tests/unit/app_dialogue_shell_tests.rs`；`crates/app/tests/unit/dialogue_asr_echo_gate_tests.rs` |
| 豆包环境与常驻通道 | `crates/app/tests/unit/doubao_context_tests.rs`；`crates/app/tests/unit/doubao_channel_app_tests.rs` |
| 豆包旧回复与插话 | `crates/app/tests/unit/doubao_conversation_freshness_tests.rs`；`crates/app/tests/unit/doubao_hold_conversation_tests.rs` |
| Live 指令与后台委派 | `crates/app/tests/unit/gpt_live_prompt_tests.rs`；`crates/app/tests/unit/gpt_live_dispatch_integration_tests.rs` |
| Live 执行与旧 TTS 排斥 | `crates/app/tests/unit/gpt_live_adapter_tests.rs`；`crates/app/tests/unit/gpt_live_announcements_tests.rs` |

这些文件提供已有测试入口，不代表已覆盖新需求，也不代表本次已运行。实施时先补聚焦失败用例，再跑受影响 crate；网络、麦克风、插话和实际播放需真实 App 验收。

## F 调整后的实施顺序与代码证据

1. 定义同源事实和三模式能力表，验证 Live 更新时机。
2. 分别接入 Legacy、豆包、Live 对话与后台规划。
3. 三模式跑同一组“汇报、追问、修订约束”场景。
4. 验收六个方向的模式切换，再扩大记忆召回。

按模式保留功能开关，发布时分别启用。某一 Adapter 失败不应停止其他模式；若回退无法执行已接受的约束，相关操作应等待核验。

| 核实事实 | 代码位置 |
|---|---|
| 三种模式的正式定义 | `crates/app/src/settings/voice/state/mode.rs:2` |
| Legacy 语音提交项目 Agent | `crates/app/src/app/dialogue/reply_loop.rs:108` |
| Legacy 与 Dock 共用提交入口 | `crates/app/src/app/agent_runtime/submit.rs:55`；`crates/app/src/app/command_dock/conversation.rs:17` |
| NormaSession 装配与直接路由 | `crates/agent/src/master/memory_session.rs:114`；`crates/agent/src/master.rs:240` |
| 豆包请求前读取环境 | `crates/app/src/app/doubao/plan/chat.rs:106` |
| 豆包 Report 直接进入 TTS | `crates/app/src/app/doubao/plan.rs:113` |
| 豆包常驻通道与收音分开 | `crates/app/src/app/doubao/channel.rs:1` |
| Live 通话启动提示词 | `crates/app/src/app/gpt_live/prompt.rs:3` |
| Live 另建后台 NormaSession | `crates/app/src/app/gpt_live/dispatch.rs:90` |
| Live 来源、项目与快照校验 | `crates/app/src/app/agent_runtime/gpt_live_adapter.rs:44` |
| 切换模式停止旧语音路径 | `crates/app/src/app/gpt_live.rs:24` |
| 后台豆包播报保留的既有要求 | `crates/app/tests/unit/desktop_voice_lifecycle_tests.rs:10` |

主实施方案已同步修订：`docs/exec-plans/2026-10-25-shared-voice-brain-context.md`。
