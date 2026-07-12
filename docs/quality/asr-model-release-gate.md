# ASR 模型推荐发布门槛

本文记录 Paraformer Bilingual 与 Zipformer Streaming ZH 的推荐资格。模型出现在 Voice
Settings 中不等于获得“推荐”标记；推荐状态只能由可重复的中文识别质量与目标硬件性能证据推动。

## 当前结论（2026-07-11）

| 模型 | 可安装/选择 | 运行时适配 | 推荐 | 原因 |
|---|---:|---:|---:|---|
| Paraformer Bilingual | 是 | Online Paraformer | 否 | 作为迁移基线保留，尚未建立本轮统一评测数据 |
| Zipformer Streaming ZH | 是 | Online Transducer | 否 | 尚未完成固定中文语料与目标 Apple Silicon 性能评测 |

代码中的 `recommended = false` 是发布护栏，不得仅因为模型能够加载或单条语音能够识别就改为
`true`。

## 质量门槛

提交仓库内可分发的 16 kHz、mono、PCM16 中文命令 WAV 与 UTF-8 标注清单。至少覆盖：

- 创建、关闭、聚焦和输入等关键产品命令；
- 中英混合、数字、路径和同音词；
- 每条关键命令的具体正断言与易混淆命令的负断言。

同一份语料依次运行两种模型。Zipformer 必须满足：

1. 关键产品命令逐条全部正确；
2. 固定集准确率不低于 Paraformer；
3. 不得以删除标注、放宽到空文本或仅检查“有输出”通过门槛。

团队真实麦克风扩展集不提交个人原始录音，但报告必须记录录音设备、环境、样本数、标注和逐条
结果，确保另一名成员可以复测。

## 性能门槛

在实际支持的 Apple Silicon 机型分别冷启动、热启动并连续运行两种模型，记录：

- 模型加载耗时；
- 首个 partial 延迟与 final 延迟；
- 稳态 CPU；
- 峰值内存；
- 测试机型、macOS 版本、应用 commit、模型不可变 ID。

任何模型加载、解压、哈希、recognizer join 或文件 IO 都必须继续运行在后台线程；评测不得通过把
阻塞工作移回 GPUI 前台来换取更好数字。

## 真实模型冒烟

安装官方模型后，可运行现有忽略测试验证真实 Paraformer 链路：

```bash
cargo test -p voice_sherpa test_real_wav_reaches_final_transcript_and_thinking -- --ignored --nocapture
cargo test -p voice_sherpa test_zipformer_real_wav_produces_partial_and_final_transcripts -- --ignored --nocapture
```

Zipformer 推荐评测必须使用受管目录中的
`zipformer-streaming-zh-int8-2025-06-30`，并在报告中同时记录 partial 与 final。单元测试、
headless GPUI e2e 和真实麦克风/L3 证据必须分别报告，不能相互替代。

## 工件与许可

两种模型均从受控模型库中钉死的官方 URL 由用户主动下载，不打入 `.app` 或 DMG。Sherpa
代码库的许可证不能自动证明每个预训练模型工件的再分发权；在 Zipformer 工件许可明确前，不得
随安装包再分发其归档或解压文件。
