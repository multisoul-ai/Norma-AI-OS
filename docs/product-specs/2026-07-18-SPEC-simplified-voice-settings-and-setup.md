# Voice Settings 简化与首次设置 SPEC

> 日期：2026-07-18
> 状态：ready-for-agent
> 来源：`ask-matt` → `grill-with-docs` 交互梳理
> 相关决策：`CONTEXT.md` 的「语音首次设置」「语音可用状态」；ADR-0017「Provider API Key 存入 macOS Keychain」

## Problem Statement

当前 Settings → Voice 把语音交互方式、麦克风、完整 ASR 模型目录、Agent Provider、模型名、Base URL、API Key、诊断、TTS 模型目录、音色试听、任务播报和窗口光效同时放在一张长页面中。用户面对的是内部语音技术管线，而不是“让 Voice 可以使用”这一任务，难以判断哪些配置必须完成、哪些只在高级场景需要调整。

全新用户没有明确的 Voice 就绪状态。用户即使知道 `⌘E`，也可能先进入录音路径，再遇到缺少 ASR 模型、Provider 或 TTS 的技术错误；用户必须自行进入 Settings，理解不同模型目录和 Provider 字段后才能修复。首次配置成本高，错误反馈发生得太晚。

现有 Voice 还把“未配置”“正在安装”“已经就绪”和“配置完成后暂时失效”混在松散的字段与运行时错误中。用户无法从底部语音入口直接判断当前状态，也没有一条从问题到修复项的短路径。

OpenAI-compatible 将成为全新 Voice Setup 的默认 Provider，API Key 因而成为常规用户凭据。当前把 API Key 保存在应用设置数据库中的做法不再适合作为默认安全边界。

## Solution

Settings → Voice 改为任务导向的双态页面：未就绪时显示 `Set Up Voice` 单页状态清单；就绪后显示简化 Voice 设置。完整模型目录、Provider 细节和诊断进入 Advanced Settings，不再默认铺满主页面。

底部语音入口投影 Voice 的当前可用状态：

| 状态 | 底部入口 | 点击或 `⌘E` |
|---|---|---|
| 从未完成必需配置 | `Set up Voice` | 打开 Settings → Voice 的首次设置清单，不开始收音 |
| 推荐语音包正在下载 | `Setting up Voice · N%` | 打开当前安装进度，不开始收音 |
| 必需能力完整且可用 | `Voice` | 按当前语音交互模式启动 `⌘E` 业务路径 |
| 曾经就绪但必需能力失效 | `Voice unavailable` | 打开修复清单并展开失效项，不开始收音 |

Voice Ready 有四个硬条件：麦克风权限已授权、识别模型可用、Agent Provider 与所选模型可用、本地 TTS 模型与音色可用。TTS 是完整 Voice 业务闭环的一部分，不允许跳过 TTS 后以纯文字回复完成首次设置。

首次设置复用 Settings → Voice，不新增独立向导弹窗。页面以单页清单显示全部状态并自动展开第一个需要用户处理的项目，各项不强制线性完成。模型下载可以与 Provider 配置并行进行。

默认提供约 644 MB 的推荐语音包，一次操作安装、校验并选中：

- Speech Recognition：`Zipformer Bilingual`，中文 + English，约 511 MB。
- Spoken Replies：`Matcha Chinese-English`，约 133 MB。
- Voice：`Default Bilingual Voice`。

全新 Agent Provider 配置默认使用 OpenAI-compatible。Base URL 默认填入 `https://api.openai.com/v1`，用户可修改；用户输入 API Key 并连接成功后，从 Provider 返回的模型列表选择模型。只有 Provider 无法返回模型列表时才显示手动模型名输入。现有配置先自动验证，验证通过则直接完成该项，不要求重复填写。Ollama 继续保留为可选 Provider，但不是首次推荐路径。

API Key 存入 macOS Keychain。应用设置数据库只保存非敏感 Provider 配置及不可反查明文的凭据引用。界面只显示凭据已保存，并提供 Replace 和 Remove，不提供读取原始 Key。

四项硬条件验证通过后即完成设置。真实语音测试作为推荐动作提供，但不作为完成门槛。若用户关闭 Settings，进行中的模型安装继续在后台运行，底部入口显示下载进度；后台完成时显示一次不抢焦点、不自动发声的 `Voice is ready` 通知。

完成后的 Voice 主页面只保留普通用户需要经常理解或调整的内容：

- Voice 状态与 `Review Setup`。
- Voice Interaction：Single Voice Command / Continuous Listening；单次模式下显示 Hold / Toggle。
- Input：紧凑麦克风选择与识别就绪状态。
- Spoken Replies：Agent task announcements、Voice-driven window inset、当前音色与 Preview。
- Advanced Settings：模型管理、Provider 细节与 Diagnostics 的单一入口。

## User Stories

1. 作为首次使用 Voice 的用户，我想从底部入口看到 `Set up Voice`，以便知道语音尚未准备好。
2. 作为首次使用 Voice 的用户，我想按 `⌘E` 后直接进入 Voice Setup，以便不先录音再遇到缺少模型的错误。
3. 作为暂时不使用 Voice 的用户，我想正常使用 Canvas、Terminal 和其他工作台能力，以便 Voice 设置不会阻塞整个应用。
4. 作为首次启动应用的用户，我不想被强制弹窗打断，以便可以先完成当前工作。
5. 作为 Voice 用户，我想点击底部 `Set up Voice` 进入同一个设置入口，以便不需要在导航中寻找配置页面。
6. 作为 Voice 用户，我想在一张清单中看到所有必需状态，以便理解距离可用还缺什么。
7. 作为 Voice 用户，我想页面自动展开第一个未完成项，以便不判断下一步应该做什么。
8. 作为 Voice 用户，我想按任意顺序处理麦克风、模型和 Provider，以便模型下载期间还能完成其他配置。
9. 作为 Voice 用户，我想明确看到麦克风权限是必需项，以便 Voice Ready 代表可以真正收音。
10. 作为 Voice 用户，我想从 Setup 直接触发 macOS 麦克风授权，以便不等第一次录音时才处理权限。
11. 作为拒绝麦克风权限的用户，我想看到明确的失败状态与系统设置恢复入口，以便知道如何修复。
12. 作为普通用户，我想一次安装推荐语音包，以便不理解 ASR、TTS 和音色目录也能完成配置。
13. 作为中英文用户，我想默认获得中英文识别与回复能力，以便无需自行比较模型语言能力。
14. 作为网络较慢的用户，我想看到推荐语音包的总大小和下载进度，以便判断等待时间。
15. 作为模型下载中的用户，我想关闭 Settings 后继续下载，以便不必一直停留在设置页。
16. 作为模型下载中的用户，我想在底部入口看到 `Setting up Voice · N%`，以便随时知道后台进度。
17. 作为模型下载中的用户，我想重新按 `⌘E` 回到当前安装进度，以便不丢失上下文。
18. 作为不想继续下载的用户，我想显式取消下载，以便控制网络与磁盘使用。
19. 作为退出应用的用户，我想安装安全中断，以便不会留下伪装为 Ready 的半安装模型。
20. 作为重新启动应用的用户，我想保留已完整安装的模型并只重试未完成项，以便不重复下载有效内容。
21. 作为高级用户，我想选择其他识别模型、TTS 模型或音色，以便覆盖推荐组合。
22. 作为普通用户，我不想在首次 Setup 中看到完整模型目录，以便避免无关选择。
23. 作为已有 Provider 配置的用户，我想 Setup 自动复用并验证现有连接，以便不重复输入配置。
24. 作为全新用户，我想默认配置 OpenAI-compatible，以便进入明确且一致的 AI 连接流程。
25. 作为使用官方 OpenAI endpoint 的用户，我想看到预填的 Base URL，以便只需输入凭据。
26. 作为使用其他兼容服务的用户，我想修改 Base URL，以便连接自定义 OpenAI-compatible Provider。
27. 作为配置 Provider 的用户，我想先验证 Base URL 与 API Key，再选择模型，以便避免同时处理多个可能错误的字段。
28. 作为配置 Provider 的用户，我想从 Provider 返回的真实模型列表中选择，以便避免拼错模型名。
29. 作为兼容 Provider 用户，我想在模型列表接口不可用时手动输入模型，以便仍可完成连接。
30. 作为配置 Provider 的用户，我想所选模型经过可用性验证后才显示 Ready，以便 Voice 不会在第一轮请求时才暴露错误。
31. 作为不使用 OpenAI-compatible 的用户，我想仍能在 Provider 选择中使用 Ollama，以便现有本地路径不被删除。
32. 作为安全敏感用户，我想 API Key 存入 macOS Keychain，以便应用数据库、备份或诊断材料不包含长期凭据明文。
33. 作为返回设置页的用户，我想只看到 `API Key saved`，以便界面不会重新暴露原始 Key。
34. 作为需要轮换凭据的用户，我想替换已保存的 API Key，以便更新连接而不先读取旧 Key。
35. 作为停止使用 Provider 的用户，我想移除已保存凭据，以便 Keychain 不保留无用密钥。
36. 作为升级用户，我想现有数据库中的 API Key 安全迁移到 Keychain，以便升级不破坏已有 Voice 配置。
37. 作为 Voice 用户，我想 TTS 模型和音色成为必需项，以便 Voice Ready 始终包含可听回复能力。
38. 作为 Voice 用户，我想在完成配置后选择运行一次真实语音测试，以便验证收音、识别、Agent 和播报闭环。
39. 作为暂时不方便说话的用户，我想跳过真实语音测试仍完成设置，以便环境限制不会卡住我。
40. 作为后台等待安装的用户，我想在完成时收到一次 `Voice is ready` 通知，以便知道现在可以使用 `⌘E`。
41. 作为不希望被打扰的用户，我想完成通知不抢焦点、不自动播报，以便当前工作不被中断。
42. 作为已就绪用户，我想底部入口恢复为 `Voice`，以便状态清晰且 `⌘E` 行为回归正常。
43. 作为 Single Voice Command 用户，我想设置完成后继续使用 Hold 或 Toggle，以便现有交互习惯不变。
44. 作为 Continuous Listening 用户，我想设置完成后继续用 `⌘E` 启停长会话，以便现有业务路径不变。
45. 作为已就绪用户，我想主 Voice 页面只显示常用交互、输入和回复选项，以便不再面对整条技术管线。
46. 作为已就绪用户，我想从主页面看到当前麦克风与识别状态，以便快速确认输入来源。
47. 作为已就绪用户，我想调整 Agent task announcements 和窗口内缘光，以便控制语音反馈体验。
48. 作为已就绪用户，我想查看当前音色并试听，以便在不进入完整目录时确认回复声音。
49. 作为高级用户，我想通过一个 Advanced Settings 入口管理模型、Provider 和 Diagnostics，以便高级能力仍然可达。
50. 作为已配置用户，我想在麦克风权限被撤销时看到 `Voice unavailable`，以便知道这是失效而不是从未配置。
51. 作为已配置用户，我想在模型文件丢失或损坏时直接进入对应修复项，以便不重走整个 Setup。
52. 作为已配置用户，我想在 Provider 或所选模型不可用时直接进入 AI Connection 修复项，以便快速恢复。
53. 作为已配置用户，我想在 TTS 模型或音色不可用时直接进入 Spoken Replies 修复项，以便恢复完整 Voice 闭环。
54. 作为 Voice unavailable 用户，我想按 `⌘E` 时不开始收音，以便已知故障不会产生新的半开语音状态。
55. 作为修复完成的用户，我想入口自动从 `Voice unavailable` 恢复为 `Voice`，以便无需额外重置。
56. 作为维护者，我想用一个明确的 Voice 可用状态投影驱动底部入口、Settings 和快捷键，以便三个界面不会各自推断状态。
57. 作为维护者，我想模型安装、Provider 验证、Keychain 和权限检查在后台执行，以便 GPUI 前台不被网络、文件或系统调用阻塞。
58. 作为 QA 工程师，我想用确定性替身覆盖完整 Setup 与修复流程，以便日常 e2e 不依赖真实网络、模型、声卡或用户 Keychain。

## Implementation Decisions

- 建立单一 Voice readiness 投影，至少表达 `SetupRequired`、`SettingUp`、`Ready` 和 `Unavailable`；`Unavailable` 必须携带可定位到麦克风、ASR、Provider 或 TTS 的失败原因，Settings 和 `⌘E` 路由消费同一投影。
- `SetupRequired` 表示从未满足全部硬条件；`Unavailable` 表示曾经 Ready 但硬条件后来失效。不得用一个布尔 `enabled` 混合两种语义。
- `⌘E` 路由先检查 readiness：非 Ready 时打开 Settings → Voice 并聚焦待处理项，不启动录音、持续聆听、Agent 回合或 TTS；Ready 时保持现有 Single Voice Command / Continuous Listening 路由。
- Settings → Voice 是 Setup 与日常配置的唯一产品入口。未就绪时渲染状态清单；就绪时渲染简化主页面；Advanced Settings 承载完整模型目录、Provider 细节和 Diagnostics。
- 首次 Setup 的逻辑硬条件为麦克风授权、可用 ASR、可用 Agent Provider + 模型、可用 TTS + 音色。界面可以把 ASR 与 TTS 合并显示为推荐语音包，但 readiness 仍逐项判断。
- 推荐语音包固定为 `Zipformer Bilingual`、`Matcha Chinese-English` 与 `Default Bilingual Voice`。一键安装必须分别沿用受管安装、哈希校验、manifest 和原子提升边界；不得用一个新的非受管目录绕过现有模型完整性约束。
- 推荐语音包安装由一个上层协调器聚合两个既有模型安装任务。两项可独立成功、失败、取消或重试；只有两项均验证通过并已选中，语音包才为 Ready。
- 下载进度 `N%` 表示推荐语音包当前需下载字节的聚合进度，不把麦克风授权或 Provider 表单伪装成百分比。没有活跃下载时，入口显示 `Set up Voice` 或 `Voice unavailable`，不显示虚假进度。
- 关闭 Settings 不拥有也不取消模型安装任务；安装生命周期归应用级后台协调器。应用退出时中断未完成任务，完整安装继续以 manifest 为事实来源，半安装不得成为可用模型。
- 麦克风权限从首次录音延后错误改为 Setup 中的显式必需项。授权和系统设置恢复仍通过现有 macOS 权限边界执行，任何可能阻塞的权限等待不得占用 GPUI 前台。
- 全新 Agent Provider 默认选择 OpenAI-compatible，默认 Base URL 为 `https://api.openai.com/v1`，但允许用户修改。Ollama 配置与运行时支持继续保留。
- 已有 Provider 配置在 Setup 打开时先后台验证；验证成功直接投影 Ready，失败才展开配置表单。验证不得仅检查非空字段，必须包含 endpoint、认证以及所选模型的可用性。
- OpenAI-compatible 连接分两阶段：先用 Base URL + API Key 验证并读取模型列表，再要求用户选择模型。模型列表不可用时才显示手动输入；手动模型仍需经过明确的 Provider 验证后才能 Ready。
- Provider 请求继续复用既有 OpenAI-compatible 协议抽象，模型发现能力作为窄接口加入 Provider 边界，不把 HTTP 细节放进 Settings UI。
- API Key 读写通过窄的凭据存储接口完成；生产适配器使用 macOS Keychain，测试使用内存替身。SQLite 只保存非敏感 Provider profile 和 Keychain 引用，不保存新 API Key 明文。
- 为已有 SQLite API Key 提供一次性、非破坏迁移：只有 Keychain 写入成功后才能清除旧值；迁移失败时不得丢失用户配置，并应把 Provider 投影为需要修复而不是静默声称 Ready。
- Keychain 中已有凭据优先于旧数据库值。界面不回填原始 Key，只提供 saved、Replace 和 Remove 状态与操作。
- readiness 的本地模型与权限检查必须后台化或由安装/权限事件增量更新；不得在渲染、鼠标热路径或每次状态读取中扫描模型目录。
- Provider 的临时失联、模型被移除、凭据失效、麦克风权限撤销、ASR/TTS 安装损坏都会把曾经 Ready 的投影变为 `Unavailable`，并保留精确修复目标。
- 四项硬条件通过后立即完成 Setup，不要求真实录音。`Try Voice` 显式运行一次现有 `⌘E` 端到端路径，不引入第二套测试专用业务管线。
- 后台完成 Setup 时只发送一次非阻塞完成通知；通知不抢焦点、不自动调用 TTS，也不替用户启动语音。
- 简化主页面保留当前模式、单次触发方式、麦克风、任务播报、窗口光效、当前音色和 Preview。模型安装/删除/切换、Provider Base URL/API Key/模型管理和完整 trace 进入 Advanced Settings。
- 普通配置继续即时生效并异步持久化，不新增全局 Save / Cancel。模型安装、Provider 验证和 Keychain 写入以各自状态和错误反馈收口。
- 任何模式、麦克风、ASR、Provider、模型、TTS 或音色变更继续遵守既有取消 generation 与共享 Voice Runtime 约束，陈旧异步结果不得恢复旧配置或自动重启麦克风。
- 该规格不改变 `⌘E` 的固定快捷键、Single Voice Command / Continuous Listening 语义、Agent 工具执行或 TTS 播放业务状态机；改变的是使用这些路径前的 readiness 门槛与 Settings 信息架构。

## Testing Decisions

- 最高层主 seam 使用现有 headless GPUI e2e：真实打开 Settings、真实点击带稳定 selector 的控件、真实发送 `⌘E` 键盘事件，并从统一 Settings/Voice e2e snapshot 观察用户可见状态。不要为每个子模块各建一套 UI 测试入口。
- 主 e2e snapshot 扩展 readiness、待修复项、推荐语音包进度、Provider 连接阶段和凭据显示状态；测试断言外部状态与可见控件，不断言内部任务数量或私有字段布局。
- e2e 覆盖 fresh store：底部显示 `Set up Voice`；按 `⌘E` 打开 Settings → Voice；录音、Dialogue 和 TTS 均未启动。
- e2e 覆盖四项清单的正负状态：每项 Ready 时有具体正断言，每项缺失或失效时有具体负断言，禁止使用“所有状态一致”一类泛化断言。
- 麦克风权限通过现有权限 seam 注入 authorized、denied、restricted；验证授权恢复入口、Ready 门槛和 `⌘E` 不误启动。
- 推荐语音包通过现有假模型 transport 与安装管理 seam 覆盖一次点击、双模型并行进度、单项失败、Retry、Cancel、关闭 Settings 后继续、重新打开恢复、退出后重扫和两项均选中。
- Provider 通过本地假 OpenAI-compatible server 覆盖 endpoint 失败、认证失败、模型列表成功、模型列表不支持、手动模型验证、所选模型失效和现有配置自动通过；测试不得访问真实付费 API。
- Keychain 使用窄凭据存储接口的内存替身做日常单测/e2e；断言保存后 UI 只显示 saved，Replace 更新引用，Remove 清除凭据，SQLite 快照不包含 API Key 明文。
- 单独覆盖旧 SQLite API Key → Keychain 的成功迁移、Keychain 写入失败时不清旧值、重载后 Keychain 优先三个场景。
- readiness 纯逻辑测试覆盖 `SetupRequired → SettingUp → Ready → Unavailable → Ready`，并逐项验证麦克风、ASR、Provider、TTS 哪一项失效会产生哪个修复目标。
- Settings e2e 覆盖 Ready 主页面只显示常用配置，完整 ASR/TTS 目录、Base URL、API Key 与 trace 默认不可见；进入 Advanced Settings 后这些能力仍可达。
- 快捷键 e2e 覆盖 Ready 状态下既有 Single Voice Command Hold/Toggle 与 Continuous Listening 行为不变，防止 readiness 门槛破坏已确认业务路径。
- 通知 e2e 覆盖 Settings 关闭后后台完成只通知一次、入口切换为 `Voice`、不抢焦点、不自动播放 TTS，重新载入不重复发送旧完成通知。
- `Unavailable` e2e 分别覆盖权限撤销、ASR 损坏、Provider 失效和 TTS 损坏；每个场景按 `⌘E` 必须打开正确修复项，且不得启动录音。
- 模型域测试继续验证哈希、manifest、受控目录与原子提升；Provider 域测试验证模型发现与连接错误分类；App 测试只验证编排，不复制底层安装或 HTTP 细节。
- 真实 macOS Keychain 读写、系统麦克风权限、受管模型安装与端到端真实语音保留为 ignored L3 smoke，并在报告中与确定性 L1/L2 证据分开说明。
- 所有新增 Rust 测试必须遵守仓库规定的场景、数据构造、执行过程、预期结果注释结构；每个断言包含失败说明，关键保留与删除状态逐一断言，数值预算写明推导。

## Out of Scope

- 阻塞整个应用的首次启动向导或强制启动弹窗。
- 修改 `⌘E` 快捷键或把 Voice 快捷键改为可配置项。
- 改变 Single Voice Command、Continuous Listening、Agent 工具执行、任务队列或 TTS 播放的业务语义。
- 自动替用户创建 OpenAI 账户、购买额度、管理账单或选择付费套餐。
- 把 Ollama 作为首次默认 Provider、自动安装 Ollama 或自动拉取 Ollama 模型；现有 Ollama 选项继续保留。
- 新增云端 ASR、云端 TTS、ASR/TTS Provider 市场或动态远程模型目录。
- 扩充现有 ASR/TTS 精选目录；本规格只指定首次推荐组合及目录的渐进披露。
- 强制用户完成真实语音测试，或在 Setup 完成时自动播放声音。
- 对 Voice 之外的 Settings 页面进行新的视觉重构。
- 跨平台凭据存储实现；当前产品目标为 macOS，凭据边界使用 macOS Keychain。
- 把 API Key 回显、导出或复制回剪贴板。
- 在 GPUI 前台同步执行网络探测、Keychain 调用、模型扫描、下载或完整性校验。

## Further Notes

- 本规格把当前“无 TTS 时仍可纯文字结束回复”的运行时降级从 Voice Ready 定义中移除。内部仍可保留错误恢复，但用户不能在缺少 TTS 时完成首次设置或进入正常 Voice 状态。
- 当前 fresh store “不自动安装或选择模型”的测试契约需要更新：应用仍不得在用户未操作时静默下载，但用户点击 `Install Recommended Package` 后会一次安装并选择推荐 ASR/TTS/Voice。
- 现有 Voice 配置矩阵、Settings 点击、麦克风权限恢复、ASR 模型目录、TTS 模型目录与快捷键模式 e2e 是优先复用的测试先例。
- `Voice unavailable` 是配置后失效，不等同于首次未配置；错误文案和修复入口必须保留这一差异。
- Advanced Settings 是渐进披露，不是删除能力。现有可选模型、Ollama、Base URL、手动模型、Trace、安装、删除、切换与 Preview 仍需可达。
- 规格已按 Markdown 写入仓库；本次没有创建外部 issue，也没有修改实现代码。后续若进入施工，应先拆分为可独立验收的 tracer-bullet tickets，再按 TDD 与 code review 流程逐项实施。
