---
title: 语音指令重构：实施计划与进度
subtitle: 合并本地识别入口，明确画布目标，统一提交后再评估 Jev
template: doc
theme: paper
lang: zh
source: 2026-11-01 · 已获授权，独立 worktree 实施中
---

用户已选定分组列表，生产设置与单次语义提交已实施。
源码、独立审查与双语真实窗口验收已完成。
最新 release 已通过，正在完成正常提交钩子与源码提交记录。
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
真实 App 双语验收与最新 release 已通过，正在记录源码提交。

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
