# Norma OS 云 ASR 候选对比

> 调研日期：2026-08-02。目标场景是 macOS 上的中文语音输入：普通话为主、夹少量英文和产品/技术词、用户可能语速慢且停顿长；当前是“按住说话、松开提交”的单次命令，后续希望升级为边说边返回的流式链路。事实只引用厂商第一方资料；价格是公开标价快照，不含税、地域差价、促销和附加功能费。

## 结论

没有可信的跨厂商官方基准能证明哪家在 Norma OS 的真实音频上“最准确”。厂商公布的 98%、错误率下降 30% 等数字使用的语料、环境、基线和统计口径不同，不能横向排序。正确做法是用同一批 Norma OS 真实录音做盲测。

建议分两层选型：

1. **中国大陆主链路 POC：火山引擎豆包流式 ASR、腾讯云 ASR 大模型 2.0、阿里云 ISI**。三者都有大陆服务、WebSocket 流式接口和明确的中英混说能力；火山/腾讯的大模型公开单价较低，阿里三个月试用适合集中压测。
2. **基准与备选：Azure Speech、讯飞实时转写、AssemblyAI Universal-3.5 Pro、OpenAI Transcription**。Azure 是当前实现，先把“整段录完再上传”改成流式才能公平比较；讯飞用于中文/方言/中英混说基准；AssemblyAI 明确支持普通话与英语的句中切换，OpenAI 则提供文件和实时两类新模型，两者可用于检验海外模型的准确率上限，但不适合作为中国大陆默认生产链路。Deepgram 的中文模型可参与纯中文基准，但其原生多语言 code-switch 列表不含中文。

不建议一开始同时接十家 Provider。先实现统一的流式 Provider seam，再接三家：**现有 Azure + 火山 + 腾讯**；跑完同语料 A/B 后再决定是否补阿里/讯飞。

## 哪家对中文支持更好

这里的“更好”只能指**公开能力更匹配 Norma OS**，不能指未经同语料测试的绝对准确率。按厂商截至本报告日期公开的能力完整度：

- **普通话主链路：火山豆包、腾讯大模型 2.0 优先 POC。** 两者都明确提供中文大模型、实时 WebSocket、中英混说和热词/上下文能力，并有大陆服务。现有资料不足以断言哪家 CER 更低。
- **慢语速、长停顿：阿里的可调参数最透明。** 实时 API 的 `max_sentence_silence` 可设 200～6000 ms，默认 800 ms；还可切换语义断句。腾讯公开 `vad_silence_time`，火山公开 VAD 与强制判停，但仍需实测阈值上限、是否只是“分句”而不是“结束整次请求”。对于按住说话、松开提交的 Norma OS，客户端应以松键为最终提交信号，服务端分句只能生成 partial/final segment，不能擅自触发 Agent。
- **同一句中英混说：腾讯 `16k_zh_en_2.0`、火山中文大模型、阿里“中英自由说”、讯飞实时大模型都有明确官方表述。** Azure 连续语言识别明确不支持同一句内切换；Deepgram 中文不在原生 `language=multi` 列表中。AssemblyAI Universal-3.5 Pro Realtime 和 OpenAI `gpt-transcribe` 可作为境外混说基准，但不能作为大陆默认链路。
- **方言广度：讯飞实时大模型的公开覆盖最广，宣称中英 + 202 种方言免切。** 这是厂商能力声明，不代表每种方言都优于其他厂商；Norma OS 如果当前只做普通话夹英文，不应为用不到的方言覆盖牺牲成本和时延。
- **专有名词：火山请求级热词/上下文、腾讯热词/自学习、阿里 `vocabulary_id` + 自学习模型、讯飞个性化热词都能做定向优化。** 热词只能提高候选权重，不保证命中；英文缩写、路径、产品名必须单独统计命中率和误提升率。

### 中文能力矩阵

| Provider / 模型 | 普通话与方言 | 慢语速 / 长停顿 | 中英混说 | 热词 / 专有名词 | 流式终点检测 | 基于公开事实的判断 |
|---|---|---|---|---|---|---|
| **火山豆包流式 ASR** | 默认中文模型支持中英及国内主流方言 | 产品文档列 VAD、强制判停、首字加速；公开总览未给出足够细的长静音阈值边界 | 明确支持 | 请求级热词、上下文 | WebSocket 双向流式，可返回分句结果并强制判停 | 功能覆盖最贴近 Norma OS，首批 POC；慢语速阈值仍需实测 |
| **腾讯 `16k_zh_en_2.0`** | 普通话、英语与多方言混合；普通 `16k_zh` 只保证少量英文 | 实时 API 提供 `vad_silence_time`；要验证 2～3 秒停顿是否只分句而不结束会话 | 明确支持 | 热词表/请求热词及自学习模型 | WebSocket 实时分句 | 功能和价格有竞争力；必须用大模型 2.0 测，不能拿免费标准模型结果代替 |
| **阿里云 ISI** | 普通话及多个领域/方言模型 | `max_sentence_silence` 200～6000 ms，默认 800 ms；可启用语义断句；停顿时必须继续发送静音帧 | 普通话可夹少量英文；另有“中英自由说”模型 | `vocabulary_id` 泛热词 + `customization_id` 自学习模型；一次请求一个词表 | VAD 断句或语义断句，可返回多句话 | **长停顿控制最透明**，适合作为慢语速专项基准 |
| **讯飞实时大模型** | 官方声明中英 + 202 种方言免切；标准版也支持普通话、中英混说 | 文档公开近/远场 VAD 模式，但未找到与阿里同等明确的客户端静音时长范围 | 明确支持 | 控制台个性化热词；文档说明专业词可提高权重 | WebSocket 连续流、分段返回；终点阈值需实测 | 方言与混说覆盖最广的公开声明，适合第二批中文基准 |
| **百度实时 ASR** | 普通话、英语、多方言通过不同模型/`dev_pid` 选择 | 官方接口强调音频格式与模型选择，未明确公开长停顿阈值调节能力 | 未明确承诺同一句自由混说 | 有热词/自学习能力，但实时中英混杂场景官方 FAQ 提示热词可能不生效 | WebSocket 实时分段；慢语速行为需实测 | 免费资源适合冒烟测试；混说和长停顿证据弱于前四家 |
| **Azure Speech** | `zh-CN` 及多个中文 locale | SDK 可设置 segmentation silence timeout；开启语言识别还会增加初始延迟 | 连续 LID 不支持同一句内改变语言 | Phrase List / Custom Speech | SDK 实时连续识别；当前 Norma 仍是停录后整段 REST | 保留为现有对照并先改流式；不应为中文混说默认开启 LID |

以上“支持”来自[火山大模型说明](https://www.volcengine.com/docs/6561/1354871)、[腾讯产品功能](https://cloud.tencent.com/document/product/1093/35682)与[实时 API](https://cloud.tencent.com/document/api/1093/48982)、[阿里实时 API](https://help.aliyun.com/zh/isi/developer-reference/api-reference)与[识别 FAQ](https://help.aliyun.com/zh/isi/support/faq-about-speech-recognition)、[讯飞大模型 API](https://static.xfyun.cn/doc/spark/asr_llm/rtasr_llm.html)、[百度实时 API](https://cloud.baidu.com/doc/SPEECH/s/jlbxejt2i)及 [Azure 语言识别](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)。这些页面证明“接口存在/厂商明确声明”，不证明相对准确率。

## 总览

| 服务 | 中文与中英混说 | 实时 / 一次性 | macOS + Rust 接入 | 免费额度 / 试用 | 公开价格快照 | 中国大陆可用性 | Norma OS 判断 |
|---|---|---|---|---|---|---|---|
| **火山引擎豆包语音** | 默认中文大模型支持中英和主流方言；双向流式支持中英文 | WebSocket 双向流式；HTTP 录音文件标准/极速/闲时 | Rust 直接实现 WSS 二进制协议；官方 SDK 侧重 iOS/Android/Linux C++/Java | [豆包官方产品页](https://www.volcengine.com/product/doubao)列流式、录音各 20 小时免费额度；领取条件和有效期以控制台为准 | 火山首页列流式 ASR 2.0 ¥1/小时；产品页另有 30 小时新客包 ¥66，渠道/版本价格需在控制台确认 | SLA 服务范围明确为中国大陆（不含港澳台） | **首批 POC，优先级高**；流式模型有强制判停、VAD、上下文/热词、首字加速 |
| **腾讯云 ASR** | `16k_zh_en_2.0` 明确支持普通话、英语和多方言混合；标准普通话仅保证少量英文词 | WebSocket 实时；短句、极速文件、异步文件 API | Rust 直接 WSS；签名需自行实现；官方 API Explorer 无 Rust SDK | 标准实时识别持续 5 小时/月；**大模型 2.0 明确无免费额度** | 标准实时 ¥3.20/小时起；大模型 2.0 实时 ¥1/小时；60 小时预付包 ¥60 | 中国大陆地域和就近域名完善；另有需单独开通的跨境版 | **首批 POC，优先级高**；持续免费适合标准模型回归，大模型要单独付费测 |
| **阿里云 ISI** | 普通话模型可识别中英混合；另有 Chinese-English Free Talk 模型 | WebSocket 实时；REST 短句/录音文件 | Rust 直接 WSS/REST；官方有 C++ 等 SDK但无 Rust | 新用户 3 个月；实时/短句在并发 ≤2 时不限总时长，录音文件每天 2 小时 | 实时按量 ¥3.50/小时；30 小时包 ¥100 | 上海、北京、深圳等大陆地域并支持就近接入 | **首批或第二批 POC**；试用最适合集中横评，但不是长期免费 |
| **科大讯飞** | 标准实时转写明确支持普通话、中英混说；大模型版称中英 + 202 方言混合 | WebSocket 实时转写；语音听写单次 ≤60 秒；文件转写 | 任意语言可直接 WSS；Rust 无官方 SDK，协议简单 | 实时转写大模型及标准版：个人 5 小时、企业 50 小时，均 1 年有效；每个开发者领取一次 | 大模型 1000 小时包 ¥4000；标准版 600 小时包 ¥2000；其他套餐见当前产品页 | 中国大陆服务 | **中文基准候选**；方言与中英混说能力公开得最完整，终点阈值需实测 |
| **百度智能云** | 实时接口分别提供普通话、英语、多方言模型；官方没有明确承诺同一实时模型中英自由混说 | WebSocket 实时；REST 短句；异步文件 | Rust 直接 WSS/HTTP；无 Rust SDK | 实时普通话/英语各 10 小时，一次性、长期有效；按实名主体发放一次 | 实时 1000 小时包 ¥1800；后付费价以价格页/控制台为准 | 中国服务，`vop.baidu.com` | **备用项**；协议清晰但音频约束严（16 kHz PCM），混说需实测 |
| **Microsoft Azure Speech** | 支持 `zh-CN` 及多种中文 locale；连续语言识别会增加初始延迟，且**不支持在同一句话内改变语言** | Speech SDK 实时；REST fast/batch；当前 Norma 实现是停录后整段 REST | 无 Rust SDK；macOS 可用 C++/Objective-C/Swift/Python SDK。Rust 实时可桥接 C++，一次性可 REST | 全球 Azure F0：实时 Standard/Custom STT 合计 5 小时/月；Azure 中国区额度需在独立订阅核实 | 定价按区域动态显示，全球与中国区分别确认 | Azure 中国（世纪互联）在 `chinaeast2`、`chinanorth2/3` 提供 Speech，但需独立中国区账号与本地端点 | **保留为现有基线**；可在合规条件满足时评估 Azure 中国区，当前延迟仍主要受一次性上传路径影响 |
| **Google Cloud STT** | V2 Chirp 3 支持普通话 `cmn-Hans-CN`；最多给 3 个候选语言后选最匹配语言，不是逐词中英 code-switch 保证 | gRPC 双向流式；REST/gRPC 同步与异步文件 | 官方客户端无 Rust；Rust 可生成 protobuf/gRPC client，工程量高于 WSS | V1 60 分钟/月持续免费；V2 无固定免费层；新账号另有 90 天 $300 Cloud credit | V2 Standard $0.016/分钟；Dynamic Batch $0.003/分钟 | 无中国大陆区域；大陆账号、网络和合规需单独验证 | **不进首批**；流式只支持 gRPC，Rust 接入成本较高 |
| **AWS Transcribe** | 全球站 `zh-CN` 支持 batch + streaming；`IdentifyMultipleLanguages` 可处理同一流中的语言切换，但建议候选不超过 5 个，且与部分高级能力不兼容 | SDK、HTTP/2、WebSocket 流式；S3 batch | **有官方 AWS SDK for Rust**，也可直接 WSS | 全球站：从首次请求起 12 个月，每月 60 分钟，不能结转；中国区另核 | 全球站示例首档通常 $0.024/分钟；区域阶梯不同，15 秒最低计费 | 北京/宁夏中国区支持流式普通话，但当前中国区不提供自动语言识别；需独立账号，价格/免费层另核 | **接入体验好但不进首批**；中国区不能依赖自动中英切换 |
| **OpenAI Audio Transcription** | `gpt-transcribe` 明确支持多语言音频与 code-switch；`gpt-live-transcribe` 支持多个 language hints，但实时中英切换质量仍需实测 | `gpt-transcribe` 支持文件、已完成文件的流式转录及 Realtime 已提交 turn；`gpt-live-transcribe` 走 Realtime WebSocket | 无官方 Rust SDK；直接 HTTP/WSS 容易 | 没有固定免费 ASR 额度；转写模型 Free API tier 不支持 | `gpt-transcribe` $0.0045/音频分钟；`gpt-live-transcribe` $0.017/实时分钟；旧 `whisper-1` $0.006/分钟 | **中国大陆不在官方支持地区列表**，从不支持地区访问可能导致封禁/停用 | **只做离线/境外基准，不作为大陆默认 Provider** |
| **Deepgram** | Nova-3 单语言模型支持中文 batch + streaming；原生 `language=multi` code-switch 列表**不含中文** | WebSocket streaming；REST prerecorded | 无官方 Rust SDK；直接 WSS/HTTP | 新账号一次性 $200 credit，官方称无需信用卡、额度不过期 | Nova-3 单语言促销价：流式 $0.0048/分钟、预录音 $0.0077/分钟 | 默认美国端点，另有欧洲端点；无大陆区域 | **纯中文海外基准候选**；中文夹英文技术词要实测，不能按“多语言”能力推断 |
| **AssemblyAI** | Universal-3.5 Pro Realtime 支持普通话等 18 种语言，并明确支持同一句内原生 code-switch | REST 异步文件；WebSocket realtime | 无官方 Rust SDK；直接 REST/WSS | 官方免费层标为最多 333 小时 streaming / 185 小时 prerecorded；具体模型可用量以账号控制台为准 | Universal-3.5 Pro Realtime $0.45/小时；异步版 $0.21/小时 | 无中国大陆区域，网络与账号可用性需实测 | **海外中英混说强基准**；能力匹配，但不是大陆默认生产链路 |

## 第一批候选的官方证据与接入重点

### 1. 火山引擎豆包语音

- [大模型产品说明](https://www.volcengine.com/docs/6561/1354871)列出双向流式、流式输入、录音文件三种模式，以及 VAD 分句、强制判停、上下文、请求级热词、首字返回加速等能力；这些能力正好对应慢速发音与长停顿场景。
- [产品更新](https://www.volcengine.com/docs/6561/162929)说明默认中文模型支持中英及国内主流方言，双向流式目前支持中英文；[产品页](https://www.volcengine.com/product/asr)也明确写有“中英文混合识别支持”。
- 流式走 WebSocket；官方 SDK 的桌面覆盖并不适合 Rust/macOS，因此更实际的路径是直接实现协议。录音文件极速版另有 [HTTP API](https://www.volcengine.com/docs/6561/1631584)。
- [火山首页](https://www.volcengine.com/)当前展示豆包语音识别模型 2.0：录音文件 ¥0.8/小时、流式 ¥1/小时；[ASR 产品页](https://www.volcengine.com/product/asr)的 30 小时新客包则是 ¥66。两个入口可能属于不同版本/销售渠道，POC 前应以控制台最终 SKU 为准。
- [智能语音 SLA](https://www.volcengine.com/docs/6561/107349)把服务范围定义为中国大陆地区（不含港澳台）。

### 2. 腾讯云 ASR

- [产品功能](https://cloud.tencent.com/document/product/1093/35682)列明 `16k_zh_en_2.0` 是“中文普通话 + 英语 + 多方言混合引擎”；普通 `16k_zh` 只描述为可识别少量英语词汇。
- [实时 WebSocket API](https://cloud.tencent.com/document/api/1093/48982)边传音频边返回结果，可直接由 Rust 实现。
- [计费页](https://cloud.tencent.com/document/product/1093/35686)同时给出关键差异：标准实时 ASR 每月 5 小时免费，大模型 2.0 无免费额度；标准实时后付费首档 ¥3.20/小时，大模型 2.0 为 ¥1/小时。
- [请求结构](https://cloud.tencent.com/document/product/1093/35639)支持就近接入和广州等大陆地域；如果客户端位于大陆，时延比海外端点更可控。

### 3. 阿里云 ISI

- [实时 API](https://help.aliyun.com/zh/isi/developer-reference/api-reference)使用 WebSocket；支持 8/16 kHz PCM 和中间结果。[语音识别 FAQ](https://help.aliyun.com/zh/isi/support/faq-about-speech-recognition)明确普通话模型可以识别夹杂英文，实时模型表另有 Chinese-English Free Talk。
- [功能与调用方式](https://help.aliyun.com/zh/isi/product-overview/features)覆盖短句、实时和文件；[SDK/API 总览](https://help.aliyun.com/zh/isi/getting-started/sdk-and-api-references)显示 REST 适合短句/文件，实时走 SDK/WebSocket。
- [计费项](https://help.aliyun.com/zh/isi/product-overview/pricing)与[计费方式](https://help.aliyun.com/zh/isi/product-overview/billing-10)说明新用户三个月试用，实时识别按量首档 ¥3.50/小时；国际版计费页进一步写明试用期实时/短句并发不超过 2 路时不限总时长。
- [地域与域名](https://help.aliyun.com/en/isi/product-overview/regions-and-domain-names)包含上海、北京、深圳等大陆地域，实时识别还支持智能就近接入。

### 4. Azure：为什么仍要保留

- [Speech SDK](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/speech-sdk)用于实时流，macOS 支持 C++、Java、Objective-C、Python、Swift；没有 Rust SDK。[REST API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/rest-speech-to-text)适合 fast/batch，而不是 Rust 里最省事的官方实时通道。
- [语言表](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support)包含 `zh-CN`、粤语和中文方言 locale。[语言识别](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-identification)会提高初始延迟，因此不应在语言已知时默认开启。
- 连续语言识别只能在句子之间改变识别语言，不支持同一句中的中英切换；因此 Norma OS 以普通话为主、夹英文术语的场景不应把 Azure LID 当作 code-switch 方案。
- 当前 Norma OS 是停录后才把整段音频发送给 Azure。长音频时“上传 + 识别”全部发生在松开之后，这解释了为什么 16.8 秒慢速录音出现约 5 秒的停录后等待。只有改成边录边传后，Azure 与其他流式候选的延迟对比才公平。
- [Azure Speech 定价页](https://azure.microsoft.com/en-us/pricing/details/speech/)的 F0 公开免费量为实时 Standard/Custom STT 合计 5 小时/月，但实际可用区域和价格需在订阅内确认。
- Azure 中国由世纪互联运营，[中国区 Speech 地域表](https://docs.azure.cn/en-us/ai-services/speech-service/regions)列出 `chinaeast2`、`chinanorth2`、`chinanorth3`；[主权云接入说明](https://docs.azure.cn/en-us/ai-services/speech-service/sovereign-clouds)要求使用独立 Azure 中国账号和 `.azure.cn` 端点。全球 F0 不能直接推定适用于中国区，应在目标订阅核对。

### 5. 讯飞与百度：基准和备用

- 讯飞[实时转写 API](https://static.xfyun.cn/doc/asr/rtasr/API.html)是 16 kHz PCM WebSocket，明确支持普通话、中英混说和英文；[大模型实时转写](https://static.xfyun.cn/doc/spark/asr_llm/rtasr_llm.html)宣称中英 + 202 方言混合。主站公开页对实时免费时长和价格展示不稳定，应在控制台领取页核对，而不是引用旧活动数字。
- 讯飞[实时转写产品页](https://www.xfyun.cn/services/rtasr)截至本报告日期已公开列出：大模型和标准版均为个人 5 小时、企业 50 小时、1 年有效；大模型 1000 小时包 ¥4000，标准版 600 小时包 ¥2000。该数值比此前“公开页未展示”更明确，但活动价格仍需在购买时复核。
- 讯飞[短句听写产品页](https://www.xfyun.cn/service/voicedictation)公开免费包为个人 1 万次、企业 10 万次、3 个月，适合命令式短句，但不能替代不限时长的实时转写选型。
- 百度[实时 WebSocket API](https://cloud.baidu.com/doc/SPEECH/s/jlbxejt2i)协议清晰，但要求 16 kHz、16-bit、单声道 PCM；中文、英语通过不同 `dev_pid` 模型选择，故中英混说不能只凭“支持中文和英语”推断。
- 百度[免费额度说明](https://cloud.baidu.com/discover/price-speech-free.html)列出普通话/英语实时识别各 10 小时，一次性、长期有效；[产品价格](https://cloud.baidu.com/product-price/speech.html)与[实时产品页](https://cloud.baidu.com/product/SPEECH/tui02.html)给出 1000 小时包 ¥1800。

## 海外服务的关键事实

### Google Cloud Speech-to-Text

- [V2 语言表](https://docs.cloud.google.com/speech-to-text/docs/speech-to-text-supported-languages)支持 `zh-CN`；[流式文档](https://docs.cloud.google.com/speech-to-text/docs/streaming-recognize)明确流式识别只通过 gRPC。
- [官方客户端库](https://docs.cloud.google.com/speech-to-text/docs/libraries)列出 C#、Go、Java、Node.js、PHP、Python、Ruby，没有 Rust。Rust 可以从 proto 生成 gRPC client，但认证、重连和流控工作比普通 WSS 多。
- [定价](https://cloud.google.com/speech-to-text/pricing)：V1 每月前 60 分钟免费；V2 Standard $0.016/分钟，Dynamic Batch $0.003/分钟。不要把 V1 免费层写成 V2 免费。

### AWS Transcribe

- [语言表](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html)明确 `zh-CN` 同时支持 batch 和 streaming；[流式多语言识别](https://docs.aws.amazon.com/transcribe/latest/dg/lang-id-stream.html)可识别同一流中的多种语言，但会增加识别复杂度，短命令仍需实测首字与定稿延迟。
- 流式多语言识别至少需要两个候选语言，官方建议不要超过五个；通常需要约一秒音频形成语言判断，而且多语言流只支持 PCM，并与自定义语言模型、内容脱敏等能力存在组合限制。
- [Rust SDK 示例](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_transcribe_code_examples.html)和[SDK 接入总览](https://docs.aws.amazon.com/transcribe/latest/dg/getting-started-sdk.html)是本轮候选中最完整的官方 Rust 路线；也可按[WebSocket/HTTP2 协议](https://docs.aws.amazon.com/transcribe/latest/dg/streaming-setting-up.html)直接接入。
- [定价页](https://aws.amazon.com/transcribe/pricing/)提供首次请求后 12 个月、每月 60 分钟免费；全球站价格按区域阶梯变化且每请求最低计费 15 秒。
- AWS [中国北京/宁夏已支持流式普通话](https://aws.amazon.com/about-aws/whats-new/2021/07/amazon-transcribe-now-supports-streaming-transcription-amazon-web-services-china/)，但 AWS 中国账号与全球账号隔离，免费层和价格也必须在中国站单独核对。

### OpenAI Audio Transcription

- [`gpt-transcribe`](https://developers.openai.com/api/docs/models/gpt-transcribe)用于完整文件、已完成文件的流式返回及 Realtime 已提交 turn，模型页明确写有多语言音频、code-switch 和多个语言提示，公开价为 $0.0045/音频分钟。
- [`gpt-live-transcribe`](https://developers.openai.com/api/docs/models/gpt-live-transcribe)用于低延迟实时增量转录，支持调节延迟、上下文、关键词和多个语言提示，公开价为 $0.017/实时分钟；其模型页没有同样明确承诺实时 code-switch 质量，仍需用中英混说语料实测。
- 旧 `whisper-1` [公开价格](https://developers.openai.com/api/docs/models/whisper-1)为 $0.006/分钟，不应再作为新接入的唯一比较对象。
- 转写模型页面显示 Free API tier 不支持，故没有可规划的固定免费额度。[API 支持地区](https://help.openai.com/en/articles/5347006)不包含中国大陆，从不支持地区访问或提供访问可能导致账号停用。

### Deepgram 与 AssemblyAI

- Deepgram 的 [模型与语言](https://developers.deepgram.com/docs/models-languages-overview/)显示 Nova-3 单语言模型支持中文 batch + streaming；但 [`language=multi`](https://developers.deepgram.com/docs/language) 的原生 code-switch 语言列表不含中文，不能把其“多语言”能力写成中文中英混说支持。[定价页](https://deepgram.com/pricing)给新账号一次性 $200 credit；Nova-3 单语言促销价为流式 $0.0048/分钟、预录音 $0.0077/分钟。
- AssemblyAI [Universal-3.5 Pro Realtime 语言说明](https://www.assemblyai.com/docs/faq/language-support-for-real-time-transcription)包含 Mandarin，并支持 18 种语言及句中原生 code-switch；[定价页](https://www.assemblyai.com/pricing/)标价为实时 $0.45/小时、异步 $0.21/小时，免费层展示最多 333 小时 streaming / 185 小时 prerecorded。免费层对具体模型的折算与权限以账号控制台为准。
- 两家都没有官方 Rust SDK，Rust/macOS 直接用 HTTP/WSS 即可；两家也都没有中国大陆数据区域，因此只适合境外基准或具备稳定境外网络的部署。

## 如何比较“更准确”

用 Norma OS 自有语料建立 200～500 条音频的固定评测集，原始 WAV 与人工真值都版本化保存。至少分五组：

1. 慢速普通话，句内有 0.5、1、2、3 秒停顿；
2. 中文夹 Rust、GPUI、Agent、token、WebSocket 等英文和缩写；
3. Norma OS 的节点名、工具名、路径和人名；
4. 麦克风近场、键盘声、空调声、远场；
5. 短命令、长指令、说到一半自我修正。

每个 Provider 固定相同的 PCM、采样率和音量，至少统计：

- **CER/WER**：字符/词错误率；英文缩写单独统计；
- **命令成功率**：转写后 Agent 是否仍执行正确意图，比纯 CER 更贴近产品价值；
- **首个 partial 延迟**：从首个音频帧到首个可见 partial；
- **停语到 final 延迟**：最后一个有声帧到最终定稿，这是慢速发音体验的核心；
- **误判停率**：长停顿中被错误截断的比例；
- **partial 回滚量**：实时文字被反复改写的字符数；
- **大陆网络成功率与 P95/P99**：至少在真实目标网络跑 1000 次，而非只看平均值；
- **单位有效命令成本**：总费用 / 最终正确执行的命令数。

评测时要把“模型”和“endpointing”拆开。慢速发音常见问题不是模型听不懂，而是 VAD/静音阈值过短，提前把一句话切成两段；或当前实现直到松开才上传，使音频时长直接放大停录后等待。相同模型在流式传输、手动松开定稿和服务端自动断句三种模式下，应分别测量。

## 最小落地顺序

1. 先把 Provider 指标统一成 `connect_ms`、`first_partial_ms`、`speech_end_to_final_ms`、`total_ms`、`audio_ms`、`provider_request_id`，日志不记录音频和完整正文。
2. 保留现有 Azure 一次性实现作为对照，再新增 Azure 流式或至少 fast transcription，确认“云模型慢”与“整段上传慢”各占多少。
3. 接火山和腾讯的 WSS；都使用同一份 PCM chunk、取消语义、超时和重连策略。
4. 用固定评测集跑盲测；只有前三名差距不稳定时再接阿里或讯飞。
5. 生产默认优先选择大陆端点；海外 Provider 只作为可配置选项或评测基线，不做隐式回退，避免跨境、账号和隐私风险。

## 不确定项与上线前复核

- 火山引擎在火山首页、豆包模型页和传统语音产品页存在不同价格/SKU；应在目标账号控制台确认最终的 `resource_id`、免费额度、有效期和按量价。
- 腾讯 5 小时/月免费额度适用于标准实时 ASR，不适用于大模型 2.0；POC 预算必须分开。
- 讯飞实时转写主站当前公开个人 5 小时/企业 50 小时、1 年有效；这属于页面当前活动口径，领取资格与购买价格仍以控制台订单为准。
- Azure 全球与 Azure 中国是独立账号、端点和商业体系；全球 F0 的 5 小时/月不能直接套用到中国区。Google/AWS/Deepgram/AssemblyAI 的美元价格和免费政策也可能调整。
- AWS 中国区虽然支持流式普通话，但当前不提供自动语言识别；全球站的 60 分钟/月免费层也不能直接套用到中国区。
- “支持中文 + 支持英语”不等于“同一句中英自由混说”；只有厂商明确写 mixed/code-switch 的才标为支持，其他都必须实测。
- 所有候选都需审查音频数据保存、训练使用、日志保留、数据区域和删除机制，再进入生产。
