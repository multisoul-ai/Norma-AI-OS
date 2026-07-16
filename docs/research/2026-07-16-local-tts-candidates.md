# Norma OS 本地中英 TTS 候选调研

> 调研日期：2026-07-16  
> 证据边界：只使用项目官方 GitHub/文档、官方模型卡与论文。厂商或作者自报速度均保留原测试硬件，不外推为 Apple Silicon 实测。  
> 实测环境：Apple M4、32 GB、macOS 15.6.1；Rust release build；`sherpa-onnx 1.13.3`；CPU provider、2 threads。本文没有把自动 ASR 回环当成人类 MOS。

## 结论先行

**技术首选改为 `sherpa-onnx + matcha-icefall-zh-en`。** 它和当前 Matcha loader 是同一后端，单模型支持中英混读；在目标 M4 上四类样本的完整合成 RTF 为 **0.061–0.078**，首个 PCM 回调约 **69–159 ms**，冷加载约 **0.84 s**。这比 Kokoro int8 的 RTF **1.33–1.72** 快约 17–27 倍，也比 Melo ONNX 的 RTF **0.61–0.72** 快约 8–10 倍。模型加 vocoder 下载约 126.7 MiB，接入只需新增 descriptor、双语 Matcha 配置和安装清单。

这个技术推荐有一个发布前置条件：上游 ModelScope 页面把权重标为 Apache-2.0，但 sherpa release 归档本身没有 `LICENSE`，模型卡也没有披露混合训练语料来源。**先向模型作者确认权重再分发与训练数据商业使用边界，并把许可证/NOTICE 随模型包固定下来。** 在合规确认完成前，它是最佳工程 POC，而不是已证明可商业发布的默认模型。

第二顺位是 **sherpa-onnx VITS-Melo `zh_en`**：M4 上已经实时（RTF 0.61–0.72），MIT 许可更清楚，但首段回调约 0.35–1.45 s、单 speaker，且英文仅限词典内词；官方包中的 `model.int8.onnx` 还是 133 字节 Git LFS 指针，实际只能使用 163 MiB FP32 模型。Kokoro 保留为“多 voice + Apache 权重”的功能候选，但当前 sherpa CPU 路径在 M4 上不实时，不能作为低延迟默认。

**需要声音克隆或真正 text-in/audio-out 双流时，单独做 CosyVoice 3 sidecar POC。** 它的能力最贴近下一代语音 Agent，但官方低延迟证据不是 Apple Silicon 数据，官方部署路径明显偏 Python/Linux/NVIDIA，完整模型仓库也远大于轻量 ONNX 路线，不应直接成为首发默认。

IndexTTS2 与 Fish Speech/S2 Pro 更适合做高质量上限对照：前者 5.9 GB、无官方流式或 Mac 指标且使用自定义模型许可证；后者官方本地推理建议至少 24 GB VRAM，当前许可证要求商业使用另行授权。两者都不符合“很快接入当前 macOS App”。

## 决策表

评分中的“接入”只表示相对当前代码库的工程改动量，不代表音质。

| 候选 | 中英 | M4 实测（完整 RTF / 首 PCM） | 克隆 | 模型体积 | 许可证 | 当前接入复杂度 | 建议 |
|---|---|---|---|---:|---|---|---|
| **Matcha `zh-en`** | **是，同段混读**；单女声 | **0.061–0.078 / 69–159 ms**；加载 0.84 s；峰值 RSS 284 MiB | 否 | archive 79.0 MB + vocoder 53.9 MB | ModelScope 标 Apache-2.0；**归档缺 LICENSE、语料来源未披露** | **低**：复用现有 Matcha backend | **技术首选；合规确认后发布** |
| **VITS-Melo `zh_en`** | 是；单女声；英文限词典 | **0.61–0.72 / 0.35–1.45 s**；加载 1.70 s；峰值 RSS 416 MiB | 否 | archive 167.0 MB；FP32 ONNX 170.4 MB | MIT | 低到中：新增 VITS backend | **许可清楚的第二选择** |
| **Kokoro v1.1 int8** | **是，同段混读**；103 voices | **1.33–1.72 / 1.66 s–整段完成**；加载 0.93 s；峰值 RSS 494 MiB | 否；固定 voice bank | archive 147.0 MB；已安装 206 MiB | Apache-2.0；需审计前端 NOTICE | **最低**：loader 已有 | **功能合格但当前不实时；不作默认** |
| **MeloTTS 原生 Python** | 是；多个英文 accent | 未安装：官方虽列 `mps`，完整依赖还含 PyTorch、Transformers、中文 RoBERTa | 否 | 208 MB checkpoint + 大量运行依赖 | MIT | 中到高：Python sidecar/打包 | **不如 ONNX 版** |
| **CosyVoice 3** | 是；9 种语言、中文方言 | 未安装：官方路径偏 Linux/NVIDIA；无 Apple Silicon 指标 | **是，跨语言 zero-shot**；真正 bi-streaming | HF snapshot **9.75 GB** | Apache-2.0 | **高**：独立 sidecar | **克隆/双流 POC 首选** |
| **IndexTTS2** | 是 | 未安装：无 Mac/MPS 或 RTF 证据 | **是**；情绪可控 | **5.9 GB** | 自定义 bilibili Model Use License | 高 | **排除快速接入** |
| **Fish Audio S2 Pro** | 是；80+ languages | 未安装：官方建议至少 **24 GB VRAM**，H200 RTF 不可外推 | **是**；真流式 | **11 GB** | 商业需另行书面授权 | 极高 | **排除本地 Mac 默认** |
| **现有 Matcha zh-baker** | **否，仅中文** | **0.077–0.106 / 未测 callback**；加载 0.86 s；峰值 RSS 280 MiB | 否 | 模型 + vocoder 约 123.4 MiB | **DataBaker 仅限非商业** | 已接入 | **仅研发速度基线** |

## M4 实测方法与结果

所有候选使用同一份 Rust probe，直接调用当前项目依赖版本 `sherpa-onnx 1.13.3`；release build、CPU provider、2 threads。每条文本连续合成 3 次，表中取 elapsed 中位数；`RTF = 合成耗时 / 音频时长`，小于 1 才能边生成边实时播放。首 PCM 是 sherpa progress callback 第一次返回的墙钟时间，不等于扬声器真正首响。

| 模型 | 中文短句 | 中文中句 | 英文短句 | 中英混合 |
|---|---:|---:|---:|---:|
| **Matcha zh-en：RTF** | **0.078** | **0.061** | **0.070** | **0.062** |
| **Matcha zh-en：首 PCM** | **76 ms** | **137 ms** | **69 ms** | **159 ms** |
| Melo ONNX：RTF | 0.724 | 0.628 | 0.613 | 0.631 |
| Melo ONNX：首 PCM | 408 ms | 1.23 s | 351 ms | 1.45 s |
| Kokoro int8 sid 0：RTF | 1.723 | 1.433 | 1.332 | 1.658 |
| Kokoro int8 sid 3：RTF | 2.127 | 1.557 | 1.509 | 1.505 |

额外观察：

- Kokoro 把线程从 2 调到 4 只小幅改善英文，8 threads 反而退化；不是简单调线程即可修复。
- Melo 官方 archive 的 `model.int8.onnx` 是 133 字节 Git LFS pointer，用它构造 sherpa engine 会触发 native abort；本次只认可 `model.onnx` FP32 数据。
- 生成 WAV 再送入仓库现有 Paraformer 中英 ASR 做回环，四个模型都能识别出主要语义，但都有专名、尾字或英文错误。这个检查只能排除明显乱码，不能替代人类盲听；试听文件位于 `target/tts-benchmark/{matcha-zh-en,melo-model.onnx,kokoro-sid3}/`。
- 当前产品 `TtsClient::synthesize` 仍传 `callback=None`，所以即使 Matcha 首 PCM 在 100 ms 量级，现有 UI 也会等待完整句子（约 0.28–0.39 s）才拿到 PCM。接入时应把 callback 真正桥接到流式播放队列。

## 与当前仓库的适配事实

当前设计要求 TTS 领域层零 GPUI、Sherpa 适配留在 `voice_sherpa`，且模型通过受控目录、固定 URL/SHA-256 和 manifest 安装；这使 sherpa-onnx 模型天然比 Python 大模型 sidecar 更快落地。[本仓库 Local TTS SPEC](../product-specs/2026-07-12-SPEC-local-tts-model-library.md)

当前代码已有：

- `TtsBackend::{Matcha, Kokoro}` 与两个内置模型 descriptor；Kokoro 目录已经固定到 `kokoro-int8-multi-lang-v1_1.tar.bz2`，并记录了精确大小与 SHA-256。[catalog.rs](../../crates/tts/src/catalog.rs)
- `SherpaTtsClient` 已经能构造 `OfflineTtsKokoroModelConfig`，使用 Rust `sherpa-onnx = "1.13"`、2 个 CPU 线程；这不是“从零接入”。[tts.rs](../../crates/voice_sherpa/src/tts.rs) / [Cargo.toml](../../crates/voice_sherpa/Cargo.toml)
- 当前 `TtsClient::synthesize` 给 sherpa 的 callback 是 `None`，所以产品路径等待 `GeneratedAudio` 后一次性转 PCM；官方 API 虽能回调生成中的 samples，当前仓库还没有利用它实现 audio-out 增量。[tts.rs](../../crates/voice_sherpa/src/tts.rs)
- 采样率校验已放宽至 8–192 kHz，因此 Melo 转换版的 44.1 kHz 不再被采样率护栏直接挡住。[tts.rs](../../crates/voice_sherpa/src/tts.rs)
- Kokoro descriptor 的安装必需文件只列出 `model.int8.onnx`、`voices.bin`、`tokens.txt` 和 `lexicon-zh.txt`，但 loader 还会无条件读取 `lexicon-us-en.txt` 与 `espeak-ng-data`。完整官方归档正常时可以加载，但当前完整性校验不能证明运行时所需文件齐全；正式 catalog 应让 required-files 与 loader 完全一致。[catalog.rs](../../crates/tts/src/catalog.rs) / [tts.rs](../../crates/voice_sherpa/src/tts.rs)

### 接入 Kokoro 前必须修的 metadata 问题

本地目录把 v1.1 int8 的 speaker 0/1 标成 `af_heart` / `af_bella`，但 sherpa-onnx 官方 v1.1 页面明确给出：

- sid 0 = `af_maple`
- sid 1 = `af_sol`
- sid 2 = `bf_vale`
- sid 3–57 = 中文女声 `zf_*`
- sid 58–102 = 中文男声 `zm_*`

因此本地标签和权重实际 speaker **不一致**，而且当前只暴露两个英文 speaker，不足以兑现“中文多 Voice”。修复时应该从官方映射生成/固定完整 Voice 清单，并选择实际中文 sid（例如首个 `zf_001` = sid 3）作为中文推荐 Voice。[sherpa-onnx v1.1 模型页](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html)

### 当前 Matcha 默认的发布风险

sherpa-onnx 官方 Matcha 页面明确警告 `matcha-icefall-zh-baker` 使用的 DataBaker 数据集仅允许非商业使用。即使 sherpa-onnx 代码是 Apache-2.0，也不能据此把该权重推定为商业可发布。当前内置目录把 Matcha 标成推荐模型，因此在确定产品发布许可前，应把它视为研发基线而不是 release catalog 默认。[官方 Matcha 模型页](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/matcha.html)

## 候选详查

### 1. sherpa-onnx Matcha zh-en：实测最快、接入最短，但先补合规证据

sherpa-onnx 的官方模型页确认 `matcha-icefall-zh-en` 支持中文和英文、单 speaker、16 kHz，并提供 Rust API；包内仍是 `model-steps-3.onnx + Vocos`，因此可以复用当前 `OfflineTtsMatchaModelConfig`，只需把前端改为 `lexicon.txt + espeak-ng-data + *-zh.fst`。[官方模型页](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/matcha-icefall-zh-en.html)

模型 archive 为 79,033,838 bytes，Vocos 为 53,882,848 bytes；本次实测 SHA-256 分别为 `271b804af570400d3bcdcb53bf6e53cc9f75180ee763b9f13eb5eaf2b0d086ef` 与 `b599142a1fb8ff03de3e84ac35ff537c619e56f4267a6fe894851a42844acf9e`。目标 M4 的完整 RTF 0.061–0.078、首 PCM 69–159 ms，是本文唯一同时满足“中英、离线、快速、低改动”的候选。

上游 [ModelScope 模型页](https://modelscope.cn/models/dengcunqin/matcha_tts_zh_en) 标注 Apache-2.0，但 sherpa 下载包没有 `LICENSE`，页面只写“中英文混合数据”，没有披露语料来源或授权链。它和 DataBaker-only 的现有 Matcha 不是同一个 checkpoint，不能直接沿用后者的“必然非商业”判断；同样也不能仅凭页面 badge 宣称训练数据可商业使用。发布前需要作者书面确认与归档内许可证。

### 2. sherpa-onnx Kokoro v1.1：代码已接，但实测不实时

官方 `kokoro-multi-lang-v1_1` 页面给出中英混合示例、103 speakers 和 24 kHz 输出；其源权重 `Kokoro-82M-v1.1-zh` 为 82M 参数，官方模型卡标注 Apache-2.0、English + Chinese、103 voices。[sherpa-onnx 模型页](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html) / [Kokoro 官方模型卡](https://huggingface.co/hexgrad/Kokoro-82M-v1.1-zh)

sherpa-onnx 提供 Rust API 示例，且 TTS FAQ 明确列出 macOS arm64（M1/M2）支持；框架跨平台说明也列出 Rust、macOS、iOS 与 arm64 CPU。[Rust Kokoro 示例](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html#rust-api) / [TTS FAQ](https://k2-fsa.github.io/sherpa/onnx/tts/faq.html) / [平台矩阵](https://k2-fsa.github.io/sherpa/intro.html)

体积以当前内置 int8 release asset 为准：GitHub Release API 返回 147,031,220 bytes；同版非量化包为 364,816,464 bytes。官方页面同时列出 int8 与非量化包，但官方 RTF 汇总表只给出 311 MB 模型项，未明确把数字标成 int8，因此不能把树莓派 RTF直接写成 int8 性能。[GitHub 官方 release API](https://api.github.com/repos/k2-fsa/sherpa-onnx/releases/tags/tts-models) / [Kokoro 模型列表](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html) / [官方 RTF 表](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/rtf.html)

限制：Kokoro v1.1 是固定 voice bank，不提供输入参考音频的 zero-shot cloning；`OfflineTts` callback 能早于整段完成返回 samples，但没有 CosyVoice 那种同一生成上下文里的增量文本输入。本次 M4 实测已证明当前 CPU adapter 下整段 RTF > 1，英文甚至到生成完成时才触发第一次 callback，因此它不满足 Norma 的低延迟默认要求。

### 3. MeloTTS：ONNX 可实时，但明显慢于 Matcha zh-en

MeloTTS 官方 README/安装文档称支持中文（夹英文）与多种英文口音，CPU 足以实时推理，并允许 `cpu/cuda/mps`；官方中文模型卡标注 MIT、checkpoint 208 MB。[官方仓库](https://github.com/myshell-ai/MeloTTS) / [安装与 API](https://github.com/myshell-ai/MeloTTS/blob/main/docs/install.md) / [中文模型卡](https://huggingface.co/myshell-ai/MeloTTS-Chinese)

但原生路径意味着 Python 3.9、PyTorch、Transformers、语言前端与额外 BERT；官方中文前端代码会加载 `hfl/chinese-roberta-wwm-ext-large`，所以“208 MB”不是完整安装大小。[官方中文 BERT 前端](https://github.com/myshell-ai/MeloTTS/blob/main/melo/text/chinese_bert.py)

sherpa-onnx 已发布 `vits-melo-tts-zh_en`：单 speaker、中文 + 英文、44.1 kHz、163 MB ONNX。它能复用现有运行时，但官方明确指出英文只能读 `lexicon.txt` 已包含的词。本次 M4 实测 RTF 0.61–0.72，确实达到实时，却比 Matcha zh-en 慢一个数量级；加上 int8 文件失效问题，只适合作为许可证更清楚的 fallback。[sherpa VITS/Melo 官方页](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/vits.html#vits-melo-tts-zh-en-chinese-english-1-speaker)

### 4. CosyVoice 3：能力最完整，工程上是 sidecar 项目

官方 README 明确列出中英在内的 9 种语言、跨语言 zero-shot voice cloning、text-in/audio-out bi-streaming 和最低约 150 ms 延迟；这让它成为“克隆 + 真双流”的首选 POC。[官方仓库](https://github.com/FunAudioLLM/CosyVoice)

但是官方安装依赖 Conda/Python，推荐 vLLM 0.9/0.11；生产部署示例使用 Docker + NVIDIA runtime，TensorRT-LLM 加速也绑定 NVIDIA。可选 `ttsfrd` wheel 在 README 中是 `linux_x86_64`，其他平台回退 WeText。官方没有给 Apple Silicon TTFA、RTF 或内存数据。[官方安装/部署段落](https://github.com/FunAudioLLM/CosyVoice#install)

模型卡标注 Apache-2.0；完整 HF snapshot 为 9.75 GB，包含 `llm.pt` 2.02 GB、`flow.pt` 1.33 GB、`hift.pt` 83.2 MB、两个约 969 MB 的 speech tokenizer，以及普通/RL、PyTorch/ONNX 的重复选择。集成前应先固定最小运行文件集，而不是直接把整仓作为模型包。[官方模型文件树](https://huggingface.co/FunAudioLLM/Fun-CosyVoice3-0.5B-2512/tree/main) / [代码许可证](https://github.com/FunAudioLLM/CosyVoice/blob/main/LICENSE)

### 5. IndexTTS2：声音克隆强，但不符合快速接入

官方仓库提供单参考音频克隆、独立情绪参考/向量/文本控制，官方模型卡标注 English 与 Chinese。它没有在 README 中提供流式接口、TTFA 或 RTF；官方加速项围绕 FP16、DeepSpeed、flash-attn、Triton/`torch.compile` 与 CUDA，且没有 Apple Silicon 基准。[官方仓库与 Python 示例](https://github.com/index-tts/index-tts) / [官方模型卡与文件树](https://huggingface.co/IndexTeam/IndexTTS-2/tree/main)

模型 snapshot 5.9 GB，另有启动时自动下载的小模型。许可证是 bilibili 自定义模型使用协议，不是 Apache/MIT；尤其要按中文版本评估商业阈值、下游约束、禁止用途和争议解决，不能只看英文摘要。[官方中文许可证](https://huggingface.co/IndexTeam/IndexTTS-2/blob/main/LICENSE_ZH.txt)

### 6. Fish Speech：速度证据与目标硬件不匹配，许可也阻断默认集成

当前主线是 Fish Audio S2 Pro。官方模型卡给出中英在内的 80+ 语言、10–30 秒参考音频 voice cloning、SGLang streaming；RTF 0.195 与 TTFA 约 100 ms是在单张 NVIDIA H200 上测得。官方本地推理文档直接建议至少 24 GB VRAM，目标 Apple Silicon Mac 没有对应证据。[S2 Pro 模型卡](https://huggingface.co/fishaudio/s2-pro) / [官方本地推理文档](https://github.com/fishaudio/fish-speech/blob/main/docs/en/inference.md)

完整 snapshot 11 GB。当前 Fish Audio Research License 只免费允许研究/非商业用途，任何商业用途要求单独书面授权，并要求分发 NOTICE 与展示 `Built with Fish Audio`。因此即使硬件问题解决，也不能直接进入产品 release catalog。[S2 Pro 文件树](https://huggingface.co/fishaudio/s2-pro/tree/main) / [官方许可证](https://github.com/fishaudio/fish-speech/blob/main/LICENSE)

旧 `fish-speech-1.5` 较小（1.47 GB）且支持中英，但模型卡是 CC-BY-NC-SA-4.0，仍不适合商业默认，并且不再代表当前官方主线。[1.5 官方模型卡](https://huggingface.co/fishaudio/fish-speech-1.5)

## 决策前还缺的证据

性能 shortlist 已经由同机实测确定，剩余工作不应再是盲目装更多模型，而是：

1. **Matcha zh-en 许可证 gate**：向作者确认模型权重可再分发、混合训练语料可用于商业产品，并取得可随 archive 保存的 LICENSE/NOTICE；无法确认就回退 Melo ONNX。
2. **人类盲听 gate**：让至少 3 人随机盲听 Matcha zh-en、Melo、Kokoro 的 WAV，评分中文自然度、英文可懂度、中英切换、专名/数字与机械感。ASR 回环不能替代这一步。
3. **产品链路 gate**：把 sherpa callback 接入现有 PCM 队列后测真实 speaker TTFA P50/P95、打断到静音、连续 30 轮、峰值 RSS；当前表只测 engine，不包含 UI/播放器调度。
4. **Melo OOV gate**：若 Matcha 合规失败，重点测 `Norma`、`Codex`、文件路径、版本号与用户项目名等词典外英文；OOV 明显则需要维护 lexicon 或重新评估 Kokoro 的其他后端。
5. **克隆单独立项**：只有 voice cloning 成为近期必须项时才做 CosyVoice 3 sidecar POC；先在 Apple Silicon 证明 RTF < 1 和稳定取消，再接 GPUI。

## 最终推荐顺序

1. **技术首选：Matcha `zh-en` via sherpa-onnx** — 中英、离线、M4 RTF 0.06–0.08、首 PCM 约 0.07–0.16 s、复用现有 backend；**以作者补齐商业语料与再分发确认为前置条件**。
2. **合规 fallback：VITS-Melo `zh_en` via sherpa-onnx** — MIT、M4 可实时，但慢约一个数量级且英文有词典边界。
3. **多 voice 备选：Kokoro v1.1 int8** — Apache-2.0、103 voices、loader 已有，但当前 M4/sherpa CPU 路径 RTF > 1；除非换后端或性能优化证明有效，不作为默认。
4. **能力升级 POC：CosyVoice 3 sidecar** — 只有声音克隆/真双流成为近期硬需求时投入。
5. **不进入快速接入：IndexTTS2、Fish S2 Pro/Fish Speech 1.5** — 体积、目标硬件或许可证至少一项明显不合格。
6. **现有 Matcha zh-baker 仅作研发基线** — 快，但缺英文且 DataBaker 数据仅限非商业。

## 一手来源索引

以下来源均于 2026-07-16 访问：

- sherpa-onnx：[平台矩阵](https://k2-fsa.github.io/sherpa/intro.html)、[TTS FAQ/macOS arm64](https://k2-fsa.github.io/sherpa/onnx/tts/faq.html)、[Matcha zh-en 模型与 Rust API](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/matcha-icefall-zh-en.html)、[Kokoro](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html)、[Kokoro v1.1 样例与 API](https://k2-fsa.github.io/sherpa/onnx/tts/all/Chinese-English/kokoro-multi-lang-v1_1.html)、[VITS/Melo](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/vits.html)、[Matcha](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/matcha.html)、[RTF 表](https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/rtf.html)、[release assets API](https://api.github.com/repos/k2-fsa/sherpa-onnx/releases/tags/tts-models)、[Apache-2.0 LICENSE](https://github.com/k2-fsa/sherpa-onnx/blob/master/LICENSE)
- Matcha zh-en 上游：[ModelScope 模型页（Apache-2.0 badge，但语料来源未披露）](https://modelscope.cn/models/dengcunqin/matcha_tts_zh_en)
- Kokoro：[v1.1-zh 官方模型卡](https://huggingface.co/hexgrad/Kokoro-82M-v1.1-zh)
- MeloTTS：[官方仓库](https://github.com/myshell-ai/MeloTTS)、[macOS/API 安装文档](https://github.com/myshell-ai/MeloTTS/blob/main/docs/install.md)、[MIT 中文模型卡](https://huggingface.co/myshell-ai/MeloTTS-Chinese)、[中文 BERT 前端](https://github.com/myshell-ai/MeloTTS/blob/main/melo/text/chinese_bert.py)
- CosyVoice：[官方仓库](https://github.com/FunAudioLLM/CosyVoice)、[Apache-2.0 模型文件树](https://huggingface.co/FunAudioLLM/Fun-CosyVoice3-0.5B-2512/tree/main)、[代码许可证](https://github.com/FunAudioLLM/CosyVoice/blob/main/LICENSE)
- IndexTTS2：[官方仓库](https://github.com/index-tts/index-tts)、[模型文件树](https://huggingface.co/IndexTeam/IndexTTS-2/tree/main)、[中文许可证](https://huggingface.co/IndexTeam/IndexTTS-2/blob/main/LICENSE_ZH.txt)
- Fish Audio：[S2 Pro 模型卡](https://huggingface.co/fishaudio/s2-pro)、[文件树](https://huggingface.co/fishaudio/s2-pro/tree/main)、[本地推理文档](https://github.com/fishaudio/fish-speech/blob/main/docs/en/inference.md)、[Fish Audio Research License](https://github.com/fishaudio/fish-speech/blob/main/LICENSE)、[Fish Speech 1.5 模型卡](https://huggingface.co/fishaudio/fish-speech-1.5)
