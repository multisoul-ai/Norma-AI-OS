# Mac 本地中英混合 ASR：候选核验与实验组合

日期：2026-10-24。状态：文献与部署能力核验，尚未下载候选模型或实测。本文服务于 Goal 实验计划，不能作为准确率或延迟的实验结果。

硬边界：采集音频、声学识别、热词处理、可选纠错及结果存储均在用户 Mac 本地完成；运行时断网可完成全部识别。阅读公开资料、预先下载权重与运行时上传音频是不同动作；候选不得依赖云端 ASR 或远程 LLM。离线实验进程也不得上传转写内容、提示词或音频派生特征。

## 证据结论

已有可落地的轻量流式模型与较大离线模型，适合比较“单模型流式”和“流式草稿 + 本地声学二遍定稿”。目前没有足够证据直接宣布哪个模型在本机、用户口音和编程术语下最优。官方文档中的支持中文、支持英文、公开单语言 benchmark、批量吞吐均不能替代句内中英混合、单用户实时延迟的本机实测。

本次保留 5 个主要模型家族，Apple 作为第 6 个有条件对照。优先试验顺序为现有 Sherpa 基线、双语 Paraformer、SenseVoiceSmall、Whisper turbo、Qwen3-ASR；后两者可与同一个流式前端组合。Whisper large-v3 只作准确率上界候选，不能预设它一定优于 turbo。

现场约束由主任务核验：Apple M4 / 16 GiB / macOS 27.0.1，已有双语 Zipformer 与 Silero VAD。优先复用这些权重建立基线；每组只加载必要模型，先顺序测再测双模型并发，必须把统一内存压力与真实 App 负载纳入结果。上述来源里的 A100、服务器并发吞吐或 Core ML 相对 CPU 加速值均不能代表这台 Mac 的表现。

| 候选 | 流式性质与混合语言证据 | Mac 后端与资源依据 | 许可和局限 | 实验角色 |
|---|---|---|---|---|
| Sherpa-ONNX bilingual Zipformer，`sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20` | 真正在线 transducer。Sherpa 明确中英双语，来源模型名称含 Chinese-English mixed；该页没有用户领域句内混合测试结果。[S1][S2] | 本地 ONNX CPU；官方列出的 int8 encoder 174M、decoder 约 13–14M、joiner 3.1M 是权重文件大小，不能当作运行内存。[S1] | 模型卡 Apache-2.0；需锁定实际包、来源和 SHA。模型较旧但不能据年份判定质量。[S2] | 低成本流式基线；测 hotword 与解码参数能否解决问题。 |
| Paraformer 双语流式 / FunASR 二遍 | Sherpa 明确提供 `streaming-paraformer-bilingual-zh-en`。当前 FunASR 官方 `paraformer-zh-streaming` 模型卡也标明 Chinese + English，220M；这仍不等于技术英语插入中文句子的质量保证。其示例 chunk 采用 60 ms 单位，`[0,10,5]` 不应被说成几十毫秒首字延迟。[S3][S4] | 优先 Sherpa ONNX CPU 与现有 Rust 接口衔接；int8 encoder 158M + decoder 68M，FP32 607M + 218M，均为磁盘权重。[S3] | FunASR 工具代码 MIT；具体模型许可独立。新版官方 HF 卡提供 `apache-2.0-20260804` 不可变修订，不能将该许可追溯套到旧 Sherpa 导出包。[S4][S5] | 与 Zipformer 单模型正面对照；再用胜出流式前端 + 离线 Paraformer 或另一模型定稿，区分架构收益与模型收益。 |
| SenseVoiceSmall | 非自回归离线识别，官方发布 checkpoint 支持普通话、粤语、英、日、韩。Sherpa 的实时示例名为 `simulated-streaming-asr`；窗口重跑/VAD 切块不是原生缓存式流式。官方单语 benchmark 不证明中英句内混合优胜。[S6][S7] | Sherpa ONNX 本地 CPU，优先 int8；也有 FunASR ONNX 路线。只固定一个明确 checkpoint，避免把 2024 原版和 2025 社区微调混为一组。[S7] | 官方 HF 模型卡为 `license: other`，指向 FunASR MODEL_LICENSE，不能称 Apache-2.0；落地发布前逐条核验权重条款。[S8][S9] | VAD 分段单模型，以及同一流式前端的快速二遍定稿。 |
| Whisper large-v3-turbo / large-v3 | 序列式离线模型；whisper.cpp 的 stream 示例每半秒反复转写音频，属于窗口式实时包装。Whisper 支持多语言，仍须测试句内切换、英语专有名词和静音幻觉。[S10][S11] | whisper.cpp 明确支持 Apple Silicon Metal、Accelerate，Core ML 可加速 encoder；MLX 官方示例为另一条 Mac 路线。turbo 809M，large 1550M 参数；OpenAI 表内 A100 速度和 VRAM 数值不当作本机表现。[S10][S11][S12] | 模型和 Whisper 代码 MIT。Core ML 首次编译会慢，冷启动和热启动必须分报。[S10][S11] | turbo 为主要高精度二遍候选；large-v3 仅当 turbo 的准确率仍不足时追加。后端对比固定权重精度与解码条件。 |
| Qwen3-ASR 0.6B / 1.7B | 官方开放权重，声明统一流式/离线，支持 30 语言和 22 中文方言；官方 SDK 当前流式仅 vLLM。不能把服务器流式/并发吞吐直接外推 Mac。[S13][S14] | **已找到 Sherpa 官方 Qwen3-ASR ONNX 部署文档，明确 macOS x64/arm64**，0.6B int8 包示例为离线 recognizer + VAD 模拟流式。另有 MLX-Audio 维护者的 0.6B/1.7B 8-bit 实现；这是该后端的主来源，不是 Qwen 官方 Mac 性能承诺。[S15][S16][S17] | Qwen 官方模型卡 Apache-2.0。Sherpa 导出示例 encoder 174M、decoder 721M，另有 frontend/tokenizer；这些不是峰值 RSS。`--stream` 文本增量不能单独证明能接受连续 PCM、保留声学缓存及低延迟。[S14][S16][S17] | 0.6B ONNX 优先可行性试验；MLX 0.6B/1.7B 作本地定稿候选，先完成后端正确性与断网验证。 |
| Apple SpeechAnalyzer / SpeechTranscriber（条件对照） | Apple WWDC 明确转写 entirely on-device，有 volatile/finalized 结果；初始化指定 locale。本次官方材料未找到明确保证“同一段内中英 code-switching”的证据。[S18][S19] | macOS 26+；运行时检查 `isAvailable`、`supportedLocales` 和安装资源。系统管理模型、可自动更新，需记录 OS 和资源信息，重现性弱于固定权重。[S18][S19] | Apple 平台 API，不能当成开源可分发权重。模型须预先安装；缺资源时不能悄悄回退云端或在测量中下载。[S18] | 仅设备/语言可用时加入一次小规模混合语音闸门；若混合错误明显则不投入主赛。 |

## 可执行的架构组合

以下是实验设计，不是测得的优劣排序。候选数应分阶段收敛，避免所有模型、量化与后端组合全排列。

| 组别 | 架构 | 要回答的问题 | 必须独立记录的成本 |
|---|---|---|---|
| B0 | 现有模型、现有 VAD/端点、现有参数 | 现在慢在哪里，错在哪里？ | 采集/重采样、缓冲、首字、停口等待、解码、事件送达、冷启动各自时间。 |
| B1 | 现有双语流式模型 + 端点/热词小范围调参 | 不增加模型能否达标？ | false endpoint、短词被截断、热词误注入、英文退化。 |
| A | Zipformer 与双语 Paraformer 单模型流式 | 最小资源下的准确率与稳定输出边界？ | 首个正确字符、稳定前缀延迟、最终 MER、长句累计漂移。 |
| B | SenseVoiceSmall + VAD/有限重叠窗口 | 单离线模型分段能否同时够快、够准？ | 端点等待、重复窗口计算、拼接遗漏/重复、文本抖动。 |
| C | 胜出流式前端 + SenseVoiceSmall 本地声学二遍 | 轻量二遍带来的精度增益值不值得？ | 两模型总内存、CPU 争抢、定稿尾延迟、草稿到定稿改写量。 |
| D | 同一流式前端 + Whisper turbo 或 Qwen3-ASR 0.6B/1.7B 定稿 | 较大模型是否改善中英切换和技术术语？ | GPU 与画布/其它本地模型争抢、能耗、定稿尾延迟、静音幻觉。 |
| E | Apple 原生流式（设备支持时） | 系统模型能否作为零自带权重路径？ | 混合语言失败、资源不可用、OS 升级后结果漂移。 |

二遍必须读取原音频执行声学复核，而不是拿一遍错字交给文本 LLM 猜；文本 LLM 纠错若后续研究，必须单列消融并完全离线。先测试单模型独立准确率，再组合两遍，防止前端把音频截坏后错误归因后端。草稿不能触发不可逆语音指令；定稿状态、取消和超时应有清晰协议。

跨组控制：相同录音、相同单声道 PCM、相同人工真值和开发/盲测划分。准确率离线回放先统一切段；端到端实验再用实时节奏送入原始连续音频，让 VAD/端点错误进入结果。中文 CER、英文 WER、混合 MER、英语术语/标识符准确率分报；同时测停口到最终文本的 p50/p95、开始说话到首个正确 partial、稳定前缀延迟、RTF、峰值内存、冷启动及连续运行温升。任何阈值先作为预注册目标，不把论文指标伪装成本机结果。

后端与模型必须分别锁版本：仓库 commit、权重 revision/SHA256、量化、线程、Metal/Core ML 开关、解码配置、VAD 配置及 OS/芯片/内存。先完成断网运行和“不上传录音、文本或派生特征”的验证，再计入可选型候选。

## 主来源

- [S1] Sherpa 官方双语 Zipformer 部署与文件信息：https://k2-fsa.github.io/sherpa/onnx/pretrained_models/online-transducer/zipformer-transducer-models.html#csukuangfj-sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20-bilingual-chinese-english
- [S2] 发布者模型卡与许可证：https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20 ；训练来源：https://huggingface.co/pfluo/k2fsa-zipformer-chinese-english-mixed
- [S3] Sherpa 官方在线 Paraformer：https://k2-fsa.github.io/sherpa/onnx/pretrained_models/online-paraformer/paraformer-models.html
- [S4] FunASR 官方流式 Paraformer 卡及不可变许可修订说明：https://huggingface.co/funasr/paraformer-zh-streaming
- [S5] FunASR 官方模型总表与工具/模型许可区分：https://github.com/modelscope/FunASR
- [S6] SenseVoice 官方已发布 checkpoint 范围与非自回归说明：https://github.com/FunAudioLLM/SenseVoice
- [S7] Sherpa 官方 SenseVoice 部署，含模拟流式与两个 checkpoint 区分：https://k2-fsa.github.io/sherpa/onnx/sense-voice/pretrained.html
- [S8] SenseVoiceSmall 官方模型卡与许可指针：https://huggingface.co/FunAudioLLM/SenseVoiceSmall
- [S9] FunASR 模型专用许可原文：https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE
- [S10] OpenAI Whisper 模型参数、测试硬件说明及 MIT 许可：https://github.com/openai/whisper
- [S11] whisper.cpp Apple Silicon 后端、Core ML 首次编译与流式示例：https://github.com/ggml-org/whisper.cpp
- [S12] MLX 官方 Whisper 示例与量化：https://github.com/ml-explore/mlx-examples/tree/main/whisper
- [S13] Qwen 官方模型与 SDK，尤其 Streaming Inference 后端限制：https://github.com/QwenLM/Qwen3-ASR
- [S14] Qwen 官方 0.6B 模型卡及 Apache-2.0：https://huggingface.co/Qwen/Qwen3-ASR-0.6B
- [S15] Sherpa 官方 Qwen3-ASR 支持平台：https://k2-fsa.github.io/sherpa/onnx/qwen3-asr/index.html
- [S16] Sherpa 官方 Qwen3-ASR 0.6B int8 部署示例：https://k2-fsa.github.io/sherpa/onnx/qwen3-asr/pretrained.html
- [S17] MLX-Audio 后端维护者 Qwen3-ASR 文档：https://github.com/Blaizzy/mlx-audio/tree/main/mlx_audio/stt/models/qwen3_asr
- [S18] Apple WWDC25 SpeechAnalyzer，含全文 transcript：https://developer.apple.com/videos/play/wwdc2025/277/
- [S19] Apple SpeechTranscriber API 可用版本与支持探测：https://developer.apple.com/documentation/speech/speechtranscriber

所有来源在上述日期读取。网页和默认分支会更新；执行实验前应记录实际 immutable revision，而不是仅保存模型名。本文未把来源自报的速度或质量排名作为最终推荐证据。
