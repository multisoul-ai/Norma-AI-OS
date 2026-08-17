# 火山引擎豆包流式 ASR 扩展设计

> 日期：2026-08-02
>
> 状态：Confirmed（2026-08-08 方案访谈确认）；实现尚未开始，本文不授权修改生产代码
>
> 参考调研：[`2026-08-02-volcengine-doubao-streaming-asr-integration.md`](../research/2026-08-02-volcengine-doubao-streaming-asr-integration.md)
>
> 既有决策：[`ADR-0026`](../adr/0026-own-built-in-asr-providers-and-azure-speech-in-asr.md)

## 1. 结论

豆包流式 ASR 应作为第三个内置 ASR **Adapter** 接入现有
`RecognizerFactory → Recognizer → RecognitionPipeline → RecognitionEvent`
**Seam**，而不是在 App 中建立第二条语音链路。

设计结果：

- `voice_input` 继续只负责麦克风、重采样、16 kHz mono PCM16 和共享 VAD，不感知火山。
- `asr` 新增 `CloudVolcengine` Provider、配置语义、WSS 二进制协议和识别实现。
- `voice_sherpa` 兼容层只做 provider-neutral 的 Continuous Listening prepared-session 适配，不出现火山类型或协议分支。
- `app` 只增加豆包 profile、Keychain 引用、Draft → Validate → Ready → Activate 和启动状态编排。
- partial 只更新字幕；只有唯一 `Final` 可以进入现有 Dialogue/Agent 回合。
- Local Sherpa 仍为默认项；Azure 与 Local 的既有配置和运行行为不变；豆包错误不得静默回退。
- 现有 `RecognitionPipeline` 后台线程直接作为 WSS 的单一 socket owner，不在 Adapter 内再增加第二层 worker 与音频队列。
- 第一版同时交付 Single Voice Command 与 Continuous Listening；Continuous 在空闲期预建 session，并保留最多 2 秒的异常兜底缓冲。

这满足“新增能力集中在一个深 **Module**、调用方继续使用小 **Interface**”的目标。

## 2. 第一性原则与设计约束

### 2.1 必须保持的不变量

1. 麦克风只采集一次，同一段音频只发送给当前显式选择的 Provider。
2. 一个 ASR session 最多产生一个终态：`Final`、`Empty`、`Failed` 或 `Cancelled`。
3. partial 永远不能启动 Agent；服务端 `definite=true` 也不是整次用户输入结束。
4. Single Voice Command 由用户松键结束；Continuous Listening 继续由共享 Silero VAD 分段。
5. 云端失败必须可见，不得自动切换 Local、Azure 或其他 Provider。
6. secret 只进入 Keychain 和运行时 secret 类型，不进入 Settings、Debug、日志或测试快照。
7. 网络、握手、recognizer 创建、终态等待和线程 join 不进入 GPUI 前台。
8. 不改变 Agent、Canvas、Terminal、TTS 和最终语音回合语义。
9. 每个进程最多一条豆包 WSS；同一时刻不会同时运行 Single 与 Continuous 的豆包 session。
10. 音频不得静默丢弃；累计 full partial 可以 latest-wins 合并，final/error 不得覆盖。

### 2.2 为什么复用现有 Seam

当前已经有 Local Sherpa 与 Azure 两个 Adapter，说明 Provider seam 不是假设，而是真实变化点。
第三个 Adapter 应增加该 **Module** 的 **Depth**，而不是把 WSS、鉴权、序列号和 Gzip
知识扩散到 App、Voice Input 或 Dialogue。

删除测试也能说明该取舍：如果未来移除豆包 Adapter，火山协议复杂度应整体消失；如果移除
`RecognitionPipeline`，partial、终态、取消、时长限制和两种交互模式的复杂度会重新散落到多个调用方。

### 2.3 非目标

- 不建设运行时插件系统或任意 Provider URL。
- 不修改 Agent 工具协议、TTS、Canvas、Terminal 或 Storage schema。
- 不将火山 Realtime Gateway 与豆包语音直接 WSS 混为一个 Adapter。
- 首版不支持 `bigmodel_nostream`、二遍识别、任意外语模型、说话人分离、时间戳编辑或会议转写。
- 首版不把服务端 VAD 当作整次输入边界。
- 首版不做自动 Provider fallback、双 Provider 投票、断线续传或发送音频后的自动重试。
- 不把厂商宣传的准确率或毫秒数字当作 Norma OS 的 SLA；正式可选必须通过第 13 节实测门槛。

## 3. 整体架构图

```text
Voice Settings / Keychain
          │  profile + credential_ref / X-Api-Key
          ▼
builtin_recognizer_factory
          │
          ▼
RecognizerFactory（现有 Seam）
          │
          ▼
RecognitionPipeline 专用后台线程（唯一 socket owner）
          │
    ┌─────┼──────────────┐
    ▼     ▼              ▼
 Local   Azure       Volcengine Adapter
Sherpa   Adapter       ├─ 20 ms → 200 ms 聚合
                       ├─ binary codec / Gzip / sequence
                       ├─ WSS transport / proxy / deadlines
                       └─ partial / last / unique final
          │
          ▼
RecognitionEvent
Partial / InputEnded / Final / Empty / Failed / Cancelled
          │
    ┌─────┴───────────┐
    ▼                 ▼
Single Voice      Continuous Listening
Command           local Silero VAD + prepared session
    │                 │
    └────────┬────────┘
             ▼
       现有 Dialogue → Agent → TTS

voice_input（保持不变）
CoreAudio → 16 kHz mono PCM16 → 约 20 ms chunk
```

只有 `Cloud Volcengine Adapter`、Continuous 的 provider-neutral prepared-session 生命周期及 App 配置入口是新增能力。音频输入、公共识别事件、最终文本进入 Agent 的路径保持原样。

## 4. 现有链路与接入后的数据流

### 4.1 Single Voice Command

1. 用户按下语音键，App 进入 `Preparing`，在后台解析当前 Provider profile 和 Keychain secret。
2. `builtin_recognizer_factory` 创建当前 Adapter；豆包 Adapter 在此阶段完成 DNS/TLS/WSS、鉴权和首个 full request。
3. WSS ready 后才启动麦克风并进入 `Recording/Listening`，避免连接期间丢失首音。
4. `voice_input` 持续产生约 20 ms PCM；公共 `RecognitionPipeline` 按现有有界通道转交。
5. 豆包 Adapter 内部聚合为 200 ms 音频包并发送；变化后的累计文本映射为 `Partial`。
6. 用户松键后，Pipeline 先产生 `InputEnded(User)`；Adapter 刷新不足 200 ms 的尾包并发送带 last flag 的最后一帧。
7. 收到服务端完整终态后返回唯一 `Final`；App 清空 partial，并沿用
   `handle_final_voice_text → begin_single_voice_command → Agent`。

### 4.2 Continuous Listening

1. App 在启动麦克风前解析 profile/Keychain，并完成首条 provider-neutral prepared session；ready 后才进入 Listening。
2. App 仍使用共享 Silero VAD 决定 `SpeechStart/SpeechEnd`；豆包服务端的 `definite` 只更新 partial/稳定分句，不结束当前 utterance。
3. `SpeechStart` 只把已准备好的 session 原子切换为 active，不在用户开口后同步执行 `factory.create()`。
4. 如果 prepared session 恰好失效，进入 Reconnecting 并最多缓存 2 秒 PCM（64,000 bytes）；2 秒内 ready 则按原顺序补发，否则整段 `Failed`，不能丢首音或提交残缺文本。
5. 本地 VAD 的 `SpeechEnd` 调用 `finish(InputEndReason::Vad)`；收到唯一 final 后沿用现有 echo gate 和 Dialogue 回合。
6. 当前 session 完成后立即预建下一条连接。临时网络错误只为下一段最多重建 3 次（500 ms、1 s、2 s）；鉴权/资源/参数/协议错误停止 Continuous、撤销 Ready，并要求用户重新 Validate。

### 4.3 Partial 与 Final 映射

| 火山事件 | Norma 事件 | 业务含义 |
|---|---|---|
| 文本发生变化，未结束整次输入 | `RecognitionEvent::Partial` | 仅更新字幕 |
| `utterance.definite=true` | 仍为 `Partial` | 句级稳定，不提交 Agent |
| 用户松键或本地 VAD 结束 | `InputEnded(User/Vad)` | 由公共 Pipeline 发出 |
| last frame 后、经真实 L3 确认的终态响应 | `Final` | 唯一可进入 Agent 的文本 |
| 空最终文本 | `Empty` | 不启动 Agent |
| error frame、鉴权、协议或网络失败 | `Failed` | 显示错误，不回退 |
| 用户取消或切换 Provider | `Cancelled` | 关闭 socket，拒绝迟到结果 |

last 后 10 秒内没有明确终态时返回 `Failed(Timeout)`；禁止把最后一个 partial 冒充 final。终态字段/sequence/flags 与零余量 last frame 的具体编码必须由目标账号官方 Demo 和真实 L3 锁定，未锁定前 Provider 不得 Activate。

## 5. ASR Module 设计

### 5.1 Provider 注册表

在 `crates/asr/src/providers/mod.rs` 中做纯增量扩展：

- `AsrProviderId::CloudVolcengine`
- 稳定存储值：`cloud_volcengine`
- display name：`Doubao Streaming ASR`
- location：`Cloud`
- capabilities：
  - `partial_results = true`
  - `max_utterance_seconds = Some(60)`

首版 60 秒是 Norma OS 的成本与交互安全上限，不是火山官方单连接上限声明。
它让 Single Voice Command 保持与 Azure 一致的上限语义，并降低误触导致的长时间计费风险。
提示文案应从 “Azure 60-second limit” 改为供应商中立的
“60-second voice input limit reached”。

`BuiltinAsrProfile` 增加：

~~~text
CloudVolcengine(VolcengineAsrProfile)
~~~

`builtin_recognizer_factory` 继续是唯一组合入口。App 不创建
`VolcengineRecognizerFactory`，也不拼接请求头。

### 5.2 Profile 与鉴权

首版只支持新版控制台 `X-Api-Key`，不实现旧版 App ID + Access Token：

~~~text
VolcengineAsrProfile
├── resource_id: String
├── credential_ref: String         # 指向 Keychain
└── protocol_mode: OptimizedStreaming
~~~

运行时 secret 类型：

~~~text
VolcengineAsrSecret([REDACTED])
~~~

规则：

- Keychain 中的 secret 只发送为 `X-Api-Key`；不保存或发送旧版鉴权字段。
- 请求发送目标新版控制台确认的 `X-Api-Resource-Id`。
- UI 可以预填 2.0 小时版候选值 `volc.seedasr.sauc.duration`，但真实 L3 通过前不得把它视为已验证值；
  若允许编辑，只接受非空 ASCII 字母、数字、点、下划线和连字符。
- endpoint 固定为官方 `bigmodel_async`，不允许用户输入任意 URL，避免 SSRF 和协议漂移。
- Debug 只显示是否配置和脱敏 resource ID，不显示 secret 或完整请求头。

### 5.3 首版请求参数

为减少变量，首版固定：

| 参数 | 值 |
|---|---|
| endpoint | `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async` |
| audio | PCM16、16 kHz、mono、raw |
| packet | 200 ms，6,400 bytes |
| `result_type` | `full` |
| `enable_punc` | `true` |
| `enable_itn` | `true` |
| `show_utterances` | `true` |
| `enable_ddc` | `false` |
| `enable_accelerate_text` | `false` |
| `enable_nonstream` | `false`，先稳定单遍状态机 |

200 ms 字节数推导：

~~~text
16,000 samples/s × 2 bytes/sample × 1 channel × 0.2 s = 6,400 bytes
~~~

二遍识别和首字加速应在真实语料 A/B 后单独决策，不能与基础接入同时引入。

### 5.4 Adapter 内部结构

建议目录：

~~~text
crates/asr/src/providers/cloud/volcengine/
├── mod.rs          # 公开 profile/config/factory
├── config.rs       # typed profile、secret、请求参数校验
├── protocol.rs     # binary frame encode/decode、Gzip、JSON、sequence
├── transport.rs    # 固定 host、TLS、WebSocket、proxy、deadline
└── recognizer.rs   # socket owner、200 ms 聚合与事件映射
~~~

该目录是一个深 **Module**：外部只看到现有 `RecognizerFactory/Recognizer`
**Interface**；鉴权、帧协议、网络并发和错误分类全部保持 **Locality**。

### 5.5 线程所有权、背压与生命周期

现有 `RecognitionPipeline` 已有 64 个约 20 ms PCM command 槽位：

~~~text
64 × 20 ms = 1.28 s 外层有界 backlog
~~~

不再增加 Adapter 内部 worker。现有 `RecognitionPipeline` 专用后台线程串行调用
`accept_pcm/finish`，并直接拥有 `VolcengineRecognizer` 及其 WSS：

- `factory.create()` 在后台用 5 秒绝对 deadline 完成 DNS/TCP/TLS/WSS、鉴权与 full request；
- `accept_pcm()` 聚合 20 ms PCM，满 6,400 bytes 时发送，并使用非阻塞/短 poll 读取可用响应；
- `finish()` 刷新尾包、发送 last，在同一后台线程中最多等待 final 10 秒；
- cancellation/Drop 关闭 socket，并保证线程有界结束；
- 每个进程最多一条豆包 WSS，Single 与 Continuous 不得并行持有豆包 session。

现有外层 64-slot backlog 仍是最后一道保护，但不能作为 Continuous 建连缓冲使用。
prepared session 失效时使用独立、可观察的 2 秒 PCM 缓冲：

~~~text
16,000 samples/s × 2 bytes/sample × 2 s = 64,000 bytes
~~~

音频顺序无法保证或 2 秒仍未 ready 时，当前 utterance 明确 `Failed(Backpressure/NotReady)`；
累计 full partial 可以 latest-wins 合并，不因字幕消费者短暂变慢而失败；final/error 永远优先且不可覆盖。

### 5.6 Sequence、尾包与重连

- 每条连接维护单调递增 sequence。
- 最后一条音频帧使用 last flag 和协议要求的负 sequence。
- 如果用户松键时存在不足 200 ms 的余量，余量作为最后一帧发送。
- 如果余量恰好为空，必须先按官方 Demo/真实 L3 验证零长度 last frame；未验证前不得 Activate。
- parser 按 header size 跳过受限长度的未知 extension；截断、长度不一致、超限 payload、未知 compression 或非法 sequence 失败关闭。压缩前后分别设置 payload 上限，防止异常分配和 Gzip 解压炸弹。
- 发送第一帧音频后不自动重连或重发，避免重复计费、重复文本和 Agent 二次执行。
- Single 建连失败不自动重试。Continuous 临时网络失败只为下一段最多重建 3 次，退避为 500 ms、1 s、2 s；鉴权、资源、参数或协议错误不重试并撤销 Ready。

## 6. App、Settings 与凭据设计

### 6.1 Settings 状态

新增独立豆包 profile，保持现有 Provider 配置互不覆盖：

~~~text
Draft → Validating → Ready → Active
  ↑          │          │
  └─ 编辑任何字段 ──────┘  # 编辑立即撤销 Ready
~~~

建议非敏感 Settings keys：

~~~text
voice.asr.volcengine.resource_id
voice.asr.volcengine.credential_ref
voice.asr.volcengine.ready
~~~

Keychain account 使用 `asr-volcengine-*` 前缀。现有通用 key-value Settings store
可以直接承载这些键，不需要修改 `storage` crate 或数据库 migration。
SQLite 的 `settings.value` 是普通 TEXT，不得保存 `X-Api-Key`；Key 本体沿用现有
[`credentials.rs`](../../crates/app/src/settings/voice/credentials.rs) 的
`security_framework` credential store，复用 service
`com.norma-ai.os.agent-provider`，数据库只保存不透明 `credential_ref`。历史明文键只允许
作为一次性迁移输入，迁移完成必须删除，不能成为豆包的正常存储路径。

异步验证必须携带完整 profile snapshot/fingerprint。只有返回时 profile 仍完全相等，才允许标记 Ready；
迟到验证不能覆盖用户刚编辑的新草稿。替换 secret 失败时恢复旧 credential reference，
并撤销新 Ready。

### 6.2 Validate 与 Test Recognition

Validate：

- 使用真实 WSS、新版 `X-Api-Key` 和 resource ID；
- 发送 200 ms 静音与 last frame，验证 TLS、鉴权、资源授权和协议闭环；静音导致 Empty/静音状态不算识别失败；
- 可能消耗极少量额度，UI 应明确说明；
- 成功只变为 Ready，不自动激活。

Test Recognition：

- 复用公共 `RecognitionPipeline` 录制 3 秒；
- 展示实际 final；
- 不改变当前 Provider；
- 失败保留草稿和已有 Active Provider。

Activate：

- 只有 Ready profile 可以激活；
- 激活前取消当前录音、Continuous runtime、partial task 和迟到结果；
- 首次激活前确认“麦克风音频将发送到火山引擎，可能产生费用”；确认一次后不在每次录音重复弹窗；
- 原子持久化 `voice.asr.provider = cloud_volcengine`；
- 不清除 Local/Azure profile。
- Active 时不能直接删除 Key；用户需先显式切换到 Local/Azure，再删除豆包凭据，避免把主动删除伪装成运行时 fallback。

### 6.3 Settings 信息架构

不新增窗口或独立大页面。继续使用现有 Speech Recognition 区域：

- Recognition source 显示 `On-device`、`Azure Speech`、`Doubao ASR` 三个来源；
- 三项采用可换行的同行选择，不再依赖当前 232 px 固定宽度；
- 一次只展开一个来源的 detail；
- 豆包 detail 按常用路径优先显示：resource ID、API Key、Validate；不显示旧版鉴权入口；
- Validate/Test 状态显示实际生效的连接路径：Direct、HTTPS CONNECT、SOCKS5 或
  `NO_PROXY` bypass，不显示代理凭据；
- Ready 与 Active 使用不同文案，Validate 和 Test 永不隐式切换；
- 明确标注“音频发送到火山引擎，可能产生费用”。

这遵循 Apple 的 Familiarity、Agency、Simplicity 和 Responsibility：
入口留在用户熟悉的位置，选择与激活分开，云端隐私/费用在行动前可见，
状态和错误即时反馈，不用动画掩盖网络等待。

### 6.4 通用化而非新增 App 厂商分支

当前 `resolve_asr_recognizer_factory` 使用名为 `load_azure_asr_secret` 的方法。
实现时应收口为供应商中立的 `load_asr_secret(credential_ref)`，然后把 resolver
继续传给 `asr::builtin_recognizer_factory`。App 不能新增：

~~~text
if provider == Volcengine { 拼 headers / 建 WebSocket / 解析 JSON }
~~~

Settings 可以拥有 provider-specific 的字段和文案；运行时识别编排必须保持 provider-neutral。

## 7. 启动顺序与“首音不丢”

豆包 WSS 比 Azure/local factory 创建更可能阻塞。现有 Single 路径虽然已在后台执行
`factory.create()`，但 UI 会提前进入 Recording；Continuous 路径还会在麦克风启动后
才初始化首个 recognizer。

实现时必须做两个供应商中立的顺序修正：

1. VoiceControl 增加 `Preparing`，只有 recognizer/WSS ready 且麦克风真正启动后才进入
   `Recording`。用户看到 Listening 后即可立即说话。
2. Continuous runtime 先准备首个 provider-neutral listening pipeline，再启动 PCM capture；
   每个 utterance 收到唯一终态后立刻预建下一 session。现有
   `SpeechStart → reset → factory.create()` 的顺序必须收口为可复用的 prepared-session
   生命周期，不能把豆包判断写进 Continuous runtime。

正常路径不采用“先录音、后台慢慢连接、依赖大缓冲”的方案。只有已经 Ready 的预建连接
在 `SpeechStart` 瞬间失效时，才启用严格上限为 2 秒的异常兜底缓冲：
`16,000 samples/s × 2 bytes × 2 s = 64,000 bytes`。连接在预算内恢复后按原顺序发送；
超过预算则整段 `Failed`，清空缓冲且不提交 partial。每个进程最多存在一条豆包 WSS，
上一段未终结时不得并行预建下一条。

```text
Stopped → Preparing → Ready → Listening → Finalizing → PreparingNext
              │          │                         │
              └─ Failed ←┴─ Reconnecting（仅异常兜底，最多 2 秒 PCM）
```

## 8. 可观测性

使用现有 `norma_voice` tracing target，新增结构化阶段而不记录正文：

| 时间点/字段 | 用途 |
|---|---|
| provider resolve start/end | 区分 Settings/Keychain 延迟 |
| WSS connect start/ready | 连接耗时 |
| microphone ready / first PCM | 首音是否被延迟 |
| first 200 ms frame sent | 客户端聚合耗时 |
| first partial | ASR 首次延迟 |
| input ended / last frame sent | 用户结束到服务端收到尾包 |
| final received | ASR 尾延迟 |
| Agent first event | “Agent 首次延迟” |
| request/connect UUID、`X-Tt-Logid` | 火山工单定位 |
| PCM bytes、音频时长、resource ID、mode | 成本与协议诊断 |
| effective proxy path | 区分 direct / HTTPS CONNECT / SOCKS5 / NO_PROXY |

严禁记录：

- `X-Api-Key` 或任何 Authorization/鉴权 header；
- 原始音频；
- 完整 partial/final 文本；
- 未脱敏服务端响应。

现有 Continuous 路径中任何记录 transcript 正文的日志都必须在接入前移除或改为字符数。
本地可以展示“本次/今日估算识别分钟数”作为信息提示，但不能称为账单或额度；计费以火山
控制台为唯一权威来源。Test Recognition 的文本只在当前 Settings 会话显示，不持久化。

建议以 session ID 串起所有阶段，最终可以分别计算：

~~~text
准备延迟 = WSS ready - provider resolve start
首字延迟 = first partial - first PCM
尾字延迟 = final - input ended
Agent 首次延迟 = Agent first event - final
端到端延迟 = Agent first event - input ended
~~~

## 9. 改动范围

| 范围 | 预计改动 | 说明 |
|---|---|---|
| `crates/asr` | 新增 Volcengine 子模块；扩展注册表/profile；增加 `tungstenite`、Gzip、代理支持 | 主要新增逻辑 |
| `crates/asr/tests` | codec golden、fake WSS、registry、pipeline 合约、代理、ignored real L3 | 主要验证 |
| `crates/app/src/settings/voice` | 新 profile keys/state、Keychain 生命周期、第三来源、Validate/Test/Activate | 用户配置 |
| `crates/app/src/app/voice` | 通用 `Preparing`、供应商中立时长提示、既有 partial/final 路由复用 | 小范围编排 |
| `crates/app/src/app/dialogue/chat.rs` | 通用 secret resolver；首个 recognizer 在采集前准备 | 不含火山协议 |
| `crates/voice_sherpa` | provider-neutral 的 prepared-session 生命周期、2 秒兜底缓冲与下一段重建 | 保留兼容门面 |
| `crates/e2e` | 豆包 Settings、Single、Continuous、错误与切换流程 | 用户可观察证据 |
| `docs` | 本设计；实施时按需补 ADR/执行计划 | 文档 |

明确不改：

- `crates/voice_input` 的 PCM 格式、设备和权限；
- `crates/agent`、`crates/dialogue` 的最终文本语义；
- `crates/tts`、`crates/runtime`、`crates/canvas`、`crates/terminal`；
- `crates/storage` schema；
- Local Sherpa 模型、Hotword 和 Azure profile keys。

受真实协议终态、代理实现和 Continuous prepared-session 的 L3 结果影响，文件数与行数不再
在设计阶段锁定；进入施工前应按第 14 节拆成独立执行计划并重新估算，不能一次性大改。

## 10. 兼容性

### 10.1 升级

- 缺少 `voice.asr.provider` 时仍默认 `LocalSherpa`。
- Local 模型 ID、Hotword、输入设备、Azure region/locale/credential_ref 原样保留。
- 新增 enum variant 的所有 match 由编译器强制补齐。
- 不迁移数据库；新键按需写入。

### 10.2 降级

旧版本无法识别 `cloud_volcengine` 时，现有 `from_stored` 会返回 `None` 并回到 Local。
豆包的独立 profile keys 和 Keychain secret 不应被旧版本删除。再次升级后可恢复配置，
但需要用户重新确认 Active 状态。

### 10.3 运行时

- 切换 Provider 会先取消当前 session，generation gate 拒绝迟到结果。
- Local/Azure 不经过豆包 recognizer、codec 或 WSS transport。
- 豆包网络失败只产生当前 session 的 `Failed`，不改变持久化 Provider。
- Continuous 与 Single 仍共享公共 Pipeline，但各自保持原输入结束策略。

## 11. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 新版控制台请求头或 Demo 更新 | 无法握手或错误授权 | 只支持 `X-Api-Key`；真实 Validate；L3 固化 golden request |
| 2.0 resource ID/免费额度未绑定 | 401/授权失败或产生费用 | resource ID 可见；激活前真实验证；不宣传免费 |
| WSS 二进制/Gzip/sequence 实现错误 | 无结果、重复或协议失败 | byte-level golden tests + fake WSS + strict parser |
| socket 同时读写造成死锁 | 录音卡住、无法取消 | 单一 socket owner；短 read poll；所有等待有 deadline |
| 网络握手期间丢首音 | 命令缺开头 | `Preparing → Listening`；先 ready 再采集 |
| 预建连接在开口瞬间失效 | 丢首音或长时间等待 | 2 秒/64,000 bytes 有界缓冲；超限整段失败 |
| 慢语速被 `definite` 提前结束 | Agent 收到半句 | `definite` 只映射 partial；Single 仅松键结束 |
| partial 重复/回滚 | 字幕跳动 | `result_type=full`；按 sequence 丢弃旧包；去重累计文本 |
| final 重复 | Agent 重复执行 | Pipeline 单终态不变量 + session generation gate |
| 音频或字幕背压混用 | 丢音或无谓失败 | 音频无法按序接收即 Failed；full partial latest-wins |
| 断线重发导致重复计费 | 成本与重复执行 | 首帧音频后不自动重连/重发 |
| secret 或转写泄漏 | 安全/隐私事故 | Keychain、redacted secret、日志 allowlist、仓库/日志扫描 |
| 压缩炸弹或超大响应 | 内存/CPU 被耗尽 | 解压后大小、JSON 深度与 frame payload 上限 |
| 企业代理/SOCKS 不兼容 | 部分网络无法连接 | L3 覆盖 direct/CONNECT/SOCKS5/NO_PROXY；不静默绕过代理 |
| UI 第三来源变拥挤 | 设置难理解 | 单来源 detail、可换行 selector、新版鉴权只显示两个字段 |
| 空闲预建连接占用并发或计费 | 成本不可控 | 一进程最多一条；不发送静音；真实账号核验计费/空闲超时 |
| 火山协议或 SKU 更新 | 线上回归 | 固定 endpoint、typed config、握手 Log ID、版本化测试语料 |

## 12. 预期效果

### 12.1 用户效果

- 中文、中英混合和方言场景获得一个可显式选择的云端识别来源。
- 说话期间能看到真实 partial；相比 Azure 的松键后整段上传，识别计算与说话并行，
  理论上可降低用户松键后的等待，但具体提升必须实测。
- 失败、费用位置和当前 Active Provider 可预测；不存在隐式云端或隐式回退。
- 慢语速 Single 命令不会因服务端分句标志自动提交半句。

### 12.2 工程效果

- 新增 Provider 不复制 Voice Input、VAD、Agent 或 TTS 链路。
- 火山协议知识集中在一个深 Module，测试可以从公共 Recognizer seam 覆盖。
- 现有 Local/Azure 行为由相同合约继续保护。
- 新增阶段日志可把总耗时拆成准备、首字、尾字和 Agent 首次延迟。

### 12.3 不作承诺

- 不承诺豆包一定比 Azure 或 Local 更准确。
- 不承诺 20 小时一定覆盖 2.0。
- 不承诺厂商宣传的 300～400 ms 是 Norma OS 的端到端延迟。
- 不在没有同语料 CER、技术词命中率和 p50/p95/p99 前替换默认 Local。

## 13. 测试与验收设计

### 13.1 `asr` 合约

1. Provider registry 明确包含 3 个 Adapter；Local/Azure 能力保持原值，豆包支持 partial。
2. frame golden tests 覆盖 header size、flags、正负 sequence、大端长度、JSON/Gzip、
   full request、audio、response、ACK、error 和截断输入。
3. fake WSS 覆盖新版 `X-Api-Key`、200 ms 聚合、尾包刷新、partial latest-wins、last、
   唯一 final、error、断线、timeout、cancel、ping/pong、2 秒缓冲、重建预算和代理选择。
4. RecognitionPipeline 具体断言 `Partial → InputEnded → 唯一 Final`，并负向断言
   Empty/Failed/Cancelled/第二个 Final 不存在。
5. secret 的 Debug、错误和默认 tracing 中没有测试 Key。

### 13.2 App/Settings

1. 缺少新键时仍选择 Local。
2. 豆包 Draft 不影响当前 Local/Azure。
3. 编辑字段或替换 Key 后撤销 Ready。
4. 迟到 Validate 不能覆盖新 profile。
5. Ready 后显式 Activate；Validate/Test 不隐式切换。
6. 往返切换 Local/Azure/Doubao 保留各自 profile。
7. 运行错误不触发 fallback。
8. Keychain 写入失败回滚旧引用；Remove 清除 Ready。

### 13.3 用户流程

- headless GPUI e2e：第三来源、渐进披露、Validate、Test、Activate、错误、切换。
- Single e2e：Preparing、partial 字幕、松键 final、Agent 只启动一次、取消。
- Continuous e2e：本地 VAD 分段、partial、final、echo gate、下一 session 预建、开口瞬间
  失效后的顺序回放、超出 2 秒后整段失败且 Agent 不启动。
- 真实 App Computer Use：Light/Dark、键盘焦点、按钮 hover/pressed/disabled、
  Ready/Active/错误状态、长文案和窄窗口。
- ignored L3：真实 2.0 resource、普通话、慢语速 0.8～3 秒停顿、中英混说、
  技术词、零长度 last、明确终态、网络中断、代理路径，并记录脱敏的阶段耗时和
  `X-Tt-Logid`。

正式可选前的量化门槛：固定网络下建连成功率 ≥ 99%；WSS ready p95 ≤ 2 秒且绝不超过
5 秒；首 partial p95 ≤ 800 ms；输入结束到 final p95 ≤ 1.5 秒、p99 ≤ 3 秒；同语料
CER 与技术词/路径/命令/中英混说命中率不得差于 Local/Azure 中更好的一个；丢首音、
重复 final、重复 Agent 回合、partial 冒充 final、失败/取消后提交 partial 均为 0；
0.8～3 秒停顿造成的提前提交为 0。

所有新增 Rust 测试必须遵循仓库要求的场景、数据构造、执行过程、预期结果注释，
数值写出推导，每个断言带失败说明，关键正负状态逐一断言。

## 14. 分阶段实施建议

只有本文确认后才进入以下阶段：

1. **协议 codec 与 fake WSS（不可注册）**

   实现 typed config、codec、解析上限和 fake WSS；不进入 Provider registry。

2. **Recognizer/transport 与真实 L3（不可注册）**

   实现单 socket owner、deadline、proxy 与 Recognizer 合约；用目标账号验证新版鉴权、
   实际 2.0 resource、零长度 last、唯一 final、取消和错误分类。

3. **Provider registry（不可激活）**

   注册 `CloudVolcengine`，但配置尚未 Ready 时不能被 resolver 创建。

4. **Settings 与 Keychain**

   增加 Draft/Validate/Ready/Test，完成真实 App UI 验收；验证不自动激活。

5. **Single Voice Command**

   加入通用 Preparing，接入现有 Final → Agent 路径，冻结 Local/Azure 回归。

6. **Continuous Listening**

   调整 provider-neutral prepared-session 生命周期，验证 VAD、echo gate、2 秒缓冲、
   下一段重建、失败不提交和最多一条 WSS。

7. **实验性显式激活**

   开放 Use Doubao；默认仍是 Local；采集错误率、阶段延迟和本地估算用量。

8. **正式可选评估**

   只有第 13 节量化门槛达标后转为正式可选；再决定二遍识别、首字加速或默认策略。

每阶段都必须跑受影响 crate 测试、Local/Azure 回归、fake/L3 对应证据、App/e2e、真实 App
Computer Use 与日志脱敏检查，并负向验证“无自动 fallback、一个 final 只启动一个 Agent”。
每阶段都应可独立回退；不得在接入阶段删除 `voice_sherpa` 兼容层或重写 Voice Settings。

## 15. 确认后仍需由目标账号提供的实施闸门

开始真实 L3 前需要从登录后的新版火山控制台确认：

- 实际授权的 2.0 小时版 resource ID；候选值 `volc.seedasr.sauc.duration` 不能直接当事实；
- 官方 Demo 当前除 `X-Api-Key` 外还要求哪些追踪 header；
- 零余量输入结束时，服务端是否接受零长度 audio-only last frame；
- last 后哪个字段、sequence 或 flags 才是唯一终态；
- 20 小时试用是否抵扣该 resource、有效期、并发/QPS 与正式价格；
- 空闲预建 WSS 是否计费、占用何种并发、何时超时以及 ping/pong 要求；
- 目标网络的 direct、HTTP CONNECT、SOCKS5 与 `NO_PROXY` 实测结果；
- 稳定错误码及其 auth/resource/params/protocol/transient 分类；
- 固定语料上的 CER、技术词/路径/命令/中英混说和延迟门槛是否达标。

这些信息只影响豆包 Adapter 的 profile 和上线闸门，不改变本文确定的公共 Seam 与数据流。
