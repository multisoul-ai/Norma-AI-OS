# Voice Settings 简化与首次设置 SPEC

> 日期：2026-07-18
> 状态：implemented
> 来源：`ask-matt` → `grill-with-docs` 交互梳理
> 相关决策：`CONTEXT.md` 的「语音首次设置」「语音可用状态」；ADR-0017「Provider API Key 存入 macOS Keychain」
> 后续修订：其中“本地 TTS 是唯一就绪条件”的表述已由 `2026-07-19-SPEC-built-in-streaming-tts-providers.md` 取代；当前条件是用户显式选择的 TTS Provider 已 Ready。

## Problem Statement

当前 Settings → Voice 把语音交互方式、麦克风、完整 ASR 模型目录、Agent Provider、模型名、Base URL、API Key、诊断、TTS 模型目录、音色试听、任务播报和窗口光效同时放在一张长页面中。用户面对的是内部语音技术管线，而不是“让 Voice 可以使用”这一任务，难以判断哪些配置必须完成、哪些只在高级场景需要调整。

全新用户没有明确的 Voice 就绪状态。用户即使知道 `⌘E`，也可能先进入录音路径，再遇到缺少 ASR 模型、Provider 或 TTS 的技术错误；用户必须自行进入 Settings，理解不同模型目录和 Provider 字段后才能修复。首次配置成本高，错误反馈发生得太晚。

现有 Voice 还把“未配置”“正在安装”“已经就绪”和“配置完成后暂时失效”混在松散的字段与运行时错误中。用户无法从底部语音入口直接判断当前状态，也没有一条从问题到修复项的短路径。

OpenAI-compatible 将成为全新 Voice Setup 的默认 Provider，API Key 因而成为常规用户凭据。当前把 API Key 保存在应用设置数据库中的做法不再适合作为默认安全边界。

## Solution

Settings → Voice 改为任务导向的双态页面：未就绪时显示 `Set Up Voice` 单页状态清单；就绪后显示简化 Voice 设置。完整 ASR 模型目录、Provider 细节和诊断不再默认铺满主页面；Local TTS Model 与音色选择直接放入 Spoken Replies。Setup 必须提供明确的“选择其他识别模型”入口，Ready 摘要必须提供 ASR/TTS 的直接更换入口。

底部语音入口投影 Voice 的当前可用状态：

| 状态 | 底部入口 | 点击或 `⌘E` |
|---|---|---|
| 从未完成必需配置 | `Set up Voice` | 打开 Settings → Voice 的首次设置清单，不开始收音 |
| 任一本地语音模型正在下载 | `Setting up Voice · N%` | 打开显示当前模型百分比与容量的安装位置，不开始收音 |
| 必需能力完整且可用 | `Voice` | 按当前语音交互模式启动 `⌘E` 业务路径 |
| 曾经就绪但必需能力失效 | `Voice unavailable` | 打开修复清单并展开失效项，不开始收音 |

Voice Ready 有四个硬条件：麦克风权限已授权、识别模型可用、Agent Provider 与所选模型可用、本地 TTS 模型与音色可用。TTS 是完整 Voice 业务闭环的一部分，不允许跳过 TTS 后以纯文字回复完成首次设置。

首次设置复用 Settings → Voice，不新增独立窗口或独立向导弹窗。Setup 始终呈现在现有 Voice Settings 弹窗内容区内，保留 Settings 导航、Voice 页头和统一关闭入口；内容区显示 `Voice Setup` 标题、四项硬条件进度和三步纵向引导。三步固定映射为：

1. Speech Recognition：同卡显示麦克风授权前置状态，并通过渐进入口打开 ASR 模型目录。
2. AI Connection：显示 OpenAI-compatible 配置与连接状态。
3. Spoken Replies：直接显示必需 Local TTS Model、音色选择、安装与试听状态。

三个步骤内容同时可见，不使用折叠卡，也不强制线性完成；模型下载可以与 Provider 配置并行进行。页面底部保留 `Finish Setup` 视觉收口，在条件未满足时为不可用状态；四项硬条件仍独立计算，不因视觉合并为三步而减少。

ASR 与 TTS 使用各自已有的模型目录独立安装、显示进度、取消和重试。首次页面不再增加跨两个目录的推荐套餐卡片：Speech Recognition 只负责识别模型，Spoken Replies 只负责 Local TTS Model 与音色，避免同一模型在不同步骤重复出现。

全新 Agent Provider 配置默认使用 OpenAI-compatible。Base URL 默认填入 `https://api.openai.com/v1`，用户可修改；用户输入 API Key 并连接成功后，从 Provider 返回的模型列表选择模型。只有 Provider 无法返回模型列表时才显示手动模型名输入。现有配置先自动验证，验证通过则直接完成该项，不要求重复填写。Ollama 继续保留为可选 Provider，但不是首次推荐路径。

API Key 存入 macOS Keychain。应用设置数据库只保存非敏感 Provider 配置及不可反查明文的凭据引用。界面默认显示固定掩码 `*********`，并提供显隐和 Remove，点击输入框可直接替换；只有用户主动点击眼睛按钮时才在当前输入实体中临时显示已加载的 Key，关闭显隐、切换 Provider、失焦或关闭 Settings 后立即清除明文展示。

四项硬条件验证通过后即完成设置。用户随后通过既有 `⌘E` 路径使用或验证 Voice，不在 Ready 横幅中新增第二个 `Try Voice` 入口。Settings 打开时，各模型目录直接显示自己的百分比、已下载容量和取消操作；用户关闭 Settings 后，模型安装继续在后台运行，底部入口汇总所有活跃安装并继续显示下载进度。后台完成时显示一次不抢焦点、不自动发声的 `Voice is ready` 通知。

完成后的 Voice 主页面只保留普通用户需要经常理解或调整的内容：

- Voice 状态横幅，以及独立的三行配置摘要卡：Speech Recognition、AI Connection、Spoken Replies；`Review Setup` 位于摘要卡底部。
- Voice Interaction：Single Voice Command / Continuous Listening；单次模式下显示 Hold / Toggle。
- Input：紧凑麦克风选择与识别就绪状态。
- Spoken Replies：Agent task announcements、Voice-driven window inset、当前音色与 Preview。
- Advanced Settings：ASR 模型管理、Provider 细节与 Diagnostics 的单一入口；不重复展示 TTS。

## User Stories

1. 作为首次使用 Voice 的用户，我想从底部入口看到 `Set up Voice`，以便知道语音尚未准备好。
2. 作为首次使用 Voice 的用户，我想按 `⌘E` 后直接进入 Voice Setup，以便不先录音再遇到缺少模型的错误。
3. 作为暂时不使用 Voice 的用户，我想正常使用 Canvas、Terminal 和其他工作台能力，以便 Voice 设置不会阻塞整个应用。
4. 作为首次启动应用的用户，我不想被强制弹窗打断，以便可以先完成当前工作。
5. 作为 Voice 用户，我想点击底部 `Set up Voice` 进入同一个设置入口，以便不需要在导航中寻找配置页面。
6. 作为 Voice 用户，我想在一张清单中看到所有必需状态，以便理解距离可用还缺什么。
7. 作为 Voice 用户，我想一次看到三个步骤及其处理入口，以便不需要逐张展开卡片寻找下一项。
8. 作为 Voice 用户，我想按任意顺序处理识别、Provider 和回复配置，以便模型下载期间还能完成其他配置。
9. 作为 Voice 用户，我想明确看到麦克风权限是必需项，以便 Voice Ready 代表可以真正收音。
10. 作为 Voice 用户，我想从 Setup 直接触发 macOS 麦克风授权，以便不等第一次录音时才处理权限。
11. 作为拒绝麦克风权限的用户，我想看到明确的失败状态与系统设置恢复入口，以便知道如何修复。
12. 作为普通用户，我想在 Speech Recognition 与 Spoken Replies 的正确上下文中分别安装模型，以便不会把识别模型和回复声音混为一谈。
13. 作为中英文用户，我想从模型目录清楚看到语言能力，以便选择支持中英文的识别与回复模型。
14. 作为网络较慢的用户，我想在每个模型条目内直接看到百分比和已下载容量，以便判断等待时间。
15. 作为模型下载中的用户，我想关闭 Settings 后继续下载，以便不必一直停留在设置页。
16. 作为模型下载中的用户，我想在底部入口看到 `Setting up Voice · N%`，以便随时知道后台进度。
17. 作为模型下载中的用户，我想重新按 `⌘E` 回到当前安装进度，以便不丢失上下文。
18. 作为不想继续下载的用户，我想显式取消下载，以便控制网络与磁盘使用。
19. 作为退出应用的用户，我想安装安全中断，以便不会留下伪装为 Ready 的半安装模型。
20. 作为重新启动应用的用户，我想保留已完整安装的模型并只重试未完成项，以便不重复下载有效内容。
21. 作为高级用户，我想从 Setup 或 Ready 摘要直接进入其他识别模型、TTS 模型或音色，以便覆盖推荐组合。
22. 作为普通用户，我不想在首次 Setup 中默认看到完整 ASR 目录，但必须能通过一个明确入口主动展开；Local TTS Model 则应直接归属 Spoken Replies，以便在正确上下文中选择回复声音。
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
33. 作为返回设置页的用户，我想默认看到 `*********`，并可按需点击眼睛按钮临时查看，以便兼顾确认配置与默认隐私保护。
34. 作为需要轮换凭据的用户，我想替换已保存的 API Key，以便更新连接而不先读取旧 Key。
35. 作为停止使用 Provider 的用户，我想移除已保存凭据，以便 Keychain 不保留无用密钥。
36. 作为升级用户，我想现有数据库中的 API Key 安全迁移到 Keychain，以便升级不破坏已有 Voice 配置。
37. 作为 Voice 用户，我想 TTS 模型和音色成为必需项，以便 Voice Ready 始终包含可听回复能力。
38. 作为 Voice 用户，我想完成配置后直接使用既有 `⌘E` 路径，以便验证收音、识别、Agent 和播报闭环时不学习第二个入口。
39. 作为暂时不方便说话的用户，我想不执行真实语音仍能完成设置，以便环境限制不会卡住我。
40. 作为后台等待安装的用户，我想在完成时收到一次 `Voice is ready` 通知，以便知道现在可以使用 `⌘E`。
41. 作为不希望被打扰的用户，我想完成通知不抢焦点、不自动播报，以便当前工作不被中断。
42. 作为已就绪用户，我想底部入口恢复为 `Voice`，以便状态清晰且 `⌘E` 行为回归正常。
43. 作为 Single Voice Command 用户，我想设置完成后继续使用 Hold 或 Toggle，以便现有交互习惯不变。
44. 作为 Continuous Listening 用户，我想设置完成后继续用 `⌘E` 启停长会话，以便现有业务路径不变。
45. 作为已就绪用户，我想主 Voice 页面只显示常用交互、输入和回复选项，以便不再面对整条技术管线。
46. 作为已就绪用户，我想从主页面看到当前麦克风与识别状态，以便快速确认输入来源。
47. 作为已就绪用户，我想调整 Agent task announcements 和窗口内缘光，以便控制语音反馈体验。
48. 作为已就绪用户，我想查看当前音色并试听，以便在不进入完整目录时确认回复声音。
49. 作为高级用户，我想通过渐进入口管理模型、Provider 和 Diagnostics，以便高级能力仍然可达。
50. 作为已就绪用户，我想让 AI Connection 与 ASR/TTS 一样提供 `Change`，以便直接返回同一个 Voice Setup 的 Provider 区域。
51. 作为已配置用户，我想在麦克风权限被撤销时看到 `Voice unavailable`，以便知道这是失效而不是从未配置。
52. 作为已配置用户，我想在模型文件丢失或损坏时直接进入对应修复项，以便不重走整个 Setup。
53. 作为已配置用户，我想在 Provider 或所选模型不可用时直接进入 AI Connection 修复项，以便快速恢复。
54. 作为已配置用户，我想在 TTS 模型或音色不可用时直接进入 Spoken Replies 修复项，以便恢复完整 Voice 闭环。
55. 作为 Voice unavailable 用户，我想按 `⌘E` 时不开始收音，以便已知故障不会产生新的半开语音状态。
56. 作为修复完成的用户，我想入口自动从 `Voice unavailable` 恢复为 `Voice`，以便无需额外重置。
57. 作为维护者，我想用一个明确的 Voice 可用状态投影驱动底部入口、Settings 和快捷键，以便三个界面不会各自推断状态。
58. 作为维护者，我想模型安装、Provider 验证、Keychain 和权限检查在后台执行，以便 GPUI 前台不被网络、文件或系统调用阻塞。
59. 作为 QA 工程师，我想用确定性替身覆盖完整 Setup 与修复流程，以便日常 e2e 不依赖真实网络、模型、声卡或用户 Keychain。

## Implementation Decisions

- 建立单一 Voice readiness 投影，至少表达 `SetupRequired`、`SettingUp`、`Ready` 和 `Unavailable`；`Unavailable` 必须携带可定位到麦克风、ASR、Provider 或 TTS 的失败原因，Settings 和 `⌘E` 路由消费同一投影。
- `SetupRequired` 表示从未满足全部硬条件；`Unavailable` 表示曾经 Ready 但硬条件后来失效。不得用一个布尔 `enabled` 混合两种语义。
- `⌘E` 路由先检查 readiness：非 Ready 时打开 Settings → Voice 并聚焦待处理项，不启动录音、持续聆听、Agent 回合或 TTS；Ready 时保持现有 Single Voice Command / Continuous Listening 路由。
- Settings → Voice 是 Setup 与日常配置的唯一产品入口。未就绪时在同一 Voice Settings 弹窗内容区渲染三步 Setup，并持续保留 Settings 导航与 Voice 页头；就绪时在原位渲染简化主页面。Setup 的“选择其他识别模型”和 Ready 摘要的 ASR `Change` 进入 Speech Recognition 的 ASR 目录；TTS `Change` 进入 Spoken Replies 内常驻的 Local TTS Model 与音色目录。Advanced Settings 继续承载 Provider 细节和 Diagnostics。
- 首次 Setup 的逻辑硬条件为麦克风授权、可用 ASR、可用 Agent Provider + 模型、可用 TTS + 音色。视觉固定为三步：麦克风并入 Speech Recognition；ASR 与 TTS 保留在各自步骤和目录中，readiness 逐项判断。
- 三步卡片必须同时呈现内容，并用左侧编号与纵向连接线表达顺序；不得回退成四张独立卡或默认折叠 Provider/TTS。`Finish Setup` 在未完成时保持不可用，Review 模式下同一位置提供返回 Ready 页的明确操作。
- ASR/TTS 安装分别沿用既有受管安装、哈希校验、manifest 和原子提升边界；两个目录各自负责成功、失败、取消、重试、选择与进度显示，不增加上层套餐协调器。
- 底部下载进度 `N%` 聚合所有当前活跃的本地模型安装，不要求模型在下载开始前已被选中，也不把麦克风授权或 Provider 表单伪装成百分比。没有活跃下载时，Setup 和底部入口均不显示虚假进度；关闭 Settings 后只由底部入口继续反馈后台进度。
- ASR/TTS 下载仍以 64 KiB buffer 流式读写，但前台进度按约 4 MiB 合并后才更新，并用后台背压发送保证非整步长的最终完成值不被丢弃；不得把每个网络读取块直接映射为 GPUI `notify()` 和整页重绘。
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
- 四项硬条件通过后立即完成 Setup，不要求真实录音。Ready 页面不提供额外 `Try Voice` 按钮，真实使用与验证统一走现有 `⌘E` 端到端路径。
- 后台完成 Setup 时只发送一次非阻塞完成通知；通知不抢焦点、不自动调用 TTS，也不替用户启动语音。
- 简化主页面保留当前模式、单次触发方式、麦克风、任务播报、窗口光效、当前音色和 Preview；Ready 摘要三行都提供 `Change`：ASR 直接展开 Speech Recognition 模型目录，AI Connection 进入同一 Voice Setup 并滚到 Provider 配置，TTS 进入 Spoken Replies 的 Local TTS Model 并滚入可见视口。TTS 目录不在 Advanced 重复出现。
- 普通配置继续即时生效并异步持久化，不新增全局 Save / Cancel。模型安装、Provider 验证和 Keychain 写入以各自状态和错误反馈收口。
- 任何模式、麦克风、ASR、Provider、模型、TTS 或音色变更继续遵守既有取消 generation 与共享 Voice Runtime 约束，陈旧异步结果不得恢复旧配置或自动重启麦克风。
- 该规格不改变 `⌘E` 的固定快捷键、Single Voice Command / Continuous Listening 语义、Agent 工具执行或 TTS 播放业务状态机；改变的是使用这些路径前的 readiness 门槛与 Settings 信息架构。

## Testing Decisions

- 最高层主 seam 使用现有 headless GPUI e2e：真实打开 Settings、真实点击带稳定 selector 的控件、真实发送 `⌘E` 键盘事件，并从统一 Settings/Voice e2e snapshot 观察用户可见状态。不要为每个子模块各建一套 UI 测试入口。
- 主 e2e snapshot 扩展 readiness、待修复项、模型安装进度、Provider 连接阶段和凭据显示状态；测试断言外部状态与可见控件，不断言内部任务数量或私有字段布局。
- e2e 覆盖 fresh store：底部显示 `Set up Voice`；按 `⌘E` 打开 Settings → Voice；录音、Dialogue 和 TTS 均未启动。
- e2e 覆盖三步视觉结构与四项逻辑状态：Settings 侧栏、Voice 页头、三步编号、纵向轨道、麦克风内嵌、Provider 直接可见、底部 Finish、Ready 三行摘要均有具体正断言；独立 Setup 窗口、独立麦克风卡和 Ready `Try Voice` 均有具体负断言。
- 麦克风权限通过现有权限 seam 注入 authorized、denied、restricted；验证授权恢复入口、Ready 门槛和 `⌘E` 不误启动。
- ASR/TTS 独立目录通过现有假模型 transport 与安装管理 seam 覆盖安装、并行进度、页内百分比与容量反馈、单项失败、Retry、Cancel、关闭 Settings 后继续、重新打开恢复、退出后重扫和选择持久化。
- Provider 通过本地假 OpenAI-compatible server 覆盖 endpoint 失败、认证失败、模型列表成功、模型列表不支持、手动模型验证、所选模型失效和现有配置自动通过；测试不得访问真实付费 API。
- Keychain 使用窄凭据存储接口的内存替身做日常单测/e2e；断言保存后 UI 默认显示 `*********`，眼睛按钮只在显式点击期间显示内存中的 Key，再次点击恢复掩码；Replace 更新引用，Remove 清除凭据，SQLite 快照不包含 API Key 明文。
- 单独覆盖旧 SQLite API Key → Keychain 的成功迁移、Keychain 写入失败时不清旧值、重载后 Keychain 优先三个场景。
- readiness 纯逻辑测试覆盖 `SetupRequired → SettingUp → Ready → Unavailable → Ready`，并逐项验证麦克风、ASR、Provider、TTS 哪一项失效会产生哪个修复目标。
- Settings e2e 覆盖 Setup 与 Ready 主页面默认不铺开完整 ASR 目录，Local TTS Model 直接位于 Spoken Replies 且不在 Ready Advanced 重复；点击“选择其他识别模型”或 ASR `Change` 后 ASR 目录在同一页面可达，点击 TTS `Change` 不得展开 ASR，且首个 TTS 模型经滚动偏移投影后必须位于 Settings 视口内。Base URL、API Key 与 trace 默认不可见，进入 Advanced Settings 后仍可达。
- 快捷键 e2e 覆盖 Ready 状态下既有 Single Voice Command Hold/Toggle 与 Continuous Listening 行为不变，防止 readiness 门槛破坏已确认业务路径。
- 通知 e2e 覆盖 Settings 关闭后后台完成只通知一次、入口切换为 `Voice`、不抢焦点、不自动播放 TTS，重新载入不重复发送旧完成通知。
- `Unavailable` e2e 分别覆盖权限撤销、ASR 损坏、Provider 失效和 TTS 损坏；每个场景按 `⌘E` 必须打开正确修复项，且不得启动录音。
- 模型域测试继续验证哈希、manifest、受控目录与原子提升；Provider 域测试验证模型发现与连接错误分类；App 测试只验证编排，不复制底层安装或 HTTP 细节。
- 真实 macOS Keychain 读写、系统麦克风权限、受管模型安装与端到端真实语音保留为 ignored L3 smoke，并在报告中与确定性 L1/L2 证据分开说明。
- 所有新增 Rust 测试必须遵守仓库规定的场景、数据构造、执行过程、预期结果注释结构；每个断言包含失败说明，关键保留与删除状态逐一断言，数值预算写明推导。

## Visual Verification Evidence

- [Voice Setup 主视图](../verification/voice-settings-setup-final.jpg)
- [Voice Setup 下半页与 Finish Setup](../verification/voice-settings-setup-lower-final.jpg)
- [Voice Ready 主视图](../verification/voice-settings-ready-final.jpg)
- [Voice Ready 模型更换入口](../verification/voice-settings-ready-model-change-final.png)
- [Voice Ready Input 区](../verification/voice-settings-ready-input-final.jpg)
- [Voice Setup 的 Local TTS Model 归入 Spoken Replies](../verification/voice-settings-spoken-replies-tts-final.png)
- [API Key 默认遮罩与显隐按钮](../verification/voice-settings-api-key-masked-final.png)
- [API Key 主动点击后的临时显示](../verification/voice-settings-api-key-revealed-final.png)

以上截图来自启用确定性预览状态的隔离开发版 `.app`，通过真实 macOS 窗口启动、滚动和截图完成；预览 seam 仅在 `e2e-test-support` feature 下编译，不进入默认生产构建。

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
- 自动回显、导出或在未点击眼睛按钮时暴露 API Key；显式临时查看不提供独立复制或导出动作。
- 在 GPUI 前台同步执行网络探测、Keychain 调用、模型扫描、下载或完整性校验。

## Further Notes

- 本规格把当前“无 TTS 时仍可纯文字结束回复”的运行时降级从 Voice Ready 定义中移除。内部仍可保留错误恢复，但用户不能在缺少 TTS 时完成首次设置或进入正常 Voice 状态。
- fresh store 不自动安装或选择模型；只有用户在 ASR 或 TTS 目录中显式点击 Install 后，应用才启动对应模型任务，并在首个有效安装完成时按各目录既有规则选择模型/音色。
- 现有 Voice 配置矩阵、Settings 点击、麦克风权限恢复、ASR 模型目录、TTS 模型目录与快捷键模式 e2e 是优先复用的测试先例。
- `Voice unavailable` 是配置后失效，不等同于首次未配置；错误文案和修复入口必须保留这一差异。
- Advanced Settings 是渐进披露，不是删除能力。现有 ASR 可选模型、Ollama、Base URL、手动模型与 Trace 仍需可达；TTS 的安装、删除、切换与 Preview 统一在 Spoken Replies 可达，不回填 Advanced。
- 规格已按 Markdown 写入仓库，并以批准的 Voice Setup / Voice Ready 设计稿作为视觉基线完成实现；后续视觉调整必须同时更新本规格与对应 GPUI e2e 结构契约。
