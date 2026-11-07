# 灵动岛实施进度与证据

权威范围与验收：`2026-11-05-norma-dynamic-island.md`。用户最新指令取消 M0 失败即停门槛，先完成所有功能，使用单测、E2E 和真实 App 验收。

交付方式更新：用户要求“你不应该合并到dev,应该提pr,我还需要测试”。M3 及后续只提交可测试 PR 并验证 CI，未经用户新的明确指示不得合入 dev；按依赖叠加分支继续功能实施。M1/M2 的 #186/#187 已在此指令前合入，不自动回退既有提交。

| 里程碑 | 交付与测试边界 | 状态 |
| --- | --- | --- |
| M0 | GPUI PopUp、AppKit 屏幕与 Spaces、Carbon；原生 E2E 焦点证据 | API/几何已验证；非激活输入未通过；继续功能实施 |
| M1 | runtime::island 纯状态机：形态、优先级、160/420/3000ms、固定、输入焦点、设置 | PR #186 已合入 dev，最新 rust checks 通过 |
| M2 | 逐工作空间捕获与后台投影、App 级 IslandModel，含后台保留工作空间 | PR #187 已合入 dev，最新 rust checks 通过 |
| M3 | 每屏岛、窗口尺寸、三页签、审批/提问/成果、跨工作空间定位/重开 | PR #189 OPEN，业务及原型看图 E2E 通过，最新 Rust CI 通过；系统级缺证留在 M6 |
| M4 | 全局 BrainHost、带 route 的事件、多项目 Read 闸门、幂等迁移、reply/report/TTS | 单一宿主、作用域/取消及迁移接线已实现，迁移真实 E2E 通过；wire 身份收口、report、全局对话与真实模型/TTS 待完成 |
| M5 | 删除左抽屉/状态胶囊和死入口，新增开关/快捷键与双语 | 待实施 |
| M6 | island --visual 模块、全检查、系统焦点/全屏/屏幕与真实模型汇报 | island 模块已登记并有 M3 业务证据；新增功能、系统与真实模型总验收待完成 |

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
- M3已提出PR #189，未合入dev。完整Clippy先指出重复加载测试夹具、observer多余借用和navigation枚举体积差异；改为crate级共享夹具、移除多余借用、Box成果变体后，`cargo clippy --workspace --all-targets -- -D warnings`通过。此增量不改变用户行为，复用上述E2E证据；正常提交hook继续执行。

### M3 PR完成，进入M4

- PR #189最新提交 `331a193916d279843f23f62077690c12c6917078`，CI run `42211172609`的rust checks于2026-11-06通过（6m9s）；两个外部路由/安全自动检查为NEUTRAL，全部已完成，无失败/待跑检查。PR仍OPEN，未合入dev。
- M4使用叠加分支 `feat/island-global-brain-20261106`，base为M3分支。先验证多根Read权限动态增删，再接带workspace/route的事件与单一宿主，随后数据迁移和岛内对话/语音汇报。
- 预先确认本机已有Claude ACP适配器和Matcha中英文TTS模型；仅检查存在性，没有读取凭据或改动用户配置。真实后端验收仍需实际执行，不能由存在性判定通过。

### M4 第一片（域层，尚未接入全局App宿主）

- ReadRoots由所有Gate克隆共享；BrainConfig/BrainService透传同一集合，并提供后台会话无需重建的根更新入口。相对路径只按原ACP cwd解析，cwd本身不隐含读取授权；关项目/清空集合撤销权限。公开Gate真实文件测试有RED/GREEN，`m4-read-roots-{red,green}.log`。
- 独立审查发现原始`~`/`$`前缀检查被join隐藏，已用真实字面目录复现RED并修复；`m4-read-prefix-{red,green}.log`，brain全量112通过、2既有忽略。此证据证明Gate与服务接线，不冒充实启ACP多根验收；App尚未调用新增动态根入口。
- 新增BrainScope与ScopedRouter纯调度层，workspace/route必须同时随Action保留。两个同名Agent进度不覆盖、同审批ID跨route关闭/释放互不影响；2条聚焦RED/GREEN通过，`m4-scoped-router-{red,green}.log`。ScopedRouter尚未替换BrainService现有dispatcher，事件全链路路由仍待接入。
- 下一片：CLI显式workspace/route及report kind协议、BrainService的必带scope事件/状态和按scope目标存储，然后App单一BrainHost、原子幂等迁移及岛内幕僚页面。M4当前未提交，不宣称里程碑完成。


### M4 CLI 作用域切片（全局宿主仍未接入）

- `norma brain --workspace W --route R` 支持显式、成对、非空的身份参数。真实 CLI 进程经隔离 Unix socket 验证：两字段完整覆盖旧项目环境，原始身份字节保留，不携带环境里的 Agent Provider/节点身份；旧完整环境调用继续兼容。身份沿现有 WireRequest 来源字段传递，未增加第二份 wire 路由来源。
- 真实进程测试先 RED（不识别 --workspace）后 GREEN，日志 `artifacts/island/m4-cli-scope-{red,green}.log`。独立审查复现 global 参数分别置于子命令两侧时 clap requires 误判；新增跨层 RED 后改为参数聚合完毕、构造请求之前验证配对。最终4/4通过，`artifacts/island/m4-cli-scope-order-{red,green}.log`；reviewer 复核四种仅传单字段的排列均退出2，不从环境补齐。
- `cargo test -p norma -p brain --no-fail-fast`在跨层修复前452通过、3既有忽略，日志 `artifacts/island/m4-cli-domain-tests.log`。跨层修复后4条身份测试及既有CLI/上下文/决策文案测试通过，日志 `artifacts/island/m4-cli-parser-regression.log`；`cargo check -p app`通过，日志 `artifacts/island/m4-cli-app-check.log`。所有Cargo命令使用CARGO_INCREMENTAL=0；行数/no-allow/i18n 2226双语键与diff-check通过。
- 这片只完成CLI到实际请求的身份传递。BrainService dispatcher/worker状态仍待作用域化，App仍未成为单一BrainHost；全局迁移、report kind/ReportRequested与幕僚页面未完成，不能据此宣称M4通过或真实ACP/桌面E2E通过。当前M4未提交；M3 PR #189仍OPEN，不合入dev。
- 相关Clippy先报新增参数使TopCommand::Brain变体过大，改为Box<BrainArgs>，未加allow。`cargo clippy -p norma -p brain --all-targets -- -D warnings`通过，日志 `artifacts/island/m4-cli-clippy-green.log`；随后真实身份与既有brain CLI共10/10通过，`artifacts/island/m4-cli-box-regression.log`。独立审查的参数配对P2已复核关闭。


### M4 事件、状态与目标的完整作用域

- `BrainEvent`与`BrainStatus`改为必带workspace/route的结构体，事件种类与状态种类分别为`BrainEventKind`/`BrainStatusKind`。Dispatcher使用ScopedRouter，审批待处理集合按(scope,id)索引；Job、重试、Ready/BackendDown/ApprovalTurnEnded、全部Trace及决策JSON均保留来源身份。
- GoalStore按workspace记录、去重、保留最近5条目标及忽略摘要；同名Agent不会跨项目覆盖，alpha的新目标不会淘汰beta。新增真实存储重开测试先RED后GREEN，`artifacts/island/m4-global-goals-{red,green}.log`，既有5条目标测试也通过。旧项目文件的workspace身份补写属于后续迁移，本片不宣称旧数据迁移完成。
- 真实BrainService线程+脚本ACP子进程测试先RED后GREEN：alpha/beta同审批ID，关闭alpha不会吞beta，所有trace保持来源。最终用FAKE_HOLD与主lane回执建立确定性屏障，不靠sleep推断关闭顺序；`artifacts/island/m4-scoped-service-barrier-green.log`。brain全量116通过、2既有忽略，`artifacts/island/m4-scoped-service-regression.log`。假适配器不执行真实CLI，不能据此声明真实CLI动作/Claude后端验收。
- App在入队时捕获scope，启动后重放不重绑旧route，active状态先校验完整scope再产生副作用。新增真实App实体/headless测试验证旧route较大turn迟到也不覆盖新route。首次App回归114通过、2失败：原提问fixture仅设Hook身份，未设App route；对齐生产注册中的同一对w/route后，App幕僚聚焦116通过、1既有真实Claude测试忽略，`artifacts/island/m4-event-app-tests-green.log`。这是headless接线证据，不冒充真实App/E2E。
- CLI缺失/半套/空白环境身份现在在连接socket前拒绝，返回workspace_required；完整旧环境仍兼容，原身份字节保留。真实CLI先RED（误为app_unavailable，即已经尝试连接）后GREEN，全部5条身份测试通过，`artifacts/island/m4-cli-missing-scope-{red,green}.log`。
- 独立审查域层与App接线未发现阻塞问题；确认问题fixture是补足生产前置条件，并未删除原委托/Hook响应/去重断言。`cargo check -p app`及`cargo clippy -p app -p brain -p norma --all-targets -- -D warnings`通过，日志`artifacts/island/m4-event-app-check-green.log`/`artifacts/island/m4-event-clippy.log`；performance/storage/行数/no-allow/Agent指南/i18n2226键/diff-check通过。
- 下一步仍为App唯一BrainHost、项目释放/后台保留生命周期与Read根动态接入、全局幂等迁移、report kind/ReportRequested和幕僚UI/真实Claude+TTS E2E。当前只是完成M4事件链路，不是M4里程碑完成；M3 PR #189保持未合入。
- 完整Norma回归触发两条既有OpenCode HTTP夹具竞态：非阻塞listener的accepted socket未恢复blocking，macOS上read_line在实际3秒超时前立即WouldBlock（该测试0.03秒失败），另一条因POST未发得到Unsent而非预设Unknown。仅对两处accepted socket补set_nonblocking(false)，保持200ms/3s读超时及原业务断言。失败日志`artifacts/island/m4-event-norma-tests.log`保留，定向7/7通过，`artifacts/island/m4-opencode-fixture-green.log`；完整Norma回归正在复验。
- 修复后完整Norma测试退出0，340通过、1既有忽略，`artifacts/island/m4-event-norma-tests-green.log`。本轮作为M4实现检查点提交到隔离分支，尚不提称完整M4的PR、不合入dev；剩余范围不变。


### M4 App 唯一宿主接线（里程碑仍未完成）

- 新增 App Global `SharedBrain`，唯一 `BrainHost` 由它持有；各项目只保存 `BrainClient`。生产项目 route 注册时订阅，`stop_project` 释放订阅，普通窗口解除不释放保留项目。全部成员根通过同一 ReadRoots handle 传入真实 BrainConfig；最后项目释放同步撤销令牌及读取权限，服务 shutdown/join 在后台。
- 全局配置只改变一次服务代次，重复项目广播不重复重启。语音开关不撤销进行中的启动；配置加载结果不能覆盖已经存在的全局配置。启动工作使用全局目录作为 cwd，不绑定首个项目；回调由捕获的 owner 接收，不依赖发起项目 Entity 仍存活。适配器环境清空固定 workspace/route，动作沿用已完成的显式来源协议。
- 新增启动竞态 RED：最后项目在启动途中释放后，旧实现允许第二个后台启动。修复为保留旧 ticket，直到迟到结果回收并完成后台关闭才准许新代启动。日志 `artifacts/island/m4-owner-late-red.log`。
- App 接线 RED 记录缺失生产 `ensure_brain_client` 入口，日志 `artifacts/island/m4-owner-app-red.log`。正确过滤器 `brain_host::owner::tests` 的 4 条测试通过，日志 `artifacts/island/m4-owner-app-green2.log`：双项目共享、状态来源隔离、根撤销、最后释放、迟到启动、配置重启与生产 stop_project。此前 `m4-owner-app-green.log` 误用了文件名过滤器，实际 0 条测试，不计证据。
- 扩展回归初轮 119 通过、1 失败、1 既有 ignored；失败为提议来源保存错误的中文 contains 断言，单独复跑通过。该消息是 LocalizedText，其他测试构造 App 或释放语言守卫会改变进程全局语言，存在语言竞争（原失败未输出消息，不能确证这是唯一原因）；改为断言完整 LocalizedText 精确相等，并用 resolve_for(ZhCn) 保留原中文语义断言。真实任务、来源保存失败及防重复派发断言均保留，新增准确双语提示键检查。日志 `artifacts/island/m4-owner-app-regression.log` / `artifacts/island/m4-owner-proposal-recheck.log`；修复后完整聚焦回归另记。
- 独立只读审查核验 owner、生产接线与测试迁移，无新增阻塞发现。该片主要采用 `service=None` 夹具证明状态机和 App 接线，不宣称真实 ACP 进程数量或退出、后台保留窗口、Claude/TTS E2E 已通过。
- 仍需完成：项目释放时取消 BrainService 的 scope/排队及在途任务、全局原子幂等数据迁移、report reply/ReportRequested、幕僚 UI 与真实模型/TTS 验收。当前全局目录启动已经接入，旧项目数据尚未迁移；本片仅开发检查点，不能作为完整 M4 交付或用户升级版本。只提后续 PR，不合入 dev。
- 修复后 App 幕僚完整聚焦回归 120 通过、1 既有真实 Claude 测试 ignored，日志 `artifacts/island/m4-owner-app-regression-green.log`；补充保留中文语义断言后的最终提议模块 4/4 通过，`artifacts/island/m4-owner-proposal-final.log`。performance/storage/行数/no-allow/Agent指南、2226 双语键、6 条 i18n 脚本测试与 diff-check 通过。最后 Clippy 与正常提交 hook 结果另记。
- Clippy 首轮指出未投递 BrainEvent 作为 Result 大 Err 变体（296 bytes）；改为返回可选未投递事件，调用者保持原有排队行为，无分配、无 allow。`cargo clippy -p app --all-targets -- -D warnings` 通过，`artifacts/island/m4-owner-clippy-green.log`。此增量后再次运行完整幕僚聚焦测试，最终结果以 `artifacts/island/m4-owner-final-tests.log` 为准。
- 最新代码完整幕僚聚焦回归再次通过：120 passed、1 既有 ignored，`artifacts/island/m4-owner-final-tests.log`。本轮保存为 M4 本地开发检查点；不提交完整里程碑声明，不合入 dev，M4 后续 PR 仍待迁移、汇报与端到端证据齐备。


### M4 项目退出取消（仍未完成迁移和幕僚交互）

- `BrainService::release_scope` 同步取消精确 workspace/route 的 token，dispatcher 清除该 scope 的 router/审批集合；所有排队、重试和迟到生产者共用已取消 token，不能复活该 route。重新打开项目使用新的 route。取消不修改其他项目的 token；全局 shutdown 取消全部 scope。
- 主任务和审批 worker 使用任务 scope 的取消信号；ACP 握手与 prompt 均可取消，握手返回后再次复查，取消期间不发出新的 Prompt 或放行迟到权限请求。`BrainClient::drop` 在撤销成员及 ReadRoots 后调用 release_scope；最后成员再关闭共享服务。线程 join 与子进程清理仍在后台。
- 域层 RED：缺少 release_scope 接口，`artifacts/island/m4-scope-release-red.log`。真实脚本 ACP 验收包含同 ID 审批隔离、双 lane 在途/排队/迟到取消、握手期间 PID 存活→退出。最终 brain 全量 118 passed、2 既有 ignored，`artifacts/island/m4-scope-release-brain-final.log`。
- 独立审查指出原测试的 beta 任意 Turn 会假通过，且发送前 Prompt trace 不能证明子进程收到请求。已改为等待 FAKE_LOG 两条真实 HOLD 输入；beta 两条 lane 必须分别恰好一个 end_turn 且无 BackendDown，alpha 两条必须 cancelled。没有释放 HOLD 文件来帮助被关闭项目完成。
- App 真实服务 RED：已有两个客户端时，drop alpha 不会取消主 lane，beta 被阻塞。清理服务后确定性失败在 beta 完成断言，`artifacts/island/m4-client-release-red.log`。App 接入取消后，新增测试同时核对脚本实际收到 alpha、beta 完成，以及最后项目释放后后台 shutdown/join 和状态通道排空；最终结果记录在 `artifacts/island/m4-scope-release-app-final.log`。
- Clippy 首轮要求对服务控制消息的大变体使用 Box，已修复。`cargo clippy -p brain -p app --all-targets -- -D warnings` 通过，`artifacts/island/m4-scope-release-clippy-green.log`；performance/storage/i18n 2226 键与 diff-check 通过。独立增量复核无新增阻塞。
- 迁移身份来源已核查：普通项目可从 WorkspaceIdentityStore::list 获得稳定身份；桌面项目遍历 DesktopWorkspaceStore 的工作空间/区域，并使用生产 `desktop:<workspace>:<area>` 身份。目录哈希不可逆，迁移不得猜成当前项目。迁移实现、report reply/汇报/TTS、幕僚 UI、M5/M6 仍待完成；仍只提 PR，不合入 dev。
- 最终 App 幕僚回归 121 passed、1 既有 ignored，`artifacts/island/m4-scope-release-app-final.log`。新增真实服务客户端测试 GREEN，证明 alpha 释放后 beta 实际完成，最后客户端释放后后台服务线程 join/状态排空；该脚本 ACP 证据不冒充真实 Claude 或桌面 E2E。行数/no-allow/Agent指南护栏也通过。本片作为 M4 开发检查点保存，迁移和完整幕僚交互仍未完成。

### M4 全局迁移域层检查点（App 尚未接线）

- 新增 `brain::migration::migrate`，后台调用时先取得迁移文件锁，将 goals、proposals、decisions、config 完整写入并同步一代目录，再以一次原子符号链接替换发布 `global/current`。再次启动只校验、复用已发布状态，不重新导入旧备份；旧项目目录、根配置和更旧根路径提议均原样保留。
- workspace 元数据能还原的目录使用精确身份；不能还原的目录使用 `archived:<hash>`。多个画布共用根路径时，更旧提议只归档一份，避免复制后重复派发。同工作空间跨文件编号冲突重编号并保留 legacy_id，跨工作空间同编号保持各自独立。
- `AskRule` 区分 App 全局规则与项目规则；旧 App 级规则继续覆盖未来项目。项目 enabled/autonomy 按原默认补齐后取权限交集，已有 App/global 的显式字段仍优先，不能靠目录顺序扩大授权。
- 4 条初始迁移测试先 RED 后 GREEN；更旧提议遗漏和损坏 autonomy 类型分别补 RED/GREEN，记录 `m4-migration-{red,green}.log`、`m4-migration-root-{red,green}.log`、`m4-migration-settings-{red,green}.log`。并发四调用恰好一次发布、当前数据损坏不重导入、根路径归属歧义归档均有公开接口测试。
- 独立对抗审查复现：缺失 current/proposals 被当空集合、项目权限扩大、非法 workspace_id 被静默改写、单文件重复提议 ID。均已补失败测试并修复；记录 `m4-migration-review-{red,green}.log`、`m4-migration-identity-red.log`、`m4-migration-duplicate-red.log`，最终聚焦 12/12 通过于 `artifacts/island/m4-migration-identity-green.log`；独立增量复核无新增阻塞。
- 最终 brain 全量 130 passed、2 既有 ignored，`artifacts/island/m4-migration-brain-final.log`；brain all-targets Clippy `-D warnings` 通过，`artifacts/island/m4-migration-clippy-final.log`。performance/storage/i18n 2226 键/行数/no-allow/Agent 指南和 diff-check 通过。
- 本片只完成迁移域层，尚未接入 App 后台准备、settings/proposals 读写和 BrainConfig.state_dir，未做真实 App 迁移启动验收。M4 的 raw wire 身份兜底、report reply/ReportRequested、岛内幕僚对话/汇报/TTS 仍未完成；M5/M6 和原型还原仍待完成。PR #189 最新 rust checks SUCCESS，仍 OPEN；后续只提 PR，不合入 dev。

### M4 迁移 App 接线与真实启动验收

- App 启动先在后台等待共享写队列、读取普通/桌面工作空间身份并迁移；唯一 owner 保存已准备目录，完成前禁止启动 BrainHost 或写配置。窗口销毁不丢全局准备结果；失败有中英文本地化提示，后续可以重试。BrainConfig、设置、提议及审批日志均使用已发布的 global/current。
- 提议完整保留 workspace_id，读取时校验来源并拒绝同文件重复 ID；AskRule 保留项目规则与 App 全局规则的区分。旧 approvals.jsonl 与模型 decisions.jsonl 合并迁移，新的审批审计必带 workspace_id/route_id。对应 App 准备、配置规则、审计路径及重复 ID 测试均保留 RED/GREEN 日志。
- 独立审查复现两个模型 GoalStore 与 App Writer 并发追加会破坏 JSONL；生产路径统一在完整单行 write_all 前获取排他文件锁，保持 GoalStore 原有同步完成语义。三写入者各 500 条 1 KiB 记录的公开接口测试先 RED 后 GREEN，逐行验证正文和身份唯一性，记录在 artifacts/island/m4-global-log-{red,green}.log。
- 完整回归发现旧 FIFO 暂停夹具在 macOS File::lock 下报 ENOTSUP；改为预持有普通日志文件锁，保留屏障未完成/最终快照断言，并新增完整日志追加断言。独立增量复核通过，未弱化或跳过生产锁。初轮失败记录 artifacts/island/m4-migration-domain-final.log；修正后 brain/ui 全量 154 passed、2 既有 ignored，artifacts/island/m4-migration-domain-final-green.log。
- App 幕僚回归 127 passed、1 既有真实 Claude ignored，artifacts/island/m4-migration-app-regression-green.log。brain/app all-targets Clippy -D warnings 与最新 App 构建通过，分别见 m4-migration-final-clippy.log / m4-migration-final-build.log；performance/storage/行数/no-allow/Agent 指南护栏、2230 双语键、6 条 i18n 脚本测试通过。
- 新增并登记 island-migration --visual。真实 App 验收于 artifacts/desktop-workspace-e2e/2026-11-06T14-16-14Z/ 完整通过，退出 0，1/1，568.64 秒（含模型看图等待）。两个工作空间的两个目标、两条完整提议、四条日志、三条确认偏好均保留，全部旧文件字节不变；中文设置读取旧值、修改语音开关只写全局配置，英文重启保持新值且复用同一代。隔离数据归档在该目录 brain-state；未结束用户正式 App。
- 三张关键图均由当前 Agent 实际读图并按 requestID/SHA256 逐项反馈。此场景保持幕僚总开关关闭，不调用 LLM/TTS；证明迁移和配置读写，不证明真实模型对话、语音播放或岛原型完整还原。
- 后续仍需适配旧 brain E2E 的新目录读盘断言和显式 workspace/route 协议，收口 raw wire 单目标兜底，完成 report reply/ReportRequested、岛内幕僚对话与汇报，再执行 M5/M6。M3 PR #189 仍 OPEN，最新 rust checks SUCCESS；M4 尚未提 PR，本轮仅保存开发检查点，不合入 dev。

### 执行顺序更新：先完成 M5（用户 2026-11-06 新指令）

- 用户明确要求“可以先做M5”，优先级改为 M5 → M4 剩余功能 → M6 总验收。保留全部范围；M4 全局对话、report/LLM/TTS 不因调整而删减。
- M4 迁移接线检查点 f1fabfe9 已正常通过 pre-commit 后提交。M5 在隔离 worktree 的新分支 feat/island-cleanup-20261106 开发，叠加在此检查点；仍只提 PR，不合入 dev。
- M5 验收映射：独立弹出/隐藏偏好由纯状态测试与后台持久化测试守住；全局快捷键由注册/录制恢复测试与真实 App 操作验证；旧入口删除由源码检索、画布布局/右侧内容区回归及 island-settings --visual 验证。隐藏后的系统通知兜底必须另有证据。
- Settings 沿用 apple-design/emil-design-eng 的清晰分组、组件库 Switch 和即时状态反馈，不新增装饰动效。新的设置存取必须后台化，退出先等待串行保存；实际页面与中英双语仍待本轮 E2E 看图。

### M5 设置检查点与交接（2026-11-06）

- 隐藏、审批/提问/完成四开关已接入 App 全局缓存、后台读取与串行保存；隐藏恢复不创建原生岛窗口，显示切换不抢前台。退出共同等待语言与岛设置版本，新增保存竞态测试先 RED 后 GREEN。App island 聚焦 28/28，settings 7/7，App build、app/runtime all-targets Clippy -D warnings 通过；详见 `artifacts/island/m5-settings-*.log`。
- `island-settings --visual` 于 `artifacts/desktop-workspace-e2e/2026-11-06T15-11-29Z/` 完整通过：退出 0，1/1 场景、5/5 步骤，927.75 秒（含看图等待），verification=model-assisted。中英四开关、精确持久化、原生合成器隐藏、重启不创建岛、恢复后悬停展开均实际验收。此证据不证明三类真实事件投递、Carbon、通知兜底或 M5 完成。
- 用户要求把剩余工作交给 Grace，交接任务书为 `docs/exec-plans/2026-11-06-norma-dynamic-island-handoff.md`。M5 变更尚未提交，交接须保留完整 diff；后续顺序 M5 → M4 剩余 → M6，所有后续只提 PR，不合入 dev。

### M5-C 旧入口删除（2026-11-06）

- 已删除：左抽屉（对话/运行概览/想法）、orb、`DesktopLeftView` 及左侧状态/拖拽/动画、状态胶囊与顶栏入口、`⌘⇧E`/`⌘⇧I`/`⌃1–9`/`⌃0`/`⌃⇧*` 快捷键、对应 i18n（drawers 102、shortcuts 26、workspace 5 个键）和文档。保留右抽屉与右侧内容区、岛复用的 overview 数据/投影/审批/问答链路；旧 `keyboard.shortcuts.v1` 中已失效的命令由 `lenient_overrides` 忽略，旧布局字段可读不报错。
- **取舍（须告知用户）**：想法清单与「提议」的派发/忽略 UI 随左抽屉一并删除，没有迁入岛。需要时应在岛内重做；`brain_host` 的提议数据与 CLI 链路未动。
- E2E：删除 WorkspaceDrawerTabs / RunOverview(+Agents/Titles) / StatusCapsule / OverviewFloat / Ideas* / DispatchEffects 场景与看图清单；CanvasDrawers、CanvasProjectDrawers 改为只验收右抽屉与画布占位（Swift 编译通过，**尚未在真实 App 运行**）。
- **未完成的 E2E 迁移**：WorkspaceAttentionRead、SettingsThemeConsistency/Visual 仍引用左栏「运行概览」，Brain* 场景仍通过 `⌘⇧E` 打开概览读盘，须在 M4（brain 迁移到 `global/current` 与岛内入口）和 M6 中改为经岛验收；此前不得宣称这些场景通过。
- 验证：runtime/storage/ui 全部通过；app/runtime/storage/ui all-targets Clippy `-D warnings` 通过；行数、no-allow、指南行数、storage、performance guardrail、check-i18n（2116 键）、test-i18n 通过。`app --lib` 并行全量测试因 GPUI 窗口类测试争用出现挂起/偶发失败，单独重跑相关测试均通过；`shortcuts::` 16/16。单线程全量重跑与真实 App 验收（画布布局、右抽屉、设置页、他 App 前台热键、通知兜底）仍待完成。

### M4 收尾：全局幕僚对话与汇报（2026-11-07）

- **raw wire 收口**：`allows_sole_target_fallback` 让缺 workspace/route 的大脑与编排命令不再退回唯一窗口（`norma_endpoint_tests::raw_wire_identity_fallback_never_applies_to_brain_or_managed_commands`）。E2E 中旧夹具因没带身份被拒，反过来证明收口生效；脚本化适配器已按 `【事件来源】` 带 `--workspace/--route`。
- **协议**：`BrainOp::Report.kind`（`ReportKind::{Status,Reply}`，旧 wire 缺字段视为 Status）、`norma brain report --kind reply`、`BrainEventKind::ReportRequested{overview}` + `TriggerKind::ReportRequest`；基础提示词与用户请求提示都说明 `--kind reply`。
- **岛内幕僚页**（`island/staff.rs`、`staff_view.rs`，纯逻辑在 `runtime::island::staff`）：全局对话（有界 40 条）、发送、汇报按钮（`bounded_overview` 最多 3000 字符并标注省略）、回复始终进对话；汇报请求的回复强制朗读（即使语音汇报关闭），其余遵循设置；未真正入朗读队列时清除字幕并提示“语音不可用，这条回复仅显示文字”；最近活动固定高度、只显示来源标签（跳过启动提示轮次）。锚点取带身份的项目会话（E2E 暴露：workspace 身份在会话上，不在桌面宿主上）。
- **desktop-e2e `--visual`**：
  - `island-staff`（脚本化大脑）通过：`artifacts/desktop-workspace-e2e/2026-11-07T02-47-25Z/`，对话、reply、汇报、提示核对（`prompts.log`：用户请求含原文+精确身份+`--kind reply`，汇报提示含带来源的有界概览）。
  - `island-staff-real`（真实 Claude Code 后端、用户本机登录）通过：`artifacts/desktop-workspace-e2e/2026-11-07T02-51-24Z/`，真实模型回复引用了运行中的 Alan，汇报时主动用 `history` 读取对话并如实说明“没有内容”。
  - `island-window` 通过：`artifacts/desktop-workspace-e2e/2026-11-07T02-07-25Z/`（含两处 UI 修复：悬停提示固定字号、无工作空间时保留启用引导）。
- **未验证 / 缺口（如实）**：
  - 真实 TTS 播放：测试环境语音不可用，只验证了“语音不可用”降级路径；逐句字幕仅有单测（`caption_index`），因没有语音入队而未在真实 App 里出现。字幕时长为字数估算（TTS 供应商不提供句边界）。
  - 全局对话借用一个带身份的项目会话作为锚点（身份是该会话的精确 workspace/route，不是前台项目）；无任何工作空间时显示启用引导。
  - 旧 brain E2E：已删除提议类场景（UI 随左抽屉移除），夹具改为带身份；`BrainApprovals/Autonomy/Questions` 等仍用 ⌘⇧E 打开已删除的概览，须改为经岛验收，**尚未迁移、不得视为通过**。

### 对抗式审查（独立 subagent）与处理（2026-11-07）

审查范围 `f1fabfe9..HEAD`，只读。发现与处理：
1. **中**：汇报请求无超时、且 pending 标记会被无关回复抢走 → 已修：`StaffChat` 请求带 120s TTL（`an_unanswered_report_request_expires`），到期后按钮恢复；仍存在“汇报等待期间恰好插入一条普通 reply 会抢走朗读标记”的窄窗口（无相关 ID），**已知限制**。
2. **中**：项目 Dock 的回复串进全局岛对话 → 已修：新增 `BrainEventKind::StaffRequest`，只有岛内请求提示 `--kind reply`；Dock 请求提示不变（`only_staff_requests_ask_for_reply_kind_reports`）。
3. 低：单项“恢复默认”未同步 Carbon 热键 → 已修（并去掉重复调用）。
4. 低：热键处理器未装上时仍会注册系统热键 → 已修（`handler` 标志）；启动时注册失败仍只有日志，设置页保存后会显示本地化错误。
5. 低：系统通知授权被拒时无反馈（授权回调为空）→ **未修，已知缺口**；隐藏岛 + 授权被拒时用户收不到通知。
6. 低：发送失败时输入被清空 → 已修（成功才清空）。
7. 低：活动记录每 3 秒初始化数据库 → 已修（只取路径，不再 `AppStore::initialize`）。
- 审查提到的 `opencode_question_host` 偶发失败：该测试用本机 HTTP 服务 + 200ms 读超时 + 真实 PTY，时序敏感；本机连跑 5 次出现 2 次失败，3 次通过。本分支未改动 opencode 相关代码，**未在 dev 基线上重跑对照，只能判为偶发**。
- 系统级热键 E2E `island-hotkey` 已写好（System Events 发 ⌃⌥⌘Space、前台为他 App），**本机 `AXIsProcessTrusted=false`，osascript 被拒绝发送按键 → ENV，未取得证据**；授权辅助功能后可运行 `scripts/desktop-e2e.sh island-hotkey --visual`。

### 方案第 8 节逐行证据（2026-11-07，分支 `feat/island-cleanup-20261106`）

| 标准 | 证据 | 结果 |
| --- | --- | --- |
| 形态切换、优先级、自动收回 | `runtime::island` 单测（runtime 全量通过） | 通过 |
| 别的 App 在前台时审批弹出，可就地允许，Norma 不抢前台 | `island-approvals` 等 `--visual`（M3 证据）；**系统级热键 `island-hotkey` 未取得证据**：本机 `AXIsProcessTrusted=false`，osascript 无法发按键，记 ENV | 审批通过；热键/前台不变 **未验证** |
| 「定位」跳到另一工作空间卡片（含窗口已关） | `island-navigation`（M3） | 通过 |
| 幕僚对话与跨工作空间语音汇报 | `island-staff`（脚本化）与 `island-staff-real`（真实 Claude Code）`--visual` 通过；`brain-dock-chat` 回归通过 | 对话/汇报文字通过；**语音朗读与逐句字幕未在真实 App 出现**（测试机无可用 TTS，仅验证“语音不可用”降级；字幕有 `caption_index` 单测） |
| 旧幕僚数据迁移后不丢 | brain 迁移单测与 M4 迁移接线检查点 | 单测通过；真实 App 启动检查沿用 M4 检查点证据 |
| 左抽屉与状态胶囊完全删除、无死入口 | 代码检索、i18n/快捷键文档清理、`canvas-drawers` 与 `canvas-project-drawers`（改为只验右抽屉）通过 | 通过；设置页“快捷键”页的真实 App 看图 **未单独做** |
| 刘海屏、外接屏、全屏 Space、点击穿透、动画中间帧 | 无 | **未验证**（无对应硬件/场景） |
| 系统通知兜底（隐藏岛时） | `NotificationGate` 单测 | 真实通知投递 **未验证**；授权被拒时无反馈（已知缺口） |
| 样式还原原型 | `island-window` 与 `island-staff` 看图（页签、胶囊、幕僚页对照 `artifacts/island/prototype-*.png`） | 幕僚页/页签通过；审批、问题卡片的逐像素对照未重做 |

其他：`app --lib` 并行全量因 GPUI 窗口类测试争用会挂起，单独重跑均通过；单线程全量见 `artifacts/island/final-app-lib.log`。仍引用已删除左栏“运行概览”的旧场景（`WorkspaceAttentionRead`、`SettingsTheme*`、`BrainApprovals/Autonomy/Questions` 等）**尚未迁移，不得视为通过**。

### 收尾补充（2026-11-07）

- 已迁移/退役旧场景：`WorkspaceAttentionRead`（契约已由 `island-artifacts` 覆盖）、`BrainApprovals`、`BrainQuestions`（契约由 `island-approvals`、`island-questions` 覆盖；“采用/委托幕僚回答”UI 随左抽屉删除，无迁移对象）已删除；`BrainAutonomy` 改用 ⌘⇧B 打开幕僚活动；`SettingsThemeConsistency/Visual` 改为只验右抽屉（**改写后未在真实 App 重跑**）；`canvas-drawers`、`canvas-project-drawers` 在真实 App 通过（OCR 模式）。
- 通知授权被拒现在会在设置页显示本地化提示（`notification_permission_notice` 单测；真实授权弹窗路径未在本机验证）。
- 汇报请求有编号：`report --reply-to <id>`，只有带正确编号的回复才朗读，无关回复不再抢走标记（`only_the_reply_to_a_report_request_gets_spoken`、`report_reply_to_is_optional_and_numeric`）；已更新脚本化夹具与提示词的需要见下。
