---
title: 语音指令重构：实施计划与进度
subtitle: 合并本地识别入口，明确画布目标，统一提交后再评估 Jev
template: doc
theme: paper
lang: zh
source: 2026-11-01 · 设置原型对齐通过 · 源码 243c66a9 · 真实麦克风验收阻塞
---

用户已选定分组列表，生产设置与单次语义提交已实施。
首轮实现与审查为 c752adfc / b5f27c88；本轮原型对齐修正为 243c66a9。
设置页的中英文、宽窄窗口与本地规则预览已通过真实 App 定向验收。
真实麦克风全链路仍阻塞，不能据此宣布整个语音工作分支验收完成或具备合并条件。
Jev 先做合成文本契约验证和旁路比较。
生产接入取决于部署、准确率和时延证据。

## A 已确定的方向与本轮范围

| 已确定 | 实施含义 |
|---|---|
| 两个本地选项合并 | 来源只分本地与云端；内部保留兼容引擎 |
| 明确画布上的谁 | Bruno、Henry 来自实时目录，不写死在提示中 |
| 同时理解动作与目标 | 区分执行者、被提及者和任务对象 |
| 候选经过宿主提交 | 所有理解来源只提议，执行器只有一个 |
| 按 Google Material 重构设置 | 分组、紧凑选择器、渐进展示、明确状态 |
| 设计已交付，现获实施授权 | 在独立 worktree 实施、验收、审查并提交 |

原本的语音评测 Goal 保持停止。
真人录音与转写仍只保存在本机。
本轮不要求补录，也不恢复批量 ASR 评测。
豆包、GPT-Live 与 TTS 协议不在改造范围。

## B 原型决策：先选信息结构

| 方向 | 信息结构 | 适用场景 | 代价 |
|---|---|---|---|
| 分组列表 | 常用设置集中展开，旁边预览理解结果 | 日常快速调整 | 页面更长 |
| 流程分步 | 输入、理解、派发、回复逐步查看 | 需要理解语音链路 | 跨组修改要切换 |
| 概览与抽屉 | 首屏显示当前配置，抽屉内编辑 | 低频配置与紧凑页面 | 单次编辑多一步 |

三版共用同一份原型状态与固定示例。
支持来源、交互模式、麦克风和回复开关。
支持目标澄清、搜索选择、浅深色和中英文。
示例解析不调用 Jev，也不访问真实麦克风。

原型：`docs/prototypes/voice-command-settings/index.html`。
按 1、2、3 或方向键切换布局。
在输入框与弹层内，按键优先操作当前控件。
外层对话模式切换沿用现有产品行为。
原型只展示选中语音指令后的内容区。

参考 prototype、frontend-design、apple-design 与 emil-design-eng。
Material 原则用于分组、48 px 菜单与独立浏览状态。
保留 macOS 快捷键、键盘焦点与可访问性习惯。
仅保留必要的开关反馈，不添加循环动效。

## C 实施阶段与依赖

```flow LR
选定原型 -> 配置迁移与设置重构
选定原型 -> 语义候选与目标目录
语义候选与目标目录 -> 唯一提交与澄清
配置迁移与设置重构 -> 定向App验收
唯一提交与澄清 -> 定向App验收
语义候选与目标目录 -> Jev合成文本契约
Jev合成文本契约 -> 有条件的旁路比较
有条件的旁路比较 -> 是否启用Jev决策
```

| 阶段 | 交付 | 完成标准 |
|---|---|---|
| P0 选定原型 | 确定三版中的信息结构与边界 | 用户对布局和主要设置项达成一致 |
| P1 合并来源与页面 | 本地/云端来源、旧配置迁移、状态反馈 | 原选择可迁移，单次与持续聆听兼容 |
| P2 统一语义 | 原话、动作、目标角色、约束与来源版本 | 聚焦失败测试转绿，无业务副作用 |
| P3 单一提交 | Voice 范围的候选、澄清、幂等与回执 | 规则与模型不会重复执行或越过校验 |
| P4 Jev 契约 | 仅合成文本的 API 与 Choice 适配验证 | 输入输出、错误、版本与取消可追溯 |
| P5 旁路比较 | Jev 无执行权地比较候选选择 | 数据范围获准，结果与时延符合约束 |
| P6 可选启用 | 明确适用场景、退出策略和预算 | 准入门槛有实测证据，否则保持旁路 |

P1 与 P2 只在接口约定后拆分实施。
不把并行列项当作授权启动其他 Agent 或重型构建。
P1 至 P3 的收益不依赖 Jev 是否可部署。

## D 配置迁移与 UI 接入

增加产品层的本地/云端来源表达。
实际引擎仍通过既有 ASR provider 适配。
ASR crate 不读取 Storage，也不读取 UI 状态。

| 旧配置 | 迁移后的来源 | 内部处理 |
|---|---|---|
| LocalSherpa | 本地 | 保留当前引擎，不强制升级到 A1 |
| LocalA1 | 本地 | 保留选择，并检查单次模式与准备状态 |
| CloudAzure | 云端 | 保留已有服务配置与凭据引用 |

引擎在录音开始前确定，并绑定本次录音。
持续聆听不能因合并入口而启用当前 A1。
A1 失败不能把 partial 冒充 final。
资源不足不自动改用云端。
切换来源仍使用既有取消与 generation 机制。
配置写入走后台，不能阻塞 GPUI 前台。

| 主要位置 | 预计改动 |
|---|---|
| `crates/app/src/settings/voice/view/setup/recognition.rs` | 合并来源选择，常用与高级信息分层 |
| `crates/app/src/settings/voice/state/a1.rs` | 复用准备、取消与兼容性状态 |
| `crates/app/src/settings/voice/state/values.rs` | 迁移来源表达，保留旧配置可解释性 |
| `crates/ui/locales/voice.zh-CN.json` | 新文案与状态国际化 |
| `crates/ui/locales/voice.en.json` | 对应英文键与占位符 |

上线来源合并，不等于把 A1 升为默认。
A1 默认切换须先完成独立的真实单次录音验收。
当前该验收尚未完成，不能继承历史模型速度结论。

## E 目标目录与唯一提交

App 从当前 Runtime 投影完整目标目录。
目录包含项目、卡片节点、成员、别名与会话状态。
当前 roster 含未上画布的成员，必须区分状态。
当前画布摘要有数量上限，不能当作完整目标目录。
同名多节点保留多个候选，不能使用首个匹配。

| 语义 | 必须保留的区别 |
|---|---|
| Bruno 或 Henry | 指向稳定的项目和节点身份 |
| Henry 检查 Bruno 的方案 | Henry 是执行者，Bruno 是被提及者 |
| Norma 自己 | 普通回答不派工给画布成员 |
| 画布外成员 | 不能用在场成员替代，也不无授权激活 |
| 未知或音近名称 | 保留正文并澄清，不被 confidence 覆盖 |
| 继续与新建 | 续聊失败不静默新建 |

已实现纯决策模块：`crates/agent/src/voice_command.rs`。
已实现宿主编排：`crates/app/src/app/agent_runtime/voice_command.rs`。
先补聚焦失败测试，再实现接口。
具体文件组织在落地时按现有职责和行数约束调整。

调整仅覆盖 Voice 且交互方式为单次的入口。
共享直接寻址规则不能因此改变豆包行为。
现有主模型输出同轮候选，不新增串行纠错模型。
需要改造当前工具可直接执行的语音分支。
提议工具只能返回提议状态，不能伪造执行成功。

宿主提交前核验 scope、revision、目标和权限。
澄清沿用逻辑请求，新录音保留独立来源标识。
提交后只按真实回执播报状态。
未知结果先核验，不自动重发。
现有内存去重不代表跨进程持久幂等。
跨进程恢复须单独验证现有回执能力并补足缺口。

## F Jev 接入的启用条件

`TYPESAFE_API_KEY` 已配置到当前用户登录环境。
新终端已验证可继承；官方模型目录与合成文本 API 已验证。
密钥不进入仓库、原型、日志或测试结果。
不把凭据配置等同于授权上传真人语音数据。

P4 先使用人工合成文本，验证固定官方端点。
记录服务实际返回的模型版本，避免只记录 latest 别名。
验证 Choice ID、概率分布、confidence 和错误结构。
取消、超时、异常与迟到结果均须覆盖。
一轮选择完整动作与目标方案，不随意拼接独立字段。

当前只核实外部 API 接法。
Jev 本机部署能力尚未确认。
若整条链路要求本机处理，远端 Jev 保持禁用。
不能因使用本地 ASR，就宣称全部模型调用都离线。

| 约束 | 计划怎样守住 |
|---|---|
| 不新增串行模型调用 | 默认复用主模型；可选 Jev 与主路径并行提议 |
| T_final p95 不超过基线 1.1 倍 | 沿用 Finish 到 Qwen 终稿的口径，另做实测 |
| T_route p95 小于 5 ms | 终稿到参数确定；模型时间不能剔除 |
| 路由错派为 0 | 验收样本出现一次错派即不启用 |
| 其他任务不能暂停 | 不结束别人的进程；资源不够就不启用并行 |

若 5 ms 门槛用于全部请求，Jev 只能旁路。
终稿后模型选择不能未经实测宣称满足该门槛。
若讨论模型路径的独立预算，须显式形成新决策。
保留 direct、model、clarify 分组，不能重命名耗时。
旁路与主路径都不能直接持有工具写权限。

## G 验收标准与证据映射

| 标准 | 对应证据 |
|---|---|
| 两种本地旧设置正确迁移 | Settings 聚焦单测；重启后的真实 App 状态 |
| 单次和持续模式能力正确 | 状态单测；真实录音的模式切换与取消 |
| 预览不派发、失败不冒用终稿 | ASR/Voice 定向单元与集成测试 |
| Bruno/Henry 与角色判断正确 | agent 语义用例；目标目录集成测试 |
| 重名、画布外与摘要缺失不误派 | 完整目录、覆盖标记与负向用例 |
| 更正和澄清不造成重复提交 | revision、迟到调用、幂等与回执集成测试 |
| 新建、回复和取消语义正确 | 类型化命令的正反断言及真实结果 |
| 页面双语可读且可键盘操作；浅色真实窗口受基线限制 | Voice 模块定向 desktop-e2e --visual |
| 所有用户提示中英文一致 | i18n 检查与双语定向验收 |
| Jev 契约及异常可控 | 合成文本契约测试；无私有数据上传 |
| 准确率和时延门槛成立 | 配对样本、配置哈希、阶段结果目录 |

修改 agent 后运行 `cargo test -p agent`。
修改 app 后运行 `cargo check -p app`。
修改 ASR 行为后运行 `cargo test -p asr`。
新文案运行 `python3 scripts/check-i18n.py`。

执行受影响范围的行数、禁止 allow 与性能护栏。
按本次 implement 授权完成聚焦验证、最终完整测试与真实 App 验收；构建最多两路，不结束其他任务。

原型已有 9 项固定示例逻辑测试通过。
它们不是生产路由测试或真人准确率结果。
浏览器宿主当前不可用，原型实屏验收尚未完成。
这项环境缺口不能替代未来真实 App 验收。

## H 讨论顺序与交付记录

### 本轮实施进度

用户已调用 implement，授权按本计划实施并创建 worktree。
工作分支：`codex/voice-command-redesign-20261031`。
基点提交：`f0740f6cc2f2a03da32e15ddb238abf3dfdb4af2`。
工作树：`.worktrees/voice-command-redesign-20261031`。

用户已选定分组列表布局，P0 完成。

| 阶段 | 当前证据 | 尚待完成 |
|---|---|---|
| P0 | 用户选定分组列表 | 完成 |
| P1 | 双来源、迁移和持续兼容；181 项语音增量与双语真实窗口通过 | 现行主题范围完成；退役浅色未实屏验收 |
| P2 | 目标角色、别名、同名、画布外、否定与引用护栏；完整测试通过 | 完成 |
| P3 | 整批校验、补答重规划、取消与真实回执；worker 3 项、模型护栏 4 项通过；审查反例关闭 | 完成；跨进程未知结果仍不自动重发 |
| P4 | Choice 契约、超时、取消及迟到覆盖；Rust 官方合成 smoke 成功 | 完成；不构成生产准入 |
| P5 / P6 | 生产 Jev 禁用 | 未满足部署、数据及路由时延准入，不启用 |

当前聚焦命令使用任务缓存包装器。
缓存任务名为 voice-command-redesign-20261031。
已通过 agent 语义 12 项、护栏 7 项、澄清与回执 5 项、Jev 纯契约 1 项和传输 3 项。
设置迁移 4 项、分组页面交互 1 项已通过；这些数字不是准确率评测。

完整 workspace 测试与独立审查修复已完成。
真实 App 双语验收、最新 release 与提交钩子均通过，源码已提交。

已按选定布局完成设置、完整目标目录和提交语义。
Jev 部署范围与时延预算继续受准入门槛控制。
不先扩大模型路由覆盖，再补执行校验。

分组列表接入生产页面；其余布局保留为原型参考。
快速路径覆盖和 Jev 部署按实际验证结果逐项记录。

每阶段记录源码提交、配置哈希、命令和结果目录。
工作区有其他人的改动，只提交本任务明确文件。
不清理、不回退、不覆盖其他任务内容。
真人转写、音频和密钥不得提交。

关联架构：`docs/design-docs/2026-10-31-voice-command-jev-architecture.html`。
原型说明：`docs/prototypes/voice-command-settings/README.md`。
规则基线：`docs/quality/agent-execution-policy.md`。

### Jev 首次契约探针（人工合成文本）

官方固定端点 `/v1/systemone`，请求别名 jev-latest，实际返回 jev-1.14.0。
“Henry 检查 Bruno 写的方案”选择 PLAN_0（Henry 执行），confidence 0.98。

完整分布为 PLAN_0=0.99、PLAN_1=0、CLARIFY=0.01。


单次整请求耗时 1960.47 ms；它不是 p95，也不是准确率评测。

该探针不满足终稿后 5 ms 路由预算，生产 Jev 继续禁用。
没有发送真人音频或转写。
探针时基点为 f0740f6cc2f2a03da32e15ddb238abf3dfdb4af2，最终源码提交见交付记录。

配置哈希：1e7c7022d4a8ac47daba6199c6d9f928631e6779ef7a62364375da1d026a7734。
结果目录：`artifacts/voice-command-redesign-20261031`。

### 当前实现范围与保守边界

待澄清草稿仅驻留本机内存，120 秒过期。
新建/回复候选可直接补答目标；其他候选复用补答回合的主模型。
重规划分别传入原话、补答与稳定目标，不新增串行纠错调用。
保留最初正文和独立补答输入，取消或目录变更后拒绝恢复。

不把进程内 operation receipt 描述为跨进程持久幂等；未知结果不自动重发。
单次语音提交暂时拒绝跨工作区写操作和位置序号删除，避免缺少稳定身份时猜测。

点名取消须保留明确成员身份，App 在提交时核验 task_id / reply_id 归属。
只存在歧义取消候选时仍会澄清，但尚不恢复目标相关的旧 task_id。

普通来源、持续聆听、豆包和 GPT-Live 不进入新的单次候选路径。
GPT-Live 页面仅将一个已有品牌字面量改用已有 i18n 键，以通过全仓文案检查。

使用 apple-design 的层级、平台习惯与可访问性；Material 用于紧凑来源按钮和渐进诊断。
控件复用 gpui-component，保持既有 TTS 交互与协议，不添加装饰性动效。

### Rust Jev 适配器与配置

Rust 合成 smoke 正确选择 PLAN_0，实际模型 jev-1.14.0。
单次耗时 1718.168417 ms，confidence 0.91。

概率为 PLAN_0=0.94、PLAN_1=0、CLARIFY=0.06。
不满足终稿后 5 ms 路由预算，生产 Jev 保持禁用。

原探针配置与哈希保持原样，没有覆盖历史记录。
最终验收配置补记 test opt-level=0、test-threads=2、两种主题与语言。

最终配置 SHA256：8842f94920cab43b4d61c027e29c83bc8221b0b5ebeef0b8120b6f42b08924cb。

配置文件：`artifacts/voice-command-redesign-20261031/configuration-final.json`。

### 审查修复

独立 Standards 审查最后复核未发现剩余可复现问题。
取消后草稿安装与取消检查持同一锁，避免重新激活旧请求。
节点结束只结算对应节点的 task IDs，保留另一实例的队列。

App 回执与 UI 事件再次检查回合和语音 generation。
Spec 审查的回执分类反例均先复现失败，再修复通过。
覆盖优化/审阅、含任务动词的进展查询、补答后无提议，以及任务正文含“解释”。

4 项公开单次模型回归全部通过，不新增模型调用。
未知目标不能通过 CreateAgent、AssignIdea 或 brain handoff 绕过。

源码配置另含 source-manifest SHA256，覆盖本轮 Rust、语言包与 E2E 场景。
清单：`artifacts/voice-command-redesign-20261031/source-manifest.json`。
Runtime 原生问答夹具一次出现 read timeout/WouldBlock。
未改动该模块，单线程定向复跑 2/2 通过；原失败记录保留。

### 最终完整测试

命令经任务缓存执行：`cargo test --workspace -- --test-threads=2`。
环境：CARGO_BUILD_JOBS=2、CARGO_PROFILE_TEST_OPT_LEVEL=0。
退出码 0；汇总 3,460 passed、0 failed、52 ignored，共 344 个测试结果段。

其中 App 为 1,734 passed、0 failed、10 ignored。
这些是工程测试计数，不是真人准确率。

结果：`artifacts/voice-command-redesign-20261031/workspace-tests-complete.log`。
汇总：`artifacts/voice-command-redesign-20261031/workspace-summary.json`。

较早 workspace-tests、workspace-tests-final、workspace-tests-verified 日志保留原失败，不覆盖。

### 真实窗口验收中的增量修复

首次场景设置900点高度，虚拟显示器实际可用870点；改为1280×850。
Azure语言表单位于首屏下方，补滚动后检查完整字段。
这些只改测试场景，保留原失败记录。

源码基线 `ThemeId::from_saved_id` 已将 paper-blue 迁移为 ink-black。
现行六个可选主题均为深色，本轮不恢复其他任务已退役的主题。
真实窗口范围调整为中文 sapphire-night、英文 ink-black。
浅色仍是未完成的真实窗口验证，不宣称通过。

英文看图发现模式说明横向穿过相邻卡片。
新增公开页面布局回归先复现失败，再沿用人格页的按钮覆盖层布局修复。
模式选择、键盘激活与持久化入口保持原状；说明在自身卡片内换行。

完整workspace的3460项绿灯属于修复前检查点。
此后只增加这一处展示修复，追加语音相关测试、重建和真实页面验收。

最终语音相关增量测试181 passed、0 failed；包含新增英文布局和模式保存回归。
命令：`cargo test -p app voice_ -- --test-threads=2`。
结果：`artifacts/voice-command-redesign-20261031/voice-ui-verified.log`。

完整测试检查点的原配置保存在 configuration-pre-layout.json。
其SHA256为bdf0922342fcf7c438cb22b0b8c19652be261fa91071add5fcb60f8d158c7eb7。
最终配置包含1280×850窗口、现行主题和更新后的源码清单。

Headless边界断言通过后，真实英文窗口仍出现单行溢出，因此未放行。
最终将英文模式说明改为两行短句，并保留卡片宽度约束。
明确分行不依赖这处自动换行结果；最终仍以真实截图核验。
此前181项绿灯不能替代这次真实绘制检查，原失败截图保留。

两行英文文案版本：voice_增量181/181通过，UI crate测试22/22通过。
UI结果：`artifacts/voice-command-redesign-20261031/ui-tests-final.log`。
i18n检查2059个双语键通过，未增加新键或硬编码用户提示。

### 最终真实 App 验收

命令：`bash scripts/desktop-e2e.sh settings-a1-configuration --visual`。
被测程序：工作树 `target/debug/norma-os`，默认开发 features。
最终运行退出码0，场景1/1通过；中文 sapphire-night 与英文 ink-black 两步均通过。
模型实际查看18个请求截图，逐项返回24个pass判据，其中8次为视觉检查。
总耗时743.918秒包含当前Agent读图与反馈，不是产品性能指标。

已检查分组、双来源、麦克风状态、Azure表单与本地诊断。
英文模式说明实际按两行显示，没有跨卡片溢出。
数据库硬断言确认浏览Azure/A1未改变运行provider。
未录音、未准备模型、未发送私人数据。

报告：`artifacts/desktop-workspace-e2e/2026-11-01T09-33-44Z/report.json`。
命令日志：`artifacts/voice-command-redesign-20261031/desktop-e2e-final-verified.log`。
汇总：`artifacts/voice-command-redesign-20261031/desktop-summary.json`。

此前窗口尺寸、滚动与两次英文布局失败目录均保留。
浅色真实窗口、A1真实单次录音、准确率和时延p95仍未验收。

### 最终构建

最新debug构建退出码0，耗时1分22秒。
最新release构建退出码0，耗时5分40秒；均限制CARGO_BUILD_JOBS=2。
命令为标准开发features与仓库定义的release profile，没有降低构建规格。

构建日志：`artifacts/voice-command-redesign-20261031/desktop-build-verified.log`。
Release日志：`artifacts/voice-command-redesign-20261031/release-build-verified.log`。
保留release程序与SHA256：`artifacts/voice-command-redesign-20261031/release-summary.json`。
这些编译耗时不是语音性能测量。

i18n、max-lines、no-allow、Storage边界、性能护栏与Agent指南行数检查均通过。
UI crate的22项测试使用Cargo默认测试线程数；workspace与voice_使用2线程。

### 交付提交与阶段索引

源码提交：`c752adfceaefe6b82cec005e614751fda4e716c4`。
分支：`codex/voice-command-redesign-20261031`。
源码清单逐项对照该提交完全一致；没有音频、真人转写、密钥或结果二进制入库。
正常钩子通过staged约束、workspace类型检查与格式检查，没有绕过hook。

| 阶段 | 保存源码的提交 | 配置与结果 | 状态 |
|---|---|---|---|
| P0 | c752adfc | 用户选定分组列表；本计划与离线原型 | 完成 |
| P1 | c752adfc | 最终配置8842f949；voice-ui-verified.log、ui-tests-final.log、最终desktop报告 | 181项语音增量、22项UI和现行主题双语窗口通过 |
| P2 / P3 | c752adfc | 语义12、护栏7、澄清5、模型护栏4、真实worker 3；完整检查点配置bdf09223 | 工程验证通过；不是准确率结果 |
| P4 | c752adfc | 原探针配置1e7c7022；jev-synthetic-probe.json、jev-rust-smoke.json | 合成文本契约完成，Jev生产禁用 |
| P5 / P6 | c752adfc | 配置中的jev_production=false | 不启用；缺少准入证据 |

完整测试检查点先于最后的设置展示修复。
展示修复之后追加181项语音测试、22项UI测试、双语实屏与release构建。
因此不把3460项历史检查点描述为该提交之后又跑过一次全仓。
所有结果都位于本工作树的artifacts目录，提交前运行的探针对应源码已保存于上述提交。

最终索引：`artifacts/voice-command-redesign-20261031/verification-summary.json`。
提交钩子日志：`artifacts/voice-command-redesign-20261031/commit-code.log`。
原准确率实验继续暂停；A1不升默认，Jev不进入生产路由。

验收与源码提交完成后，已按任务缓存工具回收本任务独占缓存13.4GiB。
工作树、默认target/debug、release程序和全部结果证据保留。
回收日志：`artifacts/voice-command-redesign-20261031/cache-finish.log`。


### 2026-11-01 真实麦克风前置核查与原型对齐修正

用户要求继续两个提交 `c752adfc` / `b5f27c88` 的真实麦克风全链路验收，随后指出设置页与选定的分组列表原型不一致，要求先优化并验收。不合并 dev，不替换用户 App，准确率 Goal 继续停止，Jev 生产入口继续禁用。

本轮核验110个源码清单项与提交一致；14个A1模型文件的长度和SHA256全匹配，Python完整性匹配。
被测debug SHA256：`a1c5274a2e1977411c9743fc470f24e7f15b85708d96be831afad37a773dce00`。
本机A1 runtime.json SHA256：`a1aa0f8de28e645fa0a806b8b2eb3ff5cdfda6f2500d1df1b4b788842fb4bb36`。

按仓库 E2EKit Preflight / Session 启动隔离debug窗口（数据、socket独立，允许用户真实按住/松键；未设置音频夹具或注入转写）。用户 App PID83621未结束、未替换。
13:06:05Z A1准备被取消，13:06:28Z A1准备报 `system memory pressure is elevated`；系统压力达到WARN（2），未绕过保护、未反复加载。
本机Ollama 11434端口无服务，未改成未经确认的云端模型；未录音、未派发。
为响应设置页修正已停止本轮隔离测试进程。六环节均仍阻塞，不能把启动与模型文件核验写成声学链路通过。
私有证据：`/Users/openclawd/.norma/acceptance/voice-command-20261101T130523Z/`；其中 `preflight.json`、`driver.log`、运行目录内 `app.log` 和截图可核验。

原型差异与验收映射：

| 标准 | 验证边界 | 状态 |
|---|---|---|
| 未就绪/就绪均为四组列表；不切换为另一套向导 | 公开设置入口布局回归、就绪状态回归 + 真实App未配置页面看图 | 通过；就绪页面本轮仅有单测，未加载真实A1 |
| 桌面右侧预览，窄窗口顺序堆叠，无横向溢出 | 1280×850 / 640×700边界测试 + 两条定向E2E | 通过 |
| 预览只做本地规则检查，使用真实画布成员，无模型调用/派发 | 预览输入、焦点、失效定向测试 + 实屏空画布交互 | 通过所列范围；成员来自实际名单，本轮未实屏验证有成员画布 |
| 来源/触发/连接/声音沿用真实配置与后台保存 | 语音184项回归 + 浏览配置不改变provider和mode的硬断言 | 通过；不是模型连接或声音播放证据 |
| 双语与当前主题可读，交互状态清楚 | i18n、真实App中文Sapphire Night/英文Ink Black | 所列两种深色主题通过；无浅色实屏证据 |

设计采用 apple-design 的层级、平台键盘与即时反馈，Material 的紧凑分组、语义状态与渐进展示；沿用 GPUI0.2.2 / gpui-component0.5.1 和现有组件。当前安装目录没有可读取的 gpui-* skill 文件，API以仓库钉死版本与邻近实现核对。
测试沿用用户已要求的公开设置渲染/交互边界，遵守TDD；headless边界只作补充，最终以定向desktop-e2e真实窗口为准。

#### 本轮实施与失败后修复

语音指令页面按原型拆为01语音输入、02理解与目标、03任务派发、04语音回复。
本地/云端来源合并呈现，引擎诊断渐进展开；保留真实触发、设备、声音和配置入口。
宽屏增加右侧本地规则预览，窄屏放到分组下方；不复制原型中的Bruno/Henry与模拟回执。
预览复用生产纯解析/校验函数，仅展示候选；接入当前项目真实名单，不持有执行或模型调用入口。

先用公开设置入口布局测试复现分组缺失，再实现布局，红灯见 `artifacts/voice-command-redesign-20261031/prototype-red.log`。
实屏输入失败后，补焦点回归，实际输入为空而预期为probe；修复为复用设置编辑器的焦点与冒泡处理。
该有效红灯见 `artifacts/voice-command-redesign-20261031/prototype-focus-red.log`。
模型空白行先由测试复现，再补双语“未配置”；红灯见 `artifacts/voice-command-redesign-20261031/prototype-model-label-red.log`。
窄屏空输入框在实屏暴露宽度塌缩，再为组件Input补完整宽度；最终双语空编辑器逐项看图通过。
早期 `prototype-preview-red.log` 因测试主题未初始化而失败，只是测试环境错误，不计为功能红灯。
全部历史失败日志和截图保留。

#### 提交、配置与工程验证

源码提交：243c66a97ec101f043c2fcb0925f3a4748f4ed72，分支 `codex/voice-command-redesign-20261031`。
20个变更源码/测试/语言包/场景文件均与验收源码清单一致。
配置：`artifacts/voice-command-redesign-20261031/prototype-configuration.json`。
配置SHA256：1cf2820f3dc014bfa6b9965ccaa21fe6043339fc6304cd19b1f47c0bfd72b27b。
源码清单：`artifacts/voice-command-redesign-20261031/prototype-source-manifest.json`；SHA256：a162a3771ccda80a13c6d7b60622f083395af2ed9143debe0cc6733b670abeb9。
被测debug SHA256：d35a4811a05acb8aeecbb14a2407357130206a8c031b35ffd7730f8d346bbb28。

`CARGO_BUILD_JOBS=2 CARGO_PROFILE_TEST_OPT_LEVEL=0 cargo test -p app --lib -- --test-threads=2`：退出0，1738 passed、0 failed、10 ignored，128.42秒。
日志：`artifacts/voice-command-redesign-20261031/prototype-app-tests-final.log`。
其中语音定向184/184、设置输入2/2；复用同一语言包的UI crate 22/22记录。
正常提交钩子的 `cargo check --workspace` 与 `cargo fmt --check` 通过，无绕过。
i18n共2086个双语键，行数、no-allow、性能、Storage边界、Agent指南行数及diff空白检查通过。
工程汇总：`artifacts/voice-command-redesign-20261031/prototype-tests-summary-final.json`。
提交日志：`artifacts/voice-command-redesign-20261031/prototype-commit-code.log`。

最终 `CARGO_BUILD_JOBS=2 cargo build -p app --release` 退出0，标准release profile，耗时12分04秒。
Release SHA256：48b730c1f74c7ca22b03f71c3de0b95f98b00e88189bc17af7e5e5026a4c60a2。
构建记录：`artifacts/voice-command-redesign-20261031/prototype-release-build-final.log`；索引：`artifacts/voice-command-redesign-20261031/prototype-release-summary-final.json`。
最终工程与页面证据总索引：`artifacts/voice-command-redesign-20261031/prototype-verification-final.json`。
这些编译与测试用时均不是语音时延指标。

#### 真实 App 结果

以下均使用本工作树debug程序、隔离数据/socket及私有虚拟显示器；用户App PID83621持续运行，未替换、未结束。

| 命令 | 结果 | 用时 | 证据目录（本工作树） |
|---|---|---|---|
| `bash scripts/desktop-e2e.sh settings-voice-command-grouped --visual` | 退出0，场景1/1、双语步骤2/2，8次视觉检查 | 613.286秒 | `artifacts/desktop-workspace-e2e/2026-11-01T14-55-19Z/` |
| `bash scripts/desktop-e2e.sh settings-voice-command-sections --visual` | 退出0，场景1/1、双语步骤2/2，6次视觉检查 | 499.140秒 | `artifacts/desktop-workspace-e2e/2026-11-01T14-46-10Z/` |

合计21个当前截图请求逐项响应，25个pass判据（含定位），14次视觉检查。
核验四组结构、宽窄布局、空模型状态、04声音与真实偏好控件、禁用/焦点/选中状态，以及输入后预览与编辑后失效。
SQLite硬断言确认浏览和预览未改变识别provider或对话模式。
这些耗时含Agent看图等待，不是产品响应时延或ASR性能。
汇总：`artifacts/voice-command-redesign-20261031/prototype-desktop-summary-final.json`。

此前同一sections场景在 `artifacts/desktop-workspace-e2e/2026-11-01T14-16-15Z/report.json` 因300秒看图响应超时退出3，归ENV且保留；未计入通过。恢复响应后重跑同一场景，没有换用其他工具绕过。

本轮设置页优化验收通过所列范围。真实录音、松键、A1终稿、目标澄清后的真实派发及回执均仍未通过或阻塞，详见本节前置核查与私有证据。
真实A1就绪、有成员画布、浅色主题和完整语音链路未由这些页面证据覆盖。
原定量实验继续停止、Jev生产禁用、未合并dev；语音工作分支尚不满足用户要求的全链路合并门槛。

### 2026-11-01 补齐A1安装与Jev配置（实现已提交，真实App验收阻塞）

继续原独立分支，基线为412f4fc9。用户要求从页面完成A1下载安装/配置，以及可持久保存、重新加载、真实验证的Jev配置；沿用四组列表，不合并dev、不替换当前App、不恢复定量实验。

已核对的契约与实施边界：

| 能力 | 实际字段和存储 | 可用性与验收边界 |
|---|---|---|
| A1 | Python运行环境路径；模型使用内置14文件目录，应用管理下载和安装位置，原runtime.json协议v1保持不变 | 固定版本Python依赖、Python可执行文件SHA256、全部模型字节数/SHA256均通过后才保存manifest；模型安装与真正准备成功分别显示 |
| A1下载 | Zipformer修订98590b7ed6443e77b714204da2757d75e1a642f4；Qwen修订89e96d92ba34aca20b3e29fb10cc284097d1219f；均来自发布者仓库API核对，仍必须匹配既有artifacts.json | 下载失败、损坏与取消不得替换旧配置；完整校验后原子提升，允许重试 |
| Jev | API Key为唯一可编辑连接字段；固定https://api.typesafe.ai/v1/systemone与jev-latest只读显示，不虚构模型列表或可选端点 | 密钥进macOS钥匙串，SQLite只存非敏感引用；使用人工合成Choice请求验证HTTP/响应契约，显示实际返回模型 |
| Jev生产 | 配置和连通测试均不产生生产执行权 | 原生产入口继续禁用，不上传真人录音、真人转写或画布内容 |

当前语音credentials辅助函数实际上写SQLite；旧测试中的Keychain注释不能当作安全存储证据。本轮Jev不复用这条明文路径，也不迁移其他供应商凭据。
测试边界沿用用户明确要求的模型安装接口、配置持久化接口、真实传输与公开设置页；TDD逐条推进。
安装原语已先出现缺失接口红灯，随后“损坏拒绝→正确内容重试→安装后损坏再校验”聚焦测试通过；记录在 `artifacts/voice-settings-capabilities-20261101/`。
后续仍须完成应用接线、真实网络/钥匙串验证、中英文定向desktop-e2e与增量检查；未完成前不声明功能验收通过。

#### 本轮收尾检查点

A1失败候选包所有权回收、运行环境缓存原子替换均先出现缺失接口红灯，再实现；最终A1聚焦6/6通过。model_store全量7/7通过。Jev旧凭据清理采用可恢复journal，并对配置跨存储操作串行化，避免多窗口覆盖清理记录。

2138个双语键、行数、no-allow、Storage/性能边界、Agent指南行数和diff空白检查通过。正在重建debug并执行两条真实App配置场景；此检查点不表示真实页面或真人麦克风链路已通过。红绿日志和后续结果保存在 `artifacts/voice-settings-capabilities-20261101/`。

首轮A1真实窗口在无效Python失败反馈通过后，因Agent看图响应超过300秒退出3（ENV），证据保留于 `artifacts/desktop-workspace-e2e/2026-11-01T17-35-54Z/`。重跑同一场景至真实基础Python依赖安装中，未以模拟文件替代。独立只读复核确认原5项发现已修复、未发现新的正常单进程配置流程阻塞反例。

全仓首轮在既有OpenCode传输测试出现WouldBlock，定向复跑1/1通过；全仓第二轮在系统内存压力升高时出现4项A1 host测试失败（均为生产内存保护拒绝），暂不宣称全仓通过。等待本次release构建释放资源后复核，不绕过保护或终止用户进程。

#### 最终实现、验证与未通过项

源码提交：`d80b673e6325be964d0d85a68ffbdb3e51c363f5`。42个源码、测试、语言包与场景文件均已提交；正常hook通过codesign readiness、workspace类型检查与格式检查，没有绕过。独立只读对抗复核确认原5项问题均修复，未发现新的正常单进程配置流程阻塞反例；这不是桌面验收通过证据。

配置SHA256：`fafdf286e86b70670ba7154e1b623a3fffeebb641e76169190f898f2a46a756d`；配置文件为 `artifacts/voice-settings-capabilities-20261101/configuration.json`。
源码清单SHA256：`dc3e9ff137d73805908786d6f9d54db187fb2157af4f94c414d473e440afcb4b`；清单为 `artifacts/voice-settings-capabilities-20261101/source-manifest.json`。
Debug SHA256：`dc77852f17b827d870aa313e0b7b2c4c96a677ac3efd766d3981d9478428e2f0`。
Release SHA256：`26f5f2a24629c3af3375ad5258d468e7805e40cff5ced41145bd5ae192c411e7`，标准release构建退出0、耗时8分37秒。
结果总索引：`artifacts/voice-settings-capabilities-20261101/verification-summary.json`；所有下述日志均在同一结果目录。

| 验收范围 | 实际结果 | 证据与限制 |
|---|---|---|
| A1安装基础能力 | 工程测试通过 | model_store 7/7；完整性损坏拒绝、正确重试、manifest保留名保护；A1聚焦6/6含真实本机HTTP、取消后清理及原配置保留 |
| A1新建Python环境 | 真实App通过此子阶段 | 从基础Python 3.12.13创建专用venv，57项固定依赖版本全部匹配，缓存指针实际保存；`fresh-runtime-evidence.json`；未加载声学模型 |
| A1从页面完成全部模型下载与登记 | 未通过 | 真实HF下载已开始并显示字节进度；依赖准备占用旧900秒预算，场景超时前尚未形成runtime.json；不能说安装成功 |
| A1失败与恢复入口 | 中文子步骤通过 | 无效Python明确报错、路径可编辑、可重试且无runtime.json；两轮均完成该步骤；其他安装故障仅有工程测试 |
| Jev配置持久化、安全失败恢复 | 工程测试通过 | 覆盖保存重开、拒绝替换、清除补偿、退休条目journal与重开清理；API Key仅进Keychain，SQLite只存引用 |
| Jev真实服务与Keychain | 真实接口测试通过1/1 | `jev-real-final.log`：实际返回jev-1.14.0、5215ms；真Keychain保存重读清除；人工无效Key被真实401/403拒绝；固定合成Choice，无生产派发 |
| Jev真实页面有效/无效、保存后重开、清除 | 阻塞，未通过 | 场景两次因看图响应超过300秒退出3，尚未完成真实连接或页面持久化验收；不得以接口测试替代 |
| 中英文真实配置页面 | 未通过完整验收 | 本轮只完成中文部分定位与A1无效Python反馈；英文重开与新增英文失败场景尚未执行；2138双语键静态检查通过不能代替实屏 |
| 生产语音与真人全链路 | 继续禁用/未通过 | Jev生产开关恒false；未恢复定量实验；没有录音、松键、A1终稿、澄清后派发与真实回执的新证据 |

受影响测试：agent单元14/14、App单元1748 passed/0 failed/11 ignored、ASR单元31 passed/0 failed/1 ignored、model_store 7/7、UI 22/22。主要来源 `workspace-tests-serial-final.log` 与 `model-ui-tests-final.log`；App耗时270.58秒，ASR耗时3.01秒。真实Jev忽略测试另行显式运行1/1通过，不把它与普通套件混算。

完整workspace三轮均未全部通过：首轮在未修改的OpenCode传输测试出现WouldBlock；第二轮4项A1 host测试被系统内存压力保护拒绝；第三轮ASR已全通过，但在未修改的OpenCode foreground时序测试失败。两个OpenCode失败分别定向重跑通过，ASR在资源恢复后31/31通过。保留 `workspace-tests-final.log`、`workspace-tests-recheck.log`、`workspace-tests-serial-final.log`、`workspace-unrelated-recheck.log` 与 `workspace-foreground-recheck.log`；不把这些定向复跑写成全仓绿灯，也不直接把未改动测试的失败归因给本次语音改动。

2138双语键、Rust行数、no-allow、Storage/性能边界、Agent指南行数及diff空白检查通过。密钥审计200个本次源码、日志、JSON和SQLite相关文件，明文命中0；结果 `secret-audit.json`。未提交密钥、模型文件、二进制、真人录音或转写。

#### 真实桌面执行记录与继续条件

| 场景 | 结果 | 耗时 | 本工作树证据目录 |
|---|---|---|---|
| A1安装第一轮 | ENV退出3，中文无效Python子步骤通过；后续看图响应超时 | 429.978秒 | `artifacts/desktop-workspace-e2e/2026-11-01T17-35-54Z/` |
| A1安装第二轮 | ENV退出3，中文无效Python子步骤通过；真实依赖安装成功，全部模型下载未完成 | 1354.473秒 | `artifacts/desktop-workspace-e2e/2026-11-01T17-46-39Z/` |
| Jev配置第一轮 | ENV退出3，看图响应超过300秒 | 415.840秒 | `artifacts/desktop-workspace-e2e/2026-11-01T18-09-43Z/` |
| Jev配置第二轮 | ENV退出3，看图响应超过300秒 | 309.218秒 | `artifacts/desktop-workspace-e2e/2026-11-01T18-19-18Z/` |

所有运行均为本工作树debug程序、隔离数据/socket及虚拟显示器；用户App PID83621未结束、未替换。截图、请求、响应、app.log与report.json均保留。上述耗时含Agent看图等待及首次安装，不是ASR或路由延迟。超时后补写的响应不追认为通过，报告结果保持ENV。

后续须先恢复300秒内可响应的模型看图通道，再运行三条已登记场景：`settings-voice-a1-install`、`settings-voice-jev-configuration`、`settings-voice-capability-errors-en`。提交中的A1场景已将依赖+1.21GB模型的安装预算扩为2700秒，但该新预算版本尚未实际运行。可选择第二轮App已准备并验证的专用Python继续，路径与57项依赖证据见 `fresh-runtime-evidence.json`；这不替代14个模型文件的真实下载、逐项SHA256和配置保存验收。模型下载使用实际可访问的固定HF地址，不以模拟文件形成成功证据。

完整A1安装、配置重开、Jev页面真实验证及英文失败反馈仍是未完成项；真人麦克风全链路延续此前阻塞，不由本轮配置工作代替。当前不满足完整设置验收或语音分支合并门槛；未合并dev、未替换用户App、未启用Jev生产入口、未恢复准确率实验。

### 2026-11-02 UTC 继续剩余配置验收（本轮配置范围通过）

保留d80b673e与9967917e。历史三次看图超时的响应文件实际晚于请求494.05、382.61、806.18秒才落盘；成功步骤多为24–57秒。驱动未在截止前得到响应，尚无证据表明超时由App、错误路径或JSON解析造成。本轮9项VisualReviewTests全部通过；保持300秒限制并优先处理当前图，不以增加超时或自动通过绕过。

Jev真实页面场景 `settings-voice-jev-configuration --visual` 于 `artifacts/desktop-workspace-e2e/2026-11-02T01-56-44Z/` 退出0、场景1/1通过。覆盖真实401、中文Keychain保存/连接、英文重开同一凭据加载、再次验证与清除；实际jev-1.14.0，中文1443ms、英文1594ms，生产入口始终禁用。此数字仅为配置连接探针，不是ASR或路由延迟。A1使用既有57项依赖验证环境，已通过无效Python反馈并开始实际HF模型下载，完整安装与英文重开仍待结果。

新增证据根：`artifacts/voice-settings-acceptance-20261102/`；正在复现并核查两项未修改OpenCode时序测试的关系，不扩大生产修改。

#### 本轮最终结论与追溯

用户本轮指定的A1完整安装/配置重开、Jev配置页面全流程及中英文失败反馈已通过真实App验收。保留此前所有失败记录；本轮没有修改App、域层、语言包或场景源码，没有需要按TDD修复的新实现问题。未合并dev、未替换用户App、未启用Jev生产入口、未恢复定量实验。真人麦克风端到端验收仍未通过，不由这轮配置结果代替；整个语音分支的合并条件不能据此宣称齐备。

产品实现提交仍为 `d80b673e6325be964d0d85a68ffbdb3e51c363f5`，此前记录提交 `9967917e9c4ebdce796b7c7833b9a7a431115efe` 保留。42项源码清单与已提交内容、当前文件逐项一致；复用原有受影响crate完整测试与release证据，未无依据重建或重跑全仓。

本轮配置：`artifacts/voice-settings-acceptance-20261102/configuration.json`，SHA256 `07aaafa44e2e6446fc1399680e461438224f1c640bc6df6210c6403014ba1aee`。
被测debug SHA256仍为 `dc77852f17b827d870aa313e0b7b2c4c96a677ac3efd766d3981d9478428e2f0`。
源码清单复用 `artifacts/voice-settings-capabilities-20261101/source-manifest.json`，SHA256 `dc3e9ff137d73805908786d6f9d54db187fb2157af4f94c414d473e440afcb4b`。
最终索引：`artifacts/voice-settings-acceptance-20261102/verification-summary.json`。

| 真实App场景（均加 --visual） | 退出码/步骤 | 墙钟耗时 | 本工作树证据目录 |
|---|---|---|---|
| settings-voice-jev-configuration | 0；3/3通过，6次视觉检查 | 783.405秒 | `artifacts/desktop-workspace-e2e/2026-11-02T01-56-44Z/` |
| settings-voice-a1-install | 0；3/3通过，4次视觉检查 | 1303.564秒 | `artifacts/desktop-workspace-e2e/2026-11-02T02-10-52Z/` |
| settings-voice-capability-errors-en | 0；2/2通过，2次视觉检查 | 444.445秒 | `artifacts/desktop-workspace-e2e/2026-11-02T02-33-19Z/` |

合计3/3场景、8/8步骤、12次视觉检查，33次当前截图请求均实际看图并原子写入逐项结论，没有预填或超时追认。全部报告为 `verification=model-assisted`。原图、请求/响应、app.log、report.json与关键截图保留。所有被测进程已退出，用户App PID83621仍运行。耗时包含下载和Agent看图等待，不是ASR或路由时延。

#### A1与Jev实际行为

- A1：从页面使用无效Python触发明确错误，无runtime.json；更正路径后，通过真实HF地址下载全部14文件（1,209,840,003字节），完整校验后保存配置。重开英文页面保留相同manifest，Check local files复查通过并出现Use Accurate启用操作；没有点击启用，provider硬断言确认未自动切换。安装、文件就绪和模型激活保持区分。
- A1运行环境：复用此前真实App创建且57项固定依赖已验证的专用Python，哈希 `31b9c9a8d50289f3a13f014b3efd8ea3534fc3eea7ca7d9809e166139910b805`；完整模型下载没有使用模拟文件或本地模型拷贝替代。最终manifest SHA256 `e0589d074c9c7f7e228fe76bcf4155008a43adc765fe46d11feae4b3c0e5de84`，独立复查证据 `a1-installed-integrity.json` 位于本轮A1目录。
- Jev：中文真实HTTP 401拒绝人工无效Key；从已授权环境导入有效Key，通过页面保存到Keychain、清空输入框，SQLite仅有引用。真实固定合成请求返回jev-1.14.0，中文1443ms。英文重启从同一引用加载，状态重新标记本次未验证；重新验证返回jev-1.14.0、1594ms，清除后回到Not configured、验证按钮禁用、旧成功状态失效。生产语音入口始终禁用。
- 中英文错误：中文/英文A1缺失Python均有明确原因与可重试入口；英文Jev另获真实HTTP 401并提示重试，密钥仅掩码。硬断言确认失败安装/未保存的错误Key均不生成可用配置。工程层既有网络损坏、取消、原配置保留与安全存储补偿测试继续有效；本轮未把所有故障类别都声明为真实桌面实测。

补跑 `CARGO_BUILD_JOBS=1 CARGO_PROFILE_TEST_OPT_LEVEL=0 cargo test -p app --lib voice_ -- --test-threads=2`：194 passed、0 failed、1 ignored，13.80秒；日志 `voice-focused-tests.log`。9项Swift VisualReviewTests通过；`python3 scripts/check-i18n.py`验证2138个双语键通过，日志分别为 `visual-protocol-tests.log` 与 `i18n.log`。上述日志均在新增证据根。实际中英文页面覆盖Sapphire Night/Ink Black；本轮没有扩大到其他主题、ASR推理或真人录音。

密钥审计本轮源码副本、日志、JSON和SQLite相关文件共4104项，真实凭据明文命中0；结果 `secret-audit.json`。密钥、模型、二进制与测试数据均未提交。

#### 看图ENV根因与恢复措施

失败直接原因是响应文件在300秒截止后才出现，驱动按规定退出3；不是“有截图就通过”。历史迟到494.05/382.61/806.18秒，实际落盘后才补写的响应均不追认。9项协议测试和本轮33次真实请求未发现文件路径、JSON解析或窗口控制通道错误。

本轮优先串行处理当前看图请求，避免在请求挂起时进行构建、长分析或其他调查；原始图片仍逐张查看，未改300秒预算，未使用OCR或Computer Use绕过。相同首张PNG（SHA256 e7ecb48713b9af2b90789619b572dfb218d38d899ffebd68a090b420ddbd2555）此前迟到806.18秒，本轮31.133秒完成响应。Jev本轮15次请求最大71.869秒、中位45.275秒；三场景全部33次请求最大116.987秒，均被驱动正常接受。详见 `jev-visual-timing.json` 与最终索引。

仓库证据能定位到Agent/看图往返响应迟到这一层；缺少平台内部链路追踪，不能进一步武断区分平台排队、图像推理和Agent调度各自耗时。该ENV本轮已恢复且三场景均通过；若复发，待满足条件仍为当前图的响应在300秒内实际落盘，不能靠放宽结论或重用旧响应消除。

#### OpenCode失败关系核查与范围

基线412f4fc9的 `crates/norma` tree为 `a98e3aff0f7f92dbbdcd6584e001602437a11904`，与本次HEAD相同；`crates/mobile_pairing` tree也相同。Cargo.lock差异仅为App新增model_store和security-framework依赖条目，无包版本变化。没有从OpenCode测试到本轮安装/配置入口的调用。使用git archive导出隔离基线源码进行对照，没有切换、清理或回退工作区。

| 版本/试验 | question_transport | foreground未知发送 | 结论边界 |
|---|---|---|---|
| 当前分支原样 | 10次，1次失败 | 10次，0次失败 | transport复现同一WouldBlock |
| 改动前基线412f4fc9原样 | 50次，3次失败 | 20次，3次失败 | 两测试在本改动前也会失败；foreground本轮为Unsent/Unknown断言 |
| 仅临时基线副本修正socket模式 | 50次，0次失败 | 20次，0次失败 | 有限样本支持夹具模式为失败来源，未修改生产代码或主工作树测试 |

两夹具都把listener设为非阻塞，但接受的socket未显式恢复阻塞。最小真实TCP探针在本机3/3次立即WouldBlock，即使设置3秒read timeout；只增加accepted_socket.set_nonblocking(false)后，3/3次能等到150ms延迟请求。transport夹具read_line/read_exact的unwrap因此可能误报服务失败；foreground夹具对首次读错误直接丢连接，可能使发送状态断言失效。临时副本两行补丁未放宽任何断言/超时，诊断补丁SHA256 `ce54aa1f495b1ffc415d4eb4d6d0d0669b891d35b6f8e08ef9375d03a8362459`。

证据：`opencode-reproduction.json`、`opencode-baseline-reproduction.json`、`opencode-fixture-diagnostic.json`、`opencode-fixture-mode.diff`、`accepted-socket-probe.log`，汇总 `opencode-diagnosis.json`，均位于新增证据根。保留每次单独日志。

这些证据表明存在与本次A1/Jev改动无源码变化关系的基线夹具问题；不把有限样本写成所有OpenCode时序问题均已解决。历史foreground的POST计数为0这一具体断言本轮未原样复现，不能断言唯一根因已全部证明。主工作树未修改OpenCode；全仓此前失败不追认为通过，本轮也没有为追求全仓绿灯扩大生产修改。配置验收已通过，OpenCode夹具稳定性与真人语音链路仍须分别处理。


### 2026-11-02 用户验收增量：应用托管 A1 引擎包（本轮安装配置验收通过，未提交）

用户确认不再要求普通用户安装、选择或配置 Python。保留此前提交 `d80b673e`、`9967917e`、`15b3c81a`，在原独立工作区继续修改；本轮基准为 `15b3c81ae52ae4ee1502a3c4db0d8f00093020dd`，差异与配置哈希见本节追溯。此前通过记录适用于旧安装流程，本次安装器和页面变化已重新进行真实 App 验收。

| 本轮验收点 | 测试或实际证据 | 当前状态 |
| --- | --- | --- |
| 无系统 Python 的安装 | `voice_a1_managed_engine_installs_without_a_user_python`：真实文件系统、归档与子进程边界夹具 | 聚焦测试、真实 App 完整安装均通过；页面无需 Python 选择器 |
| 包损坏、取消与重试 | 下载损坏、引擎包损坏、缓存保留、取消不发布的聚焦测试 | A1 10 项聚焦通过；语音受影响测试最终 198 passed、2 ignored |
| 缓存不得执行清单外文件 | 独立审查发现反例，新增 marker 失败测试实际复现后修复固定解释器路径校验 | 绿灯通过；无执行权限缓存也可自动修复 |
| 自包含、可搬迁、签名 | 固定 SHA256 的 CPython standalone、57 项 A1 固定依赖及固定 setuptools；App 安装不运行 pip/venv | 真实包 26,974 文件、1,445,314,967 B 展开、406,940,556 B 归档；227 原生文件开发签名。正式签名/公证未验收 |
| 已有配置与引擎更新 | 显式检查/启用时后台迁移引擎，复用旧模型，成功后原子换配置；失败保留旧配置 | 使用真实旧模型与不存在的旧解释器，迁移测试 1/1 通过，50.57 秒；未加载模型 |
| 简化设置、中英文重开 | 使用既有 gpui-component 按钮与分组列表；遵循 Apple 的渐进呈现和及时反馈，移除 Python 选择器及主页面长路径 | 2141 双语键检查通过；中文取消/重试、英文重开与重新校验均通过 |

引擎随 App 作为独立资源包提供，模型仍按需下载约 1.21 GB；资源包包含预编译依赖，不把构建机路径或可变环境作为用户配置。正式/开发打包配置及 macOS 开发运行器均接入引擎准备步骤。当前 MLX 固定依赖要求 macOS 26，安装包清单声明最低版本并在不兼容设备上明确拒绝；未声称在旧系统实测。

安装与激活继续分开，安装不静默替换用户当前识别供应商；检查或启用才进行对应校验/准备。Jev 生产入口保持禁用，真实麦克风完整链路仍未通过，定量实验继续暂停。本轮未合并 dev、未替换或结束用户正在使用的正式 App。

#### 本轮最终证据与追溯

本轮尚未新增提交，基准 commit 为 `15b3c81ae52ae4ee1502a3c4db0d8f00093020dd`；保留此前提交历史。源文件清单哈希 `7de08fc69ec11ee73cf58b4948a201aacd9c3cb63f198c239529720e299b6103`。完整配置及结果索引在 `artifacts/a1-managed-engine-20261102/configuration.json`，SHA256 为 `9d0b5ca757cafa253cf665b18e478685ca038de418e9e7c2027ec038ba8ac926`；逐文件源码哈希在 `artifacts/a1-managed-engine-20261102/source-files.json`。

| 实际运行范围 | 结果 | 证据目录或文件 |
| --- | --- | --- |
| A1 中文取消/重试、14 个模型文件真实下载、英文重开和复查 | 退出 0，3/3 步骤；1409.39 秒，含下载与人工逐图响应；未加载模型、未切 provider | `artifacts/desktop-workspace-e2e/2026-11-02T15-00-26Z/` |
| 英文缺失文件与 Jev 真实 HTTP 401 反馈 | 退出 0，2/2 步骤；378.39 秒；原配置保持，未保存无效 Key，生产入口禁用 | `artifacts/desktop-workspace-e2e/2026-11-02T15-24-36Z/` |
| 语音受影响测试 | 198 passed、0 failed、2 ignored；真实模型迁移另执行 1/1 通过 | `artifacts/a1-managed-engine-20261102/voice-tests.log`、`artifacts/a1-managed-engine-20261102/migration.log` |
| model_store、UI 测试 | 30 passed、0 failed | `artifacts/a1-managed-engine-20261102/domain-ui-tests.log` |
| 引擎构建器与开发运行器 | 3 项构建器测试通过；开发运行器契约通过，正式/开发 Packager 资源与前置命令解析通过 | `scripts/test-prepare-a1-engine.py`、`scripts/test-macos-dev-app-runner.sh` |
| 编译与约束 | app check、debug build、release build、i18n、行数、no-allow、指南行数、存储边界均通过 | `artifacts/a1-managed-engine-20261102/` |

两个真实 App 场景共 5/5 步骤，18 次逐图响应（含 1 次明确等待校验完成后重新取图）；未按截图存在或点击 ACK 冒充验收。14 个模型文件共 1,209,840,003 B，安装配置 SHA256 `2129e8fe605e6e160ac179d18d391ad44968718cb220aed40b2afc3df7ece36b`；新受管解释器 SHA256 `ed7add0afe13f084d7f6e45c46a5c763efacdc0f548bfc6564021e460d94a498`。引擎归档 SHA256 `90a9a4ba0be2030ef18f89421cb671f278c60ffca70ca91bc27926aca7cb8f44`。以上时间为配置/安装场景耗时，不是 T_final 或 T_route。

独立 Standards 审查发现的缓存越界执行、丢失执行权限无法自修复、构建缓存未随解释器/签名配方更新三项均已修复并复核；Spec 审查发现的旧配置迁移、引擎更新和最低系统版本提示已闭环。首轮受影响测试有一条旧原型断言仍强制要求 Python 输入框，已按新需求改成负断言，最终 198 项全部通过。正式 Developer ID 引擎包与公证、旧系统跨版本运行、真人麦克风完整链路及量化性能门槛仍未验收，本轮结果不构成整个语音分支可合并结论；OpenCode 基线关系继续引用上一节，不扩大修改或声称全仓绿灯。


### 2026-11-02 用户验收增量：安装完成反馈与 dev 集成

用户反馈“没下载成功”。对用户验收实例只读检查后，实际两次安装均成功（UTC 15:44:55 与 16:00:25）；14/14 模型文件及解释器哈希匹配，配置 SHA256 `4389805cd2eeb1f08fc7de5fe0106490ba093587e25abbf6f781292fe02ab521`。识别 provider 未切为 A1。根因是安装完成后主按钮仍显示“下载安装”，安装与启用的状态反馈不明确；不是本轮下载失败。原始诊断与修复前截图在 `artifacts/a1-installed-feedback-20261102/`，未修改用户验收实例的数据。

先补两条失败回归测试，再将成功主操作改为禁用的“已安装 / Installed”，另设“重新安装 / Reinstall”；已保存配置在文件复查通过后同步为已安装，旧校验不能覆盖正在安装状态。下方明确显示“已安装，尚未启用”及“使用本地精准 / Use Accurate”。遵循 Apple 的明确状态、直接操作与渐进呈现原则，复用既有组件，不自动激活模型。

| 增量验收 | 实际结果 | 证据 |
| --- | --- | --- |
| 状态机回归 | 先红后绿；12 passed、1 ignored | `artifacts/a1-installed-feedback-20261102/state-tests.log` |
| 受影响语音测试 | 200 passed、0 failed、2 ignored，执行10.87秒；编译7分24秒 | `artifacts/a1-installed-feedback-20261102/voice-tests.log` |
| 中英文真实 App | `settings-voice-a1-installed-actions --visual`，退出0，2/2步骤通过，537.08秒；点击禁用按钮不重复下载，配置、包目录与provider保持 | `artifacts/desktop-workspace-e2e/2026-11-02T16-39-57Z/` |
| 实际模型来源 | 复用15:00真实下载并验证的完整安装，以APFS克隆进入隔离目录；未用合成模型，未声称再次下载或进行识别 | 同上，含完整性复查与原始看图请求/响应 |
| 编译/静态检查 | 新debug构建通过；fmt、diff检查、2144个双语键、行数/no-allow/指南约束通过 | `artifacts/a1-installed-feedback-20261102/debug-build.log`、`artifacts/a1-installed-feedback-20261102/i18n.log` |

本轮12次看图请求均实际读取原图并响应，含两次明确等待校验完成后重新取图，未放宽300秒预算。实际主题为中文Sapphire Night、英文Ink Black：旧 `paper-blue` 已被仓库迁移为Ink Black，不能记为浅色主题通过。完整模型下载、取消/重试、Jev真实配置验证、安全存储和原引擎包release结果复用上两节的对应证据；最终反馈UI仅重建debug并实测，没有将旧release结果冒充本次新release。

构建期间曾因ENOSPC失败，属于环境问题，未作为测试通过。确认本次Cargo结束、取得构建锁且lsof无引用后，只回收本工作区9个闲置app增量缓存目录，未动源码、录音、模型、证据或用户App。关闭增量编译重试后debug构建成功（9分11秒），语音测试亦通过。记录 `artifacts/a1-installed-feedback-20261102/cache-recovery.json`，原失败日志保留。

集成前发现旧PR的CI在domain-contract脚本崩溃：schema已拆到 `crates/agent/src/adapter/definitions.rs`，脚本仍读adapter入口。最新dev也有此错误。先将现有两条测试夹具对齐真实拆分结构，复现2条IndexError失败，再只修正schema路径；2条测试及真实仓库契约检查均通过。独立提交 `e5f9ffcd`，未改变Agent行为或放宽契约。红绿日志位于本轮证据根。

追溯：本轮实现基准 `15b3c81ae52ae4ee1502a3c4db0d8f00093020dd`，另保留 `d80b673e`、`9967917e`。新增源码清单 `artifacts/a1-installed-feedback-20261102/source-files.json`，SHA256 `c29af5c118d18d9d85cbc03a124d20f73f8b36993b15b58882f8353a7d407b07`；配置/结果索引 `artifacts/a1-installed-feedback-20261102/configuration.json`，SHA256 `c2d6aa8a8b29b8f81c2df6df2f3065bb448e64f298ea5a110e00e6505b4f53df`。承载实现的提交以本节所在提交及后续集成记录为准，不自引用预生成commit。

用户已明确要求合并到dev，按正常hook和PR检查执行集成、保留历史提交。此授权不改变证据边界：真人麦克风录音/松键/A1终稿/目标澄清/派发/真实回执的完整链路仍未通过；Jev生产入口禁用、定量实验暂停，正式Developer ID包与公证仍未验证。不替换或终止用户当前App，不清理主工作区他人改动。合并结果待实际完成后记录，不能将合并等同全部语音验收完成。
