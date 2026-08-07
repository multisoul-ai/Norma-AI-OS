# Norma OS 新增本地中英 TTS 候选核验（Qwen3-TTS、ZipVoice、CosyVoice）

> 调研日期：2026-07-18。本文是对 [2026-07-16 候选调研](2026-07-16-local-tts-candidates.md) 的增量核验，不覆盖其中的 M4 实测。证据仅取自项目官方 GitHub、官方模型卡与 sherpa-onnx 官方发布物；官方延迟数字不等同于 Apple Silicon 实测。

## 结论

若要在现有 Rust + `sherpa-onnx` 链路中**新增**一个中英模型，优先做 **ZipVoice-Distill INT8 的受控 POC**：它是唯一同时具备中英、zero-shot 声音克隆、INT8 官方包、以及当前 `sherpa-onnx 1.13.x` Rust API 已有配置类型的候选。它仍是离线句级生成（不是 text-in/audio-out 双流），而且其官方权重卡声明训练数据为 Emilia，必须先完成权重与训练数据的商业分发审计。

如果“响应速度快”指的是 LLM 逐字输出时立刻开始说话，能力首选仍是 **CosyVoice 3 sidecar POC**（官方称双向流最低约 150 ms）；如果同时需要最高的声线/风格可控性，可评估 **Qwen3-TTS 0.6B CustomVoice**（官方称最低 97 ms）。两者都不是 sherpa-onnx/原生 Rust 模型，且没有官方 Apple Silicon 指标，因此不应取代已实测很快的 Matcha zh-en 默认路径。

| 候选 | 中英与能力 | 速度/规模（仅官方声明） | 许可与分发风险 | Norma OS 集成判断 |
|---|---|---|---|---|
| **ZipVoice-Distill INT8** | 中英、single-speaker zero-shot cloning；参考音频及其准确转写为必填输入 | ZipVoice 为 **123M 参数**；distill 版可从默认 8 步降至 4 步。sherpa 发布的 INT8 archive 为 **109,162,785 B**，另需 24 kHz Vocos **54,157,409 B**。官方说明 CPU 可用 ONNX + INT8 加速 | ZipVoice **代码仓库**为 **Apache-2.0**，但官方权重卡明确该 `zipvoice_distill` 权重训练自 **Emilia**，没有可随 sherpa release archive 一起核验的权重许可证材料；不能把代码许可外推为权重/训练数据的商业再分发许可 | **中（最值得 POC）**：本地 Rust binding 已提供 `OfflineTtsZipvoiceModelConfig`；需扩展 `TtsBackend`、安装清单与含 reference audio/text 的生成请求。官方 sherpa 示例明确为 non-streaming/离线 API，不承诺 text-in/audio-out 流式 |
| **CosyVoice 3 0.5B** | 中英在内 9 语种、跨语种 zero-shot cloning、文字输入与音频输出双流 | **0.5B**；官方称最低 **150 ms**，并提供 PyTorch/vLLM/Triton 路线。没有官方 Mac/MPS TTFA、RTF 或内存指标 | 代码仓库为 **Apache-2.0**；模型随具体模型卡和打包资产再核验。完整 snapshot 很大，旧调研记录为 9.75 GB，不适合作为常驻轻量包 | **高（独立 sidecar）**：官方安装为 Conda/Python，vLLM/TensorRT 优化偏 Linux/NVIDIA；适合“真实双流/克隆”专项，不适合直接塞入当前 Sherpa adapter |
| **Qwen3-TTS 0.6B CustomVoice** | 中文、英文等 10 语种；固定 premium voice，支持 streaming；1.7B 变体另有 voice design/克隆 | **0.6B** 轻量档；官方称双轨流式架构最低 **97 ms**，可单字符后输出首包。官方文档以 Python `qwen-tts`/Transformers 为主，FlashAttention 仅支持 FP16/BF16；未发布 Sherpa ONNX/INT8 或 Mac 实测 | 官方仓库标 **Apache-2.0**；产品打包前仍应逐一确认 tokenizer 和所选权重仓库的 LICENSE/NOTICE | **高（独立 Python 服务）**：无现成 Rust/sherpa 适配；vLLM-Omni 官方路径目前仍标为 offline inference，不应把其记为本地流式部署证据 |

## 对现有架构的具体影响

- 现有 `tts` crate 应继续只保存模型身份、文件契约和能力；ZipVoice 的 `OfflineTts` 构建及参考音频传入仍属于 `voice_sherpa`。这符合当前 `tts` 零 Sherpa、`voice_sherpa` 引擎适配的边界。
- `sherpa-onnx 1.13.3` 的本地 Rust crate 已包含 `OfflineTtsZipvoiceModelConfig`（`tokens`、`encoder`、`decoder`、`vocoder`、`data_dir`、`lexicon`）和带 `reference_audio`/`reference_text` 的 `GenerationConfig`；ZipVoice 不需要先升级整个推理引擎。
- 当前应用虽能走 `OfflineTts` 的 callback，但产品客户端仍没有把该回调接入播放器队列。因此“模型支持 callback/async API”不等于用户当前会听到流式首响；这项播放链路工作对 ZipVoice、CosyVoice 和 Qwen3 都独立存在。

## 推荐顺序

1. **保持 Matcha zh-en 为速度默认**：已有同机 M4 实测证据，新增候选不能替代它的确定性低延迟。
2. **ZipVoice-Distill INT8 做有商业许可 gate 的 POC**：验证真实 M4 的 TTFA、RTF、参考音频预处理成本、取消和峰值 RSS；尤其验收中英混读与克隆。
3. **仅在产品明确需要“边出字边说/克隆”时，CosyVoice 3 再立项**；Qwen3-TTS 作为相同级别但更重、能力更丰富的对照，不作为第一条 macOS 本地工程路径。

## 一手来源

- [ZipVoice 官方仓库](https://github.com/k2-fsa/ZipVoice)：123M、中英、zero-shot、distill、Apache-2.0、CPU ONNX/INT8 和 sherpa-onnx CPU 部署指向。
- [ZipVoice 官方模型卡](https://huggingface.co/k2-fsa/ZipVoice)：权重目录及 `zipvoice_distill` 的 Emilia 训练来源；权重许可证应在产品分发前另行取得/核验。
- [sherpa-onnx 官方 Node 示例](https://github.com/k2-fsa/sherpa-onnx/blob/master/nodejs-addon-examples/README.md)：ZipVoice zh-en INT8 archive、Vocos 依赖、async/sync 示例与 reference text 要求。
- [sherpa-onnx 官方 `tts-models` Release API](https://api.github.com/repos/k2-fsa/sherpa-onnx/releases/tags/tts-models) 与 [官方 `vocoder-models` Release API](https://api.github.com/repos/k2-fsa/sherpa-onnx/releases/tags/vocoder-models)：下载尺寸与固定 asset URL。
- [CosyVoice 官方仓库](https://github.com/FunAudioLLM/CosyVoice)：中英等 9 语种、bi-streaming/150 ms、0.5B 模型、Python/vLLM/Triton 路线和 Apache-2.0。
- [Qwen3-TTS 官方仓库](https://github.com/QwenLM/Qwen3-TTS)：0.6B/1.7B 型号、10 语种、streaming/97 ms、Python 运行时、vLLM-Omni 当前 offline 限制及 Apache-2.0。
- 本地已解析依赖：`/Users/alan/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/sherpa-onnx-1.13.3/src/tts.rs`，用于确认仓库锁定的 Rust binding 已提供 ZipVoice config 和请求级参考音频字段；该项为本地实现事实，不是上游发布承诺。
