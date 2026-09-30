# Norma iOS 01–05 本地联调记录

> 2026-09-30 单码注册更新：43 的额外邀请码步骤已被替代，44 已停止测试；45 发布与当前验收流程见 [单次扫码与派工验收](2026-09-30-norma-ios-testflight-dispatch.md)。真机验收及其余 Goal 项保持未完成。


2026-09-29。最新用户分工：真机验收由用户后置完成；继续完善剩余功能，完成后更新 Issue。**只本地运行、测试，不部署云端。** 下一阶段 Goal 已 active，#54–#58 尚未关闭；不能把单个切片完成误报为整个目标完成。最新优先级：**Norma 语音通话最后做，优先其他功能**。

**最近运行证据（2026-09-30 followup）：Xcode/Simulator 缓存权限阻断已解除，Computer Use 已实际操作 iPhone 16e 原生界面。** P02 已补入场、录音按钮呼吸/按压和电平反馈动效，修复退出确认及最大字号键盘遮挡；最新相关原生测试 5/5 通过。下文锁屏、权限和窗口错误保留为历史，不代表当前仍需用户调整权限；P02 局部结果不代表完整 UI 或真机语音验收通过。

## 最新核对：功能与验收缺口（2026-09-30 followup）

本节优先于下方历史快照。证据目录：`target/norma-ios-evidence/followup-20260930/`。

- **已验证的修复**：终端被人工接管时，手机自动消息保持原队列等待，不再误报结果未知；正式网络专项红测后转绿，1/1 通过。原任务追问的来源绑定、命令与原消息队列已支持同事务写入；既有存储全量 121/121 通过。正式追问端点与 Swift typed client 已接入；本轮正式 Swift/Node/Rust 网络 1/1、外层 Rust 1/1，三个中继回归 3/3。手机持久追问队列与一次回复落盘已接入，详见下方连续追问切片；W06/P05 入口已实现并在独立复核中，真实 Coding Agent 尚未完成，不能据协议通过标为交付。
- **构建与回归**：`cargo check -p app` 退出 0；最新串行 `cargo test -p e2e -- --test-threads=1` 退出 0，291 通过、76 明确跳过（`task-followup-e2e-regression.log`）。该结果不证明此前默认并发的间歇失败已修复。手机设置专项 3/3 通过；既有设置函数移到同职责子模块后，500 行护栏通过。
- **原生页面**：早期 `ideas-recheck.log/.xcresult` 的退出确认失败已修。最新 `idea-editor-visible-green.log/.xcresult` 记录标准/AX5 编辑与退出、原型比例、波形减少动态效果、录音恢复共 5/5 通过。根任务已用 Computer Use 完成进入→编辑→系统键盘→完成→取消放弃→返回放弃；完整录像 `P02-motion-final-cua.mp4`，约 9 秒动效预览 `P02-motion-preview.mp4`。独立 `idea_motion_review` 完成真实 Computer Use 并给本轮 P02 页面与非录音交互 PASS；动效复核为源码与录屏抽帧，真机语音仍后置。
- **仍需交付**：原任务追问的 UI 收口、新成果读取及真实验收、W06 具体工作步骤；线程 `@Norma` 真实模型编排；真实 Coding Agent 退回修改→新成果→验收；真实终端接管、后台/意外断网/Mac 与中继重启/跨网络验收；完整原型、失败态、辅助功能和旧代码清理。所查本地环境仍缺实际模型 Key，真实模型验收待配置。
- **状态边界**：GitHub CLI 最新读取 #54–#59 全部 OPEN，Goal active。已用 GitHub CLI 发布 [#54 动效进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/54#issuecomment-5901482618)，保持 OPEN；仅本地测试，无提交、推送或部署。真机扫码、Apple Speech、麦克风由用户后置，Norma 通话最后。早期 `single-mac-20260929` 证据目录本次未找到，真实 Codex 通过仅按历史文档记录引用，不宣称本轮重新验收。

## 原任务追问入口与草稿（2026-09-30 composer）

- 已接 W06「回到线程／回线程追问」与 P05 回复对象、取消、独立追问状态；来源送达不等于 Agent 收到。返回操作由原线程持有，成果与后续任务继承同一返回动作；不另建线程实例。追问的新成果读取仍未接入，不打开原成果冒充新成果。
- `WorkThreadDraft.followupTaskID` 与原文共同保存，空正文也能恢复对象；编辑保留，取消仅移除对象；发送先按对象路由，再判断普通提及。跨线程对象及成果退回／终端草稿夹带对象均拒绝；成功只清发送时 revision，新输入保留。
- 原生红测先暴露编辑丢失对象、误入 Norma 分支；随后原生草稿 5/5 通过（`composer-ui-red.log` 中的 DeviceTests 区块）。Core 专项 22 通过、1 个显式 UI fixture 跳过（`composer-core-regression.log`）；存储红绿见 `composer-store-red/green.log`。
- 首轮 UI 标准通过，AX5 任务计数因屏外懒加载失效；修正为先返回原任务位置。后续标准滚动实际卡顿，`composer-ui-hang.sample.txt` 显示主线程反复进入 SwiftUI LazyStack 布局。每条消息与附属回执／卡片改为一个稳定测量行后，同库滚动复测通过；保留原懒加载，不把整个历史改为一次渲染。
- `composer-ui-current.log/.xcresult` 中标准/AX5 追问通过、旧固定坐标进展滚动失败为历史记录。辅助函数按可读区域与实际位置滚动后，`composer-progress-adaptive.log/.xcresult` AX5进展1/1通过；五张截图覆盖首末回执、同步时间及最新正文末尾。中间 `composer-ui-verified.log` 因重装后使用旧数据目录失败，重新取得当前 container 后恢复。
- `composer-long-draft-red.log/.xcresult` 名称虽含red，实际AX5长正文1/1通过；输入自然压缩并内部滚动，无需缩小字号或改变行数。独立初审另发现禁用反馈及减少动态效果布局问题：共用IdeaPressStyle已统一淡化禁用控件，移除P02重复淡化；追问对象在Reduce Motion下改为静态切换。
- 最终 `composer-review-final.log/.xcresult` 原生UI **4/4通过**：标准追问、AX5长正文追问、未派工禁用追问与长指令展开、P02最大字号编辑退出。独立只读Agent对源码及最终截图 **Approve**，报告 `composer-ui-current-attachments/composer-ui-review.md`；这是源码/截图复核，不是独立Computer Use PASS。
- 根Computer Use重绑并Raise窗口后实际进入工作空间→交接线程→任务详情；点击追问再次返回`noWindowsAvailable`，没有完成追问实操闭环。`P05-followup-cua.mp4`记录此次尝试，不能作为追问通过证据。Xcode仍正常，不要求用户修改缓存权限。
- 仍需：独立 UI 复核收口、嵌套成果→修改任务的实际返回验证、追问新成果读取、真实 Coding Agent 连续追问，以及原 Goal 其他验收门槛。只本地测试，未部署、提交或推送；#57/#59 保持 OPEN。

### 用户调整：优先交付真机直接派工（2026-09-30）

用户希望先自行真机验收“手机工作线程→Mac智能体→结果回原线程”。当前源码的开发中继仅支持`127.0.0.1`，默认生产端点尚未部署新版中继，因此不能把模拟器联调通过当作现有iPhone安装即可使用。优先补可访问且验证证书的本地验收入口、重跑真实Coding Agent、提供安装与配对步骤；不部署云端。Xcode当前列出用户iPhone为Offline，设备连接前可继续完成代码与模拟器验证。

## 当前剩余工作速览（2026-09-30）

### 连续追问与手机持久恢复（2026-09-30 continuation，最新）

- Mac 入队、每次出队和真正 PTY 写入前核对原任务及当前回合。连续追问只接受同原任务已确认的前序追问回合；其他任务/线程/设备不能成为前序。后台解析绑定，前台不查库；等待期间换回合或新任务接替时明确拒绝。
- iOS 同事务保存新来源及独立追问，不创建第二个派工；原草稿仅按捕获的 revision 清理。回复只追加一次，引用原任务和本次来源；原任务的回复、成果与回执保留。Mac 返回并固定 `original_key`，原任务尚未产生成果时也可核对会话身份。
- 追问和派工共用原来的单操作轮转及每 5 秒预算；App 恢复扫描包含已送达来源之后的独立追问。缺少授权/成员移除/来源范围变化保留原操作；已记住签收而 Mac 查询为空时明确报错，不自动重发。
- 红测：`continuation-storage-red.log`、`continuation-network-red.log`、`continuation-frontend-red.log`、`continuation-ios-store-red.log`、`continuation-ios-recovery-red.log`、`continuation-foreground-red.log/.xcresult`。首次前台过滤未命中日志不算测试；扩展任务归属场景曾误设 fixture 为立即 Submitted，已改用既有协调任务注册 fixture。网络恢复 fixture 首次未保留设备 origin 的失败也保留，修正后通过。
- 已通过：`continuation-storage-green.log` 1/1；`continuation-frontend-final2.log` 1/1（正式 App 前台、录制 PTY）；`continuation-ios-network-final2.log` Swift 1/1 + Rust 1/1（正式中继、连续追问、手机丢回执恢复、模拟 Mac 丢账本）；`continuation-native-final.log/.xcresult` 原生 5/5；`continuation-swift-regression.log` 132 通过、7 条外部 fixture 测试跳过。行数、no-allow、存储与性能护栏退出 0。
- 仓库回归：`continuation-workspace-regression.log` 首次发现旧配对测试未穷举新增内部命令，已补拒绝分支；`continuation-workspace-final.log` 随后完成 1844 通过、89 跳过，因终端既有 `test_zsh_login_shell_loads_profile_path_before_zshrc_zoxide_init` 的 3 秒退出等待失败而退出 101。已完成部分包含 E2E 292 通过/76 跳过、存储 122/122；`continuation-terminal-recheck.log` 聚焦复跑 1/1 通过，原全仓测试二进制的同一测试也在 `continuation-terminal-same-binary.log` 复跑通过（2.37 秒），尚不能把全仓记为稳定通过或宣称已修该超时。`continuation-app-check.log` 退出 0，保留原有两条设置辅助函数未使用警告。
- GitHub CLI 已发布 [#59 连续追问进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5902406353)，保留 OPEN。[#57 连续追问进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5902417365) 也已发布，保留 OPEN；首次 EOF 核实未落库后再重试，没有重复评论。
- **交付边界**：本切片未改页面布局，未新增 UI 验收结论。W06/P05 追问入口、可恢复的回复对象与草稿、真实 Coding Agent 连续追问及独立 Computer Use 复核仍待完成。这里的 Provider 是 fixture，不能作为真实模型执行证据。

### 追问正式中继与 Swift 基础（2026-09-30 followup）

- 已实现 `thread_task_followup` / `thread_task_followup_receipt` 及 `thread_task_followup_v1` 协商。新来源必须已送达、正文一致且不同于父任务来源；复用原消息队列，不建立新 Agent 或派工。没有命令/快照/终端 worker 时拒绝新入队。已有查询和重试保持原操作及原执行 route。
- 队列真正确认 Provider 新回合后，仅返回该回合的持久回复与成果。Swift 校验原任务、来源、节点/run/session、新 turn 与成果身份，复用现有队列回执验证；Codable 保留回复正文。已补正式 Swift 网络读取、同操作重试和断线重连测试，外层检查没有第二次前台命令。
- 红测：`task-followup-network-red.log` 为最初端点缺失；`task-followup-swift-red.log` 为手机客户端接口缺失；`task-followup-source-red.log` 发现可复用父任务来源入队，现已拒绝，原子来源独立性得以保留。
- 最新验证：`task-followup-native-network-final.log` 中 Swift 1/1、Rust 外层 1/1；`task-followup-relay-regression.log` 3/3（旧派工、终端接管、新追问）；`task-followup-core-regression.log` 12/12；`task-followup-native-core.log/.xcresult` iPhone 16e/iOS 26.3 原生核心 1/1；`task-followup-norma-tests.log` 128/128；`task-followup-app-check.log` 构建检查退出0（既有设置辅助函数2条未使用警告）。行数与 no-allow 护栏通过。
- 这批基础证据之后的 Mac 前台边界和手机持久恢复已由上方 continuation 切片补齐。下一步接 W06/P05 追问入口与回复对象草稿，随后验证真实 Coding Agent、Computer Use 与独立复核；本节 Provider 为 fixture。

串行 E2E 最终回归为 291 通过、76 明确跳过，退出0（`task-followup-e2e-regression.log`）；不代表默认并发间歇问题已修。GitHub CLI 已发布 [#57 本轮进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5901856567)、[#59 本轮进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5901856596)，两票保持 OPEN。无提交、推送或部署；Goal active。

下方序号清单保留此前总体进度；本节和文首的最新证据优先。

1. **原生页面直接 @Agent 派工已通过（#57/#59）**：iOS Simulator 原生页面创建 Workspace、关联真实项目/Alice、创建线程并发送指令，经本地中继送达真实 Mac/Codex；真实文件和回复返回原线程。UI 1/1（332.633 秒）、Rust L3 1/1（1117.95 秒，含构建和人工界面准备）、身份清理 1/1 通过。重启后重新联网读取成果，宿主最终检查仅执行一次；根任务 Computer Use 已操作原线程、成果正文和返回。独立复核认可真实流程，但发现 P05/W06/W09 还原缺口；P05 成果卡与 W09 阅读层级已修，W06 已补真实回执时间线、任务身份头部与最新进展，标准/AX5 UI 2/2；终端/回线程追问、真实工作步骤与 Computer Use 仍未完成，不能扩展为全部 UI 或 Goal 通过。
2. **线程 @Norma 编排与真实模型（#57/#58/#59）**：服务端候选接口、手机 typed client、候选同事务保存和原生线程入口已接入；正式回环 HTTP 与草稿/原子回滚测试通过，供应商是明确测试替身。明确选择的同名成员保护与成果卡顺序问题已修；原生成功/最大字号 UI 2/2、原生模型 6/6 通过。AX5 关闭按钮省略问题已修、Norma 正文补图通过，独立截图复核局部 PASS；最新标准 UI 1/1。Computer Use 因 Mac 锁屏待补，不能算整个切片验收通过。本地后端当前进程及 `.env` / `.env.local` 未配置实际所需 `DOUBAO_SPEECH_API_KEY`，真实模型回合仍未验收；Mac Codex 登录不能代替网关授权。
3. **完整闭环与可靠性验收**：直接 @Agent 的真实执行、主动重连、重复派工与线程隔离已通过；真实 Agent 的退回修改→新成果→验收通过尚待验证，现有对应证据使用测试执行器。真实 App 后台、意外断网、Mac/中继重启恢复及跨网络验收仍未完成。
4. **页面与清理收口（#54–#59）**：全页面原型对照、VoiceOver、失败状态和首页其余最大字号问题；Workspace 标题/筛选/底栏已修复，标准及最大字号原生测试 2/2、独立局部截图复核 PASS，最大字号 Computer Use 仍待补。清理被替代旧代码，再按证据更新 Issue 状态。

Xcode/Simulator 权限已正常；本轮 Computer Use 受 Mac 锁屏阻挡。W09 成果验收/退回与键盘末行局部复核 PASS。真机扫码、Apple Speech、麦克风由用户后置，Norma 通话最后。GitHub CLI 本轮核对 #54–#59 仍全 OPEN。

### 手机终端控制接续（2026-09-30，最新）

Swift已接入接管/输入/交还与受控读取；人工操作和草稿保存在原任务SQLite，断线重启仅查询原结果。正式中继新增只查不执行的恢复语义。Swift整链1/1、原生核心3/3、存储119通过；W06a页面/真实光标/系统键盘尚未接入，不能据核心测试宣称手机接管页面完成。本轮Computer Use仍确认Mac锁屏；完整证据见文末。

### 正式中继终端接管更新（2026-09-30）

接管/输入/交还已复用正式中继和持久命令回执；受控读取在前台验证原输入权后续期，重连不能用旧成功回执取得输入权。连接关闭、权限降级和服务停止同步撤销能力，处理中取消也保留关闭保护。正式Node中继+Mac服务网络专项2/2；正式App前台+录制PTY专项1/1，修复竞争接管失败后残留能力卡住下一次接管的问题。Runtime204通过/3ignored，串行E2E290通过/76ignored；此前默认并发间歇失败尚未定位，不能按稳定全通过收口。Swift控制客户端、持久人工操作、W06a页面/实时输出/真实光标仍待完成；尚未交付手机接管页面。完整记录见文末。

### 终端输入权基础更新（2026-09-30）

Mac Runtime 已实现独占输入、15秒租期、原会话身份校验、人工草稿保护及可由后台撤销的原任务租约；接管中的自动派工/会话追问复用原队列等待，交还或撤销后继续。租约不可由手机字符串重建，旧租约永久失效；持有者可读取同session的后续真实回合，页面不改写原任务身份。Runtime全量203通过/3明确跳过，队列专项1/1。完整E2E曾串行/默认各288通过，但后续并发又复现会话恢复和网络超时，最终串行全量288通过/76ignored，仍不记整体稳定PASS；详细记录见文末。中继接管命令、W06a页面和真实App接管验收尚未接入。

### 终端读取基础更新（2026-09-30）

W06 原生终端入口仍待接入；底层已支持从原派工操作查询已确认 Provider 回合，经受限中继读取原终端及历史页。手机客户端、Mac 前台身份校验和原生 Core 测试已通过；没有新增 UI／Computer Use PASS，也未实现接管/交还。完整证据见文末「原任务终端读取基础」。

### 本轮真实执行证据

- `real-codex-l3-first.log`：首次在手机连接前报 `invalidCredential`；根因为验收脚本使用系统 UUID 的大写 hex 生成配对 token，与 Swift 小写契约不符。脚本已改小写，没有放宽生产验证。
- `real-codex-l3-credential-fixed.log` / `real-codex-submitted-swift.log`：重跑已越过凭据检查并完成消息接收/任务提交；真实 Swift 用例 180.588 秒后失败，执行停在 `submitted`，读取成果报 `reviewUnavailable`。两目标文件均不存在。
- Computer Use 已实际打开隔离项目、开启仅回环手机服务并观察 Codex；CLI 曾停在本轮测试目录信任提示，确认后任务正文仍停在编辑区。证据为 `real-codex-submitted-cua.png`；不能据此提前判定根因为生产派工逻辑。
- 两轮临时设备授权均已撤销，本轮隔离 App 已正常退出，两个私有 Codex 登录符号链接已删除，原登录文件未删除。网关随测试失败退出；未部署、未提交或推送。

上述证据位于 `target/norma-ios-evidence/single-mac-20260929/`。本次新增的真实测试使用运行于 macOS 的手机共享核心，不等于 iOS UI、真实手机或蜂窝网络验收通过。

### 第三轮真实 Codex 验收通过（22:45）

- 上轮隔离 Codex 的信任配置使用 `/var/...`，实际 PTY 工作目录为 `/private/var/...`，因而停在首次信任提示。测试项目创建后使用 `canonicalize()`，仅对齐本轮目录的配置与真实路径；没有修改生产派工逻辑、增加执行等待时间或手工按回车提交任务。
- `real-codex-l3-canonical.log`：Rust L3 1/1 通过；总计 525.13 秒含 App 构建和 Computer Use 界面准备。`real-codex-canonical-swift.log`：正式 Swift 整链 1/1，48.278 秒，0 失败。
- 真实 Codex 写出 `result.txt = norma-real-codex\n`、`executions.txt = executed\n`，回复 `NORMA_REAL_CODEX_OK`。冻结文件通过正式接口读取；重复派工保持原 task_ref，追加仅一次；另一个线程查询被拒绝，手机 Store 重开后原线程仅一条 Agent 回复，其他线程为空，未自动验收。
- 根任务通过 Computer Use 观察实际卡片 Done 与最终回复，截图 `real-codex-canonical-pass-cua.png`；回执 `real-codex-canonical-receipt.json`。这是实际 Coding Agent 证据；仍不覆盖 iOS 页面、Norma 编排、Mac/中继重启或跨网络。
- 本轮临时手机授权已撤销；隔离 App 正常 Quit 后进程退出，网关已结束；私有 Codex 登录符号链接已移除，用户原登录未变。只本地联调，未提交、推送或部署。#54–#59 与 Goal 保持未完成。
- 已用 GitHub CLI 同步实际进展：[#57 回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5892677488)、[#59 回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5892678956)；两票保持 OPEN。

## 本地实际运行（早期记录）

- 当前源码的 Node 网关已启动，仅监听 `http://127.0.0.1:58477`；健康检查 200，新文字接口匿名访问 401。进程为本次执行会话 `87625`；继续工作前须重新检查该句柄和健康状态，不能只凭本文认为在线。
- 隔离状态目录：`target/norma-ios-evidence/goal-01-05/local-20260929/local-service-hjrvSq/`。随机本地管理凭据只存于该目录的 0600 文件，未输出到日志；没有载入正式 Key，当前持续运行实例在受邀后也会明确返回供应商未配置，不伪造真实模型回答。
- 没有上传或重启任何正式服务。正式版地址保持原配置。现在 Debug Simulator 可用指定端口与隔离 UUID 连接本地实例，见 `ios/README.md`；尚未在模拟器启动带该配置的新 App，不能声称手机界面联调完成。

## 已通过

1. Node 22.23.2 下 `npm --prefix services/norma-cloud test`：旧语音、Plan、工具票据与邀请文字服务全部通过。日志：`target/norma-ios-evidence/goal-01-05/local-20260929/service-tests.log`。
2. 新增 `npm --prefix services/norma-cloud run test:native:text`：正式网关启动真实本机 HTTP，Swift `CloudTextClient` 实际兑换邀请、收到中文分段回复，完整结束后持久保存并重开搜索；重复 ID 拒绝，模拟上游中断不保存半句，撤销后拒绝。1 项 Swift 网络集成测试通过，供应商调用次数准确为 2（成功与中断各一次）。日志：同目录 `swift-http.log`。
3. 网络联调的模型回复是隔离的确定测试数据，供应商没有真实联网；Swift 运行在 macOS 共享核心。未把它算作真实云模型、iPhone/Keychain、原生 UI 或 Computer Use 验收。
4. 新测试已由 `xcodegen generate --spec ios/project.yml` 加入原生测试工程。未启动 fixture 的普通测试明确 skip 该专项，不会静默访问正式域名。

## 早期 iOS 运行阻断（现已解除，历史记录）

- `simctl list devices booted --json` 能读到 iPhone 17 Pro Max（`3052AC09-308A-47FB-9467-F9A57CA8B463`），但 Xcode 测试启动仍失败：CoreSimulatorService 连接无效，以及 `~/Library/Caches/org.swift.swiftpm/manifests/ManifestLoading/webrtc.dia` 写入被拒。
- 首次命令还带有相对 package cache 路径错误；改为绝对工作区路径后重新运行，仍是上述权限失败，退出 74。有效失败日志：`target/norma-ios-evidence/goal-01-05/local-20260929/native-tests-retry.log`。这不是测试断言失败，也没有执行新的 App 流程。
- 此次计划运行的 `CloudChatSendingTests`、`IdeaRecordingRecoveryTests`、`WorkThreadFlowTests`、`WorkspaceLibraryFlowTests`、语音编辑/放弃正文 UI 用例仍未执行。
- Computer Use 实际请求 Simulator 返回 `Computer Use was not approved to use Simulator`；未绕过。`devicectl list devices --timeout 10` 超时，未能确认当前是否有可用真机，不能把超时说成没有设备。

下一步：在允许 Xcode/Simulator 运行并授权 Computer Use 的环境，重跑上述原生专项与关联项目 fixture，获取新截图后交独立 UI 子 Agent 复核；继续按用户指令保持本地测试，不部署生产。


## 最新补全与独立复核

1. **#57 线程草稿**：SQLite 按线程保存正文与 revision；输入即时异步保存、快速编辑合并、返回/重读不覆盖新编辑；发送只清理捕获的版本，清理与消息落盘为同一事务；同文新版本继续保留。错误按线程隔离，超 100,000 字保留输入原文并说明原因，其他线程仍可保存/发送；后台保存复用系统有限时间。
2. **#58 本地入口**：DEBUG 专用 `127.0.0.1:<port>`，必须配合法 UUID 命名空间；数据库和 Keychain 各自隔离。显式非法配置拒绝加载，不回退生产；不接受任意 URL、不跟随重定向，Release 无此入口。
3. **#54 音频释放**：编辑器从未播放录音时，退出不再释放其他功能的音频会话；初始化失败也只释放自身已激活的会话。
4. **独立 Agent**：`new_shell_ui_review` 首轮指出同文新草稿清理与超长草稿拖累其他线程两项问题，均已修复；复核给出代码范围 PASS。音频释放、本地云凭据隔离代码也无新增问题。没有新的页面截图，**视觉/交互仍未验收**。
5. **Computer Use**：本轮再次选择 Simulator，仍返回 `Computer Use was not approved to use Simulator`，未绕过。真机后置不等于免除模拟器和页面验收。

## 本轮证据

证据目录：`target/norma-ios-evidence/goal-01-05/local-completion-20260929/`。

| 检查 | 结果与边界 |
|---|---|
| 新增测试先失败 | `red.log`、`draft-revision-red.log`：新增接口尚未实现导致编译失败；之后补实现。 |
| 想法/Workspace/线程/云历史与配置聚焦检查 | `core-focused.log`：15 项通过，含草稿重启、版本保护、超限隔离。 |
| Swift 全量首次运行 | `core-suite.log`：102 项，100 通过、1 网络专项因未启动 fixture 跳过、1 旧 `CredentialVaultTests` 失败：Keychain `-67674`。未将该失败隐藏或改为通过。 |
| 修复后其余 Core 回归 | `core-final.log`：102 项，101 通过、1 网络专项跳过；显式排除上述环境阻断的 `CredentialVaultTests`。网络专项已另行单跑通过，未将 Keychain 记为通过。 |
| 本机真实 HTTP 联调 | `swift-http.log`：1 项通过；使用新的本地客户端入口及正式网关，供应商为确定的测试流；成功/中断仅调用上游 2 次。 |
| iOS SDK Core / App | `core-ios-build.log`、`app-module.log`：编译通过，App 用 Swift 6 / iOS 17 Simulator target。不是安装或运行验收。 |
| UITests / DeviceTests | `ui-typecheck.log`、`device-typecheck.log`：类型检查通过。新增 `WorkspaceDraftTests` 与重启草稿 UI 断言已加入工程，未执行。 |
| 服务端测试 | 本轮未改网关业务；沿用前节已通过的 Node 22 全套结果，本轮重新跑了真实 HTTP 联调。 |

## 逐票进度（远端评论回执见文末）

此前 GitHub CLI 读取失败；完整执行权限生效后，已成功读取 #54–#59 的最新正文与状态，六票仍为 OPEN。以下表格已按新的原生验收证据更新，远端写入结果另在文末记录。

| Issue | 本地完成部分 | 仍需完成（不含用户后置真机） | 建议状态 |
|---|---|---|---|
| #54 想法库 | 独立文字/语音数据、搜索编辑、录音恢复、权限/正文/音频会话修复；原生文字保存编辑搜索重启流程通过 | P01/P02 完整页面交互复核 | 保持进行中；真机 Apple Speech/回放由用户验收 |
| #55 Mac 连接与授权 | 当前及未来项目授权、撤销/修订、电脑/项目摘要；已有 Swift/Rust/部分 UI 证据 | 多 Mac 在线/离线、最新版页面/辅助功能复核 | 保持进行中；真机扫码/双端确认由用户验收 |
| #56 Workspace | 多 Workspace、真实项目/Agent 关联、搜索归档重开及保存错误恢复；真实项目/Alice 的原生关联流程通过；标准/最大字号首页测试 2/2、局部截图复核 PASS | 最新 P04 原型复核、最大字号 Computer Use 与完整辅助功能 | 保持进行中 |
| #57 工作线程 | 来源、长期消息、目标/决定、草稿；持久 Mac 接收及恢复；@Agent 与冻结上下文；执行状态/具名回复回原线程；冻结成果正文、文件对比；双字号退回→修改→验收→重启自动化通过；W09 局部 UI 复核 PASS；原生 iOS UI 到真实 Mac/Codex 派工、回复和成果回原线程 1/1 | @Norma 真实模型编排、W06 终端/具体工作步骤/原任务追问、完整 P05/W05/W09 辅助功能及首页其余最大字号问题 | 保持进行中，实际 Codex 通过不代表全 App 验收 |
| #58 Norma 单聊 | W07 同屏文字、邀请/鉴权/流式与本机历史、受控本地网关入口；无需 Mac 的原生入口测试通过 | 新 App 与本地真实模型回合、原生界面联调和复核；目前本地实例未配置供应商 | 保持进行中；不部署云端 |
| #59 单 Mac 派工 | 中继/授权/会话；手机待发与恢复；完整 task_ref 事件/回复/冻结成果正文；验收记录与原操作恢复；双字号修改/验收原生自动化通过；真实 Codex 执行、重连去重、回复/成果回原线程通过；原生 iOS 页面直接派工 1/1 | 真实 Norma 编排、Mac/中继重启、后台/意外断网/跨网络与完整 E2E | 保持进行中；直接 @Agent 成功流程通过，其他门槛未过 |

GitHub 已恢复读取；更新时合并本表的新进展与用户真机分工，不覆盖其他人的最新进展。全部非真机门槛通过后可标记“开发完成／待用户真机验收”，有实际功能或运行门槛未完成的票不能提前关闭。

## 下一阶段：中继与端到端会话基础

实施依据：[本阶段 Goal](2026-09-29-norma-ios-single-mac-goal.md)、[#57/#59 规格](../product-specs/2026-09-29-norma-ios-issue-59-single-mac.md)。本轮未修改页面，没有新的视觉或 Computer Use PASS，也没有部署云端。

- 在既有 Node 网关增加在线协作中继，复用 #58 准入；受信 Mac 用配对安装证书签挑战。断线只通知通道关闭，无服务端任务队列或伪造持久回执。
- 中继校验撤销、连接归属、挑战时限、容量和帧大小。测试发现空 Origin header 和到期挑战先于定时器回调两个问题，先留下失败证据再修复。
- 新增 Rust ring / Swift CryptoKit 的 X25519、安装证书验签、HKDF、分方向 AES-GCM 会话；双方各与独立 Node crypto 对端互操作。这里只证明密码与协议基础可用，尚非真实 Mac 任务执行或 Swift→中继→Mac 整链运行证据。
- 当前进程与 `services/norma-cloud/.env` 均未提供豆包／OpenAI 供应商配置；只检查存在性，未输出密钥。GitHub CLI 读取 #59 仍返回 401，没有更新远端。

证据目录：`target/norma-ios-evidence/single-mac-20260929/`。

| 检查 | 结果与限制 |
|---|---|
| TDD 失败记录 | `relay-red.log`、`relay-origin-red.log`、`relay-expiry-red.log`、`mac-signing-red.log`、`session-red.log`、`native-session-red.log` |
| Node 服务回归 | `service-tests.log`：原语音兼容、Plan、工具、文字及中继测试全部通过；无真实供应商调用。最后的挑战到期修复另在 `relay-final.log` 通过。 |
| 中继实际 WebSocket | `relay-final.log`：邀请/撤销、离线、双向原字节、签名重放、重复 Mac、连接隔离、重启、8 通道／256 KiB／120 帧限制通过。 |
| Rust 全 crate | `norma-final.log`：127 通过，0 失败，1 项既有忽略；含新增 4 项签名／加密测试。 |
| 原生 Swift 密码互操作 | `native-session-final.log`：2 项通过；macOS 共享 Core，未运行 iPhone 页面。 |
| iOS SDK 编译 | `core-ios-build.log`：NormaCore 的 arm64 iOS 17 Simulator 编译通过；不等于 App 启动或 UI 验收。 |
| 基本约束 | `git diff --check`、`check-no-allow.sh` 通过；未提交、未覆盖其他任务改动。 |
| 全仓行数护栏 | `line-check.log` 未通过：既有未提交的 `crates/app/src/settings/mobile.rs` 为 546 行，超过 500 行；本轮未修改该文件，仍须在后续 Mac 设置收尾时按职责拆分。新增 Rust 文件未超限。 |

## 原生网络适配器接入

Mac 出站连接纳入 `MobileServer` 生命周期，使用既有安装身份与设备授权；权限改变使已有中继通道断开并重新认证，停服取消重连。Swift 使用固定服务地址及 DEBUG 回环端口，复用云设备凭据、Mac 配对凭据、现有有界 WebSocket 读写和快照校验。同通道只允许一个在途请求。当前仅开放认证／授权项目读取，不宣称 `collaboration_v1` 或持久送达。

使用正式 Node 中继和真实 Rust `MobileServer` 做实际网络测试，Swift 跑在 macOS 共享 Core。快照源是隔离项目测试源，未启动 GPUI App，也未执行 Coding Agent。没有修改页面、部署云端或更新远端 Issue。

下列日志仍位于 `target/norma-ios-evidence/single-mac-20260929/`：

| 检查 | 本轮结果 |
|---|---|
| 失败记录 | `mac-connection-red.log`、`native-connection-red.log`：新接口实现前失败；`native-relay-redirect-red.log`：发现重定向拒绝时底层网络错误未映射，已修复。 |
| Swift → 正式本机中继 → Mac 服务 | `native-mac-relay-final.log`：1 个 Rust 集成场景内调用 1 个 Swift 网络场景，通过；安装身份、连续读取、错误凭据、未认证读取、禁止配对、撤权重连、停服、取消、5s 超时、拒绝重定向及连接释放。 |
| 既有配对／授权回归 | `pairing-relay-regression.log`：2 项通过，包含当前及未来项目、修订与撤销。 |
| 既有命令生命周期回归 | `command-relay-regression.log`：3 项通过，包含撤权／停服租约、双 Mac 归属及回执写失败不重做。 |
| App / iOS Core 编译 | `app-relay-check.log`、`core-relay-ios-build.log` 通过。不是完整 App 运行或 UI 验收。 |
| Core 全量 | `core-relay-regression.log`：106 项，103 通过、2 个未加载 fixture 的网络专项跳过、1 个既有 Keychain 测试失败（`-67674`）；新增网络专项另行运行通过。 |
| E2E 全量 | `e2e-relay-regression.log`：进入 291 项 GPUI 测试后进程 `SIGABRT`，提示 `Rust cannot catch foreign exceptions`；未通过，尚未定位该启动异常。独立网络专项不受此进程影响。 |
| 护栏 | diff / no-allow 通过；`relay-line-check.log` 仍因既有 `settings/mobile.rs` 546 行失败，本轮未修改该文件。 |

重跑网络专项（本机隔离数据，不调用模型、不部署）：

```sh
NORMA_NATIVE_RELAY_CHECK=1 cargo test -p e2e --test mobile_collaboration_relay -- --nocapture
```

## 线程持久接收与回执恢复

手机消息与待发记录原子落盘，首次准备发送时冻结目标 Mac 和封套；Mac 在同一事务保存线程归属、消息与唯一接收事件后签收。重连先核对原操作回执，重复同内容返回原回执，改正文或归属拒绝。当前能力为 `thread_delivery_v1`，只同步讨论消息，不启动 Agent，也不声明完整 `collaboration_v1`。

证据仍在 `target/norma-ios-evidence/single-mac-20260929/`：

| 检查 | 结果与限制 |
|---|---|
| 失败记录 | `thread-store-red.log`、`thread-outbox-red.log`、`thread-wire-red.log`、`thread-native-red.log`：对应新增接口／能力实现前失败。 |
| Mac 存储 | `thread-store.log`：2 项通过；重开数据库后回执稳定、冲突与归属隔离、事件写入失败时整个接收事务回滚。 |
| 手机待发记录 | `thread-outbox.log`：2 项通过；操作身份稳定、目标冻结、错误回执拒绝、重复回执幂等、写入失败保留草稿。 |
| Node → Mac 网络 | `thread-wire.log`：1 项通过；持久接收、查询、内容冲突、事件去重与线程隔离。 |
| Swift → Node → Mac 网络 | `thread-native.log`：1 个 Rust 场景内调用 1 个 Swift 场景，通过；双端指纹一致、回执未落手机库后重建 store／连接恢复原回执，仅一个接收事件。 |

上述为真实本机网络与共享核心验收，尚非 GPUI App、iPhone 页面、真实 Mac App 重启或 Coding Agent 执行证据。本切片尚未重新完成所有受影响模块全量回归；前节 Keychain、GPUI 全量、模拟器和 Computer Use 限制仍单列。

## App 投递接入与当时验收阻断（历史记录）

已接入 RootView 前台投递循环：恢复云准入、按消息校验目标、每台 Mac 每轮至多两条、间隔至少 5 秒；后台取消网络等待。Mac 开启手机服务时启动中继；Debug 仅使用显式本地端口，不回退生产。线程页面按持久回执显示「已送达 Mac」，失败原因按消息呈现，回执刷新不主动滚动历史。上述为代码接入，仍缺真实 App 运行证据。

新增真实网络负例发现两条未授权消息会占据每批前两项，已先测失败再改为临时公平轮转位置；持久操作身份不变。独立只读 Agent `thread_delivery_ui_review` 首轮指出批次清掉未解决错误、云初始化失败静默停止、VoiceOver 漏读失败原因三项，修复后复核未发现新增具体代码缺陷；没有真实 App 操作／截图，**完整视觉和交互验收未完成**。

| 检查 | 结果与边界 |
|---|---|
| 批量与公平轮转 | `thread-batch-red.log` 接口实现前失败；`thread-batch.log` 通过；`thread-fairness-red.log` 为有权限消息被失败操作阻塞的断言失败；修复后 `thread-app-network.log` 通过。 |
| Mac 开发端口 | `relay-config-red.log` 实现前失败，`relay-config.log` 1 项通过；拒绝空、零、溢出端口及外部 URL。 |
| Rust 回归 | `thread-domain-regression.log`：storage / norma 合计 234 通过、0 失败、1 忽略；`app-delivery-check.log` 编译通过。 |
| Swift Core 全量 | `thread-core-regression.log`：108 项，105 通过、2 项网络 fixture 未加载而跳过、1 项旧 Keychain `-67674` 失败；真实网络专项另跑通过。 |
| iOS 编译与测试类型检查 | `core-delivery-ios-build.log`、`app-delivery-module.log`、`device-delivery-typecheck.log`、`ui-delivery-typecheck.log` 通过；不是原生测试执行。`cloud-delivery-recovery-red.log` 留下恢复入口实现前的编译失败。 |
| 原生测试 | `app-delivery-native.log` 退出 74，依赖缓存写入受限，DeviceTests 和 UITests 未执行。 |
| 护栏 | diff / no-allow / storage boundaries 通过；`app-delivery-lines.log` 仍因本轮未修改的 `settings/mobile.rs` 546 行失败。 |
| GitHub | `issue-59-read.log` 再次 HTTP 401，未更新远端。 |

### 用户要求优先修复运行环境（2026-09-29 13:42）

- `simulator-access.log`：独立 `simctl list devices booted --json` 已复现 CoreSimulatorService 连接失效、Connection refused，且 `~/Library/Logs/CoreSimulator` 日志写入被拒。
- `cache-isolation-probe.log`：把 DerivedData、克隆依赖、packageCachePath 和 XDG_CACHE_HOME 都切到 `target/` 后，`xcodebuild -resolvePackageDependencies` 仍退出 74，继续试写 `~/Library/Caches/org.swift.swiftpm/manifests/ManifestLoading/ios.dia` 并收到 Operation not permitted。目录属主均为当前用户 UID 501；没有依据修改 chmod/chown 或删除个人缓存。
- 当时执行权限是 workspace-write、approval never，不能自行提升。Computer Use 再次选择 Simulator 明确返回 `Computer Use was not approved to use Simulator`；没有绕过。
- 需要用户允许本任务运行 Xcode/Simulator 所需的执行／文件访问权限，并在 Computer Use 中允许 Simulator。授权后先重新执行缓存解析和 simctl 探针，再跑原生专项和真实界面验收。服务本身是否另有异常，待权限恢复后继续核实。

后续仍须接入现有 Agent 执行器，完成真实派工、执行事件／成果回传及完整 App 验收。#57/#59 保持进行中；Norma 语音通话最后做。

### 用户授权后的复查（2026-09-29 13:51）

- **Computer Use 已恢复**：成功访问 Simulator，读到 iPhone 16e / iOS 26.3 的主屏幕和辅助功能树；此前 Simulator 操作未授权的阻断已解除。这不是最新 Norma App 的运行验收。
- **命令执行仍受限**：本会话仍声明 `workspace-write` / `approval never`。`simulator-access-full-permission.log` 仍记录 CoreSimulator 日志写入 `Operation not permitted`、服务连接失效。
- **Xcode 仍未通过依赖解析**：沿用项目内 DerivedData、依赖和缓存路径复测，`cache-full-permission-recheck.log` 退出 74，仍无法写入 `~/Library/Caches/org.swift.swiftpm/manifests/ManifestLoading/ios.dia`。原生测试未启动。
- 需要当前任务的完整执行权限实际生效后再重试；未改系统目录权限、未删除缓存、未通过其他执行入口绕过沙箱。

### 完整权限生效：原生运行阻断已解除（2026-09-29）

本轮运行目标为 **iPhone 16e / iOS 26.3 Simulator**，并非物理 iPhone。沿用项目内构建和依赖缓存，无需修改系统目录权限或清空个人缓存。

| 检查 | 实际结果 |
|---|---|
| Simulator 与 Xcode | `simctl` 正常取得已启动设备；`cache-unrestricted.log` 依赖解析成功；原生 App 已构建、安装、启动。 |
| 投递恢复与线程 | `native-unrestricted.log/.xcresult`：WorkspaceThreadDeliveryTests 2 项、WorkThreadFlowTests 1 项通过。 |
| 其他已实现页面 | `native-regression.log/.xcresult`：文字想法离线编辑搜索重启、Workspace 创建归档重开、无 Mac 的 Norma 入口共 3 项 UITests 通过；没有真实模型调用。 |
| 全部原生单测 | 首跑 156 项中 137 通过、18 跳过、1 失败：旧角色资源测试仍断言纯黑背景。依现行浅色设计修正该过期断言后，`native-fixed.log/.xcresult` 为 **138 通过、18 跳过、0 失败**；原有 Keychain 隔离测试也通过。 |
| UI 修复的先失败后通过 | 独立复核指出未配置项目时误写“连接恢复后会继续发送”。`native-ui-red.log` 留下新增正负断言失败；修复后 `native-fixed.log` 的 WorkThreadFlowTests 1 项通过。 |
| 差异修正 | 待发提示覆盖配置/授权/连接条件；简报主编辑按钮与次操作改为可辨的蓝色，静态标题深色；来源卡补薄荷图标底和箭头，精简顶部副标题避免默认字号断行。 |

18 项跳过包括物理设备文件保护、需显式 TLS/中继/语音/通知 fixture 的专项及仅 macOS 可启动 Node 的互操作测试；跳过不算通过。旧 macOS Core Keychain 失败与当前 iOS 宿主通过为不同环境，不相互代替。

Computer Use 已在隔离 UI 测试数据上实际完成：想法库→Workspace→线程、输入 ASCII、显示软件键盘、发送后显示待发、简报编辑保存并显示版本 2、来源 Sheet 打开原想法及取消返回、字号增加一档后恢复。截图位于同一证据目录的 `computer-use-*.png`；原生全屏 1170×2532 对照图在 `native-fixed-attachments/`。

验收限制：Computer Use 的 Simulator 辅助功能树仅返回宿主控件，尚未完成 VoiceOver 朗读；其中文输入/粘贴未成功，ASCII 成功，XCTest 中文输入流程另行通过。Sheet 打开/关闭已观察，拖动展开、完整字号矩阵、减少透明度/动态效果仍未充分验证。当前截图为讨论/待发送状态，无 App 已送达、真实 Agent 执行或成果证据。

GitHub CLI 已重新读取 #54–#59；真实供应商 Key 在当前进程及 `services/norma-cloud` 的本地配置中未找到，仅存在部署工具的本地凭据，不读取或输出凭据值。云端部署未进行。继续推进实际派工、配置关联专项与剩余 UI 验收，Goal 保持 active。

#### 权限恢复后的最终复核

- 独立只读 `thread_delivery_ui_review` 已确认待发文案、简报按钮层级、来源入口与副标题三项修复 PASS。复核又发现来源 Sheet 正文被底层蓝色消息透色污染；已给阅读区增加浅色实底，系统 Sheet 导航材质保留。
- 最后修复后，`native-source-fixed.log/.xcresult` 的 WorkThreadFlowTests 为 1 项通过、0 失败。Computer Use 重新从 Workspace 进入线程、打开来源 Sheet，保存 `computer-use-source-sheet-fixed.png`；原生全屏对照位于 `native-source-fixed-attachments/`。独立 Agent 对该项复核 PASS，未将局部结论扩大为整个 P05/W05 通过。
- 已使用 GitHub CLI 实际发布 [#57 进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5884827565) 和 [#59 进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5884828353)，两票保持 OPEN；评论说明本地未提交状态及剩余真实派工、成果回传和跨网络验收范围。
- Xcode／Simulator 权限问题已解除，无需用户再次介入权限设置。全 Goal 仍未完成；真机项目后置，Norma 语音通话最后。

## 线程派工协议与既有执行器接入

`thread_task` / `thread_task_receipt` 已接入正式本地中继与 Mac `MobileServer`。派工必须引用已送达的来源消息，并校验项目范围、当前控制权限和授权修订。线程绑定与既有 `mobile_commands` 预留在同一事务保存；重新连接换 route 只取原回执，不创建第二次执行。完整执行事件尚未交付，因此仅声明 `thread_tasks_v1`，不声明 `collaboration_v1`。

Mac 前台按稳定 Agent 定义 ID 解析当前目标，复用已有 `AssignAgentTask`、FIFO、lease 和成果追踪。任务与共享上下文一起持久化。先失败测试发现原适配器只返回运行期数字 ID，现已补上任务库中的持久 `task_ref`，避免重启后误关联；Swift 端同时核对来源、线程、正文、指纹和完整任务引用。

| 检查 | 结果与边界 |
|---|---|
| 存储 TDD | `thread-command-store-red.log` 实现前失败；存储公开 API 验证原子绑定、回滚、跨线程隔离、换路由/进程不重派、旧命令不可被接管。 |
| 域层回归 | `thread-task-domain-regression.log`：norma / storage 合计 **236 通过、0 失败、1 忽略**。 |
| E2E 全量 | `thread-task-e2e-regression.log`：**277 通过、0 失败、75 忽略**；使用 `--test-threads=1`。包含 GPUI 前台、真实本机网络与旧配对/命令兼容测试；忽略项包含真实 App/供应商专项，未计入通过。此前 GPUI 进程异常在本次串行运行中未复现。 |
| Swift Core 全量 | `thread-task-core-regression.log`：109 项中 **107 通过、2 跳过、0 失败**，macOS 原生宿主。此前 Keychain `-67674` 本次未复现；2 个需显式 fixture 的网络专项另列。 |
| 正式中继网络 | `thread-task-network-red.log` 新接口前失败；`thread-task-network.log` 通过，真实 Node 服务→Mac 服务→原有有界执行通道，前台结果为显式 fixture，不是 Coding Agent。 |
| Swift 协议 | `thread-task-swift-red.log` 实现前失败；`thread-task-swift-final.log` 1 项通过：独立 Node 固定指纹向量、错误来源/正文/线程/任务引用和虚假完成拒绝。 |
| Swift→中继→Mac | `thread-task-native-final.log` 通过：Swift 核心读取并重试同一派工，更新 route 不改变原回执，新增持久 task_ref 验证通过。包含只读权限不能派工、来源缺失/参数超限拒绝及撤权取消；前台结果为显式 fixture。 |
| 真实前台适配器 | `thread-task-reference-red.log` 缺持久引用断言失败；`thread-task-foreground-final.log` 1 项通过，真实 GPUI 适配器与任务库，Provider 为 echo PTY。不是独立 `.app` 或真实 Coding Agent 运行证据。 |
| App 编译 | `thread-task-ios-build.log` iOS 构建成功，`thread-task-app-check.log` Mac 检查通过，`thread-task-release-build.log` Mac Release 构建通过；本切片未修改页面，不据此声称派工 UI 或交互已验收。 |

后续仍需接入手机线程内 @Agent/Norma 的操作生成、持久待发任务、任务与成果事件回原线程；再用真实 Mac App / Coding Agent 验收。当前接口与测试没有替代这些交付。全仓行数护栏仍有本轮未修改的 `settings/mobile.rs` 546 行历史未提交项；新增 Rust 文件均在 500 行以内，存储边界和 no-allow 护栏通过。

本切片已通过 GitHub CLI 发布 [#59 实际进展回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5885347294)，维持 OPEN，并明确说明测试夹具、尚未提交的工作树和未部署云端的范围。

## 手机派工待发记录与 App 前台恢复

复用 CollaborationWorkspaceStore：直接呼叫的消息、来源待发操作、派工操作及草稿清理在同一事务保存；Norma 后续派工可引用已有消息。任务身份、Mac/项目/Agent 定义、正文和上下文版本固定，重启或配置变化不能改派。来源签收后才能新派工；发送前绑定配对设备，先查原回执，再读最新路由/授权修订。结果未知不重派，迟到的接单回执不覆盖终态。

RootView 的现有前台循环已纳入任务恢复，消息送达与派工失败分别保存；有任务时每轮最多一条消息和一个任务，至少间隔 5 秒，最多 108 帧/分钟。排队任务轮转，避免永久阻塞后续指令。此切片没有修改页面构图，线程内 @Agent/Norma 操作生成和任务卡还未接入。

| 检查 | 结果与边界 |
|---|---|
| TDD | `task-outbox-red.log`、`task-outbox-receipts-red.log`、`task-outbox-network-red.log`、`task-outbox-batch-red.log`、`task-outbox-app-model-red.log` 保存新增公开接口/前台状态未实现时的失败。 |
| 数据损坏保护 | `task-outbox-damaged-red.log` 精确复现读取损坏任务时隐式生成来源操作、改写原数据库；改为只读检查后通过。 |
| Swift Core | `task-outbox-core-regression.log`：113 项中 **111 通过、2 跳过、0 失败**。覆盖原子回滚、草稿保留、重启、身份/目标冲突、终态不回退及损坏数据字节不变。 |
| 原生 iOS | `task-outbox-native-final.log/.xcresult`：**14 项通过、0 失败**，iPhone 16e / iOS 26.3 Simulator；包括 Task/Outbox、消息/草稿与 App 前台恢复。重新生成 Xcode 工程，确保新增 Task 测试确实进入原生测试 Target。 |
| 真实本机网络 | `task-outbox-network-fixed.log`：Swift→正式 Node 中继→真实 MobileServer 通过；3 个明确任务仅产生 3 次执行入口，丢回执/重建 Store/轮询不新增执行，移除成员后可核对已接单回执，已知接单而远端为空时不重派。 |
| 测试范围 | Mac 的项目快照/前台接单结果仍为显式 fixture，本次未运行真实 Coding Agent、任务产物或原生 @Agent UI；不等同于完整任务验收。 |
| 工程与护栏 | diff / no-allow / storage boundaries 通过。Mac Release 上个切片构建已结束并通过；本切片未修改 Mac 产品代码。原有 `settings/mobile.rs` 546 行项仍待整理。 |

初次原生命令误用相对 packageCachePath，Xcode 报 invalid absolute path（`task-outbox-app-red.log`）；改为已有绝对缓存路径即可正常运行，**没有新的权限阻断**。网络测试也暴露旧撤权探针在 Swift 编译期间排队超过 30 秒的问题；将该探针移到编译/原生专项结束后再排队，保留生产超时不变。

全量 E2E 首跑 `task-outbox-e2e-regression.log` 在已有 Agent session restore 用例的 2 秒启动等待超时，随后两个用例因测试锁中毒失败：228 通过、3 失败、60 忽略，后续集成二进制未执行。聚焦重跑 `task-outbox-restore-recheck.log` 4 项全通过；不将此局部结果冒充全量通过。

尚需：原生线程内 @Agent/Norma 的意图与上下文生成、任务回执展示、执行事件/成果回原线程、真实 App 与 Coding Agent，以及跨网络验收。当前代码尚未提交上传，未部署云端；Goal/#59 继续进行。

完整串行 E2E 重跑 `task-outbox-e2e-final.log` 最终为 **277 通过、0 失败、75 忽略**；首次启动超时未在该次复现。未放宽旧用例的 2 秒阈值，真实 App/供应商忽略项仍不计通过。

已使用 GitHub CLI 读取最新票据后发布 [#59 待发恢复进展](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5885752165)，维持 OPEN；评论明确测试替身、原生专项范围、未提交及未部署状态。

## @Agent、冻结上下文与任务 UI（2026-09-29 接续）

- 输入区提供 @ 入口；仅列当前 Workspace 参与成员，按项目/Mac 分组。草稿持久保存稳定目标身份，展示成员/项目/Mac/profile，选择器使用勾选与辅助功能已选状态；删除提及时清除隐藏目标。唯一名称可手输，同名不明确或未知成员保留草稿并说明原因。
- 消息与多个明确目标任务同事务保存；普通讨论不派工，重复来源消息不重复生成任务。冻结当前线程目标/来源想法、同 Workspace 共同事实/决定及最多 12 条当前线程历史；不包含其他线程原文/目标、其他 Workspace、整库想法或 Norma 私聊。事实超限或历史范围损坏时回滚并保留草稿。
- 具名任务卡和原生详情直接读取持久任务；区分待发送、Mac 已接收、Agent 已接收、派工失败、待核对。可展开本次共享内容，读取发送时的快照。@Norma 当前明确失败保留草稿，真实模型编排仍待接入。
- 独立只读审查发现的两项 P2 已修复：投递刷新不清除发送错误；Mac 拒绝原因从持久回执读取，重启仍可见。错误显示在输入区域，编辑草稿后清除。同名选择补 profile；最大辅助字号下固定触控框中的图标改为固定大小，正文继续使用 Dynamic Type。

所有下列日志/xcresult/图片位于 `target/norma-ios-evidence/single-mac-20260929/`。

| 验证 | 证据与结果 |
|---|---|
| 上下文 TDD | `thread-mention-scope-red.log` 先复现损坏历史被纳入；修复后拒绝并保留草稿。`thread-context-readable-red.log` → `thread-context-readable.log` 验证可解码冻结内容。 |
| 发送错误 TDD | `thread-feedback-red.log` 明确复现刷新后错误消失；`thread-feedback-green.log` 中该 UI 用例通过（跨 5 秒刷新观察 6 秒）。该轮整体因另一个测试误把兼容空值 nil 当 [] 而失败，不把整个 xcresult 写成通过。 |
| 拒绝原因 TDD / Core | `thread-rejection-red.log` 未实现接口时失败；`thread-feedback-core.log`：ThreadMention 3 + ThreadTaskOutbox 5，共 8 项通过，包括持久拒绝、未知错误回退和不自动重派。 |
| 原生 Store/Model | `thread-feedback-native.log/.xcresult`：11 项功能测试 + 1 项过期 fixture 清理，12/12 通过；同名跨项目选择、重启及删提及不派工通过。 |
| 真实 TLS 目录→UI | `mention-feedback-ui.log/.xcresult`：标准与最大辅助字号各 1 项完整流程，共 2/2 通过。配置真实 fixture 目录→选择→草稿重启→仅一张待发任务→详情→共享目标→无效提及保留。没有云模型/真实 Coding Agent。 |
| 截图 | `mention-feedback-attachments/manifest.json`：标准选择/勾选、待发卡、共享目标详情及最大字号页面；大字号 Sheet 的截图停在顶部说明，已选状态由 AX 断言确认，不能把该图当作勾选可见证据。`thread-feedback-attachments/` 保存持续错误提示。 |
| Computer Use | 本轮开头真实点击展开共享内容，保存 `mention-computer-use-context.png`，此前待发卡为 `mention-computer-use-pending.png`。最终图标修正后尝试复验，窗口报 `noWindowsAvailable (-10005)`；重新选择、Raise 和重置连接未恢复。此错误不是 Xcode 缓存拒绝，最终改动尚未完成 CU 视觉复验。 |
| 最终构建 | `thread-feedback-final-build.log`：BUILD SUCCEEDED，包含 profile 与固定图标大小。 |
| 测试资源收口 | 旧 `mention-ui-fixture.log` 因交互超过 15 分钟退出 101，不能记整体通过；其残留配对已由上述正式清理用例回收。本轮新 `mention-feedback-fixture.log` 1/1 通过，`mention-feedback-pairing.log` 配对通过，`mention-feedback-cleanup.log/.xcresult` 正式精确忘记测试电脑 1/1 通过，随后正常结束 fixture。未清空用户数据。 |

独立复核：两项 P2 已 PASS；profile 反馈已补。最终图标修正、拒绝回执页面、软键盘、减少透明度/动态效果及完整 VoiceOver 尚缺相应 Computer Use 证据；只能说明本切片代码与列明的自动化流程通过，不能将整套 P05/W06 或 Goal 标完成。

继续推进：Norma 真实模型编排、完整 task_ref 执行状态/成果返回原线程、真实 Mac App/Coding Agent、重连/后台/跨网络验收。#57/#59 维持 OPEN；真实模型 Key 尚缺，真机项目由用户后置，语音通话最后。本轮未提交、未推送、未部署。

### 最终补验与远端同步

- `thread-feedback-accessibility.log/.xcresult`：最终图标修改后的最大辅助字号发送错误流程 1/1 通过；`thread-feedback-accessibility-attachments/` 保存错误跨刷新保留、修改后消失及软键盘下输入/固定图标截图。此为 XCTest 证据，不替代 Computer Use。
- `/root/thread_delivery_ui_review` 最终结论：代码修复 PASS，标准字号所给选择/任务/共享内容截图 PASS；最大辅助字号功能 PASS，完整视觉验收 NEEDS WORK。此前两项 P2 和 profile 缺口已关闭；最终 CU 操作、大字号可见勾选、拒绝态与 VoiceOver 仍待补证。
- GitHub CLI 已重新读取并实际发布 [#57 更新](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5886670404) 和 [#59 更新](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5886671364)，两票保持 OPEN。评论明确本地未提交、未部署及真实执行/完整视觉未完成范围。
- `git diff --check` 通过；未改系统目录权限、未删除个人缓存，未用其他点击工具绕过 Computer Use 故障。

### 完整权限再次复验（2026-09-29 16:46，本机时间）

- Xcode 26.2 / iPhone 16e（iOS 26.3）重新执行原生构建与测试，命令退出 0，`TEST SUCCEEDED`。WorkspaceDraftTests 3/3、WorkThreadFlowTests.testSendErrorWithAccessibilityText 1/1 通过，未再次出现缓存拒绝。
- 日志与结果包：`target/norma-ios-evidence/single-mac-20260929/permissions-recheck-20260929.log`、同名 `.xcresult`。继续使用既有工作区绝对缓存路径，无需修改系统目录权限或删除个人缓存。
- Computer Use 重新选择 Simulator 成功，实际点击主屏 Norma 图标、更多模块入口、工作空间入口，并逐步核对页面截图；本轮未出现 `noWindowsAvailable`。App 内辅助功能树仍仅提供模拟器宿主控件，本次点击依据实时截图，不能据此宣称 VoiceOver 已验收。
- `permissions-recheck-drawer.png` 为 Computer Use 打开抽屉后由 `simctl io screenshot` 保存的原生截图，位于上述证据目录。该证据只确认运行、窗口与基础操作恢复；不替代任务详情、最大字号选择态、拒绝态及完整辅助功能复核。
- 当前无需用户再次介入 Xcode 或模拟器权限；继续推进既有 Goal。未修改产品实现，未部署云端，未关闭仍有功能遗留的 Issue。

## Mac 持久执行结果读取（2026-09-29 接续）

完成 Mac 查询侧：既有 `thread_task_receipt` 增加可选 `execution`，声明 `thread_task_results_v1`。原设备/线程/来源及当前项目读授权校验后，以完整 `task_ref` 读取原项目任务库；节点、Agent、任务正文均须匹配。复用当前构建的生产/测试数据库路径；公开持久引用读取改为只读，缺失库不创建空库。派工回执 succeeded 与实际任务 done 仍分开。

执行事件与游标持久追加，重复快照重用原事件；终态不回退、最终回复不改写。回复预览最多 16 KiB UTF-8，截取保留字符边界并标明 truncated；原文留在 Mac。成果按原设备/操作关联 Provider 完整回合，查询冻结元数据，不读取文件正文、不取节点最新成果。新增负例真实复现了同节点其他回合成果误绑定，已在存储事务内再次校验原操作来源并修复。

下列证据均在 `target/norma-ios-evidence/single-mac-20260929/`：

| 验证 | 结果与证据 |
|---|---|
| 事件存储 TDD | `thread-execution-red.log` 为公开接口缺失的编译失败；`thread-execution-green.log` 通过。事件重复/重启稳定、数字 ID 同号不同 runtime 拒绝、终态不回退/不换回复。 |
| 成果元数据 TDD | `thread-delivery-metadata-red.log` → `thread-delivery-metadata-green.log`；原操作读 turn-a，不能读同节点新 turn-b，错设备/项目/操作均为空，无文件字节。 |
| 成果归属 TDD | `thread-execution-delivery-red.log` 实际断言失败“同节点另一回合成果不能挂到原任务”；修复后 `thread-execution-storage-green.log` 7 项通过，包含原成果晚到和旧快照不清除成果。 |
| 只读恢复 TDD | `thread-execution-readonly-red.log` 实际复现查询创建空项目库；修复后纳入存储全量。 |
| 存储全量 | `thread-execution-storage-recheck.log`：116 通过、0 失败。前一次 `thread-execution-storage-final.log` 遇到同工作区其他正在编辑的 Agent 删除测试 2 项失败；观察其修复落地后重跑通过，本任务未覆盖这些文件。 |
| Mac/Node 网络 | `thread-execution-network-red-retry.log` 实际因没有执行结果失败；`thread-execution-network-green.log` 通过。最初 `thread-execution-network-red.log` 是测试未创建 Agent 定义引发外键错误，不能把它算成功复现功能缺失。 |
| E2E 全量 | `thread-execution-e2e-recheck.log`：277 通过、75 跳过、0 失败；包含完整引用防串任务、已完成结果跨线程/设备隔离与撤权。前一次编译碰到其他正在编辑的 legacy_agent_migration 函数尚未加入，代码补齐后重跑；未改写该文件。 |
| Swift 兼容网络 | `thread-execution-swift-network.log`：Rust 场景 1/1 + Swift 原生共享核心场景 1/1 通过；现有 Swift 解码器忽略新增字段，旧派工/恢复仍工作。不是 iPhone 结果展示验收。 |
| 静态检查 | `thread-execution-app-check.log` 的 cargo check 通过；git diff --check、存储边界与性能护栏通过。全仓行数仍由本轮未改的 settings/mobile.rs 546 行阻断；本轮新增/修改 Rust 文件均未超过 500 行。 |
| Mac Release 构建 | `thread-execution-release.log`：`Finished release profile [optimized]`，退出 0，耗时 4 分 10 秒。 |

测试任务/回复是显式 fixture，通过正式 Node 中继和真实 MobileServer 传输；没有真实云模型或 Coding Agent 执行证据。手机执行事件校验、事务去重、轮询、原线程回复和成果页面仍待接入，不把本切片称为完整回传闭环。本轮未修改页面；UI 子 Agent 尝试重新选择 Simulator 被其自身工具拒绝授权，未产生新的视觉 PASS；主任务前一节的实际 Computer Use 成功证据保留。未提交、未推送、未部署。

补验观察：根任务在最大辅助字号下看到想法库排序区文字竖向换行，该页面的完整大字号验收仍需修复；没有据局部线程测试宣称全 App 可访问性通过。已将测试 App 恢复为标准字号，并通过 Computer Use 核对页面恢复；后续 UI 验收继续使用隔离测试库。

## 手机执行结果同步与原线程回复（2026-09-29 接续）

本节覆盖上文“手机仍忽略 execution”这一历史状态。手机现已核验事件身份/序号/大小、完整任务引用及成果归属；旧事件忽略，同序号冲突拒绝，终态不回退或换回复。查询与派工共享现有轮转预算，已派工任务远端缺失时保留记录并报错，不重派。

原任务、唯一 Agent 回复及成果元数据同事务落盘；失败整笔回滚。Agent 回复保留来源操作，可成为共同决定来源与后续线程上下文，不进入用户 outbox。读取任务会核验回复实际存在及作者/线程/Workspace/Mac/正文，损坏数据不假装同步成功。成果目前只展示冻结记录和文件数，文件正文/对比入口尚未接入。

| 验证 | 结果与证据（均在 `target/norma-ios-evidence/single-mac-20260929/`） |
|---|---|
| 回复存储红/绿 | `thread-execution-phone-red-valid.log` 真实失败：缺 Agent 回复；修复后 green 与完整回归通过。最初 red/red-retry 是测试构造错误，不计功能红测。 |
| 回复完整性红/绿 | `thread-execution-phone-integrity-red.log` 真实失败：删除回复后读取任务未报损坏；`thread-execution-phone-guards-green.log` 聚焦 17 项通过。 |
| Core 全量 | `thread-execution-swift-regression.log`：执行 121 项，118 通过、3 明确跳过、0 失败。 |
| 正式本机网络 | `thread-execution-phone-network.log`：Rust 1/1、Swift 1/1 通过；回复与成果按完整引用读取，已派工记录缺失不重派。任务/回复仍为显式 fixture，非真实 Coding Agent。 |
| 原生 UI 红/绿 | `thread-execution-ui-red.log` 复现作者语义缺失；`thread-execution-ui-ax-red.log` 复现截断提示 AX value 为空；修复后 `thread-execution-ui-reviewed.log/.xcresult` 为 8 通过、1 fixture 跳过、0 失败。 |
| UI 覆盖 | 标准/最大辅助字号：具名回复、完成态、成果数、截断 AX value、返回原卡位置误差 ≤4pt、重启回复不重复；App 前台恢复继续查询已派工任务。 |
| Computer Use | 根任务真实操作标准字号的空间→线程→回复→任务详情→返回。`execution-reply-standard.png`、`execution-detail-standard.png` 为操作后的原生截图。完整 VoiceOver 实操尚未完成。 |
| 最大字号边界 | 原生 UI 附件在 `execution-reviewed-attachments/`，manifest 明确两种字号归属。CUA 停在工作空间列表，标题/过滤区严重换行，滚动未得到可达证据；代码为整页 ScrollView，不能直接归因为固定头部。截图 `workspace-maximum-text-observation.png`。随后恢复标准字号并实际核对。 |

独立只读复核已确认两项初审修复：截断提示进入 AX value，成果等待文案不再推断网络断线。最终截图复核另发现任务状态浅绿文字仅 2.53:1，已改用 `positiveText` 深绿（4.97:1）；卡片与详情共用同一状态前景。对比度证据 `thread-status-contrast.log`，修正后的原生运行及截图复核接续记录于下文。Apple 设计原则落实为实底阅读面、文本与图标共同说明状态、清晰对比及返回保留阅读位置。

Xcode/Simulator 与根任务 Computer Use 权限已恢复，不需要用户继续调整权限。真实模型 Key 仍缺，真实 App/Coding Agent、成果正文、后台/跨网络与完整 UI 验收尚未完成；Goal/#57/#59 保持进行中。只本地测试，未部署；真机与 Norma 语音通话继续后置。

状态颜色修复后，`thread-status-contrast-native.log/.xcresult` 标准字号 UI 流程 1/1 通过、退出 0。根任务再次用 Computer Use 完成空间→线程→详情→返回；新截图为 `execution-reply-contrast-fixed.png` 与 `execution-detail-contrast-fixed.png`（1170×2532）。最后留在标准字号的隔离测试线程，未修改用户工作空间数据。

独立 Agent `/root/thread_delivery_ui_review` 最终结论：**本切片标准字号视觉 PASS，三项 P2 均关闭**。新截图的具名回复、完成卡、截断提示与成果说明清楚；不覆盖最大字号完整 Computer Use、实际 VoiceOver、其他状态或整个 Goal。

已用 GitHub CLI 读取最新评论后实际发布进展：[57 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5888084539)、[59 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5888085735)。两票维持 OPEN；评论明确本机 fixture 和真实 Coding Agent 的区别、尚未完成的成果正文与全流程验收，未把本地实现说成已提交或部署。

## 冻结成果正文接口与权限复验（2026-09-29 18:30）

已实现 `thread_task_review`：按原 Workspace、线程、操作、完整任务引用和冻结成果指纹读取原回合。每片重新验证当前设备与项目授权，不依赖原项目窗口仍然打开；不会读取当前工作目录或任意路径。复用现有成果格式与存储，单片原始字节最多 160 KiB、序列化总长最多 32 MiB。手机完整重组、核验成果身份/总长/文件数后才交付给调用方；失败清除半份内容，取消关闭连接。

所有手机中继连接共用每 Mac 滚动帧预算：60 秒最多 108 帧，给服务 120 帧上限保留 12 帧控制余量。握手前预留额度，避免排队时间消耗握手超时；成果读取与后台投递不能分别占满服务额度。没有新增数据库或缓存服务。

| 验证 | 结果与证据（均在 `target/norma-ios-evidence/single-mac-20260929/`） |
|---|---|
| 协议红/绿 | `thread-review-network-red.log` 真实断言失败：没有冻结文件接口；`thread-review-network-green.log` Rust/Node 1/1 通过。 |
| Swift 校验与帧预算 | `thread-review-swift-red.log`、`thread-review-budget-red.log` 为新增类型缺失的编译红测；`thread-review-swift-green-final.log` 3/3 通过。首次 green 因使用不存在的错误枚举失败，已修正，未计为通过。 |
| 正式本机网络 | `thread-review-swift-network.log` Swift 1/1 + Rust 1/1 通过，完整传输 140,000 字节 Unicode 文件；错误指纹/线程/任务/偏移及另一配对设备不能读到正文。执行结果仍为显式 fixture。 |
| Core 全量 | `thread-review-core-regression.log`：执行 124 项，121 通过、3 跳过、0 失败。 |
| Mac headless E2E | `thread-review-e2e-regression.log`：232 通过、60 忽略、0 失败；本次为 `--lib`，不冒充全部集成测试或真实 Mac App 验收。 |
| 原生 iOS | `thread-review-native.log/.xcresult`：Xcode 26.2、iPhone 16e / iOS 26.3，ThreadReviewTests 3/3 通过、退出 0。 |
| 构建与静态检查 | `thread-review-app-check.log`：cargo check 通过；两条既有 settings/agents 未使用函数警告。本轮 git diff --check、存储边界与 no-allow 通过。 |
| 权限实际复验 | 新构建成功安装启动；根任务 Computer Use 读取原生页面并从想法库点击进入工作空间。Xcode 缓存、Simulator 与 UI 操作权限均可用，无需用户继续调整。 |

本节完成的是正文读取接口，**任务详情仍没有文件对比入口**；原生页面接入、完整成果验收/修改反馈、真实 Norma 编排/Coding Agent、后台/跨网络与完整 UI 验收继续待办。未修改页面，不新增视觉 PASS；Goal 与 #57/#59 保持进行中。只本地测试，未提交、未推送、未部署。

本节进度已通过 GitHub CLI 实际发布：[57 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5888472095)、[59 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5888473059)，两票维持 OPEN。权限复验截图：`target/norma-ios-evidence/single-mac-20260929/permissions-recheck-workspaces.png`。

## 原生成果阅读与最终本地回归（2026-09-29 19:16）

本节替代上文“任务详情没有文件入口”的历史状态。新增 `WorkThreadReviewView`，原任务详情可进入冻结成果列表，复用已有文件差异、系统 TextKit 与图片预览。读取完整且身份核验通过后才展示文件；支持错误、重试、取消和返回，冻结成果只提供差异／任务起点／本次交付。浅色实底承载正文，导航和菜单使用系统组件；不新增成果存储或执行器。

### 失败、修复与验证

证据根目录：`target/norma-ios-evidence/single-mac-20260929/`。

| 范围 | 证据与结果 |
|---|---|
| 入口与浅色 TDD | `thread-review-ui-red.log` 复现缺少入口、正文黑底；`thread-review-ui-green.log` 通过。 |
| 错误归因 TDD | `thread-review-cloud-error-red.log` 复现配置初始化错误被改成未授权；保留初始化原因后，`thread-review-final-regression.log` 对应 UI 用例通过。 |
| 长文卡顿 TDD | 首轮 `thread-review-files-native.log` 卡在 20,000 行正文的辅助功能快照；`thread-review-long-text.sample.txt` 定位 UIKit 逐行 AX 子元素遍历。`thread-review-text-ax-red-retry.log` 断言失败；修为单一完整可读元素，保留全文、原生选择与滚动。 |
| 实际文件 UI | `thread-review-files-native-fixed.log/.xcresult`：标准／最大辅助字号流程 2/2、ReviewViewTests 3/3，0 失败。验证 diff、原文、140,000 字节完整交付文本、切后台恢复、逐层返回与重开；无暂存区、无重复任务。 |
| 最终错误与恢复回归 | `thread-review-final-regression.log/.xcresult`：ThreadExecutionFlowTests 3/3、WorkspaceThreadDeliveryTests 3/3，0 失败。覆盖授权错误／重试、初始化失败、原位置返回、重启不重复、消息与任务错误隔离。 |
| Computer Use | 根任务操作原生 App 完成工作空间→线程→任务→成果→diff→原文→本次交付→逐级回原线程；`review-cua-files.png`、`review-cua-diff.png`、`review-cua-versions.png`、`review-cua-text.png`、`review-cua-return.png` 为同尺寸原生截图。 |
| 真实本机服务 | `thread-review-native-network-cua.log`：正式 Node 中继与 Rust MobileServer 在 UI 检查后继续完成撤权／停服验证，1/1 通过、退出 0。任务和文件是明确 fixture，没有真实 Coding Agent 执行。 |
| 隔离数据清理 | `thread-review-ui-final-cleanup.log`、`thread-review-ui-cua-cleanup.log` 各 1/1，按本次身份删除临时配对与云凭据并验证其他电脑不变；最终 fixture/sentinel 文件均不存在。 |

测试设施期间有两类失败，保留且不计作产品通过：`thread-review-native-network.log` 的测试客户端闲置关闭已修复；`thread-review-native-network-retry.log` 超过 15 分钟检查期限；首次重开因残留 fixture 被拒绝（`thread-review-native-network-final.log`）。精确清理后使用新隔离身份完成最终通过记录。误填模拟器 ID 的 `thread-review-text-ax-red.log` 不是有效功能红测。

独立只读 Agent `/root/thread_delivery_ui_review` 复核：初始化错误、固定图标和版本文案修复通过；标准字号成果阅读视觉、最大字号自动化范围和根任务 Computer Use 链路 **局部 PASS**。结论不包含完整 W09、验收／修改反馈、实际 VoiceOver、图片预览 Computer Use 或全 App 大字号；重复行 fixture 的手势位移也不能仅凭相同正文截图判定。最终原生专项共 11 项通过，不含隔离准备／清理用例。

权限结论：Xcode 26.2／iPhone 16e iOS 26.3 模拟器、缓存写入和根任务 Computer Use 均可用。一次窗口连接失活通过重新选择 Simulator 恢复，不是权限拒绝；不需要用户继续修改权限。未修改系统缓存属主或权限，未清空用户 Keychain。

剩余：真实 Norma 编排／Coding Agent、成果验收／修改反馈、后台／跨网络、全页面大字号与 VoiceOver。#57/#59 保持 OPEN，Goal active；真机与 Norma 语音通话后置。仅本地开发和测试，未提交、未推送、未部署。

## 成果验收持久记录与恢复核心（2026-09-29 19:45）

已补验收切片规格，并复用 Mac `AcceptDelivery`、命令预留/lease、原验收事务；中继只开放带原线程/派工/task_ref/冻结指纹的专用接口。读取与写入共用原成果身份检查，验收操作与派工操作独立；权限收紧后同一手机仍能读取原记录，但不能写入验收。原执行事件、回复和冻结文件保持不变。

Swift 在原任务 SQLite 记录中持久保存独立验收决定与回执；复用现有每 Mac 轮转和帧预算，不新增服务或执行器。重复点击复用操作，重启先查 Mac 回执；明确拒绝后才能由用户显式生成重试操作，结果未知不能自动重放。验收成功以 Mac 的持久记录为准，不以手机点击或队列接收为准。

| 证据 | 结果与边界 |
|---|---|
| 失败证据 | `thread-acceptance-red.log`：缺少验收能力协商断言失败；`thread-acceptance-core-red.log`：新的公开存储接口尚未实现，编译失败。不是现有 UI 流程失败。 |
| 正式本机网络 | `thread-acceptance-network.log`：Rust/Node 1/1；`thread-acceptance-swift-network-fixed.log`：Rust 1/1 + Swift 1/1，通过错来源/指纹、重复/换路由、丢回执重连、同设备权限降级/恢复、不改原执行事件及撤权停服。 |
| Swift Core 回归 | `thread-acceptance-core-regression.log`：125 项中 122 通过、3 个显式 fixture 场景跳过、0 失败。 |
| iOS 原生专项 | `thread-acceptance-native.log/.xcresult`：ThreadExecutionTests、ThreadReviewTests、MobileCommandTests 共 14 项中 13 通过、1 fixture 跳过、0 失败；iPhone 16e / iOS 26.3 Simulator，Xcode 权限未阻塞。 |
| 修正记录 | `thread-acceptance-swift-network.log` 首次因测试内局部变量同名导致编译失败；更名后的 `-fixed.log` 全部通过。 |
| 护栏 | `git diff --check`、存储依赖边界、no-allow 通过；本切片 Rust 文件未超过 500 行。 |

**本切片没有修改页面，成果页尚无验收按钮；退回修改及后续任务关联尚未实现。** 本轮无新的 Computer Use 或 UI 子 Agent PASS。网络前台调用和任务仍是明确 fixture；真实 Mac App/Coding Agent、手机新验收操作的完整 UI 链路、后台/跨网络与全页面辅助功能仍待验收。原生运行后恢复默认 App；不部署、不提交、不推送、不关闭票据，Goal 保持 active。

已通过 GitHub CLI 读取最新记录后实际更新 [#57](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5889644207) 和 [#59](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5889631799)，维持 OPEN。当前进程及 `services/norma-cloud/.env` / `.env.local` 再次检查未找到 OpenAI/豆包模型 Key；只检查配置存在性，未打印密钥值。默认命名空间 App 已在标准字号重新启动。

已通过 GitHub CLI 实际同步本节进展：[57 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5889165364)、[59 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5889166492)，两票保持 OPEN。


## 退回修改核心与成果双操作入口（2026-09-29 20:13）

退回修改现在保留原成果及意见草稿，同事务追加原线程的意见消息、关联修改任务与原成果后续引用；固定原 Mac、项目和 Agent。取消意见 Sheet 保留草稿，线程聊天草稿独立；重复请求复用同一修改任务。新 `thread_task_revision_v1` 能力协商与 `revision_of` 指纹绑定防止旧 Mac 静默执行或换目标。验收与退回决定互斥，原执行事件不变。

原生成果页已增加实际文件局部预览、具名执行结果、验收/退回双操作、意见 Sheet 和修改任务入口；复用系统导航/Sheet/玻璃底栏。提交意见时禁止编辑，任务卡的验收等待/退回状态使用对应图标和颜色。这些页面修改仍在验收中。

| 检查 | 结果与边界 |
|---|---|
| 核心 TDD | `thread-revision-core-red.log` 为新增接口缺失编译红测；`thread-revision-core-fixed.log` 5 通过、1 fixture 跳过。覆盖事务失败回滚、原成果和聊天草稿保留、重启与重复修改。 |
| 网络 TDD | `thread-revision-network-red.log` 缺能力协商断言失败；`thread-revision-network-fixed.log` Rust/Node 1/1、Swift 1/1 通过，验证关联目标/指纹、同线程新成果与重复重试。 |
| Rust 回归 | `thread-revision-norma-regression.log`：127 通过、1 既有忽略、0 失败。 |
| 原生入口红/绿 | `thread-decisions-ui-red.log` 的两条按钮缺失断言失败；`thread-decisions-files-green.log/.xcresult` 标准字号成果读取/文件版本/返回流程 1/1 通过，双操作入口存在。此用例没有点击通过或发送修改意见，不能当作完整双操作闭环通过。 |
| 隔离服务与清理 | `thread-decisions-cleanup.log/.xcresult` 1/1，按临时 Mac/namespace 清理配对和云凭据；随后 `thread-decisions-ui-network.log` 完成权限降级、撤权与停服断言，1/1、退出 0。 |

本轮没有完成新页面的 Computer Use 实际操作或独立 UI Agent 复核；仅确认根任务仍能连接 Simulator。网络执行器与产物依旧为明确测试 fixture，未调用真实 Coding Agent。GitHub CLI 最新读取确认 #54–#59 全部 OPEN，本轮未发布新评论。未提交、未推送、未部署，Goal 继续 active。

## 成果双操作完整链与大字号复核（2026-09-29 接续）

标准字号已完成退回意见→取消／重启恢复草稿→发送关联修改任务→查看新成果→Mac 持久验收→重启核对原／新任务状态→返回原线程。原成果保留，只有一个修改任务，验收结果不串到原成果。根任务 Computer Use 实际检查已验收／已退回成果及意见 Sheet；独立只读 Agent 标准字号复核 PASS。

初审指出的失败／停止结果误用绿色勾选、验收禁用缺少说明已修复；状态按实际结果展示，错误和待确认原因放入固定操作区。意见草稿复用有限后台任务保护未完成写入，与线程消息草稿分开。

| 证据 | 结果与范围 |
|---|---|
| 原生状态与草稿 | `decision-state-green-fixed.log/.xcresult`：模型 2/2，加 fixture 准备 1/1；缺 API 编译红测和首次 actor 隔离编译失败均保留。 |
| 标准完整流程 | `decision-flow-native-fixed.log/.xcresult`：1/1，通过；初轮 fixture 未保存来源身份导致 Mac 正确拒绝，修正隔离种子后通过，未放宽生产校验。 |
| Core 回归 | `decision-core-regression.log`：126 项中 123 通过、3 个显式 fixture 跳过、0 失败。 |
| Computer Use | `decision-cua-accepted.png`、`decision-cua-returned.png`、`decision-cua-revision-sheet.png`；标准字号真实导航及 Sheet 取消。最后一张为空意见聚焦态，没有软件键盘，不能用它证明键盘避让。 |
| 最大辅助字号 | `decision-flow-ax-native.log/.xcresult`：1 项失败；已到 Mac 确认新成果验收，重启后寻找原任务卡失败。线程加载后滚到底部，测试 helper 只向上滑，存在定位方向缺口；不能据此判断任务丢失，也不能判定重启恢复通过。 |
| AX5 独立复核 | `decision-flow-ax-attachments/`：双操作纵排／发送入口／验收状态可见；意见键盘图末行 `spacing.` 被预测栏遮住，须修正输入时光标和末行可见性，再复验。AX5 本轮不通过。 |
| 隔离清理与网络收口 | `decision-ax-cleanup.log/.xcresult`：1/1，精确删除本轮临时配对与云凭据，保留其他电脑；随后 `decision-ax-network.log` 完成权限降级／撤权／停服断言，1/1、退出 0。 |

最新剩余工作：① 修复 AX5 编辑遮挡及测试旧任务定位，补失败状态／VoiceOver；② 接通线程 @Norma 编排，验证本地网关调用真实模型及真实 Mac App/Coding Agent 执行；③ 后台、断网恢复和跨网络正式验收；④ 全页面原型／辅助功能复核、旧代码清理与 Issue 收口。此前发现的 Workspace 最大字号标题／筛选换行仍待修。

Xcode／Simulator／缓存及根任务 Computer Use 权限正常，无需用户继续改权限。真实模型配置仍缺；本地网络任务／文件为明确 fixture。GitHub CLI 再次确认 #54–#59 全部 OPEN，不能将标准字号局部通过当作全部票完成。真机扫码／Apple Speech／麦克风由用户后置，Norma 通话最后；只本地测试，未提交、未推送、未部署。

已通过 GitHub CLI 发布本轮实际进度：[57 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5890583528)、[59 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5890584715)，维持 OPEN。

下一步先完整验证「退回→意见重开保留→发送→关联修改任务→新成果→通过→Mac 持久回执→重启原线程状态」，再按 W09 做 Computer Use 和独立复核；其后仍需真实 Norma 编排/Coding Agent、后台/跨网络、全页面辅助功能与旧代码清理。真机由用户后置，Norma 语音通话最后。

## 最新状态核对与键盘复验（2026-09-29 21:15）

- 修正 UI 测试寻找历史任务的滚动方向后，`decision-ax-final.log/.xcresult` 完整 AX5 流程 1/1 通过（96.175 秒）；原任务查找问题已关闭。该版本的键盘截图仍未通过视觉验收。
- 意见输入已尝试系统 Form、多行 TextField 及聚焦输入时滚动到编辑区。最新 `feedback-scroll-ax.log/.xcresult` 再次完整通过，1/1、97.869 秒；包含准确意见、取消重启、关联修改任务、Mac 验收、再次重启及两张任务卡断言。**输入区域几何断言通过仍不能证明末行和光标可见。**
- Computer Use 在标准字号实际输入并显示软件键盘，`feedback-scroll-standard-cua.png` 的全文及末尾光标可见；之后清空本次隔离意见并确认发送禁用，再运行 AX5。独立只读 Agent 复核：标准输入区域 PASS；`feedback-scroll-ax-attachments/7667C0EB-3341-4DFA-A052-9C46F6D1DCB7.png` 仍缺末行 `spacing.` 与光标，AX5 视觉 FAIL，不能关闭该问题。
- `feedback-scroll-cleanup.log/.xcresult` 1/1，通过精确删除本次临时配对和云凭据；随后发出结束信号，`decision-standard-final-network.log` 网络专项正常完成，退出 0。没有保留本轮临时服务。
- 本机 `codex login status` 确认已登录 ChatGPT，可先推进直接 @Agent 的真实 Mac 执行验收；该登录不等于本地 Norma 云网关拥有模型授权。真实 @Norma 编排／模型回合、真实 Mac App 生命周期与后台／跨网络、全页面原型／辅助功能以及旧代码清理仍未完成。
- GitHub CLI 本轮读取确认 #54–#59 全 OPEN；本轮未发布新评论、未提交、未推送、未部署。Xcode／Simulator／Computer Use 权限正常，Goal 保持 active；真机后置、Norma 通话最后。

## 最终意见输入修复与当前剩余（2026-09-29 21:52）

W09 意见 Sheet 使用系统 `Form` 和一至两行展开的多行 `TextField`，超出部分由原生输入内部滚动到光标；保留全文和系统动态字号，删除未解决遮挡的手动聚焦与滚动代码。测试在输入区与键盘上方的交集截图内识别末词 `spacing.`，避免只凭字段 value、外框或预测栏判断可见。

| 证据 | 结果与范围 |
|---|---|
| 有效红测 | `feedback-ocr-red.log/.xcresult`：末词不可见，42.605 秒失败；不是权限或构建错误。 |
| 中间修复 | `feedback-two-lines-ax.log/.xcresult`：AX5 完整流程 1/1，96.486 秒；随后删除手动滚动，再跑最终版本。 |
| 最终最大辅助字号 | `feedback-native-final-ax.log/.xcresult`：1/1，97.237 秒，含末词截图、全文、草稿取消/重启、修改任务、新成果验收、重启原任务与两卡唯一。 |
| 最终标准字号 | `feedback-native-standard.log/.xcresult`：全新 fixture，完整同流程 1/1，59.847 秒。 |
| Computer Use | 根任务实际输入、显示软件键盘、清空测试意见并确认发送禁用、取消；`feedback-native-final-standard-cua.png` 保存最终版本的全文与光标。 |
| 独立只读复核 | `/root/thread_delivery_ui_review` 检查最终标准及 AX5 截图，结论为 W09 双操作与反馈输入局部 PASS，末行遮挡关闭。 |
| 隔离凭据 | `feedback-native-final-cleanup.log/.xcresult`、`feedback-native-standard-cleanup.log/.xcresult` 各 1/1；只删除各轮临时配对/云凭据，保留其他电脑。 |
| 网络收口 | 两轮清理后分别结束 fixture，`feedback-native-final-network.log` 与 `feedback-native-standard-network.log` 均完成权限降级、撤权、停服断言，1/1、退出 0；本轮临时服务均已停止。 |

临时原生光标探针因测试窗口未唤出软件键盘失败，已撤掉，不计作产品红测或验收证据。上述有效测试、截图及结果包均在 `target/norma-ios-evidence/single-mac-20260929/`。没有改动 Core/Rust 业务代码，不重复全仓回归。

当前仍需完成：① 线程 @Norma 真实编排、本地网关真实模型回合及真实 Mac App/Coding Agent 执行；② 真实 App 后台、断网重连、跨网络与不重复执行；③ 全页面原型、失败状态、VoiceOver 和最大字号首页/Workspace 布局；④ 被替代旧代码清理、回归与 Issue 收口。Mac Codex 登录可用于推进直接 @Agent，但不能代替 Norma 云模型授权。任务与成果仍为明确 fixture，W09 局部 PASS 不代表整个 Goal 完成。

Xcode/Simulator/Computer Use 权限正常，当前无需用户再次调整；#54–#59 继续 OPEN、Goal active。真机扫码/Apple Speech/麦克风由用户后置，Norma 通话最后。只本地测试，未提交、未推送、未部署。

本轮 GitHub CLI 已实际更新 [#57](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5891750360) 与 [#59](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5891769314)，保持 OPEN。#59 首次请求连接中断，先读取确认未写入后重试成功；未重复发布。验收后恢复标准字号正常 App，Computer Use 确认已回到想法库。


## 原生 iOS 页面到真实 Codex（2026-09-29 23:49）

- `real-ios-ui-long-id-fixed.log`：Rust L3 1/1，0 失败，1117.95 秒。`real-ios-pass-native-ui.log`：原生 UI 1/1，332.633 秒。`real-ios-pass-native-prepare.log` 和 `real-ios-pass-native-cleanup.log`：准备、清理各 1/1 通过。
- 本轮只安装隔离设备身份，不预填 Workspace、任务或成果。真实页面创建空间、选择项目/Alice、创建线程、发送指令；Codex 实际生成 `result.txt = norma-real-codex\n`、`executions.txt = executed\n`，回复 `NORMA_REAL_CODEX_OK`。成果保持「待验收」并显示退回/验收双入口；App 重启后重新联网读取原成果，宿主结束时直接检查磁盘仅执行一次。
- 首次原生运行被 XCTest 的 128 字符 identifier 查询限制中断，尚未派工；三个长标识查询改为完整值谓词匹配，生产身份未缩短。准备阶段同时拒绝既有凭据/电脑元数据，清理只作用于本轮 ownership 标记的身份。
- 根任务 Computer Use 真实操作原线程→任务→成果→本次交付正文→逐级返回；截图 `real-ios-cua-thread.png`、`real-ios-cua-review.png`、`real-ios-cua-result.png`。Mac Done 和真实回复截图 `real-ios-pass-mac-cua.png`。6 张原生同尺寸截图与清单在 `real-ios-pass-shots/`。
- 临时授权已撤销，原生身份清理通过；隔离 Mac App 正常 Quit，进程检查确认退出；中继随 runner 结束。本轮 `codex-home/auth.json` 仅删除符号链接，用户原登录文件不变。
- Workspace 最大字号标题/筛选/底栏已修复；`workspace-controls-spacing.log/.xcresult` 标准及 AX5 测试 2/2 通过，截图在 `workspace-controls-pass-shots/`；独立只读 Agent 局部截图 PASS，标准 Computer Use 已完成。AX5 Computer Use、全 App 原型及 VoiceOver 仍待完成。

所有相对证据路径均位于 `target/norma-ios-evidence/single-mac-20260929/`。新真实流程截图已交独立只读 Agent 复核，结论待返回。已重新检查当前进程及本地服务配置，未找到所查 OpenAI/豆包模型 Key；不输出配置值。GitHub CLI 确认 #54–#59 全 OPEN。仅本地测试，未提交、推送或部署，Goal 保持 active。


## P05 独立成果入口与 W09 预览优先（2026-09-30）

真实执行截图的独立复核发现：P05 缺独立成果入口，W09 用完整指令作大标题挤占预览；W06 缺工作情况/终端切换、进展时间线和回线程追问。前两项本轮修正，W06 仍未完成。原型保真门槛继续有效。

- P05 在对应 Agent 回复之后显示独立成果卡，无回复时跟随来源任务；仅有已保存可读成果才出现。文件数、Agent、项目和验收状态均读原任务；只有持久验收记录才显示「已验收」。点卡直接进入 W09，一次原生返回回原线程。
- W09 顶部保留简短归属/状态，优先文件预览和实际回复；原指令放「本次任务」默认折叠区，逐字保留且可选择。继续沿用实底卡片、原生导航和系统字体，不缩小辅助字号。
- `ThreadReviewFlowTests` 新增标准/AX5 公开 UI 流程。首次红测准确捕获缺少成果卡；初修暴露 SwiftUI 父展开区 identifier 覆盖子内容的辅助功能标识问题，已把标识移到标签，保留原文独立可访问性。

证据目录：`target/norma-ios-evidence/result-entry-20260930/`。

| 证据 | 结果与范围 |
|---|---|
| `red.log/.xcresult` | 1 项失败，原因是原线程不存在独立成果入口。 |
| `green.log/.xcresult` | 初修两项在展开原文标识处失败；实际正文已展开，标识冲突未掩盖。 |
| `recheck.log/.xcresult` | 标准及 AX5 UI 2/2、成果 Model 2/2，0 失败；验证直接入口、预览位置、全文展开、返回及不重复任务。 |
| `task-route.log/.xcresult` | 原有任务详情→成果→文件/版本→返回/重开路径 1/1，通过。 |
| `pass-shots/` | 4 张原生同尺寸截图及 manifest；标准首屏可见预览和结论，AX5 内容可滚动到达。 |
| `P05-cua-standard.png`、`W09-cua-standard.png`、`W09-cua-file.png` | 根任务真实 Computer Use 完成直接入口、文件预览/正文和逐级返回；未点击验收或退回，不产生新任务。 |

独立只读 Agent 已复核代码、原生及补充标准 Computer Use 截图：P05 独立成果入口与 W09 预览优先两处可局部关闭，未发现新增阻断。本次模拟器只暴露外层 AX 树，滚轮/拖动未产生可观察滚动，因此任务原文展开、AX5 手势和完整 VoiceOver 不记作 Computer Use 通过。网络中的 Agent 结果是明确 fixture，不能代替真实 Coding Agent 的退回→新任务→验收链。

本轮未完成线程 @Norma；已按实际文字服务使用的 `DOUBAO_SPEECH_API_KEY` 再次仅检查配置存在性，当前进程及服务 `.env`/`.env.local` 未配置。Goal 保持 active，真实模型、W06、异常恢复/跨网络、完整原型与辅助功能、旧代码清理继续待办。真机后置、Norma 通话最后；只本地测试，未提交、推送或部署。

本轮清理原生测试 1/1 通过，本机正式中继网络专项在权限降级/撤销/停服检查后 1/1 通过；临时fixture及结束标记由runner移除，恢复标准字号默认App。

GitHub CLI 已读取最新票据并同步本轮实际结果：[57 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5894175740)、[59 评论](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5894182653)。两票保持 OPEN；评论区分真实 Codex 页面验收与本轮网络fixture的UI修正。

## 线程 Norma 候选接口（2026-09-30）

`POST /v1/threads/plan` 已接入既有受邀手机服务，复用鉴别、撤销、并发、额度和回合记录。模型候选完整结束后校验来源范围、成员目标、字段与字节预算；服务端只回填已验证来源并返回候选，不执行 Mac 任务、不保存对话正文。修复公共去重比较，使 UUID 大小写及切换文字/编排接口都不能重复请求同一回合。

证据目录：`target/norma-ios-evidence/thread-plan-20260930/`，使用 Node 22.22.2，供应商均为明确测试替身。

| 证据 | 实际结果 |
|---|---|
| `http-red.log` → `http-green.log` | 新路由从匿名 404 的预期红测到受邀候选/讨论/身份/额度检查通过。 |
| `boundaries-red.log` | 同回合 UUID 改为大写意外返回 200，准确暴露去重缺口；已修复。 |
| `budget-red.log` | 任务和快照合计超过 Mac 32 KiB 仍返回候选；已按既有协议拒绝。 |
| `boundaries-final.log` | 正式 HTTP 两组场景通过：包括未知/重复目标、伪造来源、额外执行字段、截断/超长结果、请求/快照边界、失败后恢复、跨接口并发及生成期间撤权。 |
| `text-final.log`、`relay-final.log` | 既有受邀文字与正式中继专项均通过。 |

本切片未改原生页面，未新增 UI 验收结论。手机客户端、候选原子应用及 `@Norma` 入口仍未完成；模型真实调用仍缺本地服务配置。当前进程及服务 `.env`/`.env.local` 仅检查配置存在性，未输出密钥。W06、真实退回修改/验收、异常恢复、全页面与辅助功能、旧代码清理继续待办。Goal 保持 active，#54–#59 仍 OPEN；只本地测试，未提交、推送或部署。

已通过 GitHub CLI 实际同步本节进展：[#57](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5894465455)、[#59](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5894474287)，两票保持 OPEN。

## 手机线程 Norma 接入与当前缺口（2026-09-30 01:20）

本节更新此前“手机入口尚未接入”的状态：已经接入。成员选择器可呼叫 Norma，手机冻结当前线程快照、共同事实及明确成员选择；等待期间保留草稿，完整候选校验后，同一 SQLite 事务保存用户消息、Norma 回复和待发任务。失败整轮回滚，新输入不被清除；重复响应不重复派工，讨论不产生任务。复用云准入和既有任务投递，不新增 Norma 独立文字页。

### 已验证的修复

| 证据（均位于 `target/norma-ios-evidence/thread-plan-phone-20260930/`） | 结果与边界 |
| --- | --- |
| `store-red.log`、`store-recheck.log`、`store-atomic.log` | 新 API 红测、原子应用、成员移除整轮回滚、新草稿保留、重启与重复响应专项通过。 |
| `client-red.log`、`client-green.log`、`client-recheck.log` | Swift client 经正式本地 HTTP 返回并写入真实 SQLite；测试供应商。最终网络专项 1/1。 |
| `ui-red.log`、`ui-recheck.log/.xcresult` | 最初缺 Norma 入口的失败已修；未准入原生 UI 1/1，原生模型/草稿 5/5。SwiftUI 类型检查超时通过拆出已有筛选表达式解决。 |
| `selection-red.log`、`selection-phone-red.log` | 复现模型把用户明确选择的同名成员改派给另一位；手机原本错误建任务并清草稿。 |
| `selection-green.log`、`selection-phone-green.log` | 服务/手机共同校验稳定选择；拒绝同名误选，仍允许其他不同名成员协助。服务专项通过、Swift 3/3。 |
| `card-order-red.log/.xcresult`、`card-order-final.log/.xcresult` | 独立复核发现无文字成果插到 Norma 分工之前；聚焦测试两处断言失败。修复后归属函数由任务和成果共用；按复核纠正空文字数据后的原生模型最终 3/3 通过。 |
| `success-recheck.log/.xcresult`、`success-shots/` | 原生成功与 AX5 UI 2/2（56.148 秒）；ThreadPlan/成果模型共 6/6。发送、任务详情、返回、重启唯一性通过。此前 `success.log` 是测试目标名写错造成启动失败，未算测试证据。 |
| `cleanup.log/.xcresult` | 临时云设备撤销与精确 Keychain 删除 1/1；本地测试服务已退出，恢复默认命名空间/标准字号启动。 |

本次候选服务用测试供应商，测试 Mac 未配对，任务始终“待发送”；不得当作本次真实 Norma 推理或 Mac 执行。此前真实直接 @Agent/Codex 成功证据独立保留。当前源码 `DOUBAO_SPEECH_API_KEY` 在进程及服务 `.env`/`.env.local` 仍未配置，只检查存在性，未输出密钥。

### 独立复核及尚缺验收

`thread_delivery_ui_review` 复核确认三种任务/成果归属正确，标准截图局部通过；AX5 成员 Sheet 的“取消”变成省略号，仍须修为明确退出入口。AX5 自动截图停在任务卡，Norma 头像/正文在屏外，须滚回后补图。复核还指出“无文字回复”用例有文字但无回复 ID；测试数据已修为空文字成果，具名回复阶段再补相应文字/ID。

本轮 Computer Use `getApp` 报 `cgWindowNotFound`，`getState` 明确返回 **Mac 已锁屏，需用户手动解锁**。因此没有新的真实操作证据，不声称完整 P05 或 VoiceOver 通过。这不是 Xcode 缓存权限回退：原生构建与测试仍可执行。

剩余依次为：真实 `@Norma` 模型到 Mac 的编排闭环；W06 终端/进展时间线/回线程追问与真实退回修改→新成果→验收；后台/意外断网/Mac及中继重启/跨网络验收；全页面原型及辅助功能复核、旧代码清理与 Issue 收口。真机扫码/Apple Speech/麦克风仍由用户后置，Norma 通话最后。#54–#59 当前全 OPEN，Goal active。只本地测试，未提交、推送或部署。

已通过 GitHub CLI 读取最新记录并实际同步：[#57](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5895190178)、[#59](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5895197361)，两票保持 OPEN。

## W06 真实进展与 P05 大字号补全（2026-09-30 02:04）

本节覆盖上一节未收口的 P05 AX5，以及新增的 W06 工作情况部分；不包含终端或真实模型验证。

- **进展持久化**：派工回执、执行状态变化与任务/回复同事务保存。重复轮询、迟到回执、仅补成果不追加记录；失败不留下半条进展。旧 JSON 没有进展字段仍可读，不补造历史。页面明确显示 iPhone 同步时间，不冒充 Mac 执行时间。
- **W06 页面**：任务标题优先，随后显示原 Agent、状态、项目与 Mac；长标题至多三行，另有完整指令展开入口。工作情况展示确认过的状态序列，最终回复进入具名「最新进展」，保留成果与共享内容入口。状态映射复用任务卡，不新增执行队列。终端切换/接管/交还、真实计划步骤与关联原任务追问尚未实现。
- **P05 AX5**：成员 Sheet 使用明确的关闭图标、44pt 点击区和辅助标签；关闭不选择也不发送。截图前滚回具名 Norma 正文，重启后也验证正文完整可读；新线程名带随机后缀，避免误点此前同名线程。
- **Apple 设计落地**：浅色内容实底、系统原生导航与返回，正文支持系统 Dynamic Type，时间线图标和文字共同表达状态；长内容通过滚动/展开读取，未限制辅助字号。

### 自动化证据

以下路径均位于 `target/norma-ios-evidence/thread-plan-phone-20260930/`。

| 证据 | 结果 |
| --- | --- |
| `task-progress-red.log` / `task-progress-green.log` | Core 进展 API 红测后实现；ThreadExecution/ThreadPlan 9 通过、1 fixture 跳过。 |
| `task-progress-ui-red.log/.xcresult` | 有效 UI 红测：任务页实际 0 条进展，预期 3 条。 |
| `task-progress-ui-green.log` | 首次页面构建失败，引用了不存在的颜色 token；修为既有语义色，没有作为通过记录。 |
| `task-progress-ui-recheck.log/.xcresult` | 标准与 AX5 进展、返回、重启原生 UI 2/2。 |
| `task-header-red.log/.xcresult` | 缺任务标题标识的有效红测；随后调整任务身份层级与最新进展区。 |
| `task-header-green.log/.xcresult` | W06 标准/AX5 2/2、P05 最新标准编排 UI 1/1。P05 为正式本地 HTTP 加测试供应商，Mac 未配对。 |
| `task-detail-final.log/.xcresult`、`task-detail-final-shots/` | 最终 W06 UI 2/2，Core 6 通过、1 fixture 准备用例跳过；8 张标题/时间线/最新回复截图。 |
| `task-long-instruction-recheck.log/.xcresult`、`task-long-instruction-shots/` | AX5 长指令专项 1/1：UI 新建线程直接 @开发发送，展开后逐字核对、滚到末尾、返回重开保留全文。首次 `task-long-instruction.log` 因 DisclosureGroup 继承 ID 与测试预期遗漏提及前缀失败；按真实 AX 查询修正，未改变产品内容。 |
| `ax5-body-recheck.log/.xcresult`、`ax5-body-shots/` | P05 AX5 修正滚动方向后 1/1，正文/重启/关闭按钮证据。 |
| `ax5-cleanup.log/.xcresult` | 临时云准入撤销、精确凭据删除 1/1；随后服务退出 0，恢复默认 App 命名空间与标准字号。 |

独立只读 Agent `thread_delivery_ui_review` 对最终八张 W06 截图、P05 最新标准三图及 AX5 三图完成复核：已展示的任务头部、回执时间线、具名最新回复及 P05 关闭入口/正文/重启顺序局部 PASS，无需立即修改的视觉问题。AX5 长卡片允许分段滚动阅读，不声称整卡同时可见。具体工作步骤与终端/追问尚未实现，不能扩展为完整 W06 PASS。

随后新增的 AX5 长指令两图也由同一独立 Agent 复核 PASS：91 字原文逐字断言，末尾标记完整可读，返回重开一致；此前缺少全文展开证据的问题已关闭，Computer Use 与完整 W06 仍未通过。

根任务本轮实际调用 Computer Use `getApp`、`getState`，后者明确返回 Mac 锁屏；没有绕过锁屏，也没有把自动截图当作新的 Computer Use 通过。Xcode 原生构建、测试和缓存写入正常。所查进程和服务 `.env`/`.env.local` 仍无 `DOUBAO_SPEECH_API_KEY`，只检查配置存在性，未输出密钥值。

当前剩余：真实 Norma 模型到 Mac 的编排；W06 真实计划步骤/终端/追问；真实 Coding Agent 退回修改→新成果→验收；后台、意外断网、Mac/中继重启和跨网络；新增页面 Computer Use、完整原型/VoiceOver及失败态；旧代码清理。真机后置、Norma 通话最后，Goal active，Issue 不提前关闭。本轮仅本地测试，未提交、推送或部署。

本轮已通过 GitHub CLI 读取最新状态并实际发布进展：[#57](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5895988023)、[#59](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5896008035)，继续 OPEN。GraphQL 曾短暂返回 EOF，重读/REST 核对后再发布，未重复发布不确定请求。


## 原任务终端读取基础（2026-09-30）

本轮接续终端读取实现，保留原工作树；开始前差异和重点文件备份位于证据目录的 `before.patch` / `before/`。全程本地，无提交、推送或部署。

- Mac 从原设备/Workspace/线程/操作和完整 task_ref 查询持久派工，再用已有 `MobileTaskBaselineStore` 读取 Provider 已确认的 node/run/session/turn；只取身份元数据，不加载文件快照，也不新建执行器或队列。
- 原终端通道在一次 GPUI 更新中核对 route、完整回合及活跃协调任务。原回合完成后可读尚未复用的终端；同节点下一回合、协调任务已替换但 Hook 尚未到达、过期 route/请求均拒绝。数据库读取留在后台。
- 正式中继增加 `thread_task_terminal_v1` 单页读取，复用现有 TerminalPage 和分页；每次重验原消息项目权限，前台读取后再核验授权修订。普通 terminal/command 仍不能从中继绕过任务绑定。240KiB 序列化预算超限明确报错，不截断；这条能力不授予输入权。
- Swift 客户端核对原任务/路由/节点、回合与卡片一致性、冻结成果回合及历史分页身份；沿用现有终端页校验和同 Mac 帧预算。没有新建 Norma 私聊页面。

证据目录：`target/norma-ios-evidence/thread-terminal-20260930/`。

| 验证 | 结果与边界 |
|---|---|
| `turn-red.log` → `turn-green.log` | 新存储公开接口先失败；4/4 通过，覆盖重开、同节点后续回合、错设备/操作/项目/route、损坏身份拒绝。 |
| `foreground-red.log` → `foreground-green.log` / `foreground-recheck.log` | 终端前台通道先失败；增强后 1/1，含任务替换先于新 Hook 的拒绝。headless GPUI/录制PTY，不是真实 App。 |
| `relay-red.log` → `relay-recheck.log` | 未实现能力时失败；正式 Node 中继与真实 Mac MobileServer 1/1，终端内容是明确 fixture。首次 green 暴露非任务操作错误码回归，已恢复原 not_found 语义。 |
| `swift-red.log` → `swift-green.log` | 手机类型/接口先失败，响应校验 1/1；错误任务、线程、路由、回合和输入权均拒绝。 |
| `swift-relay-recheck.log` | Swift 共享核心→正式本机中继→Mac 服务 1/1，Rust 外层 1/1；原终端和历史页读取、既有派工/成果/撤权/停服等通过。首次 swift-relay 因两个独立测试客户端在同一 Mac 限流窗口累加超额失败；测试在稳定任务阶段结束后等待真实60秒窗口，再启动 Swift，未放宽服务限额。 |
| `native-core-recheck.log/.xcresult` | iPhone 16e / iOS26.3 原生 ThreadTerminalTests + ThreadReviewTests 4/4。首次 native-core 误写 scheme 外的 target，未执行；改为实际 NormaDeviceTests 后通过，与权限无关。 |
| `storage-suite.log` | `cargo test -p storage`：118 通过，0 失败。 |
| `e2e-suite.log` | `cargo test -p e2e`：287 通过、76 明确 ignored、0 失败；包含最新超大终端页拒绝、原终端兼容回归和任务替换保护。ignored 不计验收通过。 |
| `app-check.log` / `app-release.log` | `cargo check -p app` 与 `cargo build -p app --release` 均通过；已有 settings/agents 两处未使用函数警告。 |

存储依赖护栏、禁止 allow 护栏、`git diff --check` 均通过。全仓行数护栏仍报 `crates/app/src/settings/mobile.rs` 546 行；已从本轮开始前差异核实当时也是546行，本轮未修改该文件，列入后续清理，不记全仓护栏 PASS。

本轮没有页面修改或新的 Computer Use／UI 子 Agent 结论。W06a 界面、实时更新、真实输入接管/交还、任务追问与具体工作步骤仍未完成；真实 Norma 模型、真实退回修改→新成果→验收、后台/跨网络/重启与全页面原型/VoiceOver仍需推进。Goal active，#57/#59 不关闭；真机由用户后置，Norma 通话最后。

GitHub CLI 已读取最新状态后同步本轮进展：[57 回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5896575450)、[59 回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5896581561)。两票保持 OPEN，未关闭 Goal。


## 原终端输入权与队列等待（2026-09-30）

本轮先补输入仲裁 to-spec，再做失败测试与实现，保留当前工作树。开始前差异与重点文件副本在 `target/norma-ios-evidence/terminal-control-20260930/before.patch`、`before/`。

- 接管绑定真实原回合，再占用原 run/session；Runtime 限制普通桌面/手机输入、自动任务、消息与审批/中断的专用写入入口。持有者继续使用原屏幕签名/输入序号/文字校验和PTY，不创建新执行器。
- 输入权15秒到期；原操作只能在有效期内续期，旧会话/过期输入不能复活控制。交还不发送Enter、停止键或清空草稿。人工Enter只在新的Provider回合确认后解除保护，迟到的同回合Hook不能冒充新输入确认。没有暂停Provider进程或工具的声明。
- 复用既有任务与会话回复队列。有效红测发现接管时新任务误报Failed、追问可能从队列移除；修复后保持Queued，交还且无人工草稿时原阶段继续，正文和Enter各一次。
- 普通自动提交的Ctrl-U清理/重试保持兼容。直接`cargo test -p runtime`同时暴露原测试初始化未按feature限定`CodexReader.isolated_config`；已添加与结构定义一致的cfg，未改变生产读取器。

证据目录：`target/norma-ios-evidence/terminal-control-20260930/`。

| 检查 | 实际结果 |
|---|---|
| `runtime-red.log` → `runtime-green.log` | 原输入权接口缺失先失败；第一条独占输入/交还草稿测试通过。 |
| `boundary-red.log` → `boundary-green.log` | 发现泛化草稿拦截破坏原自动重试；限定保护接管期间的人工输入后3/3通过，包含到期与会话切换。 |
| `late-hook-red.log` → `late-hook-green.log` | 重复旧Hook误确认人工Enter的断言先失败；修复后4/4通过。 |
| `queue-red.log` → `queue-green.log` | 正式App派工原先返回Failed；复用原队列修复后1/1，通过自动任务与会话追问的等待/交还/只发送一次。headless GPUI+录制PTY，不是真实App验收。 |
| `runtime-suite-final.log` / `control-final.log` | Runtime全量200通过、3明确ignored、0失败；模块按职责归位后4项输入权测试再通过。ignored不记为验收通过。 |
| `app-check.log` / `app-release.log` | App检查与release构建通过（发布构建2分29秒），仍有既有settings/agents两处未使用函数警告。 |
| `e2e-suite.log` | 首次全量库测试239通过、2失败、60ignored。失败为旧终端网络超时及roster快照缺失；分别在`legacy-terminal-recheck.log`、`roster-recheck.log`单跑1/1通过。 |
| `e2e-suite-serial.log` / `e2e-suite-final.log` | 串行全量库测试均为238通过、3失败、60ignored；失败后未继续集成测试，不能按全仓通过计算。两张旧Agent会话恢复测试未在2秒内看到fake Claude输出，后两项因同一测试锁中毒失败。未加长超时、未跳过。 |
| 恢复测试诊断 | 增加失败时节点状态/终端内容；`restore-diagnostic-exact.log`单跑1/1、`restore-module-diagnostic.log`模块4/4通过，但前置菜单用例后再次失败。`restore-menu-0.log`最小组合中Cursor菜单测试通过、恢复测试失败；现场两个节点均Running而正文空。尚未证明根因或修复，不声称只是负载问题。最初`restore-diagnostic.log`因错误的exact过滤未执行测试，明确不算证据。 |

存储边界、禁止allow、diff空白检查通过。新代码均未超500行；全仓行数护栏仍有本轮前已存在的`crates/app/src/settings/mobile.rs` 546行，保留为未通过。

**尚未交付的用户流程**：中继侧认证接管/交还/续期与撤权释放、输入命令持久回执、受控人工后续回合的原任务来源处理、W06a实时终端/真实光标/系统键盘及Computer Use。此处方法目前只在域/E2E测试调用，不能声称手机已经可以接管。真实Norma模型、真实退回修改闭环、后台/断网/重启/跨网络、全页面原型及辅助功能、旧代码清理和以上恢复测试失败仍待完成。

本轮未修改iOS页面，未新增原生UI、Computer Use或UI子Agent结论，未重新检查当前锁屏或模型配置。真机由用户后置，Norma通话最后；仅本地测试，未提交、推送或部署。Goal active，票据不关闭。

本轮已通过GitHub CLI读取最新记录后实际同步：[57回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5897303742)、[59回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5897311481)，两票保持OPEN。所有测试/构建会话已结束。


## 可撤销终端租约与恢复测试诊断（2026-09-30）

本轮目标保持不变。先复核已有回归失败，再实现正式中继所需的后台撤权能力；不新增执行器，不改用户项目或个人模型配置。证据目录：`target/norma-ios-evidence/session-restore-20260930/`。

### 实现

- `TerminalInputLease` 由Mac创建，绑定原 `MobileTurnKey`，不可反序列化。Runtime校验同一个能力对象，取代直接使用字符串作为输入权身份；两个对象即使目标相同也不能互相接管/交还。
- 后台撤销使输入/续期/可输入页面立即拒绝，不等待额外GPUI事件；交还、到期、换会话和Runtime释放永久关闭原租约。只保留当前对象，迟到续期不能在另一个持有者交还后复活。
- 撤销仍保留人工草稿。没有草稿时原任务/追问队列继续；有草稿时拒绝自动覆盖，不注入Enter或Ctrl-U。
- 持有者页面复用原终端读取；当前页可输入，历史页不可输入。人工产生后续Provider回合时页面报告当前真实turn，租约仍指向原任务，普通线程终端接口的精确回合限制未放宽。
- **正式中继尚未调用新租约**：设备/连接/接管操作绑定、断线/撤权/停服调用撤销、持久输入回执、iOS客户端与W06a页面均仍需接入。不能据域与headless测试声称手机接管已可用。

### 测试与诊断

| 证据 | 结果及边界 |
|---|---|
| `lease-red.log` → `lease-green.log` | 新租约API缺失时编译失败；实现后包含撤权、防旧续期复活和人工草稿保护的专项通过。 |
| `owned-page-red.log` → `runtime-suite.log` | 持有者页面API缺失时失败；实现后Runtime全量203通过、3明确ignored、0失败。 |
| `queue-revoke.log` | 正式App命令/headless GPUI/录制PTY 1/1：派工等待交还；追问等待后台撤销；正文与Enter各一次，全程原PTY。不是网络撤权或Computer Use证据。 |
| `e2e-serial-recheck.log` / `e2e-default-recheck.log` | 本轮修改租约前串行和默认全量各288通过、76ignored；不是修复根因的证据。此前最小组合另连续8次通过。 |
| `e2e-after-lease.log` | 最新默认全量库测试235通过、6失败、60ignored，未继续集成测试。恢复首错及2个锁中毒连带失败，加snapshot、terminal、realtime的网络超时，不能记全量通过。 |
| `diagnostic-minimal.log` / `diagnostic-repeat-*.log` | 原最小组合再单跑及连续10次通过，说明此前组合仍具有间歇性，未加长超时/跳过。 |
| `diagnostic-concurrency.log` | 32线程诊断重现恢复错误：两个子进程为`/bin/sh`，启动标记未出现，直接`drain_all`返回false，终端仍空；收窄到解释器/脚本启动阶段继续定位，不能据此宣称根因已证实。 |
| `diagnostic-full.log` / `diagnostic-descendants.log` / `diagnostic-sampled.log` | 其他诊断运行恢复项通过，但有不同网络专项超时；没有成功采集到失败解释器的sample，不伪称有栈证据。临时诊断源码已存证并从工作树移除。 |
| `e2e-final-serial.log` | 移除临时诊断后，当前实现串行全量288通过、76明确ignored、0失败。只证明串行运行，不掩盖前述默认并发失败。 |
| `app-check.log` | 默认App编译检查通过，保留既有settings/agents两处未使用函数警告。 |

存储依赖、禁止allow、diff检查通过；新改Rust文件均不超过500行。全仓行数护栏仍因先前已有的`crates/app/src/settings/mobile.rs`546行失败。

Computer Use本轮实际选择Simulator，明确返回Mac锁屏，需要用户手动解锁；Xcode缓存权限没有新的失败证据。未修改iOS页面，未生成新的原生截图或UI子Agent验收结论；本轮未重新查模型Key。仍需完成正式中继/手机终端、真实Norma模型、真实返工验收、后台/断网/重启/跨网络、完整原型/辅助功能和旧代码清理。真机后置，Norma通话最后，只本地测试，未提交、推送或部署，Goal active。

所有本轮测试/构建会话已结束。最终串行通过不等于会话恢复间歇失败已修复，后续仍需定位；Goal和Issue保持未完成。

GitHub CLI已实际同步：[57回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5897809846)、[59回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5897814396)。两票保持OPEN，未关闭Goal。


## 正式中继接管、持久输入回执与连接撤权（2026-09-30）

本轮开始复用已有工作树备份，并额外保存`resume-before.patch`。未回滚或覆盖其他任务改动。规格沿用原任务/正式RPC/持久命令/原PTY的公开验收边界；先留下续期接口缺失、网络能力缺失及竞争接管失败的红测。

### 已实现

- 新增受任务约束的`thread_task_terminal_control_v1`，支持Claim/Input/Release/Read。从原派工记录推导项目、路由、节点和Provider回合；手机不能提交另一个终端。普通网络command仍不能反序列化内部控制命令。
- Claim/Input/Release复用已有SQLite命令回执、唯一操作及前台通道；相同操作改输入会冲突。断线后的同操作只核对原回执，不重放输入；旧Claim成功不等于当前连接持有输入权，旧Release重试不影响新持有者。
- Read复用原终端读取通道，只给当前合法持有者续期；历史页不可输入。人工后续回合显示真实turn，来源保留原任务；没有租约的旧读取仍要求精确原回合。
- 每条加密连接持有一个能力；关闭连接永久撤销，在途阻塞任务另有取消保护。按设备维护最多8个弱能力索引，权限改变和停服返回前同步撤销，不等GPUI或后台请求响应。
- 修复新发现的真实前台错误：竞争接管失败后必须撤销该次能力，避免同一连接下一次明确接管被残留对象阻挡；不影响原持有者。

### 验收证据

目录：`target/norma-ios-evidence/terminal-relay-control-20260930/`。

| 记录 | 结果与范围 |
|---|---|
| `renew-red.log` → `renew-green.log` | 新接口缺失先失败；续期不能隐式接管、重连不能冒用、撤销后不可复活，1/1通过。 |
| `relay-red.log` → `relay-basic.log` | 正式网络先因缺少协商能力断言失败；接入后原网络专项1/1通过。 |
| `foreground-red.log` → `foreground-green.log` | 正式App前台先复现失败接管未撤销；修复后1/1。原PTY只收到一次draft，人工新回合可读且可交还，旧读取不可续期。最初测试编译借用错误已修，不当成功证据。 |
| `relay-final.log` | 正式Node中继、Mac MobileServer和SQLite回执，2/2通过，含原读取/派工回归与新增接管。前台是显式fixture，不是真实Coding Agent。 |
| `control-final.log` | 最新专用网络复验1/1通过，含旧交还不影响新输入权、排队Claim期间断线、阻塞读取期间降为只读、恢复授权后显式重接管、停服同步撤销。 |
| `runtime-suite.log` | Runtime全量204通过、3明确ignored、0失败。 |
| `e2e-serial.log` | 串行全量290通过、76明确ignored、0失败；包括正式网络和headless GPUI。本轮未修复此前默认并发的恢复/网络间歇失败。 |
| `app-check.log` / `app-release.log` | 默认App编译检查与release构建通过，release耗时2分29秒；只有既有settings/agents两处未使用函数警告。 |

存储边界、禁止allow和diff空白检查通过；本轮新改Rust文件均不超过500行。全仓行数护栏仍因既有`crates/app/src/settings/mobile.rs`546行失败，不能记全仓护栏通过。

### 仍未完成

Swift控制客户端、人工操作持久保存、W06a页面、实时输出/真实光标/系统键盘及Computer Use接管验收仍需实现。后台实际网络中断、Mac/中继重启及跨网络仍需整链验证，不能把这里的RPC测试扩展成整个可靠性验收。真实Norma模型、真实返工→新成果→验收、完整原型/VoiceOver与旧代码清理继续待办。

本轮重新调用Computer Use：选择Simulator返回`cgWindowNotFound`，随后状态明确返回Mac锁屏，需要用户手动解锁。重新检查进程及服务`.env`/`.env.local`均未发现`DOUBAO_SPEECH_API_KEY`配置，仅检查存在性，未输出密钥。本轮没有修改iOS页面或产生新的UI复核结论。真机后置、Norma通话最后；只本地测试，未提交、推送或部署，Goal active，#57/#59保持OPEN。

本轮已通过GitHub CLI读取最新状态并实际发布进展：[57回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5898463766)、[59回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5898472415)，两票保持OPEN。测试和构建会话均已结束，下一步接入Swift控制客户端、持久人工操作与W06a原生页面；不重新创建执行器或重复已通过的协议基础工作。


## 手机终端控制客户端与恢复查询（2026-09-30）

本轮接入Swift控制客户端与任务SQLite中的人工操作/草稿。未修改原生页面，W06a入口、键盘、真实光标和Computer Use仍属待办。开始前保存`resume-before.patch`及`control-client-before/`，没有回滚其他工作。

### 实际改动

- 正式中继新增`lookup_only`恢复查询。缺失操作返回`not_found`，只核对已有参数和回执，不登记、派发或取得输入权；读取/续期不能使用该标志。复用既有命令存储的指纹比较，无第二个执行器。
- Swift提供明确接管、输入、交还、查询回执及受控读取；收到成功回执必须核对原设备/项目/路由/任务引用/Provider起点。受控读取允许同会话人工后续回合，普通读取不放宽。连接错误后关闭，旧成功记录不能恢复连接控制权。
- 人工操作和文字草稿复用`thread_task_outbox`可选字段，兼容旧记录。发送前保存稳定操作和屏幕参数；未解决操作不得被覆盖或加入自动派工队列。拒绝和结果未知保留输入；成功仅清理匹配revision的草稿，迟到accepted不回滚终态。
- 已完成共享核心网络互通和原生iOS核心测试；页面未接入，不能声称手机已可通过W06a接管。结果未知/未找到后的用户核对入口随页面一起补齐，不能在恢复中自动重发。

### 证据

目录：`target/norma-ios-evidence/terminal-native-20260930/`。

| 日志 | 结果与边界 |
|---|---|
| `lookup-red.log` → `lookup-green.log` | 正式网络先因缺失恢复查询能力失败，随后1/1通过；查询不存在Claim不创建输入权，参数冲突明确拒绝。 |
| `swift-store-red.log` → `swift-store-green.log` | 手机操作存储接口不存在先失败；补齐后1/1，覆盖重开、禁止覆盖、同文新草稿保护、迟到回执与匹配清理。 |
| `swift-validation.log` | 3/3；补充受控后续turn、跨任务回执拒绝、拒绝与未知结果保留草稿；不算网络/UI。 |
| `swift-wire-red.log` | 缺失接管过期错误映射先失败；随后补齐正式错误与中文说明。 |
| `swift-relay.log` → `swift-relay-recheck.log` | 首次Swift1/1通过，但外围Node在独立客户端60秒限流窗口等待后复用过期闲置连接失败；按正式60秒闲置规则重连后，Swift1/1和Rust外层1/1全部通过。前台为明确fixture，真实网络/SQLite回执，不是新真实Coding Agent证据。 |
| `native-core.log/.xcresult` | iPhone16e / iOS26.3原生核心3/3通过，Xcode缓存权限正常。 |
| `swift-suite.log` | Swift全量136项：130通过、6明确跳过、0失败；本轮新增网络专项已单独通过。 |
| `storage-suite.log` | 存储全量119通过、0失败，包含查询不登记/冲突不改写。 |
| `app-check.log` | App编译检查通过；仍有既有settings/agents两处未使用函数警告。 |

复现共享核心整链：`NORMA_NATIVE_TERMINAL_CONTROL_CHECK=1 cargo test -p e2e --test mobile_collaboration_relay thread_terminal_control_is_bound_deduplicated_and_revoked_during_pending_read -- --nocapture`。限流和闲置期限保持生产值，测试不放宽。

本轮再次实际选择Simulator，Computer Use明确返回Mac锁屏；需要用户手动解锁后补真实操作。没有新UI截图/独立UI验收结论。本轮未重新检查模型凭据。下一步接入W06a页面、真实光标、系统键盘和接管状态，协调同Mac后台同步预算；同时保留原任务追问/具体工作步骤、真实模型编排、真实返工验收、异常恢复/跨网络、完整原型/辅助功能及旧代码清理的完整Goal范围。真机后置、Norma通话最后；只本地，无提交、推送或部署。

本轮最终串行E2E：`e2e-serial.log`，290通过、76明确ignored、0失败。此项不代表默认并发的既有恢复/网络间歇失败已修复。存储边界、禁止allow及diff空白检查通过；本轮涉及Rust文件均≤500行，全仓行数仍被既有`settings/mobile.rs`546行阻挡。全部测试/构建会话已结束。

GitHub CLI已实际同步：[57回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/57#issuecomment-5898974099)、[59回执](https://github.com/multisoul-ai/Norma-AI-OS/issues/59#issuecomment-5898977765)，两票继续OPEN。下一步直接接W06a原生页；无需重做本轮已通过的客户端/持久化基础。

## W06a原生终端、系统键盘与恢复（2026-09-30）

证据目录：`target/norma-ios-evidence/terminal-ui-20260930/`。开始前保存`before.patch`、`continuation-before.patch`与`ax-fix-before.patch`及本轮未跟踪源码副本；未重置其他工作。

### 实际接入

- 原任务工作情况/终端切换；工作情况保持挂载以保留阅读位置。键盘时折叠大标题，AX5将工作情况入口放入系统导航。浅色外壳、深色实际网格、系统玻璃功能键，保持系统字号和44pt操作区。
- 一个正式前台连接串行读取、接管、写入和交还；每2秒刷新。原生UITextField保留中文组合、系统编辑与听写；Return先提交持久文字，得到真实写入回执后才发Enter，未知结果不补发。后台/退出关闭连接，返回只读，草稿保留。
- 同Mac后台投递在终端活跃时暂停，其他目标继续；会话令牌阻止旧页面释放新占用。未决操作只能查回执，人工核对保留原回执/草稿，不伪造成功，也不能绕过accepted或核对另一操作。
- 实时光标来自Mac实际PTY行列。修复桌面回看偏移影响输入签名；手机保持原网格，输入时滚动到真实光标，历史页不能输入。
- 独立审查后修复只读误报执行中、圆角裁字、连接结束残留成功提示。AX5把接管与成功反馈放一行；长身份名称/项目采用原生横向滚动，项目独立色底；错误/结果未决收起键盘，保留说明和草稿。

### 自动化与失败修复证据

| 证据 | 结果及边界 |
|---|---|
| `signature-red.log`、Runtime回归 | 红测复现桌面滚动影响实时输入签名；修复后Runtime204通过、3ignored。 |
| `swift-suite.log`、`norma-suite.log` | Swift136项：130通过、6明确跳过；norma128通过、0失败。 |
| `native-core-green.log`、`budget-green.log` | 原生核心/投递5通过；预算投递3及清理1通过。 |
| `disconnect-red.log` → `disconnect-green.log` | 连接取消残留“已接管”先真实失败；读取结束和操作失联统一stop清理后，原生投递3/3，保留草稿且释放预算。 |
| `ui-verified.log`、`ui-axfixed.log` → `ui-compact.log` | AX5实际输出55pt/27pt失败，标准通过；重新分配控制区和身份行后标准/AX5共2/2。90pt阈值未降低，未限制用户字号。 |
| `ui-cursor-red.log` → `ui-cursor-layout.log` | 第71列光标在标准/AX5均不可见，先红测。绘制offset不参与滚动定位，改成真实布局坐标后AX5通过；标准位于视口边界产生浮点误差，追加8pt可见留白，保持完整包含断言。 |
| `ui-final-check.log/.xcresult` | 最终标准/AX5共2/2：实际输出113pt/114pt，整个第71列光标位于视口内，保留8pt下侧/右侧留白；AX5后台返回只读、草稿恢复、明确重新接管及交还全部通过。 |
| `ui-final-check-cleanup.log/.xcresult`、`ui-final-check-relay.log` | 隔离配对/云凭据清理1/1；随后结束信号收口，正式网络专项1/1退出0，累计Node一次和两项UI各两次输入，没有重复写入。 |

此前各UI轮次均在原生清理成功后结束其正式网络fixture；没有把UI失败轮的网络通过当作页面通过。网络为正式中继、Mac MobileServer与SQLite回执，终端前台输出/写入目标是显式fixture；不是新的真实Coding Agent或真实PTY端到端证据。

独立只读Agent`thread_delivery_ui_review`完成最终四张截图及日志复核：当前标准/AX5终端UI切片PASS，最大字号输出高度、圆角裁字、身份混读与右侧光标问题关闭，无新增必改项。历史切换、长项目名滚动、完整VoiceOver与真实PTY尚未由本轮证明。最终截图映射见`screenshots-final-check/manifest.json`和[线程验收记录](2026-09-29-norma-ios-thread-acceptance.md)文末。Computer Use最终重新选择Simulator先报`cgWindowNotFound`，读取状态明确返回Mac锁屏，无新的手动界面验收结论。Xcode缓存权限正常；本轮只检查配置存在性，进程和`services/norma-cloud/.env`/`.env.local`均未找到所查OpenAI/豆包Key，未输出密钥。

完整Goal仍包含：真实线程@Norma模型编排、W06具体工作步骤/原任务追问、真实Coding Agent退回修改→新成果→验收、后台/断网/Mac及中继重启/跨网络、全部原型/VoiceOver、被替代旧代码清理。真机扫码/Apple Speech/麦克风由用户后置，Norma通话最后。全仓行数护栏仍被此前已有的`settings/mobile.rs`546行挡住；默认并发的恢复/网络间歇失败未修复。Goal active，Issue不提前关闭；只本地测试，未提交、推送或部署。


## 2026-09-30：直接派工与 TestFlight 43 交付

原生 iOS 页面 → 本地中继 → 真实 Mac/Codex → 原线程回复与两文件成果、重启恢复及不重复执行已通过；Computer Use 实操浏览成果和返回原线程通过。Swift Core 142 项（8 条条件跳过）无失败，原生固定证书 HTTPS 邀请/Keychain 恢复与 WSS 通过。

用户已授权发布 TestFlight。0.1.0（43）现已 VALID / IN_BETA_TESTING，原「Norma 内测」组与中文说明回读通过。只在同一 Wi-Fi 使用已启动的 Mac 专用验收版及本地中继，未部署云端。安装、配对与派工步骤见 [本次交付文档](2026-09-30-norma-ios-testflight-dispatch.md)。真实 @Norma、完整后台/跨公网及后续修改闭环仍待收口，物理 iPhone 用户验收、语音最后；#57/#59 保持 OPEN，整体 Goal active。
