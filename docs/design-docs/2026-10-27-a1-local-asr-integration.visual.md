---
title: A1 如何接入 Norma
subtitle: 本地双遍识别 · 接口、数据流与资源生命周期
template: doc
theme: blueprint
lang: zh
---

A1 作为可选本地 provider 接入现有识别管线。Zipformer 负责预览，Qwen 负责唯一终稿。

## A 接入现有代码

图中的加号表示拟新增。现有默认路径继续保留。

```flow TB
语音设置 -> Profile解析
Profile解析 -> Factory注册表
Factory注册表 -> 现有Sherpa识别器: 原默认路径
Factory注册表 -> 录音会话: 返回统一接口
录音会话 -> 现有RecognitionPipeline: 后台执行
+ Factory注册表 -> A1RecognizerFactory: 选择精准模式
+ A1RecognizerFactory -> A1Recognizer: 创建录音句柄
+ 现有RecognitionPipeline -> A1Recognizer: PCM与Finish
+ A1Recognizer -> 共享A1宿主: 取得本轮租约
+ 共享A1宿主 -> 本地A1worker: 有界管道
```

`RecognizerFactory` 和 `Recognizer` 已存在。新增 A1 实现即可进入当前录音入口。

宿主跨窗口共享。每轮 factory 只获取句柄，不重复加载模型。

## B 一段录音怎样变成终稿

```flow TB
现有麦克风与PCM管线 -> *新增A1适配器: 后台接收音频
新增A1适配器 -> Zipformer预览进程: 实时送入
新增A1适配器 -> 有界原始音频缓存: 最多60秒
Zipformer预览进程 -> 只更新预览字幕: Partial
有界原始音频缓存 -> Qwen终稿进程: 结束输入后识别
Qwen终稿进程 -> *会话校验后提交一次: 正常Final
```

Zipformer 为 INT8，使用 CPU。Qwen 为 0.6B MLX 8-bit，使用 Metal。

660ms 静音只补到 Zipformer 尾部。Qwen 使用原始音频，不接收这段补尾。

图内推理通过本地 worker 执行。空结果、取消与故障按 F 区规则处理。

## C 从开始录音到提交的时序

```sequence num
participants: 用户, App, ASR适配器, A1宿主
用户 -> App: 选择精准模式
App -> A1宿主: 后台校验并准备模型
A1宿主 --> App: Ready
用户 -> App: 开始录音
App -> ASR适配器: 创建唯一会话
ASR适配器 -> A1宿主: Begin与连续音频
A1宿主 --> ASR适配器: Partial预览
ASR适配器 --> App: 更新本轮字幕
用户 -> App: 松开快捷键
App -> ASR适配器: 停止采集并排空音频
ASR适配器 -> A1宿主: Finish一次
A1宿主 -> A1宿主: Qwen识别原始录音
A1宿主 --> ASR适配器: 终稿与结果来源
ASR适配器 --> App: 本轮唯一Final
App -> App: 检查录音代次后提交一次
note App, A1宿主: 模型加载、推理、等待与回收均在后台
```

预览不启动 Agent。正常终稿确认后才提交任务。

## D 内存如何收口

```flow LR
未加载 -> 准备中: 选择A1且压力正常
准备中 -> 就绪: 固定工件加载成功
就绪 -> 本轮识别: 独占租约
本轮识别 -> 就绪: 完成本轮并释放音频
本轮识别 -> 回收中: 取消或故障
就绪 -> 回收中: 切换模式或空闲卸载
准备中 -> 回收中: 准备失败
回收中 -> 未加载: 停止自有进程并wait
```

- App 内共享一套 A1，终稿并发为 1。
- 多轮复用模型，不积压录音任务。
- 压力异常或 App 退出时取消并回收。
- 空闲卸载期限由资源补验确定。
- 音频缓存上限不等于模型运行内存。

## E 具体改哪些文件

| 改动 | 代码落点 | 作用 |
|---|---|---|
| 复用接口 | `crates/asr/src/recognition.rs` | PCM、取消、预览与终态 |
| 扩展注册 | `crates/asr/src/providers/mod.rs` | LocalA1与factory |
| 拟新增适配 | `crates/asr/src/providers/local/a1/mod.rs` | A1Recognizer |
| 拟新增宿主 | `crates/asr/src/providers/local/a1/host.rs` | 共享模型、独占与回收 |
| 拟新增worker | `tools/local-asr-a1/worker.py` | 固定实验引擎的协议入口 |
| 扩展设置 | `crates/app/src/settings/voice/state/asr_provider.rs` | 配置解析与就绪校验 |
| 扩展录音 | `crates/app/src/app/voice/mod.rs` | 预览代次检查与后台收尾 |

完整文件清单保存在文字方案中。新增类型与文件当前尚未实现。

## F 失败时不误提交

| 情况 | 应有行为 |
|---|---|
| Qwen正常终稿 | 只提交一次 |
| Qwen正常空结果 | 保持Empty，不用预览填充 |
| 超时、崩溃或实验回退 | 标记失败，保留预览，不自动执行 |
| 用户取消或切换模式 | 使会话失效，丢弃迟到结果 |
| 内存压力异常 | 停止A1并回收自有模型进程 |
| 工件缺失或版本不符 | 保持未就绪，说明原因 |

10秒是实验故障预算，不是正常等待目标。降级提交策略属于新增行为，需要单独验收。

## G 最小实施顺序

```timeline v
1 | 适配与假worker测试 | 先验证协议、取消、唯一终态和回收
2 | 原子集一致性检查 | 固定模型，逐条对照已有转写
3 | 真实App单次录音 | 验证麦克风、字幕、终稿、多轮与释放
4 | 开放可选精准模式 | 补齐资源与发布包验收，默认保持不变
```

首批只接快捷键单次录音。连续监听需单独检查 VAD 与音频积压。

延迟先验收6条录音，每条2次。中文、英文、句内混合各2条。

首次加载单列，小样本不报正式P95排名。有具体异常才补测。

负载执行仍需明确时间窗口。本页生成不启动模型或资源测试。

## H 已有收益与待验收项

| 项目 | 当前证据 |
|---|---|
| A1整体错误率 | 11.23%，旧方案25.97% |
| 相对旧方案 | 错误减少56.75% |
| 样本范围 | 304条，2位说话人，3936个参考单位 |
| A1历史采样RSS峰值 | 1.947GiB，不能替代同条件资源验收 |
| 真实App延迟与资源释放 | 尚未验收 |
| 产品接入状态 | 仅设计，未实现 |

错误率按中文字符与英文单词计。它不是整句正确率。

文字方案：`docs/design-docs/2026-10-27-a1-local-asr-integration.md`。
