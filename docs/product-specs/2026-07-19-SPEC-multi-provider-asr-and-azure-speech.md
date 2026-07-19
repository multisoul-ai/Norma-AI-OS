# 多供应商 ASR 架构与 Azure Speech 接入 SPEC

> 日期：2026-07-19
> 状态：Draft；仅记录已确认的长期方案，不授权实施
> 来源：`ask-matt` 架构讨论与 Azure Speech 真实语音验证

## Problem Statement

Norma OS 当前只有 Local Sherpa 识别路径。识别模型领域位于 `asr`，但 Sherpa recognizer、麦克风权限、输入设备、音频采集、重采样、VAD、ASR 会话和本地 TTS 适配集中在 `voice_sherpa`；另有一个职责已被 Agent Runtime 取代但尚未清理的旧 `voice` 模块。这个结构可以运行现有本地语音，却不能清晰表达“本地或云端 ASR 供应商”，新增云端供应商时容易让 App、Settings 和语音业务状态机直接依赖厂商细节。

用户希望使用 Azure Speech，也希望后续可以接入其他 ASR 供应商。接入不能破坏现有 Single Voice Command、Continuous Listening、Hold / Toggle、partial、VAD 分段、取消、回声门控、Agent 回合和 TTS 回复行为。Local Sherpa 必须继续作为现有用户的默认 ASR 选择，Azure 配置、验证或运行失败不得让应用静默切换供应商。

当前 Azure 短音频 Demo 已证明真实服务可以识别用户提供的中英混合 M4A，但它仍是独立验证代码，没有进入正式 ASR 能力边界、Settings、Keychain、语音输入管线或两种语音交互模式。若直接把 Demo 接到 App，会继续扩大职责不清的语音模块，并形成第二条难以维护的业务路径。

## Solution

将 ASR 建设为一个零 GPUI、零 Storage 的深能力模块。它拥有 ASR 供应商、供应商配置语义、能力描述、识别会话和内置 Provider 实现；本地 Sherpa 位于 Local Provider，Azure 位于 Cloud Provider。供应商均随 App 编译发布，不建设运行时插件系统，也不允许用户通过任意 URL 加载未知协议。

建立独立的语音输入管线，负责麦克风权限、输入设备、音频采集、重采样、统一 PCM 和 VAD。语音输入管线不知道当前使用 Sherpa 还是 Azure，也不负责把音频转换为文本。ASR 不拥有麦克风或 VAD，只消费已经标准化、已经按 Single Voice Command 显式边界或 Continuous Listening VAD 边界切分的 PCM。

Single Voice Command 与 Continuous Listening 共用一个 `RecognitionPipeline`。两种模式只在输入结束策略和会话保持策略上不同：Single Voice Command 由 Hold 松开或 Toggle 再次触发结束；Continuous Listening 由 VAD SpeechEnd 结束。Provider 通过非阻塞识别会话接收 PCM，并产生统一识别事件。partial 是可选能力，final 结果是必需能力；Azure 首版只产生 final，Local Sherpa 继续产生现有 partial。

Azure 首版使用标准公共 Azure 区域的 Speech-to-text REST API for short audio。Provider 从 region 推导专用识别 Endpoint，接收 16 kHz、单声道 PCM/WAV，一次最多处理 60 秒并只返回最终结果。达到 60 秒时，识别管线自动结束并提交当前片段，同时向用户说明是供应商时长上限；不得静默截断或丢弃整段录音。

Settings 将 ASR 从“一个本地模型选择”扩展为“一个当前 ASR 选择 + 多个供应商独立配置”。Azure 配置包含标准公共 region、来自内置完整目录的识别 locale 和 Keychain 凭据引用。默认 locale 为 `zh-CN`，用户可以在可搜索目录中切换；首版不做自动语言检测，也不接受目录外自由输入。配置 Azure 草稿不会改变当前 Local Sherpa，只有验证 Ready 后的显式激活才原子切换。

长期目标按兼容阶段交付。每个阶段独立测试、独立验收、可以回退；旧兼容层只在新路径达到行为等价后删除。任何兼容闸门失败都阻止下一阶段退出旧路径。

## User Stories

1. 作为现有 Norma OS 用户，我想升级后继续默认使用 Local Sherpa，以便 Azure 接入不会改变我的语音行为。
2. 作为现有 Single Voice Command 用户，我想继续使用 Hold，以便松开 `⌘E` 时仍按原有方式提交一次语音输入。
3. 作为现有 Single Voice Command 用户，我想继续使用 Toggle，以便第二次触发 `⌘E` 时仍按原有方式提交。
4. 作为 Continuous Listening 用户，我想继续由 VAD 自动切分话语，以便云端供应商架构不会改变长会话习惯。
5. 作为 Local Sherpa 用户，我想继续看到去重后的实时 partial，以便本地识别反馈不发生退化。
6. 作为语音用户，我想每个语音片段只产生一次最终结果，以便 Agent 不会重复执行同一条指令。
7. 作为语音用户，我想空识别结果保持现有处理方式，以便没有说话时不会启动无意义的 Agent 回合。
8. 作为语音用户，我想取消录音时同时停止采集、识别 worker 和 partial 任务，以便旧音频不会进入新回合。
9. 作为语音用户，我想在 Thinking 或 Speaking 期间维持现有回声门控，以便 TTS 或环境回声不会被当作新指令。
10. 作为语音用户，我想继续使用当前输入设备和麦克风权限流程，以便新增供应商不改变系统权限体验。
11. 作为 Local Sherpa 用户，我想继续使用当前模型选择和 Hotword 行为，以便本地识别质量配置不会丢失。
12. 作为 Azure 用户，我想在 Voice Settings 中选择 Azure Speech，以便使用自己的 Azure 账户进行云端识别。
13. 作为 Azure 用户，我想只填写 region 而不是拼接识别 URL，以便避免 Endpoint 配置错误。
14. 作为 Azure 用户，我想在可搜索目录中选择识别语言，以便在不同语言任务间明确切换。
15. 作为中文用户，我想 Azure 默认使用 `zh-CN`，以便首次配置后适合当前中英混合语音命令。
16. 作为多语言用户，我想语言目录覆盖 Azure 短音频 REST 的完整受支持 locale，以便不局限于中文和英文。
17. 作为 Azure 用户，我想语言选择保存在 Azure 配置中，以便切换到 Local Sherpa 时不改变本地识别行为。
18. 作为隐私敏感用户，我想 Azure API Key 只保存在 macOS Keychain，以便 SQLite、备份和普通诊断不包含长期密钥。
19. 作为 Azure 用户，我想替换或移除凭据，以便轮换密钥或停止使用该账户。
20. 作为已配置 Local Sherpa 的用户，我想编辑 Azure 草稿时仍继续使用 Local Sherpa，以便未完成配置不破坏可用语音。
21. 作为 Azure 配置用户，我想先验证区域和凭据，再决定是否激活，以便错误配置不会成为当前 ASR 选择。
22. 作为 Azure 配置用户，我想验证失败后保留草稿，以便修复字段时不必重新输入全部配置。
23. 作为 Azure 用户，我想显式点击激活动作后才切换供应商，以便配置与选择不会混为一体。
24. 作为多供应商用户，我想切换供应商后保留各自配置，以便再次切回时恢复原来的模型、region 和 locale。
25. 作为多供应商用户，我不希望识别失败时自动回退到其他供应商，以便识别位置、隐私和费用行为可预测。
26. 作为 Azure 用户，我想认证暂时失败时保留配置，以便临时断网不会清除账户信息或 Local Sherpa 配置。
27. 作为 Azure 用户，我想在达到 60 秒时自动提交已录音频，以便不会因服务上限丢失整段内容。
28. 作为 Azure 用户，我想看到“达到 Azure 单次识别上限”的明确反馈，以便理解录音为何自动结束。
29. 作为 Azure 用户，我想短音频识别期间看到稳定的 Listening / Transcribing 状态，以便没有 partial 时仍知道系统正在工作。
30. 作为 Azure 用户，我想网络或认证错误显示明确错误，而不是空文本或伪成功，以便知道如何修复。
31. 作为 Azure 用户，我想取消正在进行的远程识别，以便开始新语音输入时不会收到旧请求的迟到结果。
32. 作为语音用户，我想迟到的 Provider 结果被会话身份拒绝，以便旧文本不会污染新的 Agent 回合。
33. 作为新用户，我想 Voice Setup 的 Speech Recognition 支持本地和云端选择，以便不必先安装本地模型才能使用 Azure。
34. 作为现有用户，我想缺少新的当前供应商设置时自动解释为 Local Sherpa，以便升级无需重新配置。
35. 作为现有用户，我想当前本地识别模型 ID 原样保留，以便供应商迁移不重置模型安装或选择。
36. 作为普通用户，我想完成 Azure 认证验证后即可激活，以便不方便说话时也能完成设置。
37. 作为谨慎用户，我想在 Settings 中主动运行 Test Recognition，以便在激活前或排错时验证真实音频闭环。
38. 作为维护者，我想新增 ASR 供应商只修改 ASR Provider 注册表和供应商实现，以便 App 不增加厂商分支。
39. 作为维护者，我想 ASR Provider 不依赖 GPUI、SQLite 或 Keychain，以便识别能力可以独立测试。
40. 作为维护者，我想 App 负责凭据解析、持久化、后台调度和 UI 副作用，以便域层不反向依赖壳层。
41. 作为维护者，我想语音输入管线独立于 ASR Provider，以便麦克风和 VAD 不随供应商重复实现。
42. 作为维护者，我想 Local Sherpa 与 Azure 共用一个识别会话协议，以便两种语音交互模式只维护一套编排。
43. 作为维护者，我想 partial 被建模为 Provider 可选能力，以便 Azure 不必伪造 partial，Sherpa 也不必放弃 partial。
44. 作为维护者，我想输入结束原因与识别终态可区分，以便用户结束、VAD、时长上限、空结果、失败和取消不会被一个模糊的关闭状态混合。
45. 作为维护者，我想每个识别会话拥有且仅拥有一个终态，以便 UI 清理和 Agent 提交可以精确执行一次。
46. 作为维护者，我想后台事件通道异常被映射为明确失败，以便 UI 不会永久停留在 Listening 或 Transcribing。
47. 作为维护者，我想旧 `voice_sherpa` 先作为兼容外观保留，以便内部迁移时 App 调用可以逐阶段保持稳定。
48. 作为维护者，我想职责已被 Agent Runtime 取代的旧 `voice` 模块最后再删除，以便清理不会与供应商迁移形成一个不可回退的大改动。
49. 作为 QA 工程师，我想用确定性 Provider 和输入替身验证完整识别管线，以便日常测试不依赖麦克风、模型或付费网络。
50. 作为 QA 工程师，我想用真实用户 M4A 验收 Azure，以便模拟服务通过不能代替真实语音证据。
51. 作为 QA 工程师，我想真实 Azure 测试默认 ignored，以便普通测试和 CI 不消耗用户额度或要求密钥。
52. 作为安全审查者，我想测试日志、Debug 输出、错误和仓库文件都不包含 API Key，以便验收本身不会泄露凭据。
53. 作为发布负责人，我想 Local Sherpa 的全部兼容闸门通过后才暴露 Azure 激活，以便新功能不以现有功能不可用为代价。
54. 作为发布负责人，我想每个迁移阶段独立提交和回退，以便问题可以定位到最小变更范围。
55. 作为未来供应商维护者，我想 Provider 是 App 内置能力而非动态插件，以便扩展性不引入未知代码、安全和版本协议风险。

## Implementation Decisions

- 长期依赖方向为 App 组合语音输入、ASR、Agent 和 TTS；语音输入负责标准化 PCM 与片段边界，ASR 负责 PCM 到文本，Agent 负责理解与工具执行，TTS 负责文本到音频。四个职责不得重新合并为一个厂商语音模块。
- `asr` 成为完整 ASR 能力模块，拥有模型领域、Provider 注册表、Provider 配置语义、能力描述、识别会话、Local Sherpa Adapter 和 Azure Adapter。它保持零 GPUI、零 Storage、零 Keychain，但允许 Provider 子模块使用受控 Sherpa 与网络依赖。
- 所有供应商均为随 App 编译发布的内置 Provider。不建设运行时插件、跨进程插件协议、Provider 市场或任意协议 URL。
- `voice_input` 成为独立能力模块，拥有麦克风权限、输入设备、AVAudioEngine 采集、重采样、PCM 和 VAD。它不转写文本，也不知道当前 ASR 选择。
- `voice_sherpa` 在迁移期作为兼容外观存在；其 ASR 实现迁入 Local Sherpa Provider，其采集/VAD 职责迁入语音输入，其 TTS Adapter 迁往 TTS 的本地 Sherpa 责任。只有所有调用者迁移且兼容测试通过后才能删除。
- 当前独立 Azure Demo 的协议与测试收口到 Cloud Azure Provider，随后删除独立厂商 crate，避免 App 同时依赖通用 ASR 与厂商 crate。
- 旧 `voice` 模块的 transcript-to-plan/Ollama planner/refiner 已被 Agent Runtime 取代，不参与新识别架构；它只在剩余常量和调用迁出、回归通过后删除。
- App 只依赖 Provider-neutral 的 ASR 选择、配置、就绪状态和识别会话接口。App 不拼接 Azure URL，不解析 Azure JSON，不匹配 Azure 错误码，也不构造 Sherpa recognizer。
- 公共识别 seam 是非阻塞的 `RecognitionPipeline` 会话：输入侧使用有界命令通道接收标准化 PCM 和结束/取消命令，输出侧使用事件流报告 optional partial、输入结束与唯一终态。阻塞识别、网络、文件 IO 和 worker join 不进入 GPUI 前台。
- 输入结束原因至少区分用户显式结束、VAD SpeechEnd 和 Provider 时长上限。一个会话可以产生零个或多个 partial，随后必须且只能产生一个 Final、Empty、Failed 或 Cancelled 终态；终态后事件流关闭。通道关闭本身不是业务终态。
- Single Voice Command 和 Continuous Listening 共用同一个 RecognitionPipeline；Single Voice Command 不创建或调用 VAD，Hold / Toggle 定义结束边界；Continuous Listening 继续使用现有 VAD 分段。
- Local Sherpa 保留现有 partial 去重、final、empty、取消、模型、Hotword 与输入设备行为。迁移只改变内部所有权和接口，不改变用户可见语义。
- Azure 首版使用 Speech-to-text REST API for short audio，而不是 Speech SDK、Fast Transcription 或 Batch Transcription。Azure Provider 缓冲一个片段的标准化 PCM，在输入结束后提交并返回 final。
- Azure 首版只支持标准公共 Azure 区域。Provider 由 region 推导 Speech STT Endpoint；不允许用户输入任意 Endpoint，也不支持 Azure Government、21Vianet、Private Link 或自定义域名。
- Azure 输入固定为 16,000 samples/s × 16 bits/sample × 1 channel，即 256 kbps PCM。60 秒对应 16,000 × 2 × 1 × 60 = 1,920,000 PCM bytes；WAV header 不计入音频时长。
- Azure Provider 声明单次输入上限 60 秒。RecognitionPipeline 达到该上限时停止接收当前片段、产生 DurationLimit 输入结束原因并提交已有音频；不得静默截断后继续录音，也不得丢弃整段输入。
- Provider 能力显式表达 partial 是否可用和单次输入时长上限。App 可以据此展示状态，但不得为每个厂商增加业务分支。
- ASR 设置模型为一个当前 Provider ID，加每个 Provider 独立的 profile。Local profile 保存现有模型选择；Azure profile 保存 region、locale 和 opaque credential reference。非当前 profile 保留但不参与识别。
- 当新的当前 Provider ID 缺失时，迁移结果必须是 Local Sherpa；现有本地模型 ID、模型安装、Hotword、输入设备和交互模式保持不变。
- Azure locale 默认 `zh-CN`，从随 App 发布的可搜索完整 Azure 短音频支持目录选择。目录由 App 版本更新，不从运行时远程加载；首版不做自动语言检测，不接受目录外自由输入。
- Azure 配置区分 Configured 与 Ready。字段完整只表示 Configured；region 和凭据通过 Azure 认证验证后才表示 Ready。普通用户不需要完成真实录音才能 Ready。
- 配置未就绪的 Azure profile 是草稿，不替换当前 Local Sherpa。只有用户对 Ready profile 执行显式激活，App 才原子更新 ASR 选择。
- ASR 切换不建立回退列表。认证、限流、网络、空文本或 Provider 错误都明确失败，不自动改用 Local Sherpa 或其他云端 Provider。
- ASR Provider Config 是可序列化的非敏感 typed enum；Provider secret 是分离、不可序列化的运行时类型。`asr` 定义配置语义和校验，App 负责 SQLite 持久化、Keychain 读写、secret 解析与 Provider 组合。
- Azure API Key 只保存在 macOS Keychain；SQLite 只保存不可反查明文的引用。Debug、默认日志、错误、测试输出和诊断不得包含 Key、Authorization Header 或完整原始响应中的用户内容。
- 远程请求、认证验证、recognizer 创建、worker join 和音频转换都在后台执行。前台只应用状态、显示进度/错误、切换 Voice readiness 并触发既有 Agent/TTS 副作用。
- Azure 激活后同时服务 Single Voice Command 与 Continuous Listening。Continuous Listening 保留现有 VAD 单段上限与回声门控；Azure 只在 VAD SpeechEnd 后请求 final，不伪造 partial。
- 真实识别文本沿用现有 Final Transcript 到 Agent/工具/TTS 的业务入口。Provider 不知道 Agent、Canvas、Terminal、任务队列或 TTS。
- 实施分七个阶段，每阶段必须通过自己的兼容闸门后才能继续：
  1. 冻结现有 Single Voice Command、Continuous Listening、partial/final/empty/cancel、VAD 与 echo gate 行为。
  2. 将 Azure Demo 收口为 Cloud Azure Provider，完成离线协议测试和真实语音测试，但不接入 App。
  3. 建立最小 Provider/Recognizer 接口并迁移 Local Sherpa 识别实现，同时保留 `voice_sherpa` 兼容外观。
  4. 建立语音输入模块并迁移麦克风、设备、重采样和 VAD，保持 App 调用与业务状态不变。
  5. 用统一 RecognitionPipeline 承接 Local Sherpa，逐项通过现有行为兼容闸门。
  6. 增加 Azure Settings、Keychain、locale 目录与显式激活，并接入两种语音交互模式。
  7. 完成全量回归和真实语音验收后，删除兼容层、独立 Azure Demo 和无调用旧 `voice` 模块。
- 阶段顺序不授权提前清理。任一兼容闸门失败时，Local Sherpa 继续使用上一阶段可用路径，Azure 不进入用户可激活状态。
- 现有 ADR-0004 和 ADR-0009 中 `asr` 零 Sherpa/零网络的边界与本规格长期目标冲突。实施前必须新增明确的 superseding ADR；在该 ADR 完成前，不得把本规格描述伪装成已经生效的仓库架构。
- ADR-0002 的模型选择与安装分离、ADR-0013 的共享语音回合语义、ADR-0017 的 Keychain 边界和 ADR-0019 的默认日志隐私继续有效。

## Testing Decisions

- 测试只验证公共行为，不锁定私有 helper、目录移动顺序、线程名称或厂商 JSON 解析函数。最高可复用 seam 优先，低层 seam 只覆盖无法从高层可靠观察的协议约束。
- 主要确定性 feature seam 是公开 RecognitionPipeline。测试注入确定性语音输入和 fake Provider，通过同一会话观察 partial、输入结束原因、唯一终态、取消、迟到结果抑制和通道异常映射；Single Voice Command 与 Continuous Listening 不各建一套识别测试入口。
- RecognitionPipeline 合约测试必须正向断言具体 Final/Empty/Failed/Cancelled 之一出现，并负向断言其他终态和第二个终态不存在。不得用“所有事件彼此匹配”这类两侧同时缺失也会通过的泛化断言。
- Local Sherpa 兼容测试冻结现有行为：Hold 与 Toggle 边界不变；partial 去重不变；每段恰好一个 final；空文本行为不变；取消停止 capture、recognizer worker 和 partial task；Continuous Listening VAD 分段不变；Thinking/Speaking echo gate 抑制不变；模型、Hotword、输入设备和麦克风权限不变。
- Local 兼容证据优先复用现有 Sherpa session、listening pipeline、Dialogue 和 App shortcut 测试。迁移测试替换旧私有所有权断言，不在新旧实现上长期维护两套重复测试。
- Azure Provider 的较低层合约测试使用本地 fake HTTP transport，验证 region-derived Endpoint、`language` locale、Content-Type、完整 WAV body、成功响应、空文本、非 Success、认证/网络错误映射和 secret 脱敏。测试不得访问真实 Azure。
- Azure 音频边界测试使用明确推导的 16 kHz mono PCM16 数值，逐一验证 60 秒正边界、超过上限自动结束、WAV header 不计入时长以及不存在静默截断。
- Provider capability 测试具体断言 Local Sherpa 支持 partial、Azure 首版不支持 partial、Azure 上限为 60 秒，并负向断言未知 Provider 或运行时插件注册成功。
- ASR profile 测试验证新设置缺失时选择 Local Sherpa、现有本地模型 ID 原样保留、Azure 草稿不激活、验证失败不覆盖当前选择、Ready 后显式激活、往返切换恢复各自 profile，以及失败时不存在自动 fallback。
- Keychain 使用现有窄凭据存储 seam 和内存替身进行日常测试。具体断言 SQLite snapshot、Debug、错误和日志不包含 fake secret；失败替换保留原可用凭据；成功替换更新 opaque reference；Remove 使 Azure 不再 Ready。
- locale 目录测试具体断言 `zh-CN` 默认存在并选中、`en-US` 等不同语言可搜索切换、目录外 locale 被拒绝、Azure locale 切换不修改 Local Sherpa 模型或 Hotword。
- App 用户流程使用现有 headless GPUI e2e：打开 Voice Settings，配置 Azure 草稿，验证 Ready，显式激活，切回 Local，再次切回 Azure；观察可见状态和统一 snapshot，不断言私有字段布局。
- Settings e2e 负向覆盖：未验证 Azure 不能激活；编辑草稿不会停止当前 Local；Test Recognition 不隐式激活；Azure 失败不触发 Local；非 Ready 时 `⌘E` 打开正确修复项而不开始录音。
- Single Voice Command e2e 使用确定性 ASR 替身覆盖 Hold、Toggle、无 partial Provider、Final Transcript 进入既有 Agent/TTS 回合和取消旧回合。Continuous Listening e2e 覆盖 VAD SpeechStart/SpeechEnd、无 partial Provider、echo gate、回复后恢复 Listening 和停止长会话。
- Azure 60 秒用户流程测试使用可控虚拟时钟或精确 PCM sample 计数，断言界面显示时长上限原因、录音转入 Transcribing、已有音频被提交，并负向断言第 60 秒后的 PCM 进入同一请求。
- 真实 Azure Speech 是硬验收标准，但作为显式运行的 ignored L3 测试，不进入默认 CI。测试从环境变量或未纳入版本控制的本地 secret 文件读取凭据，测试结束后清理临时音频和 secret 投影，任何输出不得打印 Key。
- 最终真实验收必须通过公开 RecognitionPipeline，而不是私有 Azure HTTP helper：用户提供的 M4A 经语音输入标准化为 16 kHz mono PCM/WAV，再由 Cloud Azure Provider 产生 Final。
- 真实验收使用 30 秒测试超时，并对规范化文本匹配稳定片段 `Java`、`创建一个项目`、`打开Java项目`、`我正在学习Java`；不得因标点、空白或 Azure 的非关键格式变化要求整句逐字相等。
- 普通用户激活 Azure 只要求认证验证成功；真实语音测试在 Settings 中是推荐的 Test Recognition 动作，不是 Ready 硬门槛。
- 每个阶段都先写能失败的聚焦测试再实现。所有 Rust 测试遵守仓库规定的场景、数据构造、执行过程、预期结果注释结构；超时、队列、PCM 字节数和时长等数值必须写出推导，每个断言必须带失败说明，关键正负状态逐一断言。
- 涉及 Settings 页面、布局或视觉状态的阶段必须新增/更新 e2e，并在验收时通过 Computer Use 操作真实 App；headless GPUI 和静态截图只能补充，不能替代真实 App E2E。
- 阶段验证按改动范围执行 ASR、Sherpa、Dialogue、App 和 e2e 测试；跨 crate 阶段执行 workspace 测试、依赖边界、Storage/性能护栏、最大文件行数、禁止 allow 和 Agent 指南行数检查。最终用户可见阶段还需要 release App build。
- secret 验收必须扫描工作树与默认日志，确认没有真实 Azure API Key、Authorization Header 或可反查凭据明文；聊天中曾暴露的测试凭据应轮换，不作为长期测试资产。

## Out of Scope

- 本次文档生成不实施任何代码、迁移、Settings、ADR、提交或 Issue Tracker 发布。
- Azure Speech SDK、WebSocket 实时识别、Fast Transcription、Batch Transcription、Speech Translation 或 Custom Speech。
- Azure 首版 partial 结果；UI 不通过伪文本、计时器或本地猜测模拟 Azure partial。
- 自动语言检测、连续语言切换、目录外 locale 输入或运行时远程语言目录。
- 超过 60 秒的单片段 Azure 识别、长录音分片拼接、会议转写、说话人分离或时间戳编辑。
- Azure Government、21Vianet、Private Link、Custom Domain、任意 Endpoint 输入或非标准公共区域。
- Azure Phrase List、Custom Speech 模型、Hotword 云端映射；Local Sherpa 现有 Hotword 继续保留。
- Norma 自有 Azure Key、共享桌面密钥、服务端代理、统一额度、订阅套餐、计费面板或费用预测。
- 自动 Local/Cloud fallback、Provider 优先级列表、同时向多个 Provider 发送同一音频或对结果投票。
- 运行时 Provider 插件、第三方动态代码、Provider 市场或用户自定义协议实现。
- 改变 Agent 工具协议、Master Agent 回合、TTS Provider、音频播放仲裁或 Canvas/Terminal 业务行为。
- 在兼容闸门完成前删除 `voice_sherpa`、旧 `voice`、旧设置键或现有 Local Sherpa 执行路径。
- 将真实语音测试设为普通用户完成 Voice Setup 或激活 Azure 的强制步骤。
- 在本规格中决定 Azure 请求的自动重试与模糊计费策略；实现设计必须另行明确，且不得产生重复终态或静默 fallback。
- 在本规格中决定 API Key 直传还是短期 token 缓存；两者都必须遵守 Keychain、日志隐私和后台执行边界。

## Further Notes

- 当前真实 Demo 已用标准公共 `eastus` 资源识别用户提供的中英混合 M4A，返回内容包含本规格真实验收所列四个稳定片段。这证明 Azure 服务、region-derived STT Endpoint、16 kHz mono WAV 转换和 `zh-CN` 对当前样本可用，但不等于 App 业务链已经集成。
- Azure 官方短音频 REST 约束见 [Speech-to-text REST API for short audio](https://learn.microsoft.com/azure/ai-services/speech-service/rest-speech-to-text-short)；语言与 locale 支持见 [Language and voice support](https://learn.microsoft.com/azure/ai-services/speech-service/language-support)。
- Azure 自动语言识别要求候选语言并增加初始延迟，主要通过 Speech SDK 等能力提供；首版显式 locale 的取舍见 [Language identification](https://learn.microsoft.com/azure/ai-services/speech-service/language-identification)。
- 旧 ASR 模型框架 SPEC 中“ASR 领域零 Sherpa/网络”和 Voice Settings SPEC 中“云端 ASR 不在范围内”的条款，在本规格实施并由 superseding ADR 生效后才被相应取代；模型身份、安装清单、完整性、共享模型选择与 Voice readiness 的其他契约继续有效。
- 长期改动量仍预计覆盖约 30–45 个文件和 3,000–5,000 行 diff，其中大部分是职责移动、兼容测试与 E2E；真正新增的生产逻辑主要是 Provider 协议、选择/配置和组合层。该估算不应成为一次性大提交的理由。
- 本规格要求逐阶段迁移，不要求一轮完成。每阶段的实现授权、执行计划和提交范围必须由用户另行明确确认。
- 本次请求的交付物仅为仓库中的 canonical Markdown SPEC；未创建外部 Issue，也未添加 `ready-for-agent` 标签。
