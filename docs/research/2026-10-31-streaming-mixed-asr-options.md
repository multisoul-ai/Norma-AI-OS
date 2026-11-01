# 中英混合语音识别：实时修订与二遍定稿选项

核验日期：2026-10-31。范围：公开官方文档与已有仓库调研；本次没有上传音频、调用付费识别服务或完成新一轮准确率实测。文中的“建议”是待验证的选型方向，不是排名。

同日代码补核：仓库已存在 LocalA1 开发入口（Zipformer 预览 + Qwen3-ASR 0.6B MLX8 终稿），仅支持本地单次录音，真实 App 验收和发布运行时尚未完成；见 `docs/design-docs/2026-10-27-a1-local-asr-integration.md`。当前豆包 `crates/app/src/app/doubao/plan/asr.rs` 已设置 `enable_nonstream=true`。早期研究文档的“首版暂不开二遍”已不能代表现状。跨模式方案见 `docs/design-docs/2026-10-31-qwen-asr-voice-modes.md`。

用户希望的是说话时就看到文字，后续声音到来时能改正先前听错的字，尤其改善中文句子中的英文术语。解决它需要改善声学识别与转写更新；仅在派发前增加确认，不能提高识别准确率。

## “边识别边修正”有三种机制

1. **同一次流式识别修订草稿**：新声音到来后，替换尚未定稿的文本；已定稿部分保持稳定。不能把每个中间结果追加成新句子。
2. **一句结束后复听原音频**：第一遍快速出字，第二遍使用完整句子的声音与上下文重新识别，再替换草稿。这是声学二遍，FunASR 和豆包已有明确支持。[S1][S2]
3. **只用文字做纠错**：让语言模型根据转写猜测正确词。它可能改出流畅但不符合原声的内容，不能作为前两者的替代。若以后试验，应单独计量错误修复和错误引入。

示意（仅解释交互，不是实测）：

```text
用户继续说话  →  实时草稿：「把瑞克特组件交给……」
完整句子到齐  →  音频复听：「把 React 组件交给前端卡片」
本次输入结束  →  确认终稿版本  →  Jev 提取意图和目标卡片
```

“句末定稿”与“用户这次指令已经结束”必须区分。用户可能停顿后继续说“但不要提交”。识别供应商的句级 final 不应直接变成整次任务提交。

## 可选择的方案

以下比较实现机制和部署条件；最终选择以同一批中英混合录音的对比为准。

| 方案 | 怎么做到边出字、边改正 | 中英混合依据 | 在 Norma / Mac 上的落地与代价 |
|---|---|---|---|
| **Apple SpeechAnalyzer / SpeechTranscriber** | macOS / iOS 26 起的新 API；`volatileResults` 可对同一音频范围多次修订临时结果，随后定稿。[S13][S14] | 新 API 不等于系统听写，也不同于旧 SFSpeechRecognizer。官方资料未保证句内中英切换准确率，需要直接对照。 | 本机转写，需检查硬件、可用 locale 和已安装模型。可作为低改动方向与预览前端；不能据“多语言设置”推导混合语言准确率。[S13] |
| **豆包双向流式 + 二遍识别** | `enable_nonstream=true`：先实时输出，在一句结束后重听该段音频，用二遍结果修正句子。`definite` 表示句级稳定状态。[S1] | 官方列中文、英文和部分方言；是否解决用户的术语混说，需要同录音测试，不能由语言列表推出。[S1] | 当前仓库已经使用 `bigmodel_async` 并开启二遍；需要账号、网络和按量付费。本次未新调用服务验收，不应再把“打开二遍”描述为尚未实现的优化。 |
| **FunASR 2-pass** | 在线模型先识别；每句结束后，离线模型纠错。协议可区分 `2pass-online` 与 `2pass-offline`。[S2][S3] | 可选双语 Paraformer；需验证具体 checkpoint 和英文术语准确率。FunASR 是工具体系，不能把所有组合视为同一个模型。[S4] | 可自部署，也可在本机实现相同组合。官方一键服务部署工具仅支持 Linux；Mac 内嵌优先复用已有 Sherpa ONNX 能力，需实现音频段缓存和草稿替换。 |
| **现有流式前端 + SenseVoiceSmall 复听** | 保留当前即时出字，句末将完整音频交给 SenseVoice；以二遍文本定稿。属于项目架构建议，收益待测。 | 官方发布模型含普通话、粤语、英语、日语和韩语；支持多语种不等于技术英语混说已胜出。[S5] | 本地运行，Sherpa 有部署路线。SenseVoice 原版适合分段/离线；官方列出的第三方分块方案称为 pseudo-streaming，可能牺牲准确率。需衡量双模型内存和定稿等待。 |
| **现有流式前端 + Whisper turbo 复听** | 停顿或结束后，对原音频重新转写；也可滚动窗口反复推理更新文本。whisper.cpp 的实时示例属于后一种包装。[S6] | Whisper 是多语言模型；应实测句内切换、专有名词与静音误输出，不能仅按模型大小推断效果。 | whisper.cpp 官方支持 Apple Silicon 的 Metal、Accelerate 和 Core ML。[S6] 可全本地；资源占用及尾部延迟需与画布、Agent 并发测量。 |
| **Qwen3-ASR 0.6B / 1.7B** | 模型家族有流式/离线能力；官方 Python SDK 当前流式要求 vLLM。本机可先做句末复听，或采用 Sherpa 的 VAD 模拟流式。[S7][S8][S9] | 官方提供多语言和中文方言支持；本次未获得同一用户录音下优于 Apple 或其他模型的证据。 | **已有 Sherpa 官方 ONNX Mac x64/arm64 路线，并列 Rust API**，不必判为不可本地部署。0.6B 可先试；不能把服务器流式吞吐或 vLLM 示例当 Mac 体验。 |
| **阿里云 Paraformer 实时** | 持续返回中间文本，再返回句级终稿；`sentence_end=false/true` 区分两者。[S10] | 支持自动语言识别和语言候选，可使用热词列表；具体混合准确率仍需回放。[S11] | 云端对照候选，接 WebSocket；需要账号、网络与付费。已有“中间→最终”接口不等于公开保证使用与 FunASR 相同的二遍架构。 |

WhisperKit 也是 Apple 侧封装选项，但要区分不同接口：当前官方仓库的文件转写 SSE 输出与实时音频输入不是同一能力，实时 WebSocket 本地服务器列为 Argmax Pro。不能仅看到 streaming 字样就承诺开源版本提供相同接口。[S12]

## 如何优化，而不只更换模型

建议保留“快速预览 + 可修订草稿 + 音频终稿”的统一机制，底下接不同识别引擎。同一段音频保留到定稿，按段 ID 和版本替换文本；取消、断网、超时后不能提交迟到结果。

改善中英混合应分开验证三件事：

- **语言模型本身**：同一句里夹 React、Rust、Jev、PR、branch 和具体卡片名称时，是否把英文听成中文谐音。
- **领域词提示**：仅在引擎支持的热词/上下文接口中加入项目名和卡片别名；同时测没有说这些词时是否误注入，避免“偏向某卡片”代替正确识别。
- **分句边界**：短暂停顿不要截断英文单词、否定词或后半句。二遍必须收到完整声音；前端已截坏的音频不能靠后端模型保证修复。

Jev 位于转写之后，负责意图和卡片选择。除非另有已验证的音频输入能力，不能把更换 Jev 当作中英混合 ASR 的修复。可在草稿上预计算候选意图以缩短等待，但实际派发使用本次输入的最终版本。

## 建议先做的最小对比

优先比较三条路线，避免一开始全量接入：

1. **Apple 基线 + 本地二遍候选**：先比较现用 Apple 体验与新 SpeechTranscriber，再试 Apple / 现有流式预览 + Qwen3-ASR 0.6B 或 SenseVoiceSmall 复听。已有同集文件回放支持优先验证这两个二遍候选；尚未证明组合优于 Apple。
2. **豆包单遍与二遍开关 A/B**：如果决定试云端，固定同一引擎、录音和热词，只切 `enable_nonstream`，直接测二遍的增益与等待成本。
3. **双语 Paraformer / FunASR 2-pass**：评估原生双语流式是否已经足够，及其二遍是否比双模型组合更轻。

Paraformer 云服务作为云端对照；Qwen 1.7B 作为本地准确率挑战者。顺序可以由已有实验结果调整。

同一批录音分别统计：中文错字率、英文词错误率、术语识别正确率、错误卡片率，以及首次可读文字和停口到终稿的 p50/p95。额外记录“草稿错而终稿改对”和“草稿对而终稿改错”；只报终稿整体准确率会掩盖反向纠错。先固定开发集选参数，再以未调过的录音验收。

当前不能承诺某条路线“准确率提升 X%”或“全部在 X ms 完成”。文档里的模型吞吐、静音阈值、分包时长都不是该用户 Mac 上的完整延迟实测。

## 既有仓库依据

- `docs/research/2026-10-24-offline-mixed-asr-candidates.md`：本地模型范围、Mac 后端、许可边界和实验矩阵。
- `experiments/offline-asr/runs/quality-test-v1-q1/summary.json`：已有本机回放结果之一，完整对比见下文。README 引用的旧结果汇总文档缺失，本次使用实际 run 产物。
- `docs/research/2026-08-02-volcengine-doubao-streaming-asr-integration.md`：豆包接入方案，特别是句级 definite 与整次输入结束的区分，以及首版单遍决策。
- `docs/research/2026-08-02-cloud-asr-comparison.md`：历史云端调研，本次关键能力以以下重新核验的文档为准。

## Apple 基线与现有本地证据

仓库 iOS 的 `ios/App/ChatDictation.swift` 使用旧 `SFSpeechRecognizer(locale:)`，已开启 `shouldReportPartialResults`。因此“打开部分结果”本身不是新增优化。旧 API 的 `contextualStrings` 可放项目名、卡片名和短术语，官方建议不超过 100 条。[S15] 新 `SpeechTranscriber` 的上下文功能应按实际模块支持验证，不能直接照搬旧 API 能力。桌面代码的 `AppleSpeech` 枚举名不足以证明最终执行的是 Apple 识别器，应追踪 factory。

2026-10-24 的已有实验在 Apple M4 / 16 GiB 上离线回放 ASCEND 测试集。四组 metadata 的 manifest SHA-256、304 个样本 ID、评分脚本哈希与归一化规则一致。下表只取共同的 240 条中英混合子集，参考长度 3418 个混合 token。MER 将中文按字、英文按词计算编辑错误，越低越好；它不是“错句比例”。

| 运行与模型配置 | 中英混合 MER | 原始证据 |
|---|---:|---|
| B0：双语 Zipformer int8，未补尾 | 26.27% | `experiments/offline-asr/runs/quality-test-v1-b0/summary.json` |
| S1：同一 Zipformer，补尾 660 ms | 24.34% | `experiments/offline-asr/runs/quality-test-v1-s1/summary.json` |
| N1：SenseVoiceSmall int8，CPU，自动语言 | 13.46% | `experiments/offline-asr/runs/quality-test-v1-n1/summary.json` |
| Q1：Qwen3-ASR 0.6B，MLX 8bit / Metal，自动语言 | 11.56% | `experiments/offline-asr/runs/quality-test-v1-q1/summary.json` |

每组配置、模型哈希、离线探测和机器信息位于同目录的 metadata.json。共同 manifest SHA-256 为 `822e0aae37601c95a3c6570872a0a532d7c0938ccbed98390e87b934b89f68d8`；评分规范为 `nfkc-casefold-cjkchar-word-v1`。

证据边界：这是已有文件回放，不是本轮新实测，也不是用户真实指令集。没有 Apple 对照；未验证麦克风、VAD、首次显示和停口到终稿的 App 延迟。Q1 的此后端配置明确 `native_streaming=false`，因此不能把 Qwen 家族流式能力等同于本次 MLX 配置。Whisper 另有开发集结果，但样本不同，本表不混排。

这组结果支持优先测试 Qwen / SenseVoice 作为二遍候选，不支持宣布它们已胜过 Apple 或已解决卡片名称错误。

## 交互与验收落点

按 Apple 设计技能的即时反馈和可预测原则：说话即出草稿，原位修订尚未定稿的片段；显示“识别中 / 定稿中 / 已就绪”，用户始终可停止和取消。以上是方案，尚未修改 App 页面。不要让长时间定稿工作阻塞 GPUI 前台。

验收分别映射到：段 ID 与版本替换、取消后的迟到结果、最终只提交一次由 ASR / runtime 聚焦测试验证；英文术语和混合 MER 由同集录音回放验证；字幕稳定性、可取消和真实等待时间由 App 端定向验收验证。句级 final 不能代替整次输入结束。

本轮是对既有需求 #164 的识别侧研究补充，未新增已确定实施项；用户已撤回的话术精简问题不纳入范围。

## 官方来源

- [S1] 豆包双向流式 WebSocket API，含 `enable_nonstream`、句级二遍行为与 `definite`：https://docs.volcengine.com/docs/doubao-voice/bidirectional-streaming-automatic-speech-recognition-websocket?lang=zh
- [S2] FunASR 在线 SDK，含 `2pass` 语义与 Linux 部署边界：https://github.com/modelscope/FunASR/blob/main/runtime/docs/SDK_tutorial_online.md
- [S3] FunASR WebSocket 协议：https://github.com/modelscope/FunASR/blob/main/runtime/docs/websocket_protocol.md
- [S4] Sherpa 双语在线 Paraformer 模型：https://k2-fsa.github.io/sherpa/onnx/pretrained_models/online-paraformer/paraformer-models.html
- [S5] SenseVoice 官方仓库，含模型范围与分块 pseudo-streaming 说明：https://github.com/QwenAudio/SenseVoice
- [S6] whisper.cpp 官方仓库，含 Apple 加速与滚动实时例子：https://github.com/ggml-org/whisper.cpp
- [S7] Qwen3-ASR 官方仓库，含流式 vLLM 限制：https://github.com/QwenLM/Qwen3-ASR
- [S8] Sherpa 官方 Qwen3-ASR 平台与 API 支持：https://k2-fsa.github.io/sherpa/onnx/qwen3-asr/index.html
- [S9] Sherpa 官方 Qwen3-ASR 预训练模型和 VAD 模拟流式示例：https://k2-fsa.github.io/sherpa/onnx/qwen3-asr/pretrained.html
- [S10] 阿里云 Paraformer 服务端事件：https://help.aliyun.com/zh/model-studio/paraformer-server-events
- [S11] 阿里云 Paraformer 客户端参数：https://help.aliyun.com/zh/model-studio/paraformer-client-events
- [S13] Apple WWDC25 SpeechAnalyzer，含本机执行与模型管理：https://developer.apple.com/videos/play/wwdc2025/277/
- [S14] Apple SpeechTranscriber volatileResults：https://developer.apple.com/documentation/speech/speechtranscriber/reportingoption/volatileresults
- [S15] Apple SFSpeechRecognitionRequest contextualStrings：https://developer.apple.com/documentation/speech/sfspeechrecognitionrequest/contextualstrings
- [S12] Argmax 官方 Swift 仓库，含 WhisperKit 与 Pro 接口边界：https://github.com/argmaxinc/argmax-oss-swift

以上网页与默认分支会更新；执行对比前应固定模型版本、权重哈希、后端、量化、分句参数和 OS 环境。本文没有修改业务代码，也没有将候选能力登记为已实现功能。
