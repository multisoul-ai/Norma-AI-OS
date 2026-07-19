# 免费在线语音识别 API 调研

> 调研日期：2026-07-19。范围是可由程序调用的在线 Speech-to-Text / ASR API，不含只能在网页中试用的转写工具。证据优先采用厂商官方定价、免费额度和开发文档；“免费”分为持续免费、一次性额度和限时试用，不能混为一谈。价格均为公开标价，未含税、地域差价、促销和附加功能费。

## 先说结论

- **中国大陆、长期免费、实时中文**：优先看腾讯云（每月 5 小时）；短句听写也可看讯飞（默认每日 500 次，但官方没有承诺永久维持）。
- **短期集中验证**：阿里云最宽松——新用户 3 个月内、实时识别并发不超过 2 路时不限总时长；Deepgram 的 200 美元、AssemblyAI 的 50 美元一次性额度也适合海外服务横评。
- **海外、长期免费**：Azure F0 每月 5 小时实时识别；Google Cloud Speech-to-Text V1 每月 60 分钟；ElevenLabs 每月约 2.5 小时实时或 4.5 小时批量。Cloudflare 和 Groq 更适合文件转写实验，不是原生实时流式 API。
- **海外、长期免费**还可看 IBM Watson Speech to Text Lite：每月 500 分钟，支持中文普通话和 WebSocket；但 Lite 实例连续 30 天不活动会被删除。
- **需要中国大陆网络与账号的生产可用性**：国内云通常更稳妥。海外服务即使支持中文，也不等于支持中国大陆注册、付费或低延迟访问。
- **不要把免费额度当作产品成本模型**：免费计划可能限并发、速率、文件大小、模型或 SLA，也可能随时调整。正式选型仍应以同一套中文音频做 CER、首个 partial 延迟、最终定稿延迟和大陆网络成功率实测。

## 免费类型说明

| 标签 | 含义 |
|---|---|
| 持续免费 | 按月、按日等周期自动重置；仍可能有速率、并发或模型限制 |
| 一次性额度 | 仅领取一次，额度用完不再恢复；“长期有效”不等于周期重置 |
| 限时试用 | 只在注册后的固定期限可用，到期即结束 |
| 开源自部署 | 软件可免费使用，但不是厂商提供的免费在线 SaaS；机器、带宽、运维和模型许可仍由使用者承担 |

## 中国大陆服务

| 服务 | 免费额度与持续性 | 中文与实时能力 | 注册、扣费与地域 | 免费后公开价格 | 适合场景 | 官方来源 |
|---|---|---|---|---|---|---|
| **腾讯云 ASR** | **持续免费**：实时识别 5 小时/月；录音文件 10 小时/月；额度每月 1 日发放且仅当月有效 | 普通话、中英混合及方言；WebSocket 边说边出字 | 需注册、实名并开通。2022-05 后新用户后付费默认关闭；跨境服务需单独开通 | 国内实时首档 ¥3.20/小时；30 小时预付包 ¥90 | 国内实时原型、低频长期使用 | [计费与免费额度](https://cloud.tencent.com/document/product/1093/35686) · [实时 API](https://cloud.tencent.com/document/product/1093/48982) |
| **阿里云智能语音交互 ISI** | **限时试用**：新用户 3 个月；实时识别并发不超过 2 路时不限总时长；录音文件每天最多 2 小时 | 中文普通话、部分方言及英文；WebSocket 长音频流 | 需注册、实名，创建项目、AppKey 和 Token。试用到期后停止 | 实时首档 ¥3.50/小时；30 小时资源包 ¥100 | 三个月内集中做实时压力测试与横评 | [免费试用](https://help.aliyun.com/zh/isi/getting-started/start-here) · [计费](https://help.aliyun.com/zh/isi/product-overview/billing-10) · [实时 API](https://help.aliyun.com/zh/isi/developer-reference/api-reference) |
| **百度智能云语音技术** | **一次性额度**：实时普通话 10 小时，按实名主体领取一次，官方称长期/永久有效；另有短语音调用次数额度 | 普通话；WebSocket 实时流式 | 需注册、实名并创建应用。超额后从余额扣费，余额不足停服 | 实时后付费 ¥3/小时；1000 小时包 ¥1800 | 低频验证、备用供应商 | [免费测试资源](https://cloud.baidu.com/doc/SPEECH/s/Wl9mh4doe) · [价格](https://cloud.baidu.com/doc/SPEECH/s/Tldjm0i4c) · [实时 API](https://cloud.baidu.com/doc/SPEECH/s/jlbxejt2i) |
| **讯飞开放平台语音听写** | API 文档写明应用默认 **每日 500 次**，单次最长 1 分钟；产品页对额外一次性免费包同时出现“个人 1 万/企业 10 万次、3 个月”和“5 万次、90 天”，口径冲突 | 中文、英文；WebSocket 实时返回 | 需注册、创建应用；正式商用需实名。官方未公开确认按量超额价 | 最小公开包 50 万次/¥1300，1 年有效，约 ¥2.6/千次 | 语音输入框、命令词等短句听写 | [API 与每日额度](https://www.xfyun.cn/doc/asr/voicedictation/API.html) · [产品与资源包](https://www.xfyun.cn/services/voicedictation) |
| **火山引擎豆包语音** | **一次性试用**：流式 ASR 2.0 为 20 小时、半年有效，需在控制台领取 | 中文、英文及方言；WebSocket 双向流式 | 需注册、实名。官方 SLA 服务范围为中国大陆，不含港澳台 | 豆包流式 ASR 2.0 按量 ¥1/小时；30 小时包 ¥28 | 看重正式价格的实时中文 POC | [试用额度](https://www.volcengine.com/docs/6561/1359369) · [正式价格](https://www.volcengine.com/docs/6561/1359370) · [流式 API](https://www.volcengine.com/docs/6561/1354869) |

> 华为云 SIS 支持中文实时识别，也有公开付费价格，但本次未能从当前官方文档确认 API 免费额度，因此不列入“免费候选”。参考其[计费说明](https://support.huaweicloud.com/productdesc-sis/sis_01_0020.html)。

## 海外服务

| 服务 | 免费额度与持续性 | 中文与实时能力 | 注册、扣费与地域 | 免费后公开价格 | 适合场景 | 官方来源 |
|---|---|---|---|---|---|---|
| **Microsoft Azure Speech** | **持续免费 F0**：实时 Standard/Custom Speech-to-Text 合计 5 小时/月；批量转写不在 F0 内 | 多个中文 locale；实时流式 | 免费 Azure 账号通常需支付方式；F0 是否可用受区域影响 | 定价页按区域动态显示，需在计算器确认 | 海外云长期小流量实时识别 | [定价](https://azure.microsoft.com/en-us/pricing/details/speech/) · [语言支持](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support) |
| **Google Cloud Speech-to-Text** | **持续免费，但仅 V1**：60 分钟/月；V2 从第一分钟计费。新账号另有 90 天 300 美元 Cloud credit | 普通话、粤语等；支持 streaming | 需 Cloud Billing 账号；中国大陆服务可用性需自行验证 | V2 标准 $0.016/分钟，dynamic batch $0.003/分钟；V1 价格随 data logging 设置不同 | 已使用 GCP、能接受 V1 免费层限制 | [定价](https://cloud.google.com/speech-to-text/pricing) · [语言支持](https://docs.cloud.google.com/speech-to-text/docs/speech-to-text-supported-languages) · [Cloud 免费计划](https://cloud.google.com/free/docs/free-cloud-features) |
| **IBM Watson Speech to Text** | **持续免费 Lite**：500 分钟/月；达到月额度后暂停至下月重置 | 中文普通话；WebSocket、同步 HTTP 和异步 HTTP | 新 IBM Cloud 账号要求支付信息做身份验证；Lite 实例连续 30 天不活动会被删除 | Plus 前 100 万分钟 $0.02/分钟 | 海外中文实时小流量、希望免费额度用完即停 | [Lite 定价](https://cloud.ibm.com/docs/speech-to-text?topic=speech-to-text-faq-pricing) · [语言与接口](https://cloud.ibm.com/docs/speech-to-text?topic=speech-to-text-about) · [免费账户规则](https://www.ibm.com/products/cloud/free) |
| **AWS Transcribe** | **限时试用**：从首次转写请求起 12 个月内，每月 60 分钟；不能结转 | 简体、繁体、粤语；批量和 streaming | AWS 注册需有效支付方式；超额按账号区域自动计费 | 按区域和用量阶梯计价，官方示例首档约 $0.024/分钟 | AWS 体系内的小规模验证 | [定价与免费层](https://aws.amazon.com/transcribe/pricing/) · [语言支持](https://docs.aws.amazon.com/transcribe/latest/dg/supported-languages.html) |
| **Deepgram** | **一次性额度**：新账号 $200 credit，不到期；官方称无需信用卡 | 简繁中文、粤语；Nova-3 支持预录音和 streaming | 数据面有美国、欧洲、澳大利亚端点；中国大陆账号与网络需验证 | Nova-3 单语预录音 $0.0048/分钟、实时 $0.0077/分钟（页面标为限时价格） | 额度大，适合完整 POC 和模型横评 | [定价](https://deepgram.com/pricing) · [模型与语言](https://developers.deepgram.com/docs/models-languages-overview/) |
| **AssemblyAI** | **一次性额度**：$50 credit，不到期；免费注册无需信用卡 | Universal-2 异步支持中文；多语言实时模型支持中文 | 充值时需添加支付方式；中国大陆访问需验证 | Universal-2 异步 $0.15/小时；Whisper Streaming $0.30/小时 | 批量与实时都要测试、需要说话人等附加能力 | [定价](https://www.assemblyai.com/pricing/) · [免费注册说明](https://support.assemblyai.com/articles/5370767329-can-i-sign-up-for-free) · [多语言实时](https://www.assemblyai.com/docs/universal-streaming/multilingual-transcription) |
| **ElevenLabs Scribe** | **持续免费**：Free 计划约含 Scribe v1/v2 批量 4 小时 30 分/月，或 Scribe v2 Realtime 2 小时 30 分/月；实际消耗依模型 | 90+ 语言，含普通话；Realtime WebSocket | 免费计划按月重置；大陆注册与网络需验证 | 批量约 $0.22/小时；实时约 $0.39/小时 | 想快速接入统一批量/实时 API | [API 定价](https://elevenlabs.io/pricing/api) · [实时 STT](https://elevenlabs.io/realtime-speech-to-text) |
| **Speechmatics** | 官方定价主表称 **持续免费 3000 分钟/月**：实时 1200 + 批量 1800；但同页 FAQ 又称 8 小时/月，官方页面自身冲突 | 普通话；实时和批量 | 官方称无需信用卡；应以注册后 Portal 显示为准 | 实时 Standard $0.24/小时、Enhanced $0.43/小时；批量各模型不同 | 仅在控制台核实额度后作为高免费量候选 | [定价、免费额度与语言](https://www.speechmatics.com/pricing) |
| **Gladia** | **时点敏感**：截至 2026-07-19 为持续 10 小时/月；官方公告从 **2026-07-20** 改为一次性 €50 credit，不再每月重置 | 中文；实时 Live STT | 免费注册无需信用卡；新旧账号余额迁移以官方账单规则为准 | 异步 $0.61/小时，实时 $0.75/小时 | 近期试用可以，但不能按“永久每月 10 小时”规划 | [定价](https://www.gladia.io/pricing) · [账单切换公告](https://support.gladia.io/article/credit-based-billing) · [语言](https://docs.gladia.io/chapters/language/supported-languages) |
| **Groq Speech-to-Text** | **持续免费计划的速率限额**：Whisper 模型 20 RPM、2000 请求/日、7200 音频秒/小时、28800 音频秒/日；即上限约 2 小时/小时、8 小时/日，实际还受文件与请求限制 | Whisper 多语言含中文；当前是文件/URL 转写，不是原生实时流 | 单文件免费层不超过 25 MB；限额到达后请求失败或需升级 | Whisper Large v3 Turbo $0.04/小时，v3 $0.111/小时 | 大量短录音批量转写、离线评测集 | [免费层限额](https://console.groq.com/docs/rate-limits) · [Speech-to-Text](https://console.groq.com/docs/speech-to-text) · [价格](https://groq.com/pricing) |
| **Cloudflare Workers AI Whisper** | **持续免费**：Workers AI 每天 10000 neurons，00:00 UTC 重置；`whisper-large-v3-turbo` 为 46.63 neurons/音频分钟，若只用该模型理论约 214 分钟/日 | Whisper 多语言含中文；文件转写，不是原生实时流 | 超过每日免费量时，未启用付费会失败；启用付费后按 neurons 计费 | $0.011/1000 neurons；该模型约 $0.0005/音频分钟 | 已用 Workers、需要廉价批量转写 | [Workers AI 定价与模型成本](https://developers.cloudflare.com/workers-ai/platform/pricing/) |

### 不是免费候选，但常被误认为免费的服务

**OpenAI Audio API** 没有固定的免费语音识别 API 配额；ChatGPT 的免费使用权也不能转成 API 额度。API 通常需要预付费，最低充值金额一般为 5 美元。它支持中文、文件转写及 Realtime transcription，但[中国大陆不在官方 API 支持地区名单](https://developers.openai.com/api/docs/supported-countries)。参见 [API 定价](https://openai.com/api/pricing/)、[Speech-to-Text 指南](https://developers.openai.com/api/docs/guides/speech-to-text)和[预付费说明](https://help.openai.com/en/articles/8264644-how-can-i-set-up-prepaid-billing)。

## 开源自部署：免费软件，不是免费在线 API

| 项目 | 中文/流式 | 部署形态 | 主要取舍 | 官方来源 |
|---|---|---|---|---|
| **FunASR** | 中文能力强；Paraformer 等支持流式 | 本地 Python/服务端，可自行暴露兼容 API | 需要自己承担算力、部署、并发和模型许可核验 | [GitHub](https://github.com/modelscope/FunASR) |
| **sherpa-onnx** | 提供多种中英文在线/离线模型；含 WebSocket server/client 示例 | 本地 C/C++、Rust 等多平台运行 | 适合隐私优先和原生应用，但模型质量、尺寸与许可需逐个核验 | [GitHub](https://github.com/k2-fsa/sherpa-onnx) |
| **OpenAI Whisper** | 多语言含中文；原项目以文件/分段转写为主 | 本地 Python/PyTorch | 精度基线成熟，但原生实时性与资源占用需自行工程化 | [GitHub](https://github.com/openai/whisper) |
| **whisper.cpp** | 多语言含中文；可通过示例或上层项目做准实时 | 本地 C/C++，适合 Apple Silicon | 部署轻、隐私好；真正低延迟流式、VAD 和服务治理要自己实现 | [GitHub](https://github.com/ggml-org/whisper.cpp) |

## 选型建议

| 目标 | 建议先测 | 原因 |
|---|---|---|
| 中国大陆实时语音输入 | 腾讯云、火山引擎、阿里云 | 大陆网络和中文优化更可控；分别代表持续免费、低正式价、宽松试用 |
| 海外实时语音输入 | Azure F0、Deepgram、ElevenLabs | 分别覆盖长期免费、最大一次性额度、简单的月度免费方案 |
| 批量转写评测集 | Groq、Cloudflare Workers AI、AssemblyAI | 免费吞吐或一次性额度较大，文件 API 简单 |
| 隐私优先、离线可用 | sherpa-onnx / FunASR / whisper.cpp | 数据不必上传，但需承担设备算力与工程维护 |

对 Norma OS 这样的实时语音入口，建议不要仅比较“免费小时数”。使用同一套真实音频做四项门槛测试：

1. 普通话、中英混说、口音和噪声环境的字符错误率（CER）。
2. 首个 partial result 延迟、最终结果延迟，以及 partial 是否频繁回滚。
3. WebSocket 断线重连、取消、超时、并发和长会话上限。
4. 从中国大陆实际网络发起请求的成功率，以及免费额度耗尽后的停止/扣费行为。

## 重要口径与风险

- 本表是 **2026-07-19 的官方公开信息快照**。尤其是 Gladia，将在次日切换免费政策。
- Speechmatics 的同一官方定价页存在 3000 分钟/月与 8 小时/月两种说法；讯飞同一产品页也存在不同的一次性免费包数字。两者都应以控制台实际显示为准，不能把多个数字相加。
- “无需信用卡”仅表示厂商公开注册流程不要求信用卡，不代表所有国家/地区均可注册，也不代表超额后一定自动停服。
- Google、AWS 等绑定账单账号的云服务可能在免费量耗尽后继续计费；腾讯云新账号后付费默认关闭不代表所有历史账号都如此。上线前必须配置预算告警、限额或余额保护。
- 中文支持不等于中文体验一致。模型、采样率、热词、自定义词表、标点、数字格式和说话人分离都会显著影响结果和费用。
