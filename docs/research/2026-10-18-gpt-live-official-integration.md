# GPT-Live 官方接入调查：实时双工与现有 Agent 衔接

调查日期：2026-10-18。范围：真实的 OpenAI `gpt-live-1` / Live API，不以 Realtime API 代替。材料均来自实际搜索并获取的 OpenAI 官方文档；已读取各页 `.md` 正文及 API schema。本文不代表完成真实账号连通、费用或声学效果测试。

## 1. 核心判断

**官方事实：** GPT-Live 是可以同时听、说的 full-duplex 语音模型；语音对话和后台推理、工具执行分开。官方迁移文档专门覆盖从文字 Agent 或 ASR → Agent → TTS 级联管线迁移：GPT-Live 替代独立 ASR/TTS，已有 Agent、工具、业务规则和持久状态仍可保留。[模型](https://developers.openai.com/api/docs/models/gpt-live-1)、[文字 Agent / 级联迁移](https://developers.openai.com/api/docs/guides/live-migration?migration-path=text-agent#from-a-text-agent-or-chained-pipeline)

**方案推导：** 已有自有 Agent / 工具运行时优先考虑 `delegation: { "type": "client" }`。这与官方“保留已有 Agent、自定义上下文与工作流时选 client delegation”的建议一致；它不是把 GPT-Live 作为普通 ASR 或 TTS Provider 注册进去。[委托模式选择](https://developers.openai.com/api/docs/guides/live-delegation#choose-a-delegation-mode)

**官方推荐与工程选择要分开：** 官方推荐 WebRTC，覆盖浏览器、移动端，甚至服务端音频连接；WebSocket 仍是官方支持的替代方案。若先复用 Rust 原生 PCM / WS 能力，应写成项目第一阶段选择，不能宣称“官方对原生 App 推荐 WS”。[连接选型](https://developers.openai.com/api/docs/guides/live#choose-a-connection)、[WS](https://developers.openai.com/api/docs/guides/voice-websockets?api=live)

## 2. WebSocket 协议：可确认的最小闭环

### 2.1 地址、认证与启动

| 项目 | 官方要求 |
| --- | --- |
| Endpoint | `wss://api.openai.com/v1/live/sessions` |
| URL 参数 | 不接受 query 参数；不要写 `?model=gpt-live-1` |
| 已列明认证 Header | `Authorization: Bearer $OPENAI_API_KEY` |
| 首条消息 | `session.start`，配置放在 `session` 对象中 |
| Ready 屏障 | 收到 `session.started` 后才发音频和业务命令 |
| 会话标识 | 保存返回的 `session.id` 和 `expires_at` |

来源：[Primary WebSocket reference](https://developers.openai.com/api/reference/resources/live/primary-websocket#connection)、[启动指南](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#authenticate-and-start-the-session)。当前核实的 Live 连接定义未要求 Realtime 的 Beta Header；不要凭旧 Realtime 示例添加必需配置。

以下是按官方 schema 缩减的**概念示例，未执行真实 API 调用**：

```json
{
  "type": "session.start",
  "event_id": "start_1",
  "session": {
    "model": "gpt-live-1",
    "instructions": "使用简洁中文交流。需要项目操作、查询或复杂推理时交给后端。",
    "audio": {
      "format": { "type": "audio/pcm", "rate": 16000 },
      "output": { "voice": "marin" }
    },
    "delegation": { "type": "client" },
    "store": false
  }
}
```

`model`、初始 instructions、input、voice、音频格式和委托模式在启动时确定。要换 voice / format / model / delegation mode，建立新会话；增加对话指令用 `session.instructions.append`。`session.update` 不应作为任意全量配置更新接口。[会话配置](https://developers.openai.com/api/docs/guides/live-conversations#configuration-fields)、[更新](https://developers.openai.com/api/docs/guides/live-conversations#update-a-live-session)

API reference 已包含 Decisions / transcript trigger 的额外能力，而入门 guide 的主要描述是 client / responses；本方案不依赖这些额外能力，不把 guide 简表当作整个 API 的穷尽清单。[当前完整 schema](https://developers.openai.com/api/reference/resources/live/primary-websocket#session.update)

### 2.2 音频格式与帧

| 配置 | 原始格式 |
| --- | --- |
| `{ "type": "audio/pcm", "rate": 24000 }` | 单声道、有符号 PCM16 little-endian，默认 |
| `{ "type": "audio/pcm", "rate": 16000 }` | 单声道、有符号 PCM16 little-endian |
| `{ "type": "audio/pcmu", "rate": 8000 }` | G.711 μ-law，一字节一个 sample |
| `{ "type": "audio/pcma", "rate": 8000 }` | G.711 A-law，一字节一个 sample |

同一 session 的上下行使用同一个格式，不是独立设置输入 16 kHz、输出 24 kHz。修改字段不负责转换字节；不同采样率必须真正重采样。[官方音频格式](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#choose-the-audio-format)

每条 event 都是 WebSocket **JSON text message**，音频在 JSON 内 base64，不是豆包自定义二进制帧，也不是裸 PCM binary message。上行：`{ "type": "session.input_audio.append", "audio": "<base64>" }`；下行：`{ "type": "session.output_audio.delta", "delta": "<base64>" }`。原始数据没有 WAV 等容器头；PCM chunk 必须保持完整 16-bit sample，即偶数字节，跨块保留尾字节。其他块边界可以任意，必须保持连续、有序。[发收事件](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#send-and-receive-events)、[append schema](https://developers.openai.com/api/reference/resources/live/primary-websocket#session.input_audio.append)

官方要求按录音采样率的真实节奏持续送麦克风流；一次性倾倒完整音频文件不等于实时输入。等用户先说话时也需保持输入音轨 / 静音流；EOF 不会结束会话。[流式示例说明](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#stream-audio-with-an-sdk)、[开场时保持输入](https://developers.openai.com/api/docs/guides/live-conversations#greet-before-the-caller-speaks)

**工程推导：** 若原生采集已经输出 16 kHz PCM16LE，可用 16 kHz 启动，并同时支持 16 kHz 播放；20 ms chunk 为 `16000 × 0.02 × 2 = 640 bytes`。20 ms 是候选工程参数，**不是官方要求或已验证最优值**。流控需使用应用有界队列、队列时长和实时节拍，不能等待音频 ACK，因为 audio append 没有 ACK。官方未在本次页面中给出音频 chunk 的最优大小或应用队列上限。[audio append 无 ACK](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#send-and-receive-events)

### 2.3 播放、打断与“用户到底听到哪里”

**必须明确的官方边界：** primary WS `session.output_audio.delta` 没有时戳，没有每轮 output-audio-done，也没有 Realtime `response.done` 的语音轮次完成等价物。播放状态应以本地播放器、队列和实际设备输出推进为准。[音频事件说明](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#send-and-receive-events)、[迁移映射](https://developers.openai.com/api/docs/guides/live-migration#adapt-the-connection-and-audio-lifecycle)

在已核实的 Live primary WS 客户端事件清单中，没有普通生成语音的 `played_ms` 回报命令，没有 `output_audio.clear` / `conversation.item.truncate`，也没有 `response.cancel`。因此不能把 Realtime 的播放对齐或取消事件搬入 Live wire protocol。其余未知扩展也不应以“服务端支持”描述。[完整 client events](https://developers.openai.com/api/reference/resources/live/primary-websocket#client-events)

官方可确认的输出控制方式是：在 client / media relay 侧暂时 mute 或 drop 输出，丢弃本地待播音频，再发送 `session.instructions.append` 纠偏；恢复策略由应用确定，ACK 不能作为恢复信号。`session.input_audio.mute` 只影响用户输入，不会静音模型输出、不会取消后台工作。[播放控制](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#control-playback-when-needed)

官方没有提供这个工作流的 output VAD / noise gate；如需识别候选语音段或说话指示，应用自做 output VAD / noise gate。停顿不是完整回答结束。**本次文档并未保证每条 output delta 都是有声语音，也未明确承诺持续输出静音帧**；这些都要用抓包和实机数据确认。[播前检查](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#check-speech-before-playback)

**重要工程风险：** WS 缓冲后手工丢弃部分声音，模型历史仍可能包含用户没听到的内容；官方明确提到 withheld audio 留在模型上下文。因此应保持较小播放积压、分别记录生成/排队/已播/丢弃状态，并测试纠偏后是否需要补充“刚才内容尚未交付”等事实，不能把本地队列清空当作服务端历史已截断。[播前检查与恢复](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#check-speech-before-playback)

API reference 还提供 `session.output_audio.fixture.play`，用于播放**完整预制音频**：固定 raw mono PCM16LE 24 kHz，最长 30 秒，同时中断模型讲话并静音输入；这期间输入被丢弃、不重放。`fixture.playing` 表示接受，`fixture.played` 仅表示 server mixer drain，不证明用户端播完。它不适合作为常规 duplex 播放完成反馈。[fixture 事件](https://developers.openai.com/api/reference/resources/live/primary-websocket#session.output_audio.fixture.play)、[played 语义](https://developers.openai.com/api/reference/resources/live/primary-websocket#session.output_audio.fixture.played)

## 3. Client delegation：接入已有 Agent 的关键协议

### 3.1 收到的事件不包含用户任务文本

```json
{
  "type": "session.delegation.created",
  "event_id": "delegation_event_1",
  "offset_ms": 1000,
  "delegation": {
    "id": "<opaque-delegation-id>",
    "type": "delegation",
    "target": "client"
  }
}
```

`delegation.id`、`target`、`offset_ms` 是元数据，**不携带用户话语或任务文本**。应用要保存双方 transcripts 和当前 task state，组装上下文后交给已有 Agent。不能实现成 `event.task` 直接透传，也不能只拿最新一个 ASR fragment。[client 上下文](https://developers.openai.com/api/docs/guides/live-delegation#keep-the-conversation-context-in-your-application)、[事件 schema](https://developers.openai.com/api/reference/resources/live/primary-websocket#session.delegation.created)

### 3.2 回传结果、进度与控制指令

| 用途 | Event | 内容语义 |
| --- | --- | --- |
| 应该告诉用户的已验证结果 | `session.commentary.append` | 允许模型改写后口播，不保证逐字 |
| 安静提供事实、进度和 UI 上下文 | `session.thinking.append` | 不要求马上口播，仍会影响后续讲话；不是秘密边界 |
| 改变对话行为、要求停说或逐字声明 | `session.instructions.append` | 应用编写的指令，可能打断当前讲话 |

三种都用 plain string `content`，单次最多 500 tokens，**必须带 `delegation_id`**：针对 client task 复制原 ID，一般会话背景填 `null`。允许同一 delegation 多次追加有意义的进度与结果。不要把内部推理、凭据和大 JSON 放进去。[更新类型与限制](https://developers.openai.com/api/docs/guides/live-delegation#send-the-right-kind-of-update)

```json
{
  "type": "session.commentary.append",
  "event_id": "result_1",
  "delegation_id": "<original-delegation-id>",
  "content": "代码检查已完成，发现两处问题；详细结果已显示在项目卡片。"
}
```

对应 `session.*.appended` ACK 的 `client_event_id` 关联发出的 `event_id`；ACK 只是估计内容注入完成，既不确认原样讲话，也不确认播完。[ACK 时序](https://developers.openai.com/api/docs/guides/live-delegation#send-the-right-kind-of-update)、[注入时序](https://developers.openai.com/api/docs/guides/live-conversations#understand-when-context-reaches-the-model)

**ID 必须分层：** Live session ID、Live delegation ID、应用 task revision / operation ID 是不同对象；若后台用 Responses，还有 response ID / tool call ID。官方建议为每次应用操作和更改后的请求分别记录 operation ID 和 task revision，以免重连重试、过期结果造成重复执行。[完整交互验证](https://developers.openai.com/api/docs/guides/live-delegation#verify-the-complete-interaction)

### 3.3 “停止说话”与“取消任务”分开

用户打断语音时，后台工作可以继续。用户将周四改成周五时，应用更新请求 revision、清除旧确认，按后端能力取消旧任务或丢弃旧结果；真实取消成功后才能回报“已取消”。Live 的纠偏指令、输入 mute、会话 close 都不能替代 application-owned job 的取消流程。[取消语义](https://developers.openai.com/api/docs/guides/live-delegation#keep-updates-accurate-and-useful)、[迁移更正流程](https://developers.openai.com/api/docs/guides/live-migration#route-updates-and-corrections)

`response.create` 在 Live 中用于 Responses delegation 后台工作启动 / 继续，不是“让语音模型现在开口”的每轮按钮，也不能给它完整 standalone Responses request body。client 模式中后台请求由应用自身运行。[Live 与后台区别](https://developers.openai.com/api/docs/guides/live-delegation#configure-responses-delegation)

## 4. Transcripts：双工 UI 不能只维护一条“当前轮次”

`session.input_transcript.delta` 与 `session.output_transcript.delta` 各自包含 `delta`、`start_ms`、`end_ms`。时戳是 session 起点后的毫秒，近似片段区间，不是逐词精确对齐，也不是网络到达时间。保留空格、重复词并按原序累积。没有 item ID 或 transcript-done / 完整会话轮次信号。[字幕语义](https://developers.openai.com/api/docs/guides/live-conversations#transcript-deltas)

双方可以重叠讲话，因此用户字幕和助手字幕要独立增长；助手一个“好的”不应强制结束用户仍在增长的气泡。UI 按时轴分组可以，但**字幕分组规则不能决定是否执行工具或取消任务**。[双工字幕](https://developers.openai.com/api/docs/guides/live-conversations#display-captions)

## 5. WebRTC 与 sideband：官方推荐路径

WebRTC 把音频交给协商的 media tracks，JSON events 走 data channel。客户端生成 SDP offer，可信应用服务器带项目 key 请求 `POST https://api.openai.com/v1/live/sessions`，JSON body 含 `session` 与 `transport: { "type": "webrtc", "sdp": "..." }`；客户端应用返回的 `transport.sdp` answer，然后等 data channel 的 `session.started`。HTTP 已经启动会话，**data channel 不再发 `session.start`**；WebRTC config 不填 `audio.format`，data channel 不发 `session.input_audio.append`、也不收 `session.output_audio.delta`。[WebRTC 时序](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live#understand-the-connection-sequence)、[音轨与事件](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live#handle-media-and-events)

需把私有工具与控制留在服务器时，可以附加 `wss://api.openai.com/v1/live/sessions/{session_id}/attach`，用创建会话的项目身份认证，不再启动第二遍 session。已有 primary WS 本身就能收发 events，无须为相同工作再加 sideband。双连接时必须指定唯一 action owner，避免同一工具执行两次。[服务端控制](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#attach-to-the-existing-session)、[action owner](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#assign-one-owner-for-each-action)

sideband 反射音频与 primary 有区别：input 反射无时戳；output 反射带 `start_ms/end_ms`；两者都固定 mono PCM16LE 24 kHz、无 event_id。output 的时轴范围可以存在 dropped-frame gaps，**不代表用户听到时间**；input 反射发生在 mute 之前，不能证明模型消费了采样。[反射音频](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#receive-reflected-audio)

## 6. 认证、原生 BYOK 与部署边界

**官方事实：** 当前 GPT-Live WebRTC / WS 指南把项目 API key 放在可信后端。会话创建端点应有应用 authentication / authorization / request limits / HTTPS；示例 Origin 检查不等于用户认证。[WS 认证](https://developers.openai.com/api/docs/guides/voice-websockets?api=live#authenticate-and-start-the-session)、[WebRTC 上线要求](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live#create-the-application-server)

**待验证 / 产品选择：** 原生 macOS 让用户自己提供 key、由本地 Rust 持有并直连，是 BYOK 产品形态，不能描述为官方首推部署。当前核实的 GPT-Live 文档未给出专属 Live ephemeral client-secret 流程；共享连接页面下半部的 `/v1/realtime/client_secrets` 示例属于 **Realtime API**，不能直接套给 Live。要无服务器 BYOK，需真实账户验证权限、地址、key 处理与代理行为；正式发布应明确区分 BYOK 与服务端代管两条路线。[Live 认证基线](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live)、[共享页中的 Realtime 章节](https://developers.openai.com/api/docs/guides/voice-webrtc?api=realtime#connecting-using-an-ephemeral-token)

## 7. 关闭、恢复、费用与数据

### 7.1 优雅关闭及恢复

先安装 `session.closed` listener，再发送 `session.close`；停止提交新工作但继续接收；读取最后 `usage.seconds`、`reason` 和 session snapshot 后释放 transport / audio。官方 WS 示例采用 15 秒应用超时，但这不是服务承诺。socket 先断且没 `session.closed` 时，finalization 和最终用量都应标记未确认。[关闭](https://developers.openai.com/api/docs/guides/live-conversations#usage-and-graceful-close)

`session.close` 会取消排队的 Responses 工作、拒绝后续命令；活跃 response 可能完成，但等函数结果的 response 无法再继续。client delegation 的应用任务要自行决定继续还是取消。结束原因包括 `close_requested`、`expired`、`content`、`remote_hangup`、`connection_lost`。[关闭语义](https://developers.openai.com/api/docs/guides/live-conversations#usage-and-graceful-close)

本次官方材料能确认 `session.started.session.expires_at` 和 `expired` 终止，但**未确证一个适用于全部账号的固定最长会话分钟数**；不要套用 Realtime 的时长上限。掉线后可新建 session 携带已存 text history；若旧会话保存完成可 fork，都是新 session ID，并非原 transport 原地恢复。恢复前核对已执行操作与未完成任务。[失败恢复](https://developers.openai.com/api/docs/guides/live-conversations#recover-from-a-failed-connection)、[fork](https://developers.openai.com/api/docs/guides/live-conversations#store-and-fork-a-session)

### 7.2 费用与限额

当前 `gpt-live-1` 标价 **$0.05 / 分钟，按秒计费，不向整分钟进位**；后台模型与工具另计。活跃时间包含用户说话、模型说话、双方静音和后台任务等待；mute 不停止计费。10 分钟活跃 voice 为 $0.50，另加 Agent / 工具费用。[模型价格](https://developers.openai.com/api/docs/models/gpt-live-1#pricing)、[计费口径](https://developers.openai.com/api/docs/guides/voice-latency-cost?api=live#voice-session-costs)

WebRTC 的 `POST /v1/live/sessions` 创建在初始化时计 15 秒，运行后抵扣实际时长，不是在运行时长上另加 15 秒。不要把该 WebRTC 规则未经证实扩展成 WS 规则。[初始化费用](https://developers.openai.com/api/docs/guides/voice-latency-cost?api=live#webrtc-initialization-charges)

`session.usage.updated.usage.seconds` 是累计快照，12 → 15 意味共 15 秒，不能求和；最终以 `session.closed` 为准。当前模型页并发限制：Free 不支持，Build 50、Launch 300、Grow 500；实际账号访问和项目限额需确认。[用量](https://developers.openai.com/api/docs/guides/live-conversations#usage-and-graceful-close)、[并发](https://developers.openai.com/api/docs/models/gpt-live-1#rate-limits)

### 7.3 存储和数据控制

`store` 默认 false，不等于整个 API 完全零保留：官方数据表列 Live 不用于训练、标准 abuse-monitoring retention 为 30 天；application state 为无或保存 30 天。ZDR 有单独资格和限制。[数据表](https://developers.openai.com/api/docs/guides/your-data#storage-requirements-and-retention-controls-per-endpoint)

项目 storage 功能需启用且数据策略允许；`store: true` 保存完成录音供下载 / fork，保留 30 天。ZDR 下 `store` 按 false 处理，不能下载 / fork；公共 API 不提供 stored-session deletion endpoint。Live 支持美欧 data residency，后台模型和工具各有自身控制。[Live 数据控制](https://developers.openai.com/api/docs/guides/your-data#v1livesessions)

录音下载为 binary stereo WAV：input 左声道、output 右声道；它与实时 primary WS 的 mono 格式不同。[录音下载](https://developers.openai.com/api/docs/guides/live-conversations#download-a-recording)

## 8. 实现前必须用真实会话补证的项目

1. 账号是否已有 `gpt-live-1` 权限，以及真实 project / region / proxy 可用性；本次没有调用收费 API。
2. WS 普通 output 在静音期间的实际发帧节奏、队列积压、能否稳定实现双工插话；官方未给语音输出 done / playback 时钟。
3. 扬声器外放的 AEC、耳机与蓝牙路由、设备切换和采样率变化；full-duplex 模型不能替代客户端声学链路。
4. client delegation 与 transcript 的到达相对时序；delegation 不含任务文本，应测试延迟片段、纠正、简短确认和多次委托。
5. 输出清队列后模型上下文与用户已听内容的差异，以及恢复策略；不能假设存在未文档化 truncate / played 事件。
6. 断线、过期、关闭中迟到任务、重连 operation 去重、最终 usage 缺失。
7. 16 kHz 与 24 kHz 对中文自然度和延迟的实际差异；20 ms chunk、播放缓存和 VAD 阈值属于工程实验参数。
8. 对应真实任务的“后端成功”和“用户实际听到结果”分别验收，包含插话、取消、迟到结果和静默等待。

上述验收方向与官方要求的任务成功、上下文更正、guardrail、实际播放和完整关闭一致；数值目标应由项目基线实测制定，不把研究提案写成官方 SLA。[迁移验收](https://developers.openai.com/api/docs/guides/live-migration#validate-the-migration)、[完整交互验证](https://developers.openai.com/api/docs/guides/live-delegation#verify-the-complete-interaction)

## 9. 已实际获取的一手材料

- [Getting started with GPT-Live](https://developers.openai.com/api/docs/guides/live)
- [GPT-Live 1 模型](https://developers.openai.com/api/docs/models/gpt-live-1)
- [Live primary WebSocket API reference](https://developers.openai.com/api/reference/resources/live/primary-websocket)
- [Live sideband WebSocket API reference](https://developers.openai.com/api/reference/resources/live/sideband-websocket)
- [WebSockets（Live 标签）](https://developers.openai.com/api/docs/guides/voice-websockets?api=live)
- [WebRTC（Live 标签）](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live)
- [Managing GPT-Live sessions](https://developers.openai.com/api/docs/guides/live-conversations)
- [Delegation and tools in GPT-Live](https://developers.openai.com/api/docs/guides/live-delegation)
- [Prompting GPT-Live](https://developers.openai.com/api/docs/guides/live-prompting)
- [Migrate to GPT-Live](https://developers.openai.com/api/docs/guides/live-migration)
- [Server-side controls（Live 标签）](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live)
- [Cost optimization（Live 标签）](https://developers.openai.com/api/docs/guides/voice-latency-cost?api=live)
- [API pricing](https://developers.openai.com/api/docs/pricing)
- [Data controls](https://developers.openai.com/api/docs/guides/your-data#v1livesessions)
