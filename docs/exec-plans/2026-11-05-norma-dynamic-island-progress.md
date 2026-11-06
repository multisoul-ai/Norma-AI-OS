# 灵动岛实施进度与证据

权威范围与验收：`2026-11-05-norma-dynamic-island.md`。用户最新指令取消 M0 失败即停门槛，先完成所有功能，使用单测、E2E 和真实 App 验收。

交付方式更新：用户要求“你不应该合并到dev,应该提pr,我还需要测试”。M3 及后续只提交可测试 PR 并验证 CI，未经用户新的明确指示不得合入 dev；按依赖叠加分支继续功能实施。M1/M2 的 #186/#187 已在此指令前合入，不自动回退既有提交。

| 里程碑 | 交付与测试边界 | 状态 |
| --- | --- | --- |
| M0 | GPUI PopUp、AppKit 屏幕与 Spaces、Carbon；原生 E2E 焦点证据 | API/几何已验证；非激活输入未通过；继续功能实施 |
| M1 | runtime::island 纯状态机：形态、优先级、160/420/3000ms、固定、输入焦点、设置 | PR #186 已合入 dev，最新 rust checks 通过 |
| M2 | 逐工作空间捕获与后台投影、App 级 IslandModel，含后台保留工作空间 | PR #187 已合入 dev，最新 rust checks 通过 |
| M3 | 每屏岛、窗口尺寸、三页签、审批/提问/成果、跨工作空间定位/重开 | 实现中；独立窗口 smoke E2E 已通过，业务 E2E 待完成 |
| M4 | 全局 BrainHost、带 route 的事件、多项目 Read 闸门、幂等迁移、reply/report/TTS | 待实施 |
| M5 | 删除左抽屉/状态胶囊和死入口，新增开关/快捷键与双语 | 待实施 |
| M6 | island --visual 模块、全检查、系统焦点/全屏/屏幕与真实模型汇报 | 待验收 |

## M0 证据摘要

独立 worktree `../norma-island-m0-20261105/` 保留探针与原始日志。真实 GPUIPanel 支持 canBecomeKeyWindow，窗口层级 101，collectionBehavior 写入/读回 337；Carbon 注册返回 0。仅有 DELL P2422H，NSScreen safeAreaInsets.top=0，与无刘海判定一致；几何单测经过 red/green。

用户实际反馈“输入不了”。原生 E2E 复用生产 ns_input.rs 和 capture.rs，先确认 key=false/app_active=false；点击后 Input 接受文字且前台 PID 未变，但 app_active=true，严格非激活断言失败。激活对照组能输入。截图已实际读取。此证据不能证明全局系统键盘投递，也不等于 GPUI 所有配置均不可行。输入使用已确认方案中的点击激活退路，审批必须另验收。

## 第 8 节验收映射

| 验收要求 | 最终证据 |
| --- | --- |
| 形态/优先级/自动收回 | M1 runtime::island 单测 |
| 其他 App 前台就地审批，不抢前台 | M3/M6 island E2E --visual + 前台 PID 断言 |
| 跨空间及已关窗口定位 | M2/M3/M6 island E2E |
| 全局幕僚对话和语音汇报 | M4/M6 真实 Claude Code + report/TTS 实际播放日志 |
| 旧数据无损迁移 | M4 brain 旧目录 fixture 单测 + 真实 App 启动 |
| 删除全部旧入口 | M5 代码检索 + 设置/快捷键 E2E |
| 刘海/外接屏/全屏 Space | M6 真机/原生 E2E；环境缺失不标为通过 |

PR #185（方案及原型）已在最新 CI rust checks 通过后合入 dev，merge commit e88dd4f5。

## M1 本地验证

- `runtime::island` 的四形态、160ms 悬停、420ms 离开、3000ms 完成通知经过先红后绿；时间由调用方传入，空闲时无截止时间。
- 9 个聚焦测试覆盖提醒优先级与队列、固定/输入焦点、默认与保留页签、独立弹出开关、隐藏不丢待办、重复指针事件与迟到 timer。
- `cargo test -p runtime --no-fail-fast`：351 passed，11 ignored，0 failed。首次全量中的既有 `opencode_question_host` 回执断言失败，单独复跑及完整确认轮均通过，未修改无关 OpenCode 代码。
- `cargo tree -p runtime --prefix none` 无 GPUI。状态机文件 265 行，测试按 crate 单元模块维护。
- 原始 red/green 与全量日志位于本 worktree `artifacts/island/`。PR #186 head `0f993fc5` 最新 CI run `42149147481` 的 rust checks 通过，正常 hook 与普通 merge 完成，合入提交 `723d99b5`。

## M2 测试边界

- App 级 `IslandModel`：按数据库、工作空间、项目作用域汇总；旧缓存不重复显示实时项目。同一区域若有两个独立 Runtime，保留两个宿主的待办，操作身份必须包含 source；共享成果只显示一次。完全相同的可见数据不通知。
- 活动捕获/投影：从所有保留会话捕获运行态、审批和提问；工作空间元数据、持久化状态与成果在后台读取，逐工作空间投影；旧后台结果不能覆盖新一代状态。
- 生命周期：关窗但保留项目时贡献不消失；真正停止宿主时清除贡献，迟到任务不能复活已释放的来源。
- 用户流程的真实 App 证据在 M3 接入窗口后补充，M2 数据测试不声明为桌面 E2E。

## M2 实现与本地证据

- `crates/app/src/island/model.rs` 保存 App 级缓存：目录/已读成果使用每数据库的最新刷新版本，实时贡献使用各宿主的最新版本。旧任务不能复活已删目录或已停止来源，较慢宿主的真实待办仍会被接收。
- `activity_cache/capture.rs` 仅捕获内存；独立 500ms 监控不依赖活动窗口。`project.rs` 在后台读取全部工作空间及成果并投影。当前窗口旧概览暂由 `current.rs` 保持原有内存缓存语义；其结果不会发布到全局模型，且全局读取失败不阻断它。M5 删除旧界面时移除这一兼容入口。
- 8 条 `island_` 聚焦测试通过：跨空间/后台提问、关窗保留与释放、相同数据零通知并验证观察器有效、双实时来源同 request ID、不复活已读成果、数据库隔离、乱序/释放回调、未打开空间删除、存储故障隔离。
- 既有桌面模块 288 条测试全部通过，按每批 8 条拆成 36 个独立进程，测试内容和断言未跳过。单进程扩展回归曾在视频壁纸 AVFoundation 解码中停滞；线程采样记录 dispatch 64 线程上限，拆分进程后包含这些场景的全部测试通过。未修改无关视频解码实现。
- storage/performance guardrails、500 行、no-allow、i18n 2172 双语键与 6 条 i18n 脚本测试通过。无新增用户可见文案。
- 独立对抗式审查发现的多宿主覆盖、旧目录占优、删除不刷新均经过失败用例后修复，并补充成果去重和通知正断言；后续兼容审查补充存储故障隔离。
- 原始证据：`artifacts/island/m2-final-tests.log`、`artifacts/island/m2-desktop-batches/report.json`、`artifacts/island/m2-desktop-hang.sample.txt`，以及同目录内 red/green 日志。


## M2 合入

PR #187 head `13d946f5f1328daacdb282ad13ee3af9707d2df6`，CI run `42156643417` 的 rust checks SUCCESS，其余自动检查已完成。普通合入提交 `4ce23691c4a2c2662f4aabd81946fd782f514d27`。

## M3 当前证据（尚未完成里程碑）

- 当前分支 `feat/island-ui-20261105`，同一隔离 worktree 继续开发，未提交。
- 独立每屏 NSPanel、NSScreen 刘海/无刘海几何、形态尺寸、透明圆角命中、多屏展开所有权已接入；透明点击穿透尚缺系统输入实证。
- `runtime::island` 当前 17 条聚焦测试通过；新增几何、拔屏/跨屏焦点、完成事件、精确记忆授权均有先失败后修复记录。
- App 12 条 `island_` 聚焦测试曾全通过：含跨空间精确路由、已关窗口复用原 Entity/Runtime、重建输入控件保留未完成草稿、成果已读隔离。后续新增异步去重/失败提交测试仍在推进。
- `island-window --visual` 在真实 debug App 完整通过，退出码 0，1/1；证据目录 `artifacts/desktop-workspace-e2e/2026-11-06T04-38-11Z/`。实际核对了黑色胶囊、三页签/默认选中、固定与幕僚引导、Esc 后同一原生窗口缩到 330×34 点。耗时 532s 含模型读图等待，不代表动画/启动延时。
- 此前真实 E2E 发现并修复：GPUI key 回调持 mutex 同步 resign 导致死锁（vendored 窄补丁先释放 lock）；AppKit 标题栏交通灯覆盖胶囊（异步移除标题栏）；按钮默认避免获取焦点导致 Esc 无路由（仅设置 GPUI 内部 focus，不激活 App）。保留失败日志/截图和进程采样。
- 独立审查后修复：关闭岛窗口回调改读独立 Global 句柄快照，避免 manager Entity 重入；拔屏/跨屏释放旧 hover/input focus；InputState 按新窗口 remount，保留纯草稿；Codex 仅已识别 commandApproval 去掉协议身份，保留全部执行参数。
- “总是允许”保存精确到数据库、工作空间、项目、根目录、卡片、Provider、工具及完整参数的授权哈希；确认原请求投递后才保存，后续逐请求仍走现有审批/审计链。无法建立精确操作范围的请求不开放记忆授权。M5 将提供清除已记住授权的设置入口。
- 设计沿用已加载 `apple-design` / `emil-design-eng`：黑底信息层级、明确状态反馈、组件库按钮/Input、原生窗口随形态缩放；动效/减少动态效果与完整业务视觉验收仍待完成。
- 当前 smoke 不替代真实系统键盘、前台 App 不变、透明区域点击穿透、刘海屏或全屏 Space 验收。

### M3 业务 E2E 增量

- `island-approvals --visual` 通过，证据 `artifacts/desktop-workspace-e2e/2026-11-06T05-22-18Z/`：允许一次、下一请求仍审批、拒绝分别由原请求准确消费一次，前台 PID 不变。
- `island-remembered-approval --visual` 通过，退出码 0，1/1，281 秒（含看图等待）；证据 `artifacts/desktop-workspace-e2e/2026-11-06T05-49-36Z/`。确认精确操作已记住后，不同 request ID 的同操作自动允许；原生消费记录恰两次，前台 PID 不变。
- `island-native-approval-fallback --visual` 通过，退出码 0，1/1，258 秒；证据 `artifacts/desktop-workspace-e2e/2026-11-06T05-55-03Z/`。原生菜单因换行不能严格识别时保留提醒，只给定位，不猜测放行；定位到原卡后权限菜单仍待处理，无消费记录。保留空 decisions 的聚焦单测有 RED/GREEN，未放宽菜单校验。
- `island-questions --visual` 修复后通过，退出码 0，1/1，549 秒；证据 `artifacts/desktop-workspace-e2e/2026-11-06T05-59-34Z/`。单选 Blue 与自由文本 Cobalt blue（空格保留）经原 Hook 各消费一次；两次都实际看图确认成功反馈，表单消失，没有普通 PTY 输入。旧轮 `2026-11-06T05-38-48Z/` 虽传输成功但显示未确认，不能作为完整通过证据。
- 提问修复：durable intent 完成只代表排队，岛等待精确来源题目的真实传输回执；提交期间保持 pending、避免重复。延迟回执、失败保留草稿、跨窗口 remount 共 3 条单测通过，日志 `artifacts/island/m3-question-receipt-{red,green}.log`。后续独立审查发现前次消息残留，先用预置成功消息复现，再在有效提交时清除；3 条回归通过，日志 `artifacts/island/m3-question-stale-feedback-{red,green}.log`。该清旧反馈增量由单测覆盖，前述 E2E 二进制尚不含该单行增量。
- `island-artifacts --visual` 通过，退出码 0，1/1，798 秒（含跨上下文看图等待）；证据 `artifacts/desktop-workspace-e2e/2026-11-06T06-27-29Z/`。从 Other workspace 标记 alpha/beta 回合已看，仅清对应三条提醒；打开 gamma 跳回来源空间并读取已提交文件的冻结正文；重启后英文空态不复活已读提醒，两条冻结成果和三个源文件保留。实际英文待办页签为 Needs you，已修正场景判据的同义旧称 Needs attention。
- 成果场景保留前两轮失败：`2026-11-06T06-11-24Z/` 在定位后收起，脚本重复同位置 hover 没有再次进入，改为胶囊时显式点击；`2026-11-06T06-15-44Z/` 重启加载新形变二进制后触发 GPUI current_view 断言，属于产品问题，已把动画帧申请从通用 sync 移到 render。上述最终通过轮验证该崩溃已修复。
- 环境故障记录：本任务磁盘空间耗尽导致过一次编译与截图 ENV 失败；确认无构建占用后仅回收本 worktree 的 debug incremental，保留源码和全部测试证据。未结束用户已运行的正式 App。

### M3 形变与增量审查

- 参考 apple-design、emil-design-eng 与 review-animations：顶部中心锚定，展开 180ms / 收起 120ms 的 ease-out cubic；窗口先扩大、最后一帧才缩小，命中仅覆盖当前黑色表面；中途反向续接呈现几何。Esc/快捷键/定位立即生效，减少动态效果时仅 100ms 淡入。滚动内容避开底部圆角，防止 GPUI 矩形裁切泄露不透明背景。
- 形变纯状态测试先 RED 后 GREEN，日志 `artifacts/island/m3-transition-{red,green}.log`。真实成果 E2E 已验证普通展开与圆角；最终增量二进制继续跑 island-window。
- 独立审查两个 P2 均已修复：减少动态效果中途反向由 opacity_from 续接，日志 `artifacts/island/m3-transition-opacity-{red,green}.log`；逻辑已收起但形变未完成时 Esc 仍发布即时版本和通知，日志 `artifacts/island/m3-transition-escape-{red,green}.log`。两项复核通过，无未闭环发现。

### M3 原型还原（用户要求优先补齐）

用户指出“样式的还原度，目前比较差”。现有业务通过证据不代表视觉还原通过。重新用独立 Chrome 浏览器打开权威 HTML，保存 `artifacts/island/prototype-{pending,running,staff,alert}.png` 作为视觉基准。内置浏览器连接不可用，使用独立 Playwright/Chrome 实例打开本地 HTML，不连接用户浏览器会话。

| Before | After | Why |
| --- | --- | --- |
| 四个独立大按钮，顶部始终计数 | 标题与空间数、标题行图钉、三段统一底座及计数 | 原型将概要、固定和切换分层 |
| 统一灰色按钮、无身份层次 | 白色主按钮、拒绝红字、头像、语义标签与两级描述 | 保持原型视觉优先级 |
| 运行任务纵向大卡片 | 紧凑横向任务行、次级步骤与计时、右侧定位 | 还原列表密度 |
| 空态固定 544 点高 | 实际内容测量、保持顶部锚定并限制长列表高度 | 原型由内容决定高度 |

- 严格视觉判据已先跑 RED：`artifacts/desktop-workspace-e2e/2026-11-06T06-53-34Z/`，退出码 1；明确缺少标题、空间数和统一分段底座，固定位置错误，空态未居中。
- 高度约束单测先红后绿：`artifacts/island/m3-style-height-{red,green}.log`。短内容总高 194 点、长内容最高 544 点、收起固定 34 点，顶部锚点保持不变。
- 此轮样式实现及真实 App GREEN 尚在进行，不能标完成。当前 i18n 2219 双语键通过。此前 `island-window` 增量轮 `2026-11-06T06-45-28Z/` 因看图期间尺寸变化退出 3，不计通过。

### M3 样式第二轮与交互边界

- `island-window --visual` 严格判据 GREEN：`artifacts/desktop-workspace-e2e/2026-11-06T07-08-54Z/`，退出0，1/1。标题/工作空间数、右上图钉、三段底座、居中空态、内容高度和Esc缩窗均实际看图核对。
- 键盘展开后实际内容测量仍须立即完成，避免二次形变；`m3-style-keyboard-{red,green}.log` 已复现并修复。圆角测试检查180ms形变每一毫秒的可绘制内容边界；动画期间body底边与tabs横向采用当前圆角安全带，固定目标宽测量仍用16pt，避免反馈循环；`m3-style-corners-{red,green}.log` 4条形变测试通过。此几何证明不冒充真实中间帧截图。
- 提问严格视觉 RED：`2026-11-06T07-13-58Z/`，无直接选项/输入，还混入运行空态。已改为直接选项、输入与发送同行，每题保留独立草稿。第二轮 `2026-11-06T07-33-16Z/` 的首次表单、选择状态已看图通过；因模型操作间延迟超过Hook有效期，发送阶段退出3，完整流程仍需重跑。夹具 `question-failed=1`，未消费答案；不计完整通过。
- 审批严格视觉 RED：`2026-11-06T07-22-43Z/`，提醒顶部缺所属空间、底部仍显示定位和总是允许。已按原型改为所属空间、全部待办、拒绝/允许一次；总是允许保留在展开待办内，等待新二进制GREEN。
- 独立审查新增提问生命周期反例：Alert需单屏交互所有权以防Input在两屏反复remount；焦点需按输入实体身份管理，不能被旧屏/同题旧输入的迟到Blur清掉；模型先移除表单或取消问题也要释放焦点；自由文本清空应恢复纯多选答案。这些边界正在先红后绿收口，不宣称M3完成。

### M3 样式第三轮与输入修复

- Alert只在当前交互屏显示，其他屏保持Compact；Alert内输入焦点可带入Expanded，旧屏Focus/Blur被忽略。`artifacts/island/m3-alert-input-{red,green}.log` 记录状态机反例与修复。
- 真实两个Input控件逆序切换焦点，已验证实体级Blur过滤；自由文本清成空白后取消custom并保留多选。先前测试未激活TestWindow，因此没有实际Focus事件；修正前置条件后真正复现了空白custom错误，见 `m3-question-active-red.log`。`m3-question-focus-green.log` 18/18通过，包括表单先移除后的提交焦点释放。
- 系统恢复旧Input焦点不等于用户编辑。生产Focus监听不再激活App或撤销委托；激活只在实际输入点击，权限准备在点击/文字Change/选择/提交执行。真实Input+原生问题委托测试已RED→GREEN，`m3-focus-restore-{red,green}.log`；最新聚焦集19/19通过。此测试证明委托行为，不冒充系统前台PID验收。
- 运行行补真实Provider，事实→activity→overview透传，`m3-provider-red.log`先暴露缺失数据，`m3-provider-green.log`5/5通过。运行列表增加工作空间摘要、已完成行、带头像姓名的待开始折叠行；只要有实际行就不显示空态。原生审批兜底统一使用原型卡片层次。
- i18n 2226双语键、6条脚本测试、performance/storage/行数/no-allow/Agent指南护栏通过；UI视觉验收仍待新二进制完整运行。
- 新导航E2E登记 `island-navigation --visual`，真实跨空间定位、关窗重开和PTY进程身份验证。首次 `e2e-test-support` 构建发现既有 `settings/agent_hooks.rs` 重复导入Sender，保留失败日志并删除冗余导入；不更改Hook行为。
- 后续仍只提PR，不合入dev；M3尚未完成，M4–M6继续保留在Goal内。

- 增量审查发现Failed/Takeover被计入待办却只在运行页展示，已补只有单类注意事项的RED；统一待办计数为Review/File。生产Ego完成状态`ego_completed`现与`done`共用完成判定，原生/Ego/ready三类状态经生产投影的测试通过；`m3-completed-green.log`两个相关测试集10/10。
- 一次增量编译因磁盘不足中断，确认无编译进程后仅删除本worktree `target/debug/incremental`和残留`*.rcgu.o`临时对象（40,812个，18,747 MiB），源码、可执行文件、静态库和证据均保留；后续Cargo使用`CARGO_INCREMENTAL=0`避免重复膨胀。失败轮不计通过。
- `island-navigation` Swift驱动已编译成功，`artifacts/island/m3-navigation-driver-build.log`；尚未运行真实流程。

### M3 操作确认与短暂状态取证

- 按原型resolve/toast补齐2200ms操作确认：原Hook/审批传输确认后显示绿色成功反馈，保留展开面板或继续下一条提醒，不冒充智能体任务完成。确认身份保留到投影移除，避免迟到快照复活提醒。
- 新问答轮2026-11-06T08-28-58Z首次表单与Blue真实消费成功，但缺少操作反馈，严格判据RED。补齐反馈后2026-11-06T08-39-47Z已实际截图看到回复已提交；因模型响应时2200ms期限已过、窗口尺寸改变，驱动退出3，本轮不计完整通过。
- 针对短暂提示新增显式visualSnapshotCheck，只判断所捕获PNG，继续强制requestID/SHA256/逐项多模态结论，禁止坐标；普通check/locate保留实时窗口校验。10条驱动测试通过，新增用例先RED后GREEN，日志m3-transient-visual-{red,green}.log。原Hook身份/答案/次数仍是独立硬断言。
- 独立审查3项P2分别先用聚焦测试复现：旧操作反馈污染下一次任务完成、展开列表从迟到投影重新显示已处理审批、重放completions=false提前关闭操作确认。修复接受Completed时清旧反馈、列表/计数/输入挂载共用过滤后的展示快照、完成开关只控制真实Completed。runtime聚焦25/25通过；App及真实E2E增量仍在运行。
- 原型精度增量：Alert顶部6pt、Activity顶部4pt、Alert外圆角32pt、命令框上下8pt、回复框圆角9pt；保持Expanded原有12pt上距。以权威HTML中对应CSS值为依据，等待最新App看图确认。

- App聚焦最终23/23通过，日志m3-feedback-controller-green2.log；输入挂载测试使用生产Controller/真实Input，不调用TestWindow不支持的原生窗口接口。最初测试因TestWindow raw handle失败的记录保留在m3-feedback-controller-green.log，不计产品失败或绿色证据。
- 独立审查复核3处状态修复，未发现新确定P1/P2；补充瞬时截图后同一窗口必须恢复Compact的硬断言，避免确认永久停留的测试假阳性。驱动不能把旧PNG用于点击。guardrails、storage、行数、no-allow、Agent指南、2226键i18n与diff-check通过。

- 新样式问答E2E 2026-11-06T09-03-37Z退出0，1/1，270秒；Blue与Cobalt blue按对应tool_use_id各消费一次，两次成功反馈截图和自动恢复Compact硬断言通过。第二题上方仍残留上一题成功文案，记录为视觉缺陷，不能据业务通过宣称完整还原。
- 新样式审批E2E 2026-11-06T09-09-04Z退出0，1/1，412秒；原型层级、顶部工作空间、全部待办/拒绝/允许一次布局看图通过；allow/deny各被原生请求消费一次，处理审批未改变前台PID。第二请求同样残留上一请求成功文案，归入同一缺陷。
- 跨请求残留已补Controller回归，m3-feedback-lifetime-red.log先复现；在收回或切换新Alert身份时清除旧消息，问答第二题看图判据明确禁止上题文案，等待GREEN。

- 跨请求反馈修复GREEN：App聚焦24/24，m3-feedback-lifetime-green.log；新二进制构建m3-feedback-lifetime-build.log通过。真实问答2026-11-06T09-26-12Z退出0，1/1，375秒，第二题明确看图验证无上一题成功文案，Blue与Cobalt blue各由原Hook消费一次，两次反馈均恢复Compact。

### M3 最新验收与剩余项

- 新样式记忆审批 `island-remembered-approval --visual`：`artifacts/desktop-workspace-e2e/2026-11-06T09-33-26Z/`，退出0，1/1，399.48秒。Alert进入全部待办，在Expanded选择总是允许，精确授权反馈可见；不同request ID的同操作自动允许，原请求恰消费两次，前台PID不变。
- 新导航 `island-navigation --visual`：`artifacts/desktop-workspace-e2e/2026-11-06T10-16-09Z/`，退出0，1/1，2015.24秒（含模型读图等待）。手机服务已在隔离端口启用；跨空间运行列表和定位通过；关主窗口后岛仍显示任务，定位重开新窗口号，原Agent PID 42498不变，启动记录只有一条，原终端内容恢复。此证据不证明重开后PTY持续输出。
- 导航早期ENV轮保留：09-40-49未滚动至手机连接、09-43-57关窗MouseUp后的旧窗口ACK不可用、09-51-49模型响应超时。脚本仅在MouseUp且精确原窗口已消失时接受窗口不可用，不重放输入，也不吞其他错误；独立增量审查完成。
- `CARGO_INCREMENTAL=0 cargo test -p runtime -p norma -p ui --no-fail-fast`退出0；日志 `artifacts/island/m3-pr-domain-tests.log`。i18n 2226双语键、6条脚本测试、performance/storage/行数/no-allow/Agent指南护栏和diff-check通过。
- 新样式成果 `island-artifacts --visual`：`artifacts/desktop-workspace-e2e/2026-11-06T10-59-56Z/`退出1，PRODUCT。最新成果卡布局、alpha/beta标记已看及只保留gamma均通过；跨空间已到来源项目，但打开正文报“storage sqlite failed: database is locked”。不能引用旧GREEN代替这次失败，须修复后重跑。
- M3尚未提交，接下来只提PR，不合入dev。M4全局幕僚、M5旧入口删除与设置、M6总验收仍待完成；刘海屏、全屏Space、Carbon系统输入与动画中间帧仍缺真实证据。

- 成果锁冲突修复：`DesktopContentTabStore::open_for_presentation`改用IMMEDIATE事务，在读标签前取得写锁，避免deferred事务升级立即Busy。公开API双连接并发测试RED→GREEN，另5条既有标签测试通过，日志 `artifacts/island/m3-content-lock-{red,green}.log`；App调用仍在background_executor，独立只读审查无阻塞发现。
- 最终成果GREEN：`artifacts/desktop-workspace-e2e/2026-11-06T11-17-52Z/`，`island-artifacts --visual`退出0，1/1。标记已看仅清目标回合；跨空间打开gamma冻结正文；重启英文空态、两条原始成果与三个源文件均验证。新App构建与storage全量测试退出0，日志 `m3-content-lock-build.log` / `m3-content-storage-tests.log`。runtime/norma/ui全量合计724通过、12既有忽略。
- 已将审批、问答、运行、成果4张原始真实App截图归档至 `docs/exec-plans/evidence/2026-11-05-norma-dynamic-island/m3/`，便于PR逐图核对原型。M3业务实现已可提交，系统层缺证仍在M6清单，不宣称总Goal完成。
