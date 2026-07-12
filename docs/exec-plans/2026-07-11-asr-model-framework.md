# ASR 模型框架与 Zipformer 分阶段实施计划

**Goal:** 先用 Paraformer 建立并验证可扩展 ASR 框架，再以数据驱动方式接入 Zipformer Streaming ZH。
**Design:** [`docs/design-docs/2026-07-11-asr-model-framework.md`](../design-docs/2026-07-11-asr-model-framework.md)

## 实施状态（2026-07-11）

- 阶段 1 已完成：零 GPUI `asr` crate、受控模型库、安装清单、校验与原子安装器已落地。
- 阶段 2 已完成：Paraformer 已迁入统一可用模型与 recognizer factory；后台模型管理、新
  Settings、听写/持续聆听模式及快捷键路由已落地。
- 阶段 3 已完成：Zipformer 固定工件、Transducer 配置、安装/选择/失败回退已落地；真实模型
  冒烟仍需本机下载官方模型后运行 ignored 测试。
- 阶段 4 尚未通过：Zipformer 保持未推荐；固定中文 WAV、团队麦克风数据和目标 Apple
  Silicon 性能结果按 [`docs/quality/asr-model-release-gate.md`](../quality/asr-model-release-gate.md)
  收集后，才能另行决定推荐状态。

## 全局约束

- 每个新功能切片必须按 Red → Green 执行，不批量先写所有测试。
- 单元测试必须包含场景、数据构造、执行过程和预期结果；每个断言带失败说明。
- `asr`、`dialogue`、`voice_sherpa` 保持零 GPUI；网络、解压、哈希、模型加载与 recognizer join 不进 GPUI 前台路径。
- 不升级 `gpui =0.2.2` 或 `sherpa-onnx =1.13.x`；Zipformer 使用现有 Sherpa Rust API。
- 不使用 `#[allow(...)]`，Rust 文件不超过 500 行。
- 不迁移旧 ASR/模式设置；保留 Agent/LLM 配置。
- 本计划不实现 TTS 模型管理、远程模型市场或非 Sherpa 引擎。

## 阶段 1：ASR 领域框架

### 1.1 建立 `crates/asr`

- [x] 先写失败测试：不可变 `AsrModelId` 可序列化并拒绝空值；两个不同版本 ID 不相等。
- [x] 新建 workspace member 和最小类型：`AsrModelId`、`AsrModelDescriptor`、`AsrCapabilities`、`AsrArtifact`。
- [x] 再写失败测试：受控模型库能按 ID 查询，未知 ID 返回明确错误。
- [x] 实现静态、随应用发布的模型库，首个条目只登记 Paraformer。

**Exit:** `cargo test -p asr` 通过；`asr` 不依赖 GPUI、Storage、Sherpa 或 Dialogue。

### 1.2 安装清单与可用性

- [x] 先写失败测试：清单 ID/哈希与模型库不匹配时不能产生 `AvailableAsrModel`。
- [x] 实现 `InstallationManifest`、`AvailableAsrModel` 与 `AsrInstallationState`。
- [x] 先写失败测试：缺少必要文件、清单损坏、目录不存在都不是 Ready。
- [x] 实现只从固定 Application Support 模型根目录解析安装，不全盘扫描。

**Exit:** 只有文件和清单完整的安装才会进入 Ready。

### 1.3 原子安装器

- [x] 先写失败测试：错误 SHA-256、归档路径穿越、缺少必要文件和中途失败不得覆盖 Ready 安装。
- [x] 以可注入的归档输入实现哈希、安全解压、文件清单校验、清单写入和原子 rename。
- [x] 安装器不发网络请求；app 后台层负责下载并把归档交给安装器。

**Exit:** 安装器的单元测试只使用临时归档，不依赖网络或真实模型。

## 阶段 2：Paraformer 完整纵向迁移

### 2.1 统一 Sherpa recognizer factory

- [x] 先写失败测试：Paraformer 安装要求 encoder/decoder/tokens，不要求 joiner。
- [x] 用一个 factory 替换 `asr_stream.rs` 和 `session.rs` 中重复的 builder。
- [x] 修改 `SherpaProvider`、`SherpaListeningPipeline` 和启动路径，接收 `AvailableAsrModel` 而不是裸 `Path`。
- [x] 保持 `dialogue::AsrClient` 的 PCM、partial、final 与 reset 合约不变。

**Exit:** Paraformer 通过新框架完成真实流式识别，无 Paraformer 专属路径进入 app Settings。

### 2.2 app 后台下载与模型管理状态

- [x] 使用本地假 HTTP server 先写下载成功、中断、重试与错误哈希测试。
- [x] 使用 workspace 已有 `ureq` + rustls 下载，所有阻塞工作放到 `background_executor`。
- [x] 建立 app 层 `AsrModelManagerState`，映射 NotInstalled / Installing / Ready / Failed 并序列化下载、删除和切换动作。
- [x] 当前模型禁止删除；失败安装不修改选择。

**Exit:** 下载/安装不阻塞 GPUI，错误可重试，重启后状态从安装清单恢复。

### 2.3 Voice Settings 状态与 UI

- [x] 先为 `VoiceInteractionMode::{Dictation, ContinuousListening}` 和 `DictationTrigger::{Hold, Toggle}` 写保存/默认/切换测试。
- [x] 新增 `voice.interaction.mode`、`voice.dictation.trigger`、`voice.asr.model_id`；删除 app 对旧 ASR/模式键的读写。
- [x] 使用 `ui::component` 中适用的 Select/Dropdown、Segmented Control、Switch/Button 构建模式选择和模型列表；实现前加载 `gpui-elements`、`gpui-actions`、`gpui-styling` 及本仓库 `gpui-component` 参考。
- [x] 同页保留 Agent/LLM/TTS，但按当前模式渐进展开；Advanced 默认折叠。
- [x] 模型列表显示语言、流式能力、真实下载体积、安装状态与推荐状态。
- [x] 无 Ready 模型时，`⌘⇧V` 显示可恢复提示，不启动麦克风。

**e2e:** 模式切换、听写触发、未安装模型、成功安装/选中、当前模型删除保护、Settings 重启恢复。

**Exit:** 只有 Paraformer 时，新 Settings 和 `⌘⇧V` 两种模式已完整工作，不依赖 Zipformer 专属逻辑。

## 阶段 3：Zipformer Streaming ZH

### 3.1 模型工件与 Transducer 适配

- [x] 从官方固定 release 获取 `sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30.tar.bz2`，记录精确 URL、归档字节数和 SHA-256。
- [x] 先写失败测试：Zipformer 必须要求 encoder/decoder/joiner/tokens，缺 joiner 不可 Ready。
- [x] 在受控模型库加入不可变 Zipformer 条目。
- [x] 在统一 Sherpa factory 中增加 `OnlineTransducerModelConfig`，保持 Paraformer arm 不变。
- [x] 用真实官方模型增加 ignored smoke test，验证 partial/final 文本。

**Exit:** 新增 Zipformer 仅修改模型库与 Sherpa factory，Settings 不增加专属分支。

### 3.2 模型切换与失败回退

- [x] 先写失败测试：活跃会话切换会停止旧会话，新模型加载失败时仍选中 Paraformer。
- [x] 切换时在后台释放/加载 recognizer，前台只更新模型状态和语音 UI。
- [ ] 补 L2/L3 e2e：Paraformer → Zipformer 成功，Zipformer 加载失败回退，重启后保留不可变 Zipformer ID（需要官方模型工件；当前由确定性 lower seam 与 ignored 真模型测试覆盖）。

**Exit:** 两模型均可安装、选择、重启恢复，切换不产生孤儿会话或无效选择。

## 阶段 4：质量、性能与发布门槛

- [ ] 建立仓库内小型可分发的中文命令 WAV+标注基线集，关键命令逐条正负断言。
- [ ] 建立团队真实麦克风扩展集，不必提交个人原始录音，但记录可重复评估结果。
- [ ] 比较 Paraformer/Zipformer 的关键命令准确率，Zipformer 不得低于 Paraformer，关键命令必须全部正确。
- [ ] 在目标 Apple Silicon 机型记录模型加载、首次 partial、final、CPU 和峰值内存。
- [ ] 只有质量与性能基线通过后，才在模型库把 Zipformer 标为推荐。
- [ ] 在发布文档说明模型来源与许可证不确定性；不将 Zipformer 归档打入 `.app`/DMG。

## 验证矩阵

```bash
cargo test -p asr
cargo test -p dialogue
cargo test -p voice_sherpa
cargo test -p app voice_settings
cargo test -p app app_dialogue
cargo test -p e2e
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

在阶段 3/4 的真模型验收中再运行被 `#[ignore]` 标记的官方模型和真麦克风测试，并在验收报告明确标注 L2/L3 层级。
