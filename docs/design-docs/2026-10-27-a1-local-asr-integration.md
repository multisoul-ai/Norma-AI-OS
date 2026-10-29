# A1 本地精准识别集成方案

状态：已实现可选开发入口及快捷键单次录音适配，ASR 回归通过；完整 App 构建因系统内存压力停止，真实界面/模型/麦克风验收尚未完成，发布运行时尚未打包。默认方案不变。

原始设计流程图：`docs/design-docs/2026-10-27-a1-local-asr-integration.html`（保留设计阶段快照，不是产品截图）；当前实现以本文及 `docs/exec-plans/2026-10-27-a1-local-asr-implementation.md` 为准。

## 1. 集成决策

新增可选本地 A1 provider，以现有 `RecognizerFactory` 接口接入。复用现有麦克风、录音会话、识别事件与后台结束录音流程。首版采用受管本地 worker，沿用实验的 Zipformer 预览进程和 Qwen 终稿进程，固定模型与推理依赖。

模型为双语 Zipformer INT8 与 Qwen3-ASR 0.6B MLX 8-bit。Zipformer 输出预览；Qwen 对本轮原始音频重新识别。音频与转写仅在本机处理。工件准备与推理执行分离，推理 worker 禁止联网。

### 已有接入链路

```text
VoiceSettingsValues
  → resolve_builtin_asr_profile()
  → builtin_recognizer_factory()
  → AsrRecordingSession::start()
  → RecognitionPipeline 后台工作线程
  → Recognizer::accept_pcm() / finish()
  → Partial / InputEnded / Final / Empty / Failed / Cancelled
  → App 的录音代次检查
  → handle_final_voice_text()
```

A1 接入这条链路的 factory 与 recognizer 层，不另建麦克风或提交入口。现有 factory 每次录音都会被创建；因此引擎宿主必须由 App 级共享服务持有，不能仅把引擎放入短生命周期 factory。

## 2. 已实现接口与数据流

以下接口已落地；运行时与真实麦克风验证仍有明确缺口。

- `AsrProviderId::LocalA1` 与 `BuiltinAsrProfile::LocalA1`：分别标识 provider 与两模型的固定配置。
- `A1RecognizerFactory`：持有共享宿主的引用，声明支持预览，单次最多 60 秒。
- `A1Recognizer`：实现现有 `Recognizer`；每个实例绑定唯一录音 ID 与引擎代次。
- `A1EngineHost`：由 App 级服务持有，跨窗口共享；具体进程管理归 `asr`，不反向依赖 App。
- `worker.py`：同一脚本按角色启动为两个独立进程，分别运行预览与终稿模型；Rust 宿主管理二者。只提取实验必要推理模块，不携带评分器和录音数据集。

```text
麦克风
  │ 现有 voice_input：采集、重采样、PCM16
  ▼
RecognitionPipeline（后台线程）
  │ A1Recognizer：会话 ID、取消令牌、长度与队列检查
  ▼
本地管道 → Rust A1 宿主
              ├─ Zipformer CPU 预览进程 → Partial → App 字幕
              └─ Qwen 进程内的有界原始音频缓存
                          │ Finish：先排空本轮音频，再冻结长度
                          ▼
                  Qwen MLX / Metal 终稿进程
                          │ final_source、text、会话 ID
                          ▼
                A1 结果分类与会话有效性检查
                   ├─ 正常终稿 → Final → App 代次检查 → 提交一次
                   ├─ 正常空结果 → Empty → 不提交
                   ├─ 超时 / 回退 / 崩溃 → Failed → 保留预览供用户查看
                   └─ 取消 / 旧会话结果 → 丢弃，回收本轮资源
```

Zipformer 结束时一次性补入 660ms 静音音频；它不是固定 sleep，也不写入 Qwen 的原始音频缓存。预览文字不发送给 Agent；只有当前录音的 Qwen 正常终稿进入现有提交链路。

## 3. 本地进程协议与生命周期

Rust 使用非阻塞 UnixStream 对接 worker 标准输入/输出；标准错误仅输出诊断。采用小端长度前缀、JSON header 与二进制 PCM16，协议版本、会话 ID 和序号必须匹配。每帧音频最多 32,000 字节、回复最多 65,536 字节。各模型在进程内按实验口径转换 float32，不用 JSON 数组承载采样点。

| 消息 | 作用 | 必须校验 |
|---|---|---|
| 宿主 prepare / worker ready | 后台校验并加载固定工件 | 宿主检查工件/解释器哈希；worker 检查依赖版本、网络禁用和 ready 来源 |
| Begin | 取得唯一活动录音租约 | 会话 ID、引擎代次、宿主是否空闲 |
| Audio | 传递音频块 | 序号、16kHz 单声道格式、帧长、累计音频上限 |
| Finish | 标记输入完成并请求终稿 | 音频先排空；同会话仅接受一次 |
| Partial / Final | 返回预览或终稿 | 请求 ID、结果来源、终态是否已消费 |
| 宿主 cancel / shutdown | 取消令牌、关闭管道并回收进程组，无独立协议消息 | 禁止迟到结果提交；kill/wait 仅处理自己创建的子进程 |

同一 App 进程最多持有一套 A1 引擎。多窗口共享宿主，只允许一个活动录音，终稿槽位为 1，不积压录音队列。忙时明确返回状态，不另起一份模型。

选择精准模式后可后台准备模型，Ready 后才开始该模式的麦克风会话。一次使用期间模型复用，不逐句加载。首次准备和暖运行分开计时，不用暖运行数据承诺首次体验。

单次沿用 60 秒音频上限。原始 PCM16 缓存上界为 16,000 × 2 × 60 = 1,920,000 字节；转换成 float32 后为 3,840,000 字节。这只是音频缓存，不代表模型运行内存。

退出 App、切换出精准模式或触发内存压力保护时，前台只取消令牌并取得清理票据，后台 kill/wait 自有进程组。宿主持有真实 Child 句柄，先检查是否已回收，不按进程名查杀。worker 另有父进程存活监测。

空闲卸载时间需要资源补验后确定。内存压力非正常时不开始加载；运行中出现压力异常，取消当前 A1 工作并释放模型。实验中的 512MiB swap 容差用于实验停止，不能直接当作普适产品阈值。

## 4. 结果、降级和取消规则

现有 `RecognitionEvent` 的事件类型可以复用。worker 必须额外携带实验已有的 `final_source`、`fallback` 和错误诊断，由 A1 适配器分类。

| 情况 | 集成行为 | 与现有证据的关系 |
|---|---|---|
| Qwen 正常非空终稿 | 发出唯一 Final；通过 App 代次检查后提交 | 沿用实验正常推理配置 |
| Qwen 正常空终稿 | 发出 Empty，不用 Zipformer 结果覆盖 | 保持实验空结果语义 |
| Qwen 超时或异常，实验引擎产生回退文本 | 适配器识别降级并发出 Failed；预览留待用户查看，不自动提交 | 产品提交策略新增，需异常测试 |
| 用户取消 | 先使会话失效，取消推理，清理后才可复用或重建宿主 | 不把迟到终稿写入下一次录音 |
| 切换模式或退出 | 取消会话，后台 kill/wait 自有子进程树 | 需要真实 App 验收 |
| 模型缺失、哈希不符或宿主未就绪 | 精准模式保持未就绪，显示具体原因 | 不静默换模型或云端 |

finish 的预览收尾与 Qwen 终稿共用 10 秒故障预算，不能解释为正常延迟目标。当前先等待预览收尾，再请求 Qwen，编排与实验并行收尾存在差异，历史延迟不能直接当作本次集成结果。两个 worker 各有 60 秒启动上限。非阻塞 IPC 每 5ms 检查取消/截止时间；独立清理票据可以在没有新 PCM 时回收活动进程。

现有 App 已在后台执行 `session.finish()`，并用 recording generation 检查最终结果。A1 需要将相同会话约束扩展到预览转发，防止旧 worker 的预览覆盖新录音字幕。

## 5. 代码落点

| 状态 | 文件或模块 | 实际职责 |
|---|---|---|
| 复用 | `crates/asr/src/recognition.rs` | 复用 Recognizer、取消令牌和规范化事件；补契约测试 |
| 扩展 | `crates/asr/src/providers/mod.rs` | 注册 LocalA1 与 profile，factory 接收共享宿主 |
| 新增 | `crates/asr/src/providers/local/a1/mod.rs` | A1 导出入口 |
| 新增 | `crates/asr/src/providers/local/a1/transport.rs` | 有界消息帧、请求 ID 和结果分类 |
| 新增 | `crates/asr/src/providers/local/a1/host.rs` | factory、recognizer、模型宿主、独占租约、取消与回收 |
| 新增 | `crates/asr/src/providers/local/a1/config.rs` | 固定工件及解释器完整性检查 |
| 新增 | `crates/asr/src/providers/local/a1/resources.rs` | 系统内存压力守护 |
| 新增 | `crates/asr/runtime/a1/worker.py` | 固定依赖下的两个模型角色入口 |
| 新增 | `scripts/prepare-a1-local.py` | 开发环境静态注册和校验；不加载模型、不改变用户设置 |
| 扩展 | `crates/app/src/settings/voice/state/asr_provider.rs` | 解析 A1 profile，增加模型与运行时就绪校验 |
| 扩展 | `crates/app/src/app/dialogue/chat.rs` | 首版拒绝连续监听启用 A1 |
| 扩展 | `crates/app/src/app/voice/recognition.rs` | 对接取消、错误与本轮预览；保留唯一终稿出口 |
| 扩展 | `crates/app/src/app/voice/mod.rs` | 预览增加录音代次检查；模型准备、结束和回收均后台执行 |
| 新增 | `crates/app/src/settings/voice/view/setup/a1.rs` | 本地精准选项、静态校验、显式准备并启用；默认值不变 |

新增文件按职责拆分。豆包混合与 GPT-Live 继续使用原有独立链路；共用模式选择只增加 A1 取消/回收，不替换云模式的识别器、音频协议或凭据。

## 6. 交付与验收顺序

| 阶段 | 最小交付 | 当前状态 / 验收证据 |
|---|---|---|
| 1. 适配器 | 固定 worker、模型宿主、现有 Recognizer 接口 | 已实现；假 worker 覆盖协议边界、空终稿、失败、取消、旧 ID、重复 Finish、进程回收与模型复用 |
| 2. 引擎一致性 | 固定工件和配置运行原代表子集 | 工件已静态校验；真实推理等待用户具体窗口，保留与历史转写的差异 |
| 3. 单次录音 App 接入 | 从现有麦克风到唯一终稿的代码链路 | 已接入；真实麦克风、松键、字幕、多轮、退出回收仍待运行验收 |
| 4. 可选模式开放 | 可见精准选项、就绪反馈、开发环境注册 | 已实现；配置页面定向验收见实施记录，正式运行时打包尚未实现 |

连续监听不纳入首批交付。现有连续监听在结束一段语音时等待终稿；A1 增加第二遍处理后，需要单独检查 VAD 边界和下一段音频积压，再决定接入。

### 延迟验证收敛：先验证基本体验

按用户“不需要大批量测试”的最新意见，延迟验证不重复运行 304 条，也不自动启动 48 条×3 轮。A1 接入后先固定 6 条短录音：中文、英文和句内混合各 2 条。每条运行 2 次，串行记录全部结果；首次模型准备单独记录。若只测引擎适配器，报告明确标为文件回放；真实 App 验收必须走麦克风入口。

小样本报告列出每次的首预览、结束输入到终稿、取消/空结果/异常及资源观察，不估计有代表性的 P95，也不宣布速度排名。先判断是否明显卡顿、是否逐句重复加载、是否多轮后累积资源。只有具体异常才触发针对性补测。

此前单独约定的资源补验仍有 48 条、A1/F1 各 3 轮的冻结协议；本次方案不修改或执行它，也不将其作为小样本延迟验收的隐含前置。实际负载执行仍等待具体窗口。

接入后的端到端埋点需区分：加载/Ready、音频开始、首个非空预览、用户松开或 VAD 停说边界、终稿到达、UI 应用和资源释放。快捷键松开与真实停说不是同一时间点。

发布包必须包含可定位的固定运行时和原生库，校验模型与 tokenizer，并在隔离的发布环境测试。内部验证可以复用固定本地运行时；正式产品不能引用实验目录或依赖用户安装 Python。签名、依赖许可、Apple Silicon 兼容及离线运行属于发布验收。

## 7. 已有证据与未完成部分

304 条、2 位说话人，共 3,936 个参考单位：B0 错误率 25.97%，F1 为 23.73%，A1 为 11.23%。A1 对 B0 错误减少 56.75%，对 F1 减少 52.68%。错误率按中文字符与英文单词计，不是整句正确率。

历史进程树采样峰值：F1 0.741GiB，A1 1.947GiB。历史运行窗口不同，没有完整压力与逐轮 swap 边界；不据此声明正式资源排名。真实 App 的延迟、多轮和释放尚未验收。

评分依据：`/Users/openclawd/Documents/asr-experiment-evidence/b0-f1-a1-matched48-2026-10-26/quality-audit-reconstructed.json`。

资源依据：`/Users/openclawd/Documents/asr-experiment-evidence/a1-f1-memory48-2026-10-27/MEMORY-STATUS.md`。

图页可复现命令：

```bash
node /Users/openclawd/.agents/skills/answer-me-with-html/scripts/am.mjs render \
  docs/design-docs/2026-10-27-a1-local-asr-integration.visual.md \
  --no-open -o docs/design-docs/2026-10-27-a1-local-asr-integration.html
```
