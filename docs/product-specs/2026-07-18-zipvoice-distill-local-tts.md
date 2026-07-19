# ZipVoice-Distill INT8 本地 TTS 集成规格

## 目标

在保留 Matcha zh-en 默认路径的前提下，将 `sherpa-onnx-zipvoice-distill-int8-zh-en-emilia` 作为实验性、用户主动下载的本地中英 TTS 模型接入 Norma OS。

## 产品约束

- 模型不随应用打包、不自动下载、不替换 Matcha 默认模型。
- 模型卡片明确标记 Experimental，并保留 Emilia 权重/训练数据分发审计门槛。
- ZipVoice 使用一个全局参考声音配置：16 kHz、单声道、PCM WAV，时长 3–15 秒，并要求用户填写与音频完全一致的转写。
- 用户可以通过 macOS 原生文件选择框选择 WAV，也可以直接输入本地路径。
- 输入路径时，只有按 Enter 或点击 Import 才读取与校验文件；普通输入过程不得访问文件系统。
- 导入成功后，将音频复制到 Norma OS 应用数据目录并保存转写；移除参考声音时同时删除复制的音频和转写。
- 参考声音缺失或无效时，禁止选择或试听 ZipVoice，并保持当前已选模型不变。
- 参考声音可以试听；更换声音必须回到 Settings 重新导入。
- v1 不增加声音授权确认框。

## 技术边界

- `tts`：声明模型身份、官方安装清单、实验性/参考声音能力，以及引擎无关的 WAV 校验与受控导入。
- `voice_sherpa`：构造 `OfflineTtsZipvoiceModelConfig`，把参考 PCM 与准确转写写入 `GenerationConfig`。
- `app`：在后台执行文件读取、复制、模型构建与试听；GPUI 前台只更新状态和渲染。
- Settings 使用任务型即时反馈，不增加全局 Save/Cancel。

## 验收标准

1. Local TTS 目录包含 ZipVoice-Distill INT8，下载包和 Vocos 均通过固定 SHA-256 校验。
2. 有效安装包含 `encoder.int8.onnx`、`decoder.int8.onnx`、`tokens.txt`、`lexicon.txt`、`espeak-ng-data` 和 `vocos_24khz.onnx`。
3. 参考 WAV 的格式、声道、采样率、时长或转写不合规时，导入失败且不得覆盖已有有效配置。
4. 原生选择框与路径输入都能产生同一导入流程；路径输入仅在 Enter/Import 时触发文件读取。
5. 缺少参考声音时选择 ZipVoice 失败，原有模型/Voice 选择保持不变；导入成功后可选择、试听和合成。
6. 移除参考声音后，受控副本和转写均消失，ZipVoice 立即变为不可选择。
7. M4/32 GB 热状态性能门槛为 P95 首个 PCM 小于 1 秒、RTF 小于 1、额外峰值内存小于 1 GB；未达到前继续保留 Experimental 标记。

## 当前 Gate 证据（2026-07-18）

- M4/32 GB 本机实测完整 PCM：中文约 1.82 秒、RTF 约 1.83；英文约 2.01 秒、RTF 约 1.18。当前 sherpa adapter 非流式，因此首个 PCM 等于完整 PCM，未达到 `<1 s` 与 `RTF <1`。
- `/usr/bin/time -l` 观测进程总峰值 RSS 约 545 MB；该数值不是严格的“额外峰值”，不能据此关闭内存 gate。
- Emilia 权重与训练数据的再分发审计尚未形成批准记录。
- 目录通过 `M4PerformancePending` 与 `EmiliaDistributionAuditPending` 两个显式 gate 推导 Experimental；任何一个 gate 未关闭都不得转为稳定模型。
