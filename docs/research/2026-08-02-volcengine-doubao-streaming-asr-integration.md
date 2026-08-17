# 火山引擎豆包流式 ASR：官方协议与 Norma OS 接入研究

> 调研日期：2026-08-02；方案确认日期：2026-08-08。范围仅限火山引擎官方产品页、官方文档和官方控制台入口；未采用社区文章、第三方 SDK 或厂商间不可比的排行榜。文中的“提升、降低、最优、300～400 ms”等结论均为厂商声明，除非明确标为 Norma OS 的接入建议；本报告没有完成真实账号调用、网络抓包或同语料准确率测试。

## 结论

豆包流式 ASR 可以直接接入 Norma OS，且现有音频契约与其最保守的官方 API 契约高度吻合：**16 kHz、mono、PCM16**。确认采用 2.0 小时版候选资源、官方推荐的**双向流式优化版** `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async`，将现有约 20 ms PCM chunk 聚合为 200 ms 音频帧；Single 由用户松键、Continuous 由本地 Silero VAD 结束输入，客户端随后发送显式最后一包，只有 last frame 之后的明确服务端终态响应才能向 Agent 提交唯一 final。

最需要避免的三个误区：

1. `definite=true` 是句级结果稳定标志，不等于整次按键录音已经结束；整次输入仍应由客户端的松键/取消动作控制。
2. “流式输入” `bigmodel_nostream` 虽然通过 WebSocket 喂音频，但官方说明它主要在音频超过 15 秒或收到最后一包后返回结果，不适合拿来做逐字预览。
3. 火山官方页面存在版本与销售口径差异，包括新旧鉴权请求头、输入格式/采样率和免费额度；Norma OS 已固定只接新版控制台，但真实 L3 仍必须核对目标项目的请求头、`resource_id` 和计费 SKU。

## 0. 已确认的 Norma OS 范围与决策

2026-08-08 的方案访谈已经确认以下产品与工程边界：

- 本方案只接入豆包流式 ASR，不接入豆包端到端实时语音模型；Norma 现有 Agent、工具调用、Dialogue 和 TTS 保持不变。
- 第一版同时支持 Single Voice Command 与 Continuous Listening；Continuous 继续由本地 Silero VAD 决定 utterance 边界，服务端 `definite=true` 只产生 partial。
- Continuous 在进入 Listening 前预建一条 WSS session；每段结束后预建下一条。预建失效时最多缓存 2 秒 PCM（`16,000 × 2 bytes × 2 s = 64,000 bytes`），仍未就绪则整段明确失败，不提交残缺文本。
- 只支持新版控制台 `X-Api-Key`；API Key 本体保存于 macOS Keychain，SQLite 只保存 `credential_ref`、非敏感配置和 Ready 状态。
- 首版固定单遍 `bigmodel_async`、`result_type=full`、标点与 ITN 开启、DDC 与首字加速关闭；二遍识别留待同语料 A/B。
- 每个 utterance 最长 60 秒；WSS 建连使用 5 秒绝对 deadline，last frame 后等待 final 最多 10 秒。
- 不自动回退 Local/Azure，不重放已发送音频。Continuous 的临时网络失败只为下一段最多重建 3 次（500 ms、1 s、2 s）；鉴权、资源、参数或协议错误不重试并撤销 Ready。
- 每个进程最多一条豆包 WSS。第一版支持直连、`HTTPS_PROXY`、`ALL_PROXY` 与 `NO_PROXY`，不承诺 PAC 或交互式企业代理。
- 日志不得记录 API Key、原始音频、partial/final 正文或完整服务端响应；只记录脱敏标识、音频时长/字节数、阶段耗时、错误分类和 `X-Tt-Logid`。

目标账号的准确 `resource_id`、last frame 的零长度兼容性、终态判据、空闲连接计费/超时及稳定错误码仍属于真实 L3 实施闸门，不在调研中猜测。

## 1. 产品模式与适用场景

火山的[语音识别大模型产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)把流式能力分为两类；[大模型流式语音识别 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)又给出双向流式的普通版和优化版，因此实际有三条 WSS 路径：

| 模式 | WSS 地址 | 官方返回语义 | Norma OS 判断 |
|---|---|---|---|
| 双向流式 | `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel` | 每输入一包通常返回一包，尽快返回已识别字符 | 能做实时预览，但官方已推荐优化版 |
| 双向流式优化版 | `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async` | 仅当结果变化时返回新包；厂商称 RTF、首字和尾字时延有提升 | **首选 POC**；支持二遍识别 |
| 流式输入 | `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_nostream` | 输入音频超过 15 秒或收到最后一包后返回识别结果；厂商称准确率更高 | 适合句级/松键后结果，不适合逐字预览 |

官方 API 页建议音频单包和发包间隔都保持在 100～200 ms；对双向流式，页面明确称 200 ms 分包性能最优。页面还宣称：平均音频时长为 5 秒时，流式输入模式可在 300～400 ms 内返回，但没有公开计时起点、分位数、网络环境和样本规模，因此不能把它当作 Norma OS 的延迟 SLA。[产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)列出的正式版默认并发为 10，且注明以控制台为准、可购买扩容。

## 2. 语言、方言和中英混合

### 官方明确声明

- [产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)写明双向流式支持中英文；双向流式二遍和流式输入覆盖中英文及方言。
- 同一页面列出的方言包括：粤语、四川、陕西、冀鲁、兰银、江淮等可输出方言文本；上海话、闽南语、山西话、客家话等输出普通话文本；也覆盖东北话、北京话等带口音普通话。
- [产品动态](https://www.volcengine.com/docs/6561/162929?lang=zh)说明默认中文模型支持中英及国内主流方言，并再次注明普通双向流式仍只支持中英文；流式输入另支持多种外语模型。
- 火山首页当前将豆包语音识别模型 2.0 描述为“混合语言精准识别”，并列出流式语音识别价格；这是产品定位，不是公开可复现的中英 code-switch 基准。[火山引擎首页](https://www.volcengine.com/)

### API 参数边界

[大模型流式语音识别 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)的 `audio.language` 参数只适用于 `bigmodel_nostream`，双向流式不通过该字段切换语言。`language` 为空时，文档称默认模型能识别中英文及一组国内方言；指定 `zh-CN`、`en-US` 等值时则选择对应语言。`enable_auto_lang` 也只在 `nostream` 支持，不能推导为普通双向流式的自动多语种能力。

### 不能从官方声明推出的结论

- 没有公开、可复现、与 Azure/腾讯/阿里使用同一语料的中文 CER 或中英混说 WER，不能据此断言豆包“市面最准确”。
- 厂商写有“相比传统模型错误率降低 30%”“垂直领域降低 50% 以上”“口音错误率降低 60%”等数字，但[产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)没有公开基线模型、测试集、录音设备、噪声条件和置信区间，只能标注为厂商内部口径。
- 官方列出支持某方言，不代表每个地域口音、慢语速、病句或夹英文术语都稳定；必须用 Norma OS 真实录音测试。

## 3. SDK、API 与平台覆盖

- [通用流式语音识别 SDK 概览](https://www.volcengine.com/docs/6561/125686?lang=zh)公开列出 Android、iOS、Linux C++、Linux Java；没有列出 Rust 或 macOS 原生 SDK。
- [大模型流式识别 SDK](https://www.volcengine.com/docs/6561/1395846?lang=zh)重点给出 Android/iOS 组件，并要求配置 WSS host、URI、App ID、Access Token、Resource ID 和 `Seed` 协议类型。
- 对 Norma OS，最低耦合的实现是直接使用 WSS 二进制协议；无需把移动端 SDK 或 C++ 桥接带入 macOS 应用。
- 官方另有[录音文件极速版 HTTP API](https://www.volcengine.com/docs/6561/1631584?lang=zh)，但那是文件/整段识别，不是本报告讨论的实时流式通道。

### 是否支持 HTTP/2 流式

截至调研日期，[大模型流式 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)、[产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)和[大模型 SDK 文档](https://www.volcengine.com/docs/6561/1395846?lang=zh)只公开 **WebSocket/WSS** 流式接入，没有公开独立的 HTTP/2 或 gRPC 流式 ASR endpoint。因此可以得出的结论是“**官方未文档化 HTTP/2 流式接口**”，不能进一步断言服务端底层永远不会协商 HTTP/2。

## 4. 开通、鉴权与 `resource_id`

[旧版控制台快速入门](https://www.volcengine.com/docs/6561/163043?lang=zh)要求注册、实名认证、在豆包语音控制台开通服务；新版控制台同样需要登录后在目标项目开通模型，具体授权信息来自目标项目/应用。直接 WSS 的鉴权放在 WebSocket 升级 HTTP 请求头，不放在首个 JSON payload 中。

### 旧版控制台凭证

以下内容仅记录官方文档差异，不能作为 Norma OS 第一版的可选配置或兼容路径。

| Header | 来源与含义 |
|---|---|
| `X-Api-App-Key` | 控制台 APP ID / App Key，非秘密标识 |
| `X-Api-Access-Key` | 控制台 Access Token，应按秘密管理 |
| `X-Api-Resource-Id` | 已开通服务对应的资源 ID |
| `X-Api-Request-Id` | 官方表格建议随机 UUID |
| `X-Api-Sequence` | 官方表格给出的固定值为 `-1` |

### 新版控制台凭证

当前[大模型流式 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)写明新版控制台“只需要 `X-Api-Key`”，以单个 APP Key 替代旧版 `App ID + Access Token`；仍需 `X-Api-Resource-Id` 以及请求追踪字段。Norma OS 第一版只实现这一套鉴权，不能同时猜测、拼装或回退到旧版请求头。

### Resource ID

| 模型/计费形态 | `X-Api-Resource-Id` |
|---|---|
| 豆包流式 ASR 1.0 小时版 | `volc.bigasr.sauc.duration` |
| 豆包流式 ASR 1.0 并发版 | `volc.bigasr.sauc.concurrent` |
| 豆包流式 ASR 2.0 小时版 | `volc.seedasr.sauc.duration` |
| 豆包流式 ASR 2.0 并发版 | 本轮官方资料未形成足够稳定的交叉确认，必须从目标控制台复制，不能按命名规律推断 |

前三项来自[大模型流式 API 的鉴权表](https://www.volcengine.com/docs/6561/1354869?lang=zh)及版本资料。官方[QPS/并发查询接口](https://www.volcengine.com/docs/6561/1476626?lang=zh)也能证明 1.0 小时版和并发版是不同资源授权；2.0 是否已在目标账号开放、并发版的实际 ID 都必须由控制台确认。

[RTC 的 ASR 配置文档](https://www.volcengine.com/docs/6348/1807452?lang=zh)进一步明确区分 `volc.bigasr.sauc.duration`（1.0）和 `volc.seedasr.sauc.duration`（2.0）。新接入优先用 2.0，不应把 1.0 的资源授权、价格或测试结论自动套到 2.0。

### 官方页面内部不一致

同一个 API 页的鉴权表列出 `X-Api-Request-Id`，但页面中的请求示例使用 `X-Api-Connect-Id`；新版控制台表格写 `X-Api-Key`，紧随其后的示例却仍显示旧版 `X-Api-App-Key` 与 `X-Api-Access-Key`。这不是客户端可以安全猜测的细节，POC 时应下载控制台当前示例、实际握手，并记录服务端返回的 `X-Api-Connect-Id` 与 `X-Tt-Logid` 供工单定位。

## 5. WebSocket 二进制协议

### 5.1 四字节基础头

所有整数使用大端序。基础 header 为 4 字节，每个字段按高/低 4 bit 编排：[官方协议详情](https://www.volcengine.com/docs/6561/1354869?lang=zh)。

| Byte | 高位 | 低位 |
|---|---|---|
| 0 | Protocol version | Header size，实际字节数为值 × 4 |
| 1 | Message type | Message-type-specific flags |
| 2 | Serialization | Compression |
| 3 | Reserved | Reserved |

当前版本为 `0b0001`，常规 header size 值为 `0b0001`，即 4 字节。Header size 大于 1 时，后续存在 header extension，解析器不能硬编码 payload 永远从 byte 4 开始。

### 5.2 消息类型

| 值 | 含义 |
|---|---|
| `0b0001` / `0x1` | full client request：首包请求参数 |
| `0b0010` / `0x2` | audio-only request：音频数据 |
| `0b1001` / `0x9` | full server response：识别结果 |
| `0b1011` / `0xB` | server ACK：确认帧；不是识别文本 |
| `0b1111` / `0xF` | server error：无效消息、序列化方法不支持等错误 |

Message flags 的官方定义为：`0` 无 sequence；`1` 后跟正 sequence；`2` 无 sequence、但标记最后一包；`3` 后跟负 sequence、同时标记最后一包。若使用 sequence，它位于 header 后、payload size 前，为 4 字节有符号大端整数。

### 5.3 序列化与压缩

- Serialization `0` 表示无序列化，`1` 表示 JSON。Full client request 通常使用 JSON；audio-only payload 是原始音频字节。
- Compression `0` 表示不压缩，`1` 表示 Gzip。服务端按客户端声明的压缩方式返回；`payload size` 是**压缩后** payload 的长度。
- `payload size` 为 4 字节无符号大端整数。解析顺序必须先读 header size，再按 flags 读取可选 sequence，随后读取 payload size，最后按 compression 解压并按 serialization 解析。

### 5.4 正常交互顺序

1. 发起 WebSocket HTTP GET 升级，带上鉴权和追踪请求头。
2. 握手成功后发送一次 full client request：`header + payload_size + JSON payload`。
3. 连续发送 audio-only request：`header [+ sequence] + payload_size + audio payload`。官方建议每包 100～200 ms，双向流式优先 200 ms。
4. 服务端下发一个或多个 full server response；普通双向流式可能频繁覆盖累计文本，优化版只在结果变化时返回。
5. 用户松键后，不是直接关闭 socket，而是发送带 last flag 的最后一个 audio-only request；有 sequence 时最后一包使用负 sequence。
6. 收到服务端终态响应后再结束 session；错误帧则进入失败终态。

## 6. 音频请求参数与格式

首个 JSON 包的主要结构是 `user`、`audio`、`request`，当前协议页没有要求再次在 JSON 内发送 App ID 或 Token。[大模型流式 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)给出的最保守 wire contract 如下：

| 字段 | 官方约束 |
|---|---|
| `audio.format` | `pcm` / `wav` / `ogg` / `mp3`；PCM/WAV 内部必须为 `pcm_s16le` |
| `audio.codec` | `raw` / `opus`，默认 `raw`；OGG 必须配 OPUS；MP3 时该字段不生效 |
| `audio.rate` | 默认 16000，API 参数表写“目前只支持 16000” |
| `audio.bits` | 默认 16，API 参数表写“暂只支持 16 bits” |
| `audio.channel` | `1` mono 或 `2` stereo，默认 mono |
| `request.model_name` | 当前填写 `bigmodel` |

这里存在官方口径冲突：[产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)的能力矩阵称“采样率无要求”，并为流式输入列出更多封装格式，而直接 API 的参数表收紧到上表。工程实现应以具体 endpoint 的 API 参数表为准，POC 先使用 16 kHz mono PCM16；其他采样率或容器格式必须用目标资源授权实测，不能只依赖产品总览。

### 对输入法有价值的参数

- `enable_itn`：数字/货币等书面化，默认 true。
- `enable_punc`：标点，默认 true。
- `enable_ddc`：语义顺滑，默认 false；会删除或改写停顿词、语气词、重复词，不能与“逐字忠实转写”混为一谈。
- `show_utterances`：返回分句、分词与停顿信息。
- `result_type`：`full` 为累计全量结果，`single` 为不带此前分句的增量结果。初版建议固定 `full`，先避免增量分句替换、回滚和拼接错误；确认状态机后再评估 `single`。
- `corpus.boosting_table_id/name`：控制台热词表；`corpus.context` 可请求级直传热词或上下文。协议页给出的上限因模式不同：普通双向流式热词约 100 tokens，`nostream` 可到 5000 个词；上下文另有轮次/token 约束。
- `enable_accelerate_text`：首字加速；官方明确警告可能降低首字准确率。`accelerate_score` 为 0～20，数值越大出字越快，因此不能默认拉满。
- `enable_nonstream`：仅双向流式优化版支持二遍识别。它让流式文本先上屏，再按 VAD 分句用 `nostream` 重识别以提高最终准确率。

## 7. VAD、慢语速和结束语义

[大模型流式 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)公开了三个容易混淆的参数：

| 参数 | 官方含义 | 默认/边界 | 对慢语速的影响 |
|---|---|---|---|
| `vad_segment_duration` | 语义切句的最大静音阈值；超过后切成两句，但不决定判停，不改变 `definite` 的位置 | 默认 3000 ms | 更适合保留慢语速长停顿；配置 `end_window_size` 后失效 |
| `end_window_size` | 强制判停；静音超过阈值直接输出 `definite` | 默认 800 ms，最小 200 ms | 800 ms 对慢用户可能过短，应专项测试 1.5～3 秒，但公开页未给最大值 |
| `force_to_speech_time` | 音频超过该时长后才允许判停，需配合 `end_window_size` | 最小 1 ms，官方推荐 1000 ms，并提示可能影响准确率 | 可避免刚开口即被短静音切断，但不是长停顿的完整解决方案 |

`utterances[].definite=true` 表示一个分句已确定。开启二遍识别时，官方说明 `nostream` 二遍结果会输出该标志；它仍然只是句级稳定状态，不是整个 WebSocket 或本次按键输入的完成事件。

对 Norma OS 最稳妥的语义映射是：

- 服务端非 `definite` 文本 → `RecognitionEvent::Partial`；
- 松键并成功发送最后音频帧 → `RecognitionEvent::InputEnded(User)`；
- 服务端最后响应中的最终完整文本 → `RecognitionEvent::Final`；
- error frame、握手失败、协议解析失败或在终态前断链 → `RecognitionEvent::Failed`；
- 用户取消 → 主动中止并发出 `RecognitionEvent::Cancelled`，不得把已有 partial 当作 final 提交 Agent。

这是基于 Norma OS 现有事件模型的接入建议，不是火山协议字段名称。现有契约见 [`recognition.rs`](../../crates/asr/src/recognition.rs)；provider 归属与不得静默回退原则见 [ADR-0026](../adr/0026-own-built-in-asr-providers-and-azure-speech-in-asr.md)。

## 8. 响应、错误与可观测性

Full server response 的 payload 是识别结果 JSON，核心字段包括完整 `result.text`，以及在开启 `show_utterances` 时返回的 `utterances[]`：`text`、`start_time`、`end_time`、`definite` 和可选 words。[协议详情](https://www.volcengine.com/docs/6561/1354869?lang=zh)

Message type `0xF` 是协议级 error response。客户端必须先按 header 的 compression 解压，再读取错误码/错误消息；不能尝试把错误 payload 当识别 JSON。官方页面没有给出“所有错误都可重试”的保证，因此建议分层处理：

- 鉴权、资源未授权、参数/格式错误：配置失败，不自动重试；
- 并发/限流或服务忙：协议层允许考虑有限退避，但 Norma OS 已确认当前 utterance 直接失败，只为 Continuous 的下一段按 500 ms、1 s、2 s 最多重建 3 次；
- 网络断开：若尚未收到终态结果，本次命令标为失败；除非协议未来明确支持续传，否则不要从中间音频帧盲目续接。

握手响应会给出 `X-Tt-Logid`，官方强烈建议记录用于问题定位。建议日志只记录 request/connect UUID、resource ID、模式、音频字节数/时长、partial/final 到达时间与字符数、错误码和 `X-Tt-Logid`；不要打印任何鉴权 secret、原始音频或完整转写正文。

## 9. 别把两套“Realtime API”混在一起

本报告主体是豆包语音的直接 WSS 二进制协议。火山的边缘大模型网关还提供一套[基于事件的 Realtime API](https://www.volcengine.com/docs/6893/1527759?lang=zh)，endpoint 是 `wss://ai-gateway.vei.volces.com/v1/realtime`，鉴权是网关 `Authorization: Bearer ...`；其客户端事件包括 `transcription_session.update`、`input_audio_buffer.append`、`input_audio_buffer.commit`，服务端事件包括实时 result 和 completed。

该网关文档明确标注仍处于测试阶段，而且它的 API Key、事件名和模型绑定方式与直接语音 WSS 不同。Norma OS 第一版 provider 不应把两套协议混装；如果未来希望兼容 OpenAI Realtime 风格，可将其作为独立 adapter 评估。

## 10. 免费额度与公开价格

### 10.1 免费额度政策结论表

| 问题 | 截至 2026-08-02 可确认的结论 | 证据边界 |
|---|---|---|
| 20 小时是每月赠送吗？ | **不是公开承诺的月度免费层，应按一次性试用总量做预算。** [豆包产品页](https://www.volcengine.com/product/doubao)对 ASR 只写“20 小时”，同一免费额度表对确实按月重置的联网资源明确写“2 万次/月”；[实时对话式 AI 计费文档](https://www.volcengine.com/docs/6440/1392584?lang=zh)也写“每个语音技术应用 20 小时”，没有“每月”或“每月重置”。 | 官方没有直接写一句“仅一次”，因此更严格的表述是：**没有任何公开依据可以把它当成每月 20 小时**。RTC 文档的“每应用”口径不能自动外推为直接 WSS 新建多个应用都可重复领取。 |
| 是否有有效期？ | **直接流式 ASR 的 20 小时有效期，公开资料未说明，需在目标控制台确认。** | 不要套用[音视频字幕生成 FAQ](https://www.volcengine.com/docs/6561/114291?lang=zh)的“20 小时、半年”：那是另一个服务。也不要套用 30 小时促销资源包的 12 个月有效期。 |
| 是否需要领取？ | **不是注册账号即自动可调用。** 旧版控制台流程要求先注册、实名认证、创建应用并在服务详情中点击试用/启用对应服务；首次启用后为试用版。[旧版控制台快速入门](https://www.volcengine.com/docs/6561/163043?lang=zh) | 公开文档没有说明还需另领优惠券；新版控制台的具体按钮、到账时间和项目归属只有登录后可见。本轮访问官方控制台公开入口时被重定向到登录页，无法读取账号级额度。 |
| 2.0 是否享受 20 小时？ | **公开资料不能确认 20 小时一定抵扣 2.0 的 `volc.seedasr.sauc.duration`。上线预算应先按“不保证”处理。** | 产品页免费表只写泛化的“Doubao-流式语音识别”，没有写 1.0/2.0 或 `resource_id`；RTC 免费试用页也只写“大模型流式语音识别”。官方[新春活动页](https://www.volcengine.com/activity/newyear)明确售卖“豆包流式语音识别模型 2.0”30 小时资源包，但这是付费促销，不能证明 2.0 享有 20 小时免费试用。 |
| 20 小时用完后会自动收费吗？ | **试用版额度耗尽本身不会自动把服务变成正式后付费；必须由用户点击“开通”并确认计费。** 一旦转为正式版，旧版控制台文档明确写明试用赠送用量清零、立即开始后付费，账单扣减账户余额。 | 如果目标项目此前已经开通正式版，则用量当然可能直接进入后付费；不能仅凭“还有免费额度”判断不会产生账单。上线前必须检查服务状态和费用预警。 |

### 10.2 领取主体与其他限制

- **账号与实名认证**：[旧版控制台快速入门](https://www.volcengine.com/docs/6561/163043?lang=zh)要求注册、登录并完成实名认证后才能开通服务。免费额度表本身没有给出“仅个人”“仅企业”或“仅新客”的公开限定，因此不能自行补充这些条件。
- **应用/项目范围**：RTC 组合产品的官方口径是“每个语音技术应用 20 小时”；直接 ASR 产品页没有公开说明额度按账号、项目、应用还是 `resource_id` 发放。旧版控制台一个火山账户最多创建 10 个应用，但这不等于可以合规获得 10 × 20 小时；是否重复发放需目标控制台确认。新版控制台以项目管理资源，不能把旧版“每应用”规则无条件搬过去。
- **转正式版会清零**：旧版控制台明确写明从试用版转成正式版时，未使用的试用赠送用量清零。若要充分利用试用额度，不应在验证前误点正式开通；若要生产使用，则不应依赖试用版的限额和服务保障。
- **并发/QPS**：[产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)只明确正式版默认并发为 10、以控制台为准并支持购买扩容；没有公开给出 20 小时试用版的并发/QPS。因此 20 小时只是用量额度，不能理解成并发保障。
- **用途与反滥用**：旧版控制台将试用版定位为“简单的测试使用”；[语音模型体验服务规则](https://www.volcengine.com/docs/6561/1866421?lang=zh)还明确禁止恶意抢占活动资源、批量刷单等非常规行为，并保留取消资格、回收资源的权利。因此不能通过批量创建应用或账号来推导、叠加免费时长。
- **地域**：公开免费额度页面没有列出专门的地域限制；但[智能语音 SLA](https://www.volcengine.com/docs/6561/107349?lang=zh)的服务范围是中国大陆（不含港澳台），且把非中国大陆 IP 请求导致的问题列为免责情形。这是服务保障边界，不等于官方已经声明境外不能调用。
- **促销资源包不是免费额度**：[ASR 产品页](https://www.volcengine.com/product/asr)的 30 小时 ¥66 新客包，以及[新春活动页](https://www.volcengine.com/activity/newyear)的 2.0 版 30 小时、12 个月有效期、每用户限购 5 个，均是付费活动商品；其“新客、限购、有效期”不可反向套到 20 小时免费试用。
- **资源包与免费试用不是同一抵扣物**：旧版控制台文档说明正式版时长资源包可在同服务的不同 APPID 间共享、且优先抵扣；公开资料没有说明免费试用额度也能跨应用或跨项目共享。
- **欠费与关停**：服务转正式版后会按后付费扣余额，欠费可能关停并最终回收服务；免费额度不能替代余额、账单预警和成本上限控制。

### 10.3 当前公开价格口径

截至本次补充核查，[豆包产品页](https://www.volcengine.com/product/doubao)和[实时对话式 AI 计费示例](https://www.volcengine.com/docs/6440/1392584?lang=zh)当前都以 **¥4.5/小时**展示“大模型流式语音识别”。这修正了本报告初稿采用的 ¥1/小时旧页面口径。它仍未把价格直接映射到 2.0 `volc.seedasr.sauc.duration`，因此正式上线必须以目标控制台的服务版本、`resource_id`、后付费价和账单为最终依据。

## 11. Norma OS 接入建议

以下是结合当前仓库边界作出的工程建议，不是火山官方承诺：

1. 在 `asr` 域内新增独立 provider，不让 `voice_input` 感知火山协议；保持 ADR-0026 的依赖方向。具体落点建议为 [`providers/cloud/volcengine/`](../../crates/asr/src/providers/cloud/)；在 [`providers/mod.rs`](../../crates/asr/src/providers/mod.rs)扩展 typed provider ID、descriptor、`BuiltinAsrProfile` 和 factory 分发，不把火山分支写进 App 业务流程。
2. [`crates/asr/Cargo.toml`](../../crates/asr/Cargo.toml)复用 workspace 已钉住的 `tungstenite`，并显式声明 Gzip codec 依赖，不依赖传递依赖。Profile 只保存 `resource_id` 和 credential reference；首版只接受新版 `X-Api-Key`，密钥本体只写 macOS Keychain。App 侧沿用 [`settings/voice`](../../crates/app/src/settings/voice/) 的 Draft → Validate → Ready → Activate，不能让明文 secret 进入 SQLite、Debug 或日志。
3. **不要等第一块 PCM 才建连。** Single 进入 `Preparing` 时在后台完成 DNS/TLS/WSS、鉴权和 full client request，ready 后才启动麦克风并显示 Listening。Continuous 在启动采集前预建首条 session，每段结束后预建下一条；若 `SpeechStart` 时预建连接失效，最多缓存 2 秒 PCM，超时则整段失败，不能依赖现有会静默丢包的采集队列。
4. 继续接收现有 16 kHz mono PCM16 的约 20 ms chunk，由 recognizer 聚合成 200 ms（6,400 bytes）；输入结束时立即刷新不足 200 ms 的尾包。若专项压测证明 100 ms 更优，再单独调整。
5. **不新增第二层 socket worker。** 现有 `RecognitionPipeline` 已经为 Recognizer 创建专用后台线程并串行调用 `accept_pcm/finish`，该线程直接作为 WSS 的单一 owner。`VolcengineRecognizer` 持有 socket：`accept_pcm` 聚合/发送音频并以非阻塞方式排出可用 partial，`finish` 发送 last/负包后在同一后台线程中有界等待唯一 final。这样避免内部音频/事件队列、额外线程和双层背压；取消或 Drop 必须关闭 socket 并有界收口。
6. 音频背压与字幕背压分开处理：音频无法按序接收时整段 `Failed`，不得丢包；累计 full partial 可以 latest-wins 合并，不能因 UI 来不及显示字幕而让整段失败；final/error 永远不得被覆盖。
7. 初版固定单遍 `result_type=full`，关闭 `enable_accelerate_text`、`enable_ddc` 与 `enable_nonstream`，开启标点、ITN、`show_utterances`；先建立基线，再在同语料 A/B 中评估二遍识别。
8. Single 只由用户松键结束，Continuous 只由本地 Silero VAD 结束；服务端 `definite` 只做分句/partial。分别测试 0.8、1.5、2、3 秒静音，不得因 `definite` 过早触发 Agent。
9. 每个 utterance 最长 60 秒；建连使用 5 秒绝对 deadline，last 后 final deadline 为 10 秒。Single 建连失败不自动重试；Continuous 临时网络失败只为下一段最多重建 3 次（500 ms、1 s、2 s），不重放已发送音频；鉴权、资源、参数或协议错误不重试并撤销 Ready。
10. Transport 支持直连、`HTTPS_PROXY` 的 HTTP CONNECT、`ALL_PROXY` 的 SOCKS5 与 `NO_PROXY`；第一版不承诺 PAC 或交互式企业代理。每个进程最多保留一条豆包 WSS，空闲预建连接不发送音频。
11. 指标至少记录：进入 `Preparing`、WSS ready、进入 `Listening`、首音频帧、首 partial、最后音频帧、last 已发送、首 `definite`、final、Agent 首事件；并带脱敏 session ID 与 `X-Tt-Logid`。日志只记录字符数，不记录 partial/final 正文。

## 12. 测试证据与分阶段 rollout

以下是建议的实现验收顺序；本报告没有执行这些测试：

1. **协议 codec golden tests**：固定字节逐一验证 version/header size、四种 message type、flags、正负 sequence、大端 payload size、Gzip/JSON、last frame、full response 和 error frame；解析器还要拒绝截断、超长、未知压缩方式与不一致长度。测试必须遵守仓库对场景、数据推导、执行过程和正负断言的完整注释要求。
2. **本地 mock WSS 集成测试**：验证新版 `X-Api-Key`、full client request 仅发一次、20 ms→200 ms 聚合、partial latest-wins、`definite` 不触发整次提交、Single 松键/Continuous 本地 VAD 发送 last、终态去重、取消、终态超时、服务端 error、连接中断、2 秒预备缓冲、3 次重建预算、音频背压失败和字幕背压合并。
3. **ignored 真实 L3**：由环境变量或未入库的本地 secret 提供新版控制台凭据，并从目标控制台读取实际 2.0 小时版 `resource_id`；发送固定 16 kHz mono PCM16 语料，验证零余量 last frame、唯一 final 判据、稳定文本片段和 `X-Tt-Logid`，同时打印脱敏的建连/首 partial/尾字耗时。默认测试和 CI 不访问真实服务。
4. **Settings 与 E2E**：覆盖 Draft → Validate → Ready → Activate、Keychain 写入/替换/清除、重启恢复、错误凭据、未授权 resource ID、切换 provider，以及火山失败时不得静默回退 Local。若后续修改可见 Settings 页面，按仓库规则补真实 App 的 Computer Use 验收。
5. **固定语料横评**：用同一批普通话、0.8～3 秒停顿、中英混说、路径/命令/产品名和噪声录音，记录 CER、技术词命中率、误判停率、首 partial、停语到 final、连接成功率与 p95/p99；先比较单遍 `bigmodel_async` 与 Azure/Local 基线，二遍识别留到基础链路稳定后的独立 A/B。
6. **量化发布门槛**：固定网络下连接成功率至少 99%；WSS 准备 p95 ≤ 2 秒且绝不超过 5 秒；首 partial p95 ≤ 800 ms；输入结束到 final p95 ≤ 1.5 秒、p99 ≤ 3 秒；普通话 CER 与技术词/路径/命令/中英混说命中率不得差于 Local/Azure 中更好的一个；丢首音、重复 final、重复 Agent 回合、partial 冒充 final均为 0。
7. **上线阶段**：codec/mock（不可注册）→ Adapter + 真实 L3（不可注册）→ 注册表（不可激活）→ Settings/Keychain/Validate/Test → Single → Continuous prepared-session → 实验性显式 opt-in → 达标后正式可选。默认始终是 Local，不设置静默云端回退。

## 13. 上线前必须复核的未知项

- [ ] 从目标新版控制台下载当前官方 Demo，确认 `X-Api-Key` 之外仍需哪些追踪请求头，以及 `X-Api-Request-Id` 与示例中的 `X-Api-Connect-Id` 如何使用。
- [ ] 目标项目实际授权的 2.0 小时版 `resource_id`；必须从控制台复制，不能把候选值 `volc.seedasr.sauc.duration` 当作已验证事实。
- [ ] last frame 后哪个服务端字段/sequence/flags 是整次请求的唯一终态判据；`definite=true` 不足以回答此问题。
- [ ] 尾部恰好无剩余 PCM 时是否接受零长度 audio-only last frame；必须以官方 Demo 和真实 L3 为准。
- [ ] 2.0 在 `bigmodel_async` 上支持的具体语言、方言和单遍能力是否与公开总览完全一致。
- [ ] 目标控制台中 20 小时试用的实际归属范围（账号/项目/应用/`resource_id`）、起止时间、剩余量，以及是否明确覆盖 2.0 `volc.seedasr.sauc.duration`；公开资料未说明。
- [ ] 新版控制台启用试用的实际按钮和到账流程、试用并发/QPS，以及旧版“转正式版清零”规则是否原样适用；公开入口登录前不可见。
- [ ] ¥4.5/小时、2.0 新客 30 小时活动包与控制台最终账单的税费、活动、资源包抵扣和版本关系。
- [ ] `end_window_size`、过长静音对连接/计费/最终返回的影响，以及 2～3 秒停顿是否只分句、不结束整次请求；Norma 仍只接受本地 VAD 作为 Continuous 边界。
- [ ] 单连接最长持续时间、空闲超时、最大累计音频大小、服务端 ping/pong 要求，以及只预建但不发送音频是否计费/占用何种并发。
- [ ] error code 的完整稳定枚举、哪些可重试、重试后的计费与幂等语义；以控制台当前 API 附件和真实错误响应为准。
- [ ] 中国大陆不同网络、直连、`HTTPS_PROXY`、`ALL_PROXY`、`NO_PROXY` 与企业出口下的 DNS/TLS/WSS 首连、p50/p95/p99 首字/尾字延迟。
- [ ] 用 Norma OS 同一批慢语速、中英混说、路径/命令/产品名录音，与 Azure、本地 Sherpa 做盲测；没有此证据前不宣称更准确。

## 官方来源索引

- [语音识别大模型产品说明](https://www.volcengine.com/docs/6561/1354871?lang=zh)
- [大模型流式语音识别 API](https://www.volcengine.com/docs/6561/1354869?lang=zh)
- [语音产品动态](https://www.volcengine.com/docs/6561/162929?lang=zh)
- [大模型流式识别 SDK](https://www.volcengine.com/docs/6561/1395846?lang=zh)
- [通用流式语音识别 SDK 概览](https://www.volcengine.com/docs/6561/125686?lang=zh)
- [豆包语音旧版控制台快速入门](https://www.volcengine.com/docs/6561/163043?lang=zh)
- [QPS/并发查询接口说明](https://www.volcengine.com/docs/6561/1476626?lang=zh)
- [RTC 配置 ASR：1.0/2.0 Resource ID](https://www.volcengine.com/docs/6348/1807452?lang=zh)
- [Realtime API 调用 Doubao 语音识别](https://www.volcengine.com/docs/6893/1527759?lang=zh)
- [实时对话式 AI 计费与免费试用规则](https://www.volcengine.com/docs/6440/1392584?lang=zh)
- [豆包产品页：当前公开价格与 20 小时免费额度](https://www.volcengine.com/product/doubao)
- [豆包 ASR 产品页：30 小时新客活动包](https://www.volcengine.com/product/asr)
- [火山引擎新春活动：2.0 版 30 小时付费资源包](https://www.volcengine.com/activity/newyear)
- [语音模型体验服务规则：免费体验与反滥用限制](https://www.volcengine.com/docs/6561/1866421?lang=zh)
- [智能语音 SLA：大陆服务范围与境外 IP 免责](https://www.volcengine.com/docs/6561/107349?lang=zh)
- [音视频字幕生成 FAQ：另一个 20 小时/半年服务，不能套用](https://www.volcengine.com/docs/6561/114291?lang=zh)
- [豆包语音产品页](https://www.volcengine.com/products/Audio-editing-and-sound-processing)
