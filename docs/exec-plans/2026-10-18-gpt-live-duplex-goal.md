---
title: Goal · GPT-Live 实时双工接入与自主验收
subtitle: 实现 → 离线验证 → 真实 App → 真实云端；四层证据齐全才完成
template: doc
theme: paper
lang: zh
date: 2026-10-18
status: 已定义 · 实现与功能验收均待执行
---

在 Norma OS macOS 桌面新增 GPT-Live 通话模式。用 `gpt-live-1` 持续听说，通过 client delegation 调用现有 Norma Agent。Agent 自主执行测试、操作隔离 App、核对项目副作用并整理验收证据。

## A Goal 与完成边界

```callout info 当前状态
用户要求生成 Goal 任务，并将提供 API Key 用于测试。接入调研和任务定义已完成；生产实现与功能验收尚未执行。创建任务不代表功能完成。
```

| 项目 | 交付要求 |
| --- | --- |
| 核心用户流程 | 选择 GPT-Live → 连接 → 持续双向语音 → 发起项目任务 → 插话与补充 → 收到真实回执 → 挂断。 |
| 后台 | 复用 Persona、NormaSession、NormaToolRuntime 与桌面 Adapter。 |
| 首个媒体实现 | WS + 16 kHz PCM16LE；双向音频独立推进。 |
| 媒体验收不通过时 | 定位后修复；必要时采用官方推荐的 WebRTC，沿用同一委派与工具边界。 |
| 用户配置 | 模式、凭据引用、音色、设备与就绪状态可见，配置可重载。 |
| 兼容性 | Legacy / Doubao 原行为、默认值、配置和语音快捷键继续可用。 |
| 交付物 | 实现、测试、可运行二进制、复现命令、脱敏报告和真实窗口截图。 |

首轮范围为 macOS 桌面和用户自带 Key。iOS、平台共享 Key 网关和 SIP 不属于本 Goal。参考[接入方案](../design-docs/2026-10-18-gpt-live-desktop-integration.html)与[官方协议调研](../research/2026-10-18-gpt-live-official-integration.md)。

## B 可直接执行的 Goal 任务

```text title="Goal 执行指令"
在 Norma OS macOS 桌面实现 GPT-Live 实时双工通话并自主验收。
唯一任务清单：docs/exec-plans/2026-10-18-gpt-live-duplex-goal.md。
技术方案：docs/design-docs/2026-10-18-gpt-live-desktop-integration.md。
官方事实：docs/research/2026-10-18-gpt-live-official-integration.md。

新增独立 GptLive 模式，以 gpt-live-1 和 client delegation 接入现有 Norma Agent。
先用 WS + 16k PCM16LE；不得以 Realtime API 或 ASR→Chat→TTS 冒充 Live。
复用现有音频、Persona、工具、权限和持久化回执，不复制项目副作用实现。
先完成不依赖 Key 的协议、工具、生命周期和真实 App 设置验收。
用户提供 Key 后执行真实握手、持续语音、任务委派和外放插话测试。
逐项执行 C–G 的任务与验收；失败则修复并重跑相关场景。
每项填写证据、命令、实际结果和状态，不能用测试文件存在代替运行。
只有全部必需验收通过、无未处理高风险问题时，才能标记 Goal complete。

遵守 AGENTS.md：先 TDD；测试注释和断言完整；单文件≤500行；不加allow。
GPUI 前台不阻塞；域层不依赖 GPUI/Storage；数据库操作放 app 后台。
UI 先读 apple-design、相关 gpui-*；菜单遵循 Material，覆盖交互反馈。
真实 App 先跑 desktop-e2e；覆盖不到才用 Computer Use。
独立数据目录和 socket，不终止用户常驻 App，不改用户项目做破坏测试。
Key 不进入仓库、命令参数、日志、HTML、截图或 fixture。
未获 Key、账号无权限或环境未就绪，记录事实，不能冒报云端通过。
实施阶段涉及并发、权限与持久化；收口前用独立 subagent 做对抗式审查。
本任务定义不包含发布、推送或安装替换常驻 App。
```

Goal 执行时先记录当前分支、HEAD 与用户未提交文件。需要隔离施工时从当前已验证基线建立 worktree，保留现有改动。此文档及对应 HTML 是任务定义，本轮不修改生产代码。

## C 分阶段交付与检查点

| 阶段 | 执行内容 | 通过条件 | 初始状态 |
| --- | --- | --- | --- |
| G0 基线与验收夹具 | 核对源码、依赖与当前构建；冻结用例；准备假 Live 和隔离项目 | 每条验收有测试接缝；基线结果、环境和失败归属有记录 | 待执行 |
| G1 Live 协议 | 新增零 GPUI/Storage 能力层；连接、事件、连续音频、有界队列、错误与关闭 | AC01–AC04、AC11 聚焦测试通过 | 待执行 |
| G2 委派与工具 | 双向转写时间线、委派关联、revision、专属来源、账本与结果路由 | AC05–AC10 正反例通过；实际副作用次数正确 | 待执行 |
| G3 桌面产品接线 | 采集/AEC、播放、配置、状态、静音和挂断；任务取消独立 | 真实 App 配置与生命周期可用；旧模式回归通过 | 待执行 |
| G4 离线与真实窗口验收 | 假上游集成、重连/超时/项目切换、深浅主题、键盘；审查修复 | AC01–AC14 的非云端部分有运行证据 | 待执行 |
| G5 Key 与真实云端 | 真实 Live 会话、真实项目工具、设备回环、插话、稳定性与用量 | AC15–AC18 通过；Key/ENV 相关待验全部清零 | 待 Key；未开始 |
| G6 收口 | 修复审查问题、跑最终检查、更新证据表与使用说明 | H 完成定义全部成立 | 待执行 |

每个实现阶段先补一条可失败的聚焦测试，再写最小实现。只重复失败、变更或仍有疑问的验证。一个聚焦测试超过 15 分钟未得到绿灯，报告具体命令与阻塞。

G1–G4 可以在没有真实 Key 时推进。不要为避免等待 Key 降低 G5 的标准；也不要把账号认证错误当作产品逻辑已验证。

## D 离线验收：行为与反例

所有状态先记为“待执行”。表中名称是待新增测试或场景，尚不是已有测试。

| 编号 / 用例名 | 刺激与步骤 | 必须出现 | 必须不出现 |
| --- | --- | --- | --- |
| AC01 `live-startup-barrier` | 本地假 WS 延迟 started，再发送它；检查配置 | session.start 首发；started 后有音频 | ready 前音频；Realtime 路由或逐句 commit |
| AC02 `live-duplex-overlap` | 假上游连续输出时持续推送输入，后台任务挂起 | 同一观测窗口中输入、输出计数都增长 | 等播放或任务完成才收音 |
| AC03 `live-pcm-and-backpressure` | 16k mono PCM；奇数字节、无效 base64、突发慢网 | 格式和样本对齐正确；积压有上限、指标和处理策略 | 错格式进入 sink；无界增长；静默堆积 |
| AC04 `live-caption-overlap` | 用户和助手重叠片段，附带迟到片段 | 双方字幕分别增长；原文与时轴保留 | 助手附和强制结束用户输入 |
| AC05 `live-delegation-context` | 委派先到，转写后到；再测空上下文 | 保留委派，补齐后只运行一次；缺信息会澄清 | 空正文派工；每片段一次请求 |
| AC06 `live-correction-revision` | 旧请求处理中补充新范围；旧结果迟到 | 执行前/回传前核对 revision；采用新范围 | 过期结果口播；隐瞒已产生副作用 |
| AC07 `live-tool-durable-dedupe` | 重放通知、并发重复调用、重启后重放、unknown | 同一操作不重做；明确的新请求仍可执行 | unknown 自动重做；新 ID 一律视作新意图 |
| AC08 `live-source-and-authorization` | 讨论、模糊“可以”、明确请求、删除错误目标 | 合法请求成功；来源、项目与目标校验生效 | 伪装 DoubaoVoice；讨论触发写入 |
| AC09 `live-speech-vs-task-cancel` | 已接受任务时打断语音；再显式取消任务 | 插话后任务继续；明确取消进入真实取消路径 | 停播误取消；未取消成功就报成功 |
| AC10 `live-receipt-and-playback` | accepted、完成、失败和 appendix ACK 分别到达 | 状态分开；回传≤500 tokens且关联正确 | accepted 报完成；ACK 报完整播完 |
| AC11 `live-close-and-late-events` | 正常 close、无 closed、旧 generation 音频/调用 | 资源有界释放；最终用量确认或标未确认 | 重开后旧音频；旧项目执行；前台 join |
| AC12 `live-settings-readiness` | 缺 Key、缺后台、配置重载与失败连接 | 通话/工具就绪分开；错误具体；配置可恢复 | 缺后台阻止闲聊；断线偷偷切旧模式 |
| AC13 `live-mode-regression` | Legacy/Doubao/GptLive 切换并重开 | 仅当前模式工作；旧配置和快捷键保留 | 双重收音；旧 TTS 与 Live 重复播报 |
| AC14 `live-ui-feedback` | 深浅主题，用鼠标及键盘遍历控件 | hover/pressed/focus/selected/disabled 清晰 | 只用颜色区分状态；窄窗口遮挡主操作 |

AC07 必须从 Adapter 或真实隔离项目核对副作用计数。仅检查 mock 收到了几次调用不够。崩溃窗口无法确定结果时，保持 unknown 并核对状态，不承诺普遍 exactly-once。

输入、输出队列都以音频时长记录积压。G0 冻结测试上限并写清推导；测试须证明越界后行为确定。计时逻辑用可控时钟；避免依靠任意 sleep 使测试碰巧通过。

## E Key 到位后的真实验收

Key 仅用于用户要求的 GPT-Live 测试。测试前读取当前官方 Live 文档，核对模型和鉴权。通过现有凭据机制或进程环境注入；测试报告只记凭据引用，不记值。不会因为进入 Goal 而自动扩大其他外部访问。

| 编号 | 真实执行步骤 | 通过条件 |
| --- | --- | --- |
| AC15 真实连接与声音 | 用测试凭据连接 gpt-live-1；连续上传 PCM，接收并在真实 App 播放；正常关闭 | 官方 session_id、started、实际声音和 closed 有对应证据；鉴权失败单列 |
| AC16 真实任务执行 | 在隔离项目查询状态；创建测试 Agent 或卡片；派只读任务；查询回执；明确取消另一测试任务 | 有实际 node/task ID 与项目状态；口播与真实结果一致；没有重复派工 |
| AC17 真实双工与插话 | 播报中说话、轻附和、明确“停一下”、补充任务范围；分别测可用耳机和内置外放 | 输出期间仍收音；明确打断停止旧播报；持续对话；无自激回声或误派工 |
| AC18 稳定性与恢复 | 连续通话10分钟；静音/恢复；挂断重开3次；有界模拟断网后恢复 | 资源/队列不持续增长；无旧音频或旧项目操作；未确认任务先核对，用量单独记录 |

AC15 的音频夹具必须按真实采样节奏推送，包含静音。自动判断至少检查输入/输出 PCM、字幕语义和任务事实。握手成功、非空音频和“模型说完成”都不能单独判定通过。

声学验收用固定语句与可重复的本地语音激励，实际经过测试 Mac 的扬声器与麦克风。录音只在内存中分析，报告保存脱敏事件和指标。无可用设备或系统授权时记为 ENV；数字注入不能替代真实外放结论。

以下为本项目初始工程门槛，不是官方 SLA，也不是已测结果：

| 指标 | 默认门槛 | 测量定义 |
| --- | --- | --- |
| 本地明确停止 | 10次全部≤250ms | 停止输入事件到 sink 停止且待播队列失效；核对硬件输出 |
| 明确语音插话 | 每种设备10次，至少9次≤1秒停止旧播报 | “停一下”语音开始到旧播报结束；会话与收音保持 |
| 普通中文回复 | 10个固定闲聊请求，首个有用语音 p95≤3秒 | 从用户实际停语到实际播放；附和不计有用回复 |
| 稳态资源 | 10分钟无单调积压，达到设计上限时有确定行为 | 记录音频队列时长、内存、采集/播放线程和重开前后计数 |

必须报告原始样本数和失败样本，不能只挑最快一次。若门槛失败，修复或评估 WebRTC；不能改宽门槛后直接记绿。目标调整需保留理由与原结果，并向用户说明。

p95 使用最近秩法。10个样本时取排序后第 `ceil(0.95×10)=10` 项，并保留全部样本。

## F 自主验收的运行方式

使用 `norma-desktop-e2e` 的真实 App 流程。被测二进制为 `target/debug/norma-os`，使用独立 `NORMA_TEST_DATA_ROOT` 和 `NORMA_SOCKET`。只管理本次 owned 进程，不结束用户已打开的 Norma。

新增场景按模块维护：

| 模块 | 场景计划 | 证据位置 |
| --- | --- | --- |
| settings | `settings-live-configuration` | `Scenarios/settings/`、`checks/settings/`、`Fixtures/settings/` |
| voice | `voice-live-lifecycle`、`voice-live-duplex` | `Scenarios/voice/`、`checks/voice/`、`Fixtures/voice/` |
| agent | `agent-live-delegation` | `Scenarios/agent/`、`checks/agent/`、`Fixtures/agent/` |

上述路径均在 `test/desktop-e2e/` 下。每个模块只修改自己的 Module.swift；Catalog 仅追加 manifest。当前驱动表达不了的声音/流程，先记录覆盖缺口，再用专用集成测试或 Computer Use 补齐。不会为本任务擅自添加 App 测试钩子。

环境检查缺权限或锁屏时，退出码3记 ENV，并等待环境恢复；不要换输入方式绕过。产品断言失败记 PRODUCT，修复后重跑受影响场景。每张关键截图逐张看图，写符合、不符合或不确定；文件存在不算视觉通过。

验证命令按实际修改范围执行：

```bash title="实现阶段验证命令 · 新增crate/场景后才可运行"
cargo test -p live_voice
cargo test -p agent
cargo check -p app
cargo test --workspace
cargo build -p app --release
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-socks-proxy-support.sh
bash scripts/desktop-e2e.sh settings
bash scripts/desktop-e2e.sh voice
bash scripts/desktop-e2e.sh agent
```

改到 `voice_input`、`audio`、`storage` 或 `ui` 时追加对应 crate 测试。app 受影响的单元/集成用例也要运行。最终 workspace 检查只在对应基线与变更范围内解释，不能悄悄忽略失败。

协议 fixture 放 `crates/live_voice/tests/support/`；域层单测放该 crate 的 `tests/unit/`，集成测试放 `tests/*.rs`。app/agent 的测试同样按 crate 组织。真实 API 用例默认 ignored，显式启用并有超时；禁止无限重试消耗额度。

本轮只定义任务，以上构建、产品测试和真实通话均未执行。

## G 证据台账与审查

| 证据等级 | 需要记录 | 判定边界 |
| --- | --- | --- |
| L1 单元 | 失败测试→实现→通过记录、命令、退出码与具体断言 | 只证明纯逻辑或协议规则 |
| L2 集成 | 假 WS、真实 Adapter/账本与隔离项目的前后状态 | 只证明本地执行结果，不证明云端或声学 |
| L3 真实 App | 二进制路径、构建版本、独立数据目录、report.json、app.log、截图与看图结论 | 真实窗口通过，不替代真实模型连接 |
| L4 真实云端/声学 | 模型/session ID、设备、时延样本、任务 ID、实际结果、关闭用量 | 实际听说和任务结果分别核验 |

运行产物写入 `artifacts/gpt-live/<run-id>/`；desktop-e2e 保留其原有证据目录并记录引用。`run-id` 绑定源码 HEAD、工作区差异摘要、二进制与环境。可长期保存的脱敏结论汇总到 `docs/quality/2026-10-18-gpt-live-acceptance.md`，实施时再创建。

每条 AC 的记录格式：`状态 / 命令或步骤 / 期望 / 实际 / 证据路径 / 已知限制`。状态使用待执行、通过、PRODUCT、ENV、待Key或待核对。截图或图片须保存文件，并在报告中引用绝对路径。

收口前让独立 subagent 做只读对抗式代码审查。给出固定基准、变更范围及 AC 清单；重点寻找委派乱序、来源权限、重连重复、取消误用和测试假阳性。主 Agent 核验发现，修复后补失败用例，不以审查意见替代运行证据。

## H 完成定义与初始记录

- [ ] G0–G6 的产物和实际运行记录齐全。
- [ ] AC01–AC18 的必需部分全部通过，待Key、ENV和待核对项清零。
- [ ] 真实 App 内可连续双工交谈并触发真实项目工具。
- [ ] 耳机及内置外放的结果分别记录；必要设备未验则不宣称整体声学通过。
- [ ] 时延门槛达标，或已明确说明偏差并获得用户对目标调整的决定。
- [ ] 插话、修正、显式取消、项目切换和重开不会产生重复或过期副作用。
- [ ] 既有模式、快捷键、生命周期与性能护栏无本次引入的回归。
- [ ] Key 未泄露；真实测试结束后连接释放，最终用量记录完整或异常已解决。
- [ ] 必需检查通过；无未处理高风险审查发现。
- [ ] 最终给出可运行二进制、复现步骤、证据索引和真实剩余限制，再标记 complete。

| 日期 | 记录 | 结果 |
| --- | --- | --- |
| 2026-10-18 | 完成架构与官方文档调研，生成接入方案 | 文档已生成，生产代码未改 |
| 2026-10-18 | 用户要求生成可自主验收的 Goal，稍后提供 Key | 本任务定义已生成，所有功能验收待执行 |

缺 Key 不等于功能通过；构建通过不等于 App 验收通过；假上游通过不等于真实 GPT-Live 可用。
