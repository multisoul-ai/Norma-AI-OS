---
title: GPT-Live 与豆包：当前实现架构
subtitle: 按本地源码核对 · feat/gpt-live-duplex · 2026-10-18
template: doc
theme: paper
lang: zh
---

## A 主要差异是什么

```callout info 两条语音链路，共用项目工具
豆包当前使用 ASR → 文本模型 → TTS。GPT-Live 使用持续音频会话。
GPT-Live 遇到项目操作时，再交给 Norma 后台模型执行工具。
```

本页比较的是 Norma 目前的两套接入代码。豆包厂商其他实时语音产品不在此范围内。

你截图中的 GPT-Live 悬浮转写框已取消。内部双向转写继续用于指令关联与更正，Dock 保留通话反馈。

## B 豆包现在怎样工作

```flow
麦克风 -> ASR识别: 16k PCM / WebSocket
ASR识别 -> 文本模型: SpeechEvent::Final 后发起请求
文本模型 -> 语音合成: 回复文本 / HTTP
语音合成 -> 扬声器: 流式 24k PCM16
文本模型 -> 项目工具: function calls
项目工具 -> 文本模型: 真实回执
```

文本模型固定为 `doubao-seed-2.0-mini`。Chat 使用 HTTP 请求，并读取完整 JSON 响应。

ASR 在独立任务中持续收音。检测到用户开始说话，会取消旧回复并中断播放，因此现有豆包也能插话。

这套接入使用豆包服务 Key 配置 ASR、Chat 和 TTS。执行项目工具不依赖另一个可配置的 Norma 后台模型。

## C GPT-Live 现在怎样工作

```flow
麦克风 -> Live会话: 连续 16k PCM / 20ms 上行
Live会话 -> 扬声器: 持续音频输出
Live会话 -> 委派桥接: 委派事件 + 双向转写
委派桥接 -> Norma后台: 原话 / 更正 / 项目上下文
Norma后台 -> 项目工具: NormaSession / typed tools
项目工具 -> 结果回传: 真实回执与任务事实
结果回传 -> Live会话: session.append
```

语音模型固定为 `gpt-live-1`。入口为 `/v1/live/sessions`，HTTP 升级为 WebSocket。

会话设置使用 `delegation: client` 和 `store: false`。普通语音聊天不额外经过本地 ASR → Chat → TTS。

收到 `session.delegation.created` 后，桥接层结合已观察原文、稳定意图和 revision 准备任务。项目工具由配置的 Norma 后台模型规划执行。

工具结果通过 `session.append` 回到 Live。回传文本限制为 480 个 UTF-8 字节；收到 ACK 不等于语音已经播完。

## D 逐项对照

| 维度 | 当前豆包接入 | 当前 GPT-Live 接入 |
|---|---|---|
| 对话链路 | ASR → 文本模型 → TTS | 单个持续音频会话 |
| 回复触发 | ASR Final 后启动一轮回复 | Live 持续处理输入和输出 |
| 收音与插话 | 持续监听；Started 取消旧回复并停播 | 持续上行；VAD 冻结旧工具规划；明确停播按 output epoch 隔离 |
| 语音模型 | 固定 Chat 模型 + 独立 TTS voice | 固定 gpt-live-1 + Live voice |
| 工具规划 | 豆包 Chat 直接生成 function calls | Live 委派 → 可配置 NormaSession |
| 项目执行 | NormaToolRuntime 与 App Adapter | 同一工具体系，经 Live 专用 Adapter 校验 |
| 写入去重 | MobileCommandStore；project/session/call ID | LiveOperationStore；稳定意图、操作与 claim token |
| 未知执行结果 | 不应自动重复副作用 | 持久记录 unknown，先只读核对 |
| 授权来源 | DoubaoVoice | 独立 GptLiveVoice；复验原话、revision 和通话代次 |
| 音频输出预算 | 原有播放队列 | 另加 1 秒排队及已调度音频预算 |

两边都要以真实工具回执判断执行结果。模型口头说“完成”不能证明项目已经改变。

GPT-Live 没有沿用豆包旧的删除豁免。写操作会按用户原话和准确目标重新校验。

## E 为什么设置页多出一套模型

```callout info “项目工具”配置的是后台执行者
GPT-Live Key 管实时通话。项目工具下的供应商和模型，管 NormaSession 的工具规划。
这是当前实现复用 Norma 工具与权限体系的架构选择，并非语音聊天本身要求第二套模型。
```

| 配置项 | 实际作用 | 什么时候用到 |
|---|---|---|
| GPT-Live API Key | 连接 Live 语音服务 | 开始通话 |
| Live voice | 选择语音输出音色 | Live 播报 |
| 项目工具：供应商 / 模型 | 给 Norma 后台选择工具规划模型 | 创建卡片、操作项目、派发任务等委派 |
| 后台 Base URL / Key | 连接上述供应商 | 执行后台规划 |
| 项目中的 Coding Agent | 接受已派发的编码任务 | 项目工具派工后 |

这些名称容易混淆：Live 模型负责对话；Norma 后台负责工具规划；项目中的 Coding Agent 负责具体编码任务。

后台未就绪时，语音聊天仍可连接，工具委派会报错。使用云端后台执行项目操作时，还会产生该模型的调用用量。

这多出一段后台请求，也增加了配置和排障成本。具体时延仍需实测。

当前 GPT 页只展示后台供应商和模型选择。Base URL、Key 与连接验证尚未完整呈现在这里，首次配置入口需要改善。

![当前测试版的项目工具入口，未连接通话](/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T12-13-27Z/03b-review-dark.png)

## F 哪些能力在复用

两边复用 `voice_input` 的权限、设备、采集和 AEC，以及 macOS 音频输出基础设施。

两边也复用 Persona、Memory、NormaToolRuntime、项目副作用和任务协调能力。GPT-Live 新增零 GPUI/Storage 的 `live_voice` 协议层。

GPT-Live 的 App 桥接负责委派、转写关联、更正和取消。持久账本负责执行去重与结果核对，GPUI pump 负责真实项目操作。

取消悬浮框只改变展示。通话、内部转写、工具授权、Dock 状态和设置中的详细错误仍保留。

## G 目前有哪些验证边界

| 项目 | 当前结论 |
|---|---|
| 本次自动验证 | 71 项相关回归及 Check、Clippy、Release 构建通过 |
| 本次真实设置回归 | 前 5/6 步通过；最后一步截图不可用，整体未计通过 |
| 哪个实际更快 | 需要同设备、同网络的真人对照；架构图不能给出时延结论 |
| 耳机与外放的插话、回声 | 留待真人统一验收 |
| 云端项目工具完整流程 | 本地整链已有测试，真实云端项目验收待执行 |
| 看到屏幕 | 当前两条链路均未接屏幕截图或视频上传；项目状态来自文本与工具 |
| 用量准确性 | 曾出现本地 600 秒、服务端 821 秒，仍待核对 |
| 字幕声学脚本 | 原脚本依赖悬浮字幕；本次移除后须在连接前阻断，不能收费后再失败 |

## H 源码依据

| 范围 | 关键文件 |
|---|---|
| 豆包编排与插话 | `crates/app/src/app/doubao/plan.rs`、`plan/conversation.rs` |
| 豆包识别、Chat、合成 | `plan/asr.rs`、`plan/chat.rs`、`plan/tts.rs` |
| Live 协议与连接 | `crates/live_voice/src/session.rs`、`providers/openai.rs` |
| Live 委派与后台 | `crates/app/src/app/gpt_live/bridge.rs`、`dispatch.rs` |
| 结果、账本与执行 | `gpt_live/report.rs`、`gpt_live/durable.rs`、`agent_runtime/gpt_live_adapter.rs` |
| 浮层与通话反馈 | `app/voice/caption.rs`、`app/dialogue/shell.rs`、`settings/voice/view/gpt_live.rs` |

本文描述本地分支现状。音频格式、事件名和限制来自当前源码；尚未完成的真人验收已单独列明。
