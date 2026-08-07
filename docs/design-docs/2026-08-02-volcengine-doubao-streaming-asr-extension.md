# 火山引擎豆包流式 ASR 扩展设计

> 日期：2026-08-02
>
> 状态：Proposed，等待确认；本文不授权修改生产代码
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
- `voice_sherpa` 兼容层只做 provider-neutral 的 Continuous Listening 适配，不出现火山类型或协议分支。
- `app` 只增加豆包 profile、Keychain 引用、Draft → Validate → Ready → Activate 和启动状态编排。
- partial 只更新字幕；只有唯一 `Final` 可以进入现有 Dialogue/Agent 回合。
- Local Sherpa 仍为默认项；Azure 与 Local 的既有配置和运行行为不变；豆包错误不得静默回退。

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
- 首版不支持 `bigmodel_nostream`、任意外语模型、说话人分离、时间戳编辑或会议转写。
- 首版不把服务端 VAD 当作整次输入边界。
- 首版不做自动 Provider fallback、双 Provider 投票、断线续传或发送音频后的自动重试。
- 首版不承诺准确率或固定毫秒 SLA；必须用 Norma OS 同语料实测。

## 3. 整体架构图

~~~mermaid
flowchart LR
    Trigger["⌘E / Voice Bar"] --> OneShot["Single Voice Command<br/>AsrRecordingSession"]
    Trigger --> Continuous["Continuous Listening<br/>DialogueListeningRuntime"]

    subgraph Input["voice_input（保持不变）"]
        Mic["CoreAudio 麦克风"] --> PCM["16 kHz · mono · PCM16<br/>约 20 ms chunk"]
        PCM --> VAD["共享 Silero VAD"]
    end

    PCM --> OneShot
    VAD --> Continuous

    subgraph App["crates/app（少量扩展）"]
        Settings["Voice Settings<br/>Local / Azure / Doubao"]
        Profile["typed profile + Ready 状态<br/>仅保存 credential_ref"]
        Keychain["macOS Keychain<br/>API Key / Access Token"]
        Resolver["resolve_builtin_asr_profile<br/>通用 secret resolver"]
        PartialUI["partial → Voice Caption"]
        FinalRoute["唯一 final → Dialogue<br/>→ Agent → 既有 TTS"]
        Settings --> Profile
        Keychain --> Resolver
        Profile --> Resolver
    end

    subgraph Asr["crates/asr（公共 Seam 保持）"]
        Registry["Builtin Provider Registry<br/>Local · Azure · Volcengine"]
        Factory["RecognizerFactory Interface"]
        Recognizer["Recognizer Interface<br/>accept_pcm / finish / cancel"]
        Pipeline["RecognitionPipeline<br/>有界命令与事件通道"]
        Events["RecognitionEvent<br/>Partial · InputEnded<br/>Final / Empty / Failed / Cancelled"]

        Local["Local Sherpa Adapter"]
        Azure["Cloud Azure Adapter"]
        Volc["Cloud Volcengine Adapter（新增）"]

        Registry --> Factory
        Factory --> Recognizer
        Local -. implements .-> Recognizer
        Azure -. implements .-> Recognizer
        Volc -. implements .-> Recognizer
        Recognizer --> Pipeline --> Events
    end

    Resolver --> Factory
    OneShot --> Pipeline
    Continuous --> Pipeline
    Events --> PartialUI
    Events --> FinalRoute

    subgraph VolcInternal["Volcengine Adapter 内部（新增，调用方不可见）"]
        Aggregate["20 ms → 200 ms 聚合<br/>full cumulative result"]
        SocketOwner["单一 socket-owner I/O worker<br/>有界队列 + deadline + cancellation"]
        Codec["4-byte header / sequence<br/>Gzip / JSON / error frame"]
        WSS["固定 WSS<br/>bigmodel_async"]
        Aggregate --> SocketOwner --> Codec --> WSS
        WSS --> Codec --> SocketOwner
    end

    Volc --> Aggregate
    WSS <--> Cloud["火山引擎豆包<br/>流式 ASR 2.0"]
~~~

图中只有 `Cloud Volcengine Adapter`、其内部协议实现及 App 的配置入口是新增能力。
音频输入、公共识别事件、最终文本进入 Agent 的路径保持原样。

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

1. App 仍使用共享 Silero VAD 决定 `SpeechStart/SpeechEnd`。
2. 每个 VAD utterance 使用同一 `RecognitionPipeline` 和所选 Provider 的新 session。
3. 豆包服务端的 `definite` 只更新 partial/稳定分句，不结束当前 utterance。
4. 本地 VAD 的 `SpeechEnd` 调用 `finish(InputEndReason::Vad)`；收到唯一 final 后沿用现有 echo gate 和 Dialogue 回合。
5. 当前 session 完成后，在下一段语音到来前预建下一条豆包连接，避免下一次开口时才握手。

### 4.3 Partial 与 Final 映射

| 火山事件 | Norma 事件 | 业务含义 |
|---|---|---|
| 文本发生变化，未结束整次输入 | `RecognitionEvent::Partial` | 仅更新字幕 |
| `utterance.definite=true` | 仍为 `Partial` | 句级稳定，不提交 Agent |
| 用户松键或本地 VAD 结束 | `InputEnded(User/Vad)` | 由公共 Pipeline 发出 |
| last frame 后的完整最终文本 | `Final` | 唯一可进入 Agent 的文本 |
| 空最终文本 | `Empty` | 不启动 Agent |
| error frame、鉴权、协议或网络失败 | `Failed` | 显示错误，不回退 |
| 用户取消或切换 Provider | `Cancelled` | 关闭 socket，拒绝迟到结果 |

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

官方新旧控制台鉴权不同，Profile 必须显式表达，不能自动猜测：

~~~text
VolcengineAsrProfile
├── auth_mode
│   ├── ApiKey
│   └── LegacyAppAccess
├── app_key: Option<String>        # 仅旧鉴权；非 secret
├── resource_id: String
├── credential_ref: String         # 指向 Keychain
└── protocol_mode: OptimizedStreaming
~~~

运行时 secret 类型：

~~~text
VolcengineAsrSecret([REDACTED])
~~~

规则：

- 新版 `ApiKey` 发送 `X-Api-Key`。
- 旧版 `LegacyAppAccess` 发送非秘密 `X-Api-App-Key` 与 Keychain 中的
  `X-Api-Access-Key`。
- 两种模式都发送目标控制台确认的 `X-Api-Resource-Id`。
- 默认 resource ID 为 2.0 小时版 `volc.seedasr.sauc.duration`，但必须在 UI 中可见；
  若允许编辑，只接受非空 ASCII 字母、数字、点、下划线和连字符。
- endpoint 固定为官方 `bigmodel_async`，不允许用户输入任意 URL，避免 SSRF 和协议漂移。
- Debug 只显示鉴权模式、是否配置和脱敏 resource ID，不显示 secret 或完整请求头。

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
├── config.rs       # typed profile、auth、secret、请求参数校验
├── protocol.rs     # binary frame encode/decode、Gzip、JSON、sequence
├── transport.rs    # 固定 host、TLS、WebSocket、socket timeout
├── worker.rs       # 单一 socket owner、命令/事件循环
└── recognizer.rs   # Recognizer/Factory Adapter、200 ms 聚合与事件映射
~~~

该目录是一个深 **Module**：外部只看到现有 `RecognizerFactory/Recognizer`
**Interface**；鉴权、帧协议、网络并发和错误分类全部保持 **Locality**。

### 5.5 并发、队列与生命周期

现有 `RecognitionPipeline` 已有 64 个约 20 ms PCM command 槽位：

~~~text
64 × 20 ms = 1.28 s 外层有界 backlog
~~~

豆包 Adapter 内部再使用：

- audio command queue：4 个 200 ms frame，即 0.8 秒；
- provider event queue：32 个结果变化事件；
- socket read poll：不超过 50 ms，以便及时观察取消；
- connect/handshake deadline：5 秒；
- last frame 后 final deadline：10 秒。

这些是初始工程预算，不是服务 SLA；实现阶段需通过慢网络和取消测试校准。
内层队列满时必须 `Failed(Backpressure)`，不能丢音频或无限堆积。

`Recognizer::accept_pcm` 只完成两件事：

1. 把 20 ms PCM 聚合成 200 ms frame 并提交给内部 worker；
2. 非阻塞排出已经到达的 partial。

它不得执行阻塞式 WebSocket read。`finish` 刷新尾包、发送 last，并在后台
RecognitionPipeline worker 中有界等待唯一 final。取消令牌应使 socket worker
在一个 read poll 内关闭连接。

### 5.6 Sequence、尾包与重连

- 每条连接维护单调递增 sequence。
- 最后一条音频帧使用 last flag 和协议要求的负 sequence。
- 如果用户松键时存在不足 200 ms 的余量，余量作为最后一帧发送。
- 如果余量恰好为空，先按官方 Demo/真实 L3 验证零长度 last frame；若服务拒绝，
  再采用不增加常态 200 ms 延迟的兼容策略。此项是实现前协议闸门。
- 收到未知 header extension、长度不一致、未知 compression 或非法 sequence 时失败关闭。
- 发送第一帧音频后不自动重连或重发，避免重复计费、重复文本和 Agent 二次执行。
- 仅允许在尚未发送音频时，对明确可恢复的握手失败进行一次受控重试；首版可以完全关闭自动重试。

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
voice.asr.volcengine.auth_mode
voice.asr.volcengine.app_key
voice.asr.volcengine.resource_id
voice.asr.volcengine.credential_ref
voice.asr.volcengine.ready
~~~

Keychain account 使用 `asr-volcengine-*` 前缀。现有通用 key-value Settings store
可以直接承载这些键，不需要修改 `storage` crate 或数据库 migration。

异步验证必须携带完整 profile snapshot。只有返回时 profile 仍完全相等，才允许标记 Ready；
迟到验证不能覆盖用户刚编辑的新草稿。替换 secret 失败时恢复旧 credential reference，
并撤销新 Ready。

### 6.2 Validate 与 Test Recognition

Validate：

- 使用真实 WSS、当前鉴权模式和 resource ID；
- 发送 100 ms 静音与 last frame，验证 TLS、鉴权、资源授权和协议闭环；
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
- 原子持久化 `voice.asr.provider = cloud_volcengine`；
- 不清除 Local/Azure profile。

### 6.3 Settings 信息架构

不新增窗口或独立大页面。继续使用现有 Speech Recognition 区域：

- Recognition source 显示 `On-device`、`Azure Speech`、`Doubao ASR` 三个来源；
- 三项采用可换行的同行选择，不再依赖当前 232 px 固定宽度；
- 一次只展开一个来源的 detail；
- 豆包 detail 按常用路径优先显示：鉴权模式、resource ID、凭据、Validate；
- 旧鉴权的 App Key 放在选择 `LegacyAppAccess` 后渐进披露；
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

批准实现时应做两个供应商中立的顺序修正：

1. VoiceControl 增加 `Preparing`，只有 recognizer/WSS ready 且麦克风真正启动后才进入
   `Recording`。用户看到 Listening 后即可立即说话。
2. Continuous runtime 先准备首个 provider-neutral listening pipeline，再启动 PCM capture
   和 worker；后续 utterance 在空闲期预建下一 session。

不采用“先录音、后台慢慢连接、依赖大缓冲”的方案，因为它会在慢连接时制造音频丢失、
高延迟或无界内存三选一的问题。

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

严禁记录：

- `X-Api-Key`、Access Token、Authorization/鉴权 headers；
- 原始音频；
- 完整 partial/final 文本；
- 未脱敏服务端响应。

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
| `crates/asr` | 新增 Volcengine 子模块；扩展注册表/profile；增加 `tungstenite`、Gzip 直接依赖 | 主要新增逻辑 |
| `crates/asr/tests` | codec golden、fake WSS、registry、pipeline 合约、ignored real L3 | 主要验证 |
| `crates/app/src/settings/voice` | 新 profile keys/state、Keychain 生命周期、第三来源、Validate/Test/Activate | 用户配置 |
| `crates/app/src/app/voice` | 通用 `Preparing`、供应商中立时长提示、既有 partial/final 路由复用 | 小范围编排 |
| `crates/app/src/app/dialogue/chat.rs` | 通用 secret resolver；首个 recognizer 在采集前准备 | 不含火山协议 |
| `crates/voice_sherpa` | provider-neutral 的 prepared pipeline 启动顺序 | 保留兼容门面 |
| `crates/e2e` | 豆包 Settings、Single、Continuous、错误与切换流程 | 用户可观察证据 |
| `docs` | 本设计；批准后按需补 ADR/执行计划 | 文档 |

明确不改：

- `crates/voice_input` 的 PCM 格式、设备和权限；
- `crates/agent`、`crates/dialogue` 的最终文本语义；
- `crates/tts`、`crates/runtime`、`crates/canvas`、`crates/terminal`；
- `crates/storage` schema；
- Local Sherpa 模型、Hotword 和 Azure profile keys。

粗略规模为 20～28 个文件、约 1,500～2,500 行新增/调整，主要集中在协议、测试和
Settings；这是设计估算，不是提交目标。实施必须分阶段，不能一次性大改。

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
- Local/Azure 不经过豆包 worker、codec 或 WSS。
- 豆包网络失败只产生当前 session 的 `Failed`，不改变持久化 Provider。
- Continuous 与 Single 仍共享公共 Pipeline，但各自保持原输入结束策略。

## 11. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 新旧控制台鉴权不一致 | 无法握手或错误授权 | typed auth mode；真实 Validate；不猜 headers |
| 2.0 resource ID/免费额度未绑定 | 401/授权失败或产生费用 | resource ID 可见；激活前真实验证；不宣传免费 |
| WSS 二进制/Gzip/sequence 实现错误 | 无结果、重复或协议失败 | byte-level golden tests + fake WSS + strict parser |
| socket 同时读写造成死锁 | 录音卡住、无法取消 | 单一 socket owner；短 read poll；所有等待有 deadline |
| 网络握手期间丢首音 | 命令缺开头 | `Preparing → Listening`；先 ready 再采集 |
| 慢语速被 `definite` 提前结束 | Agent 收到半句 | `definite` 只映射 partial；Single 仅松键结束 |
| partial 重复/回滚 | 字幕跳动 | `result_type=full`；按 sequence 丢弃旧包；去重累计文本 |
| final 重复 | Agent 重复执行 | Pipeline 单终态不变量 + session generation gate |
| 内外队列积压 | 高尾延迟或内存增长 | 双层有界预算；满时显式 Failed，不丢包 |
| 断线重发导致重复计费 | 成本与重复执行 | 首帧音频后不自动重连/重发 |
| secret 或转写泄漏 | 安全/隐私事故 | Keychain、redacted secret、日志 allowlist、仓库/日志扫描 |
| 企业代理/SOCKS 不兼容 | 部分网络无法连接 | L3 覆盖系统代理；首版明确支持范围；不能静默直连绕过策略 |
| UI 第三来源变拥挤 | 设置难理解 | 单来源 detail、可换行 selector、渐进披露旧鉴权 |
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
3. fake WSS 覆盖新旧鉴权、200 ms 聚合、partial 去重、last、final、error、断线、
   timeout、cancel、ping/pong 和 backpressure。
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
- Continuous e2e：本地 VAD 分段、partial、final、echo gate、下一 session 预建。
- 真实 App Computer Use：Light/Dark、键盘焦点、按钮 hover/pressed/disabled、
  Ready/Active/错误状态、长文案和窄窗口。
- ignored L3：真实 2.0 resource、普通话、慢语速 0.8～3 秒停顿、中英混说、
  技术词、网络中断，并记录脱敏的阶段耗时和 `X-Tt-Logid`。

所有新增 Rust 测试必须遵循仓库要求的场景、数据构造、执行过程、预期结果注释，
数值写出推导，每个断言带失败说明，关键正负状态逐一断言。

## 14. 分阶段实施建议

只有本文确认后才进入以下阶段：

1. **协议与 Adapter（不可激活）**

   实现 typed config、codec、fake WSS、worker 和 Recognizer 合约；不改 App 选择。

2. **注册表与真实 L3**

   注册 `CloudVolcengine`，用目标账号验证鉴权、2.0 resource、零长度 last frame、
   partial/final 和取消；仍不暴露普通用户入口。

3. **Settings 与 Keychain**

   增加 Draft/Validate/Ready/Test，完成真实 App UI 验收；验证不自动激活。

4. **Single Voice Command**

   加入通用 Preparing，接入现有 Final → Agent 路径，冻结 Local/Azure 回归。

5. **Continuous Listening**

   调整 provider-neutral prepared pipeline 顺序，验证 VAD、echo gate 和 session reset。

6. **显式激活与灰度**

   开放 Use Doubao；默认仍是 Local；采集错误率、阶段延迟和成本。

7. **上线评估**

   用固定语料比较 Local/Azure/Doubao，再决定二遍识别、首字加速或默认策略。

每阶段都应可独立回退；不得在接入阶段删除 `voice_sherpa` 兼容层或重写 Voice Settings。

## 15. 确认后仍需由目标账号提供的实施闸门

开始真实 L3 前需要从登录后的火山控制台确认：

- 使用新版 `X-Api-Key` 还是旧版 App ID + Access Token；
- 实际授权的 resource ID 是否为 `volc.seedasr.sauc.duration`；
- 20 小时试用是否抵扣该 resource，以及有效期；
- 试用/正式并发限制；
- 目标网络是否需要 HTTP/SOCKS 代理；
- 是否批准首版固定 60 秒 Norma 安全上限；
- 是否在首版只用单遍 `bigmodel_async`，二遍识别留待 A/B。

这些信息只影响豆包 Adapter 的 profile 和上线闸门，不改变本文确定的公共 Seam 与数据流。
