# GPT-Live 接入验收台账

用户最新安排：先完成功能，真人语音与真实云端项目工具由用户后续统一验收。当前不再发起真人通话，也不再等待设备确认。原 AC01–AC18 保留；延期项不计为通过。原自动Goal已由用户清除；本文保留为实施与验收记录。

软件功能与本轮自动验证已完成：后台错误与澄清、只读查询正文、工具准入、独立停止与恢复播报、持久任务终态事实、只读操作核对。最终Release与真实设置页结果见本页末尾。

## 工作区与证据

- 实施：`.worktrees/gpt-live-duplex`，分支 `feat/gpt-live-duplex`，基准 `b643d177e355846337842c2ec4b4152043bf5c03`，worktree 已锁定。
- 未提交、推送或替换用户常驻 App。只停止 desktop-e2e 的 owned PID；`/Applications/Norma OS.app` 与其他开发实例均保留。
- 源码及日志备份：`/Users/openclawd/.codex/artifacts/gpt-live-20261018/`。变更文件数量与SHA256以最新source-manifest.json为准。不要运行旧恢复脚本覆盖新源码。
- 开发构建缓存统一为上述目录的 `build/app`；旧 `build/domain` 已移除。曾仅清理本任务增量缓存及重复缓存以恢复磁盘空间，不清理其他工作区。
- HTML 方案和 Goal 文件已保留。Chrome 对 `file://` 访问曾被 URL 安全策略拒绝，未改用本地 HTTP、exec/open 或 CDP 绕过；文件链接不代表已成功在浏览器打开。

## 当前运行检查

下表日志均在外部备份目录的 `logs/`。测试数量只表示该次运行，不能替代整个 AC 的行为证据。

| 范围 | 命令、实际结果 | 证据与限制 |
| --- | --- | --- |
| GPT 设置、音频循环、状态与取消 | `cargo test -p app --lib gpt_live --features e2e-test-support`，退出0；28项通过 | `app-live-usage-worker-green.log`；包含真实本地WS的PCM解码、精确节拍、静音fixture、跨窗撤Key、Host关窗、累计/异常用量。没有真实云端或扬声器。 |
| 共享豆包回归 | 同配置 `cargo test -p app --lib doubao --features e2e-test-support -- --test-threads=1`，退出0；71过、2 ignored | `app-live-shared-regression-final.log`；含新增Live播放预算的2项硬件回调边界测试；ignored非本轮云端证据。 |
| 语音设置回归 | 同配置 `cargo test -p app --lib voice_settings --features e2e-test-support -- --test-threads=1`，退出0；91过 | `app-voice-settings-final.log`；与GPT筛选有交集，不能相加宣称独立用例总数。 |
| Live / Voice input / UI | `cargo test -p live_voice -p voice_input -p ui --all-features`，退出0；合计72过、6 ignored | `live-input-ui-regression-fixed.log`；设备/模型依赖项仍ignored。新增fixture字段造成的首次编译失败已补全两个旧fixture后重跑。 |
| UI最新语言包 | `cargo test -p ui`，退出0 | `app-live-ui-tests-final.log`；包含通话/用量/设备文案。 |
| 域层Clippy | `cargo clippy -p live_voice -p voice_input -p audio --all-features --all-targets -- -D warnings`，退出0 | `live-input-audio-clippy-final.log`；没有用allow压制诊断。 |
| App编译与开发二进制 | `cargo check -p app`、`cargo build -p app` 已退出0 | `app-live-final-check.log`、`app-live-desktop-build-final.log`；异常用量补丁后最新重跑见 `app-live-latest-{check,build}.log`。开发无优化构建，不作时延结论。 |
| Settings条件事务 | `cargo test -p storage`，退出0 | `storage-tests.log`；条件冲突零写入，真实SQL触发器失败证明整批回滚。该部分本轮仅格式化。 |
| Agent域层和协议历史检查点 | Live + Agent 139过、1 ignored；相关域层Clippy退出0 | `domain-tests-context-agent-fixes.log`、`domain-clippy.log`；后续Live回归以上方最新日志为准。真实Adapter副作用和持久账本未验。 |
| 机械约束 | 行数、no-allow、指南行数、Storage边界、性能护栏、SOCKS、TTS网络边界均退出0 | `g3-mechanized-checks.log`；`git diff --check`同记。workspace及release检查留在G6，尚未完成。 |

统一开发参数：

```bash
CARGO_TARGET_DIR=/Users/openclawd/.codex/artifacts/gpt-live-20261018/build/app \
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
cargo test -p app --lib gpt_live --features e2e-test-support \
  --config 'profile.dev.package."*".opt-level=0' \
  --config profile.dev.debug=0 --config profile.dev.opt-level=0
```

## 本轮实际复现与修复

| 反例 | RED → GREEN 与实际含义 |
| --- | --- |
| 已schedule PCM绕过时长背压 | `app-live-playback-budget-{red,green}.log`；Live队列及硬件未完成音频合计最多1秒。600ms首包未完成时第二个600ms拒绝。首轮RED为缺少新入口，随后又以实际回调丢失反例验证预算契约。 |
| callback丢失提前释放预算 | `app-live-playback-callback-{red,green}.log`；RED观测预算0ns但硬件stop仍阻塞，修复后保持600ms至确认停止。完成、interrupt、stop、入队失败均由RAII释放。 |
| 到期上传被输入抢占 | `app-live-clock-fairness-{red,green}.log`；手动poll媒体future、可控时钟和200ms缓冲+20ms到期突发，旧代码误报溢出，新代码先排出一帧。服务端解码0x33真PCM，合成静音不能冒充收音。 |
| 共享stop阻塞前台 | 独立审查指出预览取消在GPUI前台调用stop；保留stop异步发送，只有后台播放队列调用stop_and_confirm。设备ACK在player.stop返回后发送。未据此宣称物理硬件停止时延已验。 |
| 跨窗配置仍保留旧通话 | 模式、设备、合法Live引用变化会取消旧媒体；音色仅下次生效；重复选择同模式不断线。`app-live-config-lifecycle-{red,green}.log`及最新28项运行。 |
| pending无关写失败后错过远端撤Key | `app-live-pending-resync-{red,green}.log`；先证明pending使同步跳过，再用真实SQL触发器让无关BaseUrl写失败，生产完成回调必须再同步并取消旧token。 |
| Host保留后台关窗漏停 | `app-live-host-detach-red.log` → `app-live-host-resync-green.log`；保留实体而不Drop，也须取消Host自己的媒体。 |
| 忽略中间用量、伪造0秒 | `app-live-usage-{snapshot,forward}-red.log` → `app-live-latest-green.log`；真实WS用量转发；累计快照覆盖、不相加；未报告只显示未确认文字。 |
| 异常worker把已知用量退回0 | `app-live-usage-worker-{red,green}.log`；已知3.5秒后收到未知默认终态，仍保留3.5且未finalized，错误原因可见。 |
| 静音fixture没有证明停留期释放 | 新fixture提供启动/释放观察；先等待mute释放旧资源，再unmute开启新generation；满事件队列取消也退出。只证明资源生命周期，不替代物理麦克风。 |

静音UI反馈仅表示本地上传gate已更新，不表示服务端ACK或物理设备停止回执。实际麦克风数据在mute期间被过滤，上传时钟使用合成零PCM继续推进。

播放有两层明确预算：协议输出事件队列最多1秒；App播放器的等待及已schedule未完成PCM合计最多1秒，总体为有界分层预算。App本地麦克风缓冲200ms，协议上行另有1秒预算；旧设备采集队列也保留其既有边界。达到上限会明确报错结束，不静默积压。

## 真实 App 设置验收

执行：`bash scripts/desktop-e2e.sh settings`。开发二进制复制到本worktree的 `target/debug/norma-os`，独立数据目录；socket位于随机、0700权限的短 `/tmp/norma-e2e-…/norma.sock`，避免深worktree路径超出macOS SUN_LEN。

最新通过运行：`2026-10-18T05-11-03Z`，退出0，1/1场景，5步全部通过，13.38秒。7张最新截图已逐张打开；前一轮 `04-57-57Z` 也为1/1。证据目录：

`/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T05-11-03Z/`

| 步骤 | 实际结果 |
| --- | --- |
| 打开语音设置，选择GPT-Live | 可见三模式与独立连接区域；SQLite `voice.desktop.mode=gpt_live`。 |
| 缺Key时尝试连接 | 连接/静音禁用；点击后仍缺Key，未进入连接状态；无凭据引用。没有真实Key或云端调用。 |
| 选择Cedar | 真实按钮保存，SQLite `voice.gpt_live.voice=cedar`。 |
| 浅色主题重启 | GPT模式和Cedar恢复，缺Key状态保留。 |
| 切回Doubao和Legacy | 两个原模式可用，保存值分别为doubao/legacy；GPT音色保留。 |

早期运行 `04-54-14Z` 确实发现第三卡换行把连接按钮推下首屏，已减小卡片最小宽度并重跑。`04-56-36Z` 的最后一步因OCR把“豆包 2.0”识别成“豆包2.0”失败，属于匹配器问题，增加无空格的同义候选后完成整个流程。首次深路径socket失败已改短路径；开发App的Sparkle缺失和手机服务端口占用警告不作为语音设置通过的证据，也不用于声称后台手机功能已验。

参考Apple Design：沿用Settings分组和系统排版；按钮使用现有组件；操作即时更新状态，不添加无功能动效。选中同时使用勾号、边框和颜色；缺Key以文字说明并禁用连接。菜单仍复用现有设备选择器。真实交互清单位于 `test/desktop-e2e/checks/settings/settings-live-configuration.checks.md`。

逐图已实际打开核对：

| 截图 | 结论 |
| --- | --- |
| `01-live-dark.png` | 符合：三模式同排，GPT有勾号，缺Key、连接和静音在首屏可见。 |
| `02-live-disabled.png` | 符合：点击后状态不变，禁用控件可识别。未覆盖键盘触发。 |
| `03-audio-dark.png` | 符合：Cedar勾号、设备和麦克风权限文案可读，错误ASR就绪文案已消失。 |
| `04-live-light.png` | 符合：浅色下GPT选择、缺Key和禁用状态保持，无密钥正文。 |
| `05-audio-light.png` | 音色恢复与麦克风文案符合；焦点/hover未出现在图中，相关项不确定，不据此判完整AC14通过。 |
| `06-doubao-regression.png` | 符合：豆包勾选和原语音交互/连接区，无GPT连接区混入。 |
| `07-legacy-regression.png` | 符合：Legacy勾选和原设置引导，GPT未高亮。 |

示例截图：

![GPT-Live深色设置](/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T05-11-03Z/01-live-dark.png)

最终文案和异常用量补丁后的重跑已退出0，见 `desktop-settings-live-latest.log`。上表引用最新截图；该场景不覆盖真实通话、完整键盘遍历或物理音频。

## AC状态与剩余工作

以下“本地通过”只对应L1/L2行为，不能扩称该项相关的云端或物理设备也通过。A=`app-live-default-focused-tests.log`，D=`workspace-live-domain-completion-node22-tests.log`，F=`app-live-delivery-full-regression.log`；日志位于外部备份的`logs/`。

| 验收 | 当前证据与范围 | 尚缺的实际证据 |
| --- | --- | --- |
| AC01 | 本地通过；D的`microphone_waits_for_started_then_streams_live_audio`检查启动屏障。 | 最新交付版真实服务连接归AC15统一复验。 |
| AC02 | 本地通过；A的`live_audio_loop_streams_both_directions_and_releases_playback`经真实本地WS验证上下行。 | 真人持续双工与后台任务挂起时的实际听说。 |
| AC03 | 本地通过；D检查PCM格式/上限，F检查调度与播放预算，可控sink回调夹具有独立证据。 | 原生设备与弱网下队列、内存稳态指标。 |
| AC04 | 本地通过；D的双向转写时序测试及A的音频循环保留双方字幕。 | 内部转写时序继续回归；用户已取消悬浮显示，真人验收改核对实际听说与项目结果。 |
| AC05 | 本地通过；D检查乱序上下文，A检查observed规划、只读回执与写授权保留。 | 云端委派、迟到转写与澄清的完整项目流程。 |
| AC06 | 本地通过；A的Bridge和真实Canvas Adapter在执行前复验取消/revision，保留已接受回执。 | 云端任务规划时的真人更正与迟到结果。 |
| AC07 | 本地通过；A的SSE→NormaSession→MemoryAdapter→真实pump创建一张Browser，换owner重放仍为一张；unknown保留与原命令只读核对也已测。 | 真实App/云端项目重启和unknown完整恢复；不自动重做仍是必须条件。 |
| AC08 | 本地通过；D中来源、目标、类别、引用和显式授权正反例通过。 | 云端实际项目操作与目标核对。 |
| AC09 | 本地通过；A验证停播期间仍上传真PCM、恢复不播旧音频；任务取消经现有真实取消路径测试。 | 运行中真实任务的停播/取消分离及口播结果。 |
| AC10 | 本地通过；A检查480 UTF-8字节回报、真实查询正文、任务持久终态、ACK与背压重试。 | 真实任务终态回传、服务端接收及实际听到的播报分别核对。 |
| AC11 | 本地通过；A/F检查静音释放fixture、跨窗配置、Host关窗、代际、取消与异常用量保留。 | 真实设备释放、物理停止时延、异常网络和多次重开。 |
| AC12 | 本地与真实设置局部通过；A检查独立凭据、实际VAD准入和缺后台仍可闲聊，最终设置场景检查缺Key。 | 最新交付版真实鉴权错误、网络错误及恢复。 |
| AC13 | 本地与真实设置局部通过；F的旧语音回归及最终场景切换Legacy/Doubao/GPT并重启，配置保留。 | 物理采集时仅当前模式工作、无重复TTS与完整快捷键流程。 |
| AC14 | 部分通过；最终Release设置场景6步、8图符合，三模式按钮越界回归已修复。 | 全部hover/pressed/focus、键盘遍历、窄窗口和通话中控件状态。 |
| AC15 | 部分通过；历史真实云端与App连接、数字音频已有记录。 | 最新交付版实际听到声音、真人原文与关闭session相互对应。 |
| AC16 | 待执行；离线真实Adapter和DB不能替代真实云端项目。 | 用户统一验收时配置可用Norma后台，核对实际node/task ID、派工、查询和取消。 |
| AC17 | 待执行；此前扬声器激励无转写，记ENV，不视为声学通过。 | 耳机/外放各自真人插话、停止时延样本与回声误触发。 |
| AC18 | 待核对；历史10分钟连接未掉线，但本地600秒与服务端821秒不一致。 | 最新交付版重开、断网恢复、资源指标和服务端用量准确性。 |

## 工具与官方时轴边界

App保留pending delegation与原始转写，从累计已观察证据准备语义候选。已接NormaSession::turn_delegated、MemoryAdapter、GPT专属桌面pump与持久账本；不再等待API不存在的完整话语信号。回传仅使用Adapter回执，不使用模型final_text充当事实。G2软件与本地整链已完成，真实云端项目验收仍延期。

## G2持久账本检查点

`LiveOperationStore`新增到Storage，按project、宿主观察到的稳定intent和operation身份认领。宿主不能把每个新model delegation ID都当作新意图。`BEGIN IMMEDIATE`保证并发首次执行权唯一；同一身份参数冲突拒绝，明确新意图仍可认领。

4项本地集成测试已通过，日志 `storage-live-cross-db-green.log`：重启后返回原accepted回执及node/task身份；不同回执不能覆盖；真实文件副作用后Failed保持unknown；两事务并发只append一行；未结算旧owner重启后unknown；跨库lease写入拒绝且目标库保持Pending。每次认领在同一事务内生成持久128-bit claim token，record同时核对token、owner和命令指纹。

独立审查补出的跨库lease反例已RED复现并修复；修复后未发现新的存储阻断问题。该结论仅限账本核心。后续条件接线见下方检查点；真实云端工具、unknown核对界面及恢复动作仍未完成。Storage/Agent全套与Clippy的新运行结果见 `storage-agent-live-ledger-{tests,clippy}.log`，App检查见 `app-live-ledger-check.log`。

官方转写 `start_ms/end_ms` 和委派 `offset_ms` 使用同一服务端session时轴；官方没有保证它等于累计上传样本或本地wall-clock。`UploadedAudio.start_sample`只能用于本地诊断，未经校准不能用来自动授权。Sherpa长语音切段也不等于新的语义话语；不确定关联保持Pending。

offset只作关联，不切字或表示转写已完成；新语音立即冻结旧规划；已接受任务不能因停播或断线被误取消。后续要完成真实项目、跨delegation去重、unknown核对与重启恢复，原Goal范围保持。

域层已完成的正反例还包括：独立GptLiveVoice来源、删除授权类别/目标、否定偏好与撤销记忆、直接寻址、执行中取消、上游tool ID重放、无上游ID的Ollama兼容、历史滚动和释放ABA。相关历史RED/GREEN日志持续保留。

只有G0–G6及AC01–AC18全部必需部分通过，且没有未处理高风险审查发现时，才能标记complete。缺Key、ENV或局部构建通过均不代表任务完成。

## Key到位后的真实运行检查点

Key通过隐藏输入保存于仓库外0700目录内的0600临时文件，只把文件路径传给测试。没有写入源码、命令参数或日志。真实App场景只在自己的隔离数据库中加载凭据，结束后删除整份隔离数据目录及SQLite侧文件；用户常驻App未被终止。测试Key仍用于后续尚未完成的已授权验收。

| 实际运行 | 结果和限制 | 日志 |
| --- | --- | --- |
| 10秒真实问候 | 非零PCM 262400字节、助手字幕11字、最终12秒，退出0；无物理设备。 | `live-real-cloud-smoke.log` |
| 25秒数字双工首轮 | 中文“停一下”被识别为“听一下”，插话语义断言失败；保留失败样本。 | `live-real-cloud-duplex.log` |
| 25秒数字双工第二轮 | 固定“请停止数数。现在只回答苹果”，真实client delegation 1次；输出期间非零输入146帧，助手实际回答苹果；最终33秒，退出0。未执行项目操作。 | `live-real-cloud-duplex-v2.log` |
| 真实App通话 | `voice-live-call`退出0，1/1场景、3步、16.69秒；麦克风/AEC启动、静音释放、恢复重新采集、最终7秒。 | `desktop-live-cloud-call-v3.log` |
| 10分钟协议稳定性 | 测试退出0，3个不同session；首会话本地600秒、服务端最终821秒，mute/unmute ACK 2个，上行最高3200字节=100ms；两个1秒重开会话各报告0秒。用量差异未解决，不能将AC18整体判通过。 | `live-real-cloud-stability.log` |
| 稳态进程资源 | 独立协议测试进程每30秒记录RSS/CPU；已观察RSS约10–13MB，无持续单调增长。不是原生App音频资源稳态结论。 | `live-real-cloud-stability-resources.jsonl` |
| 独立原始WS时钟诊断 | 不经过LiveConnection、AudioPacer或session解析器；16k和24k各发送500帧=10秒PCM，本地约10秒，均报告12秒；resolved format与请求一致。仍未解释长会话821秒。 | `live-real-proxy-wire-clock.log` |
| 直连与资源查询 | 不走环境代理的原始TLS连接10秒超时；通过现有代理升级成功。对上述store=false会话查询元数据均404，未取得额外用量信息。 | `live-real-direct-clock.log`、`live-cloud-session-metadata.json` |

真实App最新已看图的证据：`artifacts/desktop-workspace-e2e/2026-10-18T05-48-07Z/`。三图均逐张打开：`01-live-connected.png`通话/挂断/静音清晰，`02-live-muted.png`有勾号和取消静音文字，`03-live-closed.png`恢复连接按钮并显示最终7.00秒；无Key明文。测试后二进制日志发现一次正常挂断NotReady竞态，因此这次窗口通过不代表关闭流程已完全通过。修复后的源码需要再构建和重跑。

该场景前两次失败分别为OCR把连接标题当按钮、把勾号识别为文字导致找不到取消静音按钮；均修复测试定位器后重跑，未据此修改产品行为。

当前物理设备为DJI蓝牙麦克风输入、Mac mini内置扬声器输出，音量55且未静音。用户已确认DJI麦克风可用；耳机路由仍未确认。没有把数字注入或麦克风Ready写成声学通过。

## 委派与落账接线检查点

条件路径为LiveBridge→NormaSession::turn_delegated→NormaToolRuntime授权→LiveDurableAdapter→MemoryAdapter→GPT专属pump→真实桌面执行。pump重新核对项目、generation、revision、原话及cancellation，不借用Doubao来源或删除豁免。无用户原文时保持Pending；共享VAD只消费实际成功上传的PCM，用于立即冻结旧规划，不用本地样本数猜服务端坐标。新语音后必须新增用户证据才重评。

已有新增聚焦运行：`app-live-bridge-wired-tests.log`和`app-live-upload-vad-green.log`均32项通过。覆盖真实画布只新增一张Browser、取消和修订后零新增；副作用后调用方被中断仍独立落账，重启且换call/revision仍返回原node；部分副作用Failed必须触发unknown闸门并阻止新step；边界缺失不claim，accepted不被改成完成。

独立复核发现的完成快照竞态、多Agent unknown停止缺口、挂断取消延迟和关闭过滤过宽已修复。聚焦RED/GREEN、域层379项和App34项通过，域层Clippy、App检查/构建及机械约束均退出0。该轮结果不代替后续observed改动的验证。


## 最新通话复验与时轴诊断

`desktop-live-cloud-call-v5.log`退出0，1/1场景、3步，60.47秒。证据为 `artifacts/desktop-workspace-e2e/2026-10-18T06-30-20Z/`。连接、静音、恢复、挂断均经过真实窗口；资源释放日志为 `seconds=10.0 finalized=true error=""`。隔离凭据数据库目录已删除。

三图均实际打开核对：连接图的通话中、挂断和静音可读；静音图同时有勾号和取消静音文字；关闭图恢复连接按钮、静音禁用并显示最终10.00秒。均无Key明文。这次运行使用关闭分类修复后的二进制，未包含后续observed源码增量。

前次v4在进入连接前遇到macOS Vision `e5rtError(...,13)`，驱动未识别欢迎页。没有把该结果作为通话失败证据；保留原报告，后次完整复跑通过。

独立原始WS的60秒补充诊断：16k发送2997帧=59.94秒，服务端报告77秒；24k发送2998帧=59.96秒，报告81秒。本地各约60秒，resolved format匹配。见 `live-real-proxy-wire-clock-60s.log`。它绕过产品调度和解析，说明差异也存在于独立发送器；原因及账单仍待核对，不据此追加无必要长收费测试。

## Observed委派修订检查点

官方复核未找到转写完成fence。新增`prepare_observed`从已观察双向原文准备语义候选，offset仅作关联。稳定intent由call身份与首个未消费原文event的SHA256组成，late更早时间戳、revision及新delegation不改变它。

VAD取消后，完全相同的旧证据不能再次认领；新增用户片段才允许重评。已观察撤回只保留后续明确子句；载荷或引号内的更正词不能变成别的动作授权。更正过的直接寻址交回语义规划，不能仍把旧任务发送Agent。WorkspaceMemory::Write也必须有明确记忆授权。

只读查询不进入副作用账本。旧规划被补字取消且只有只读回执时释放认领，保留完整写授权；pending或可能有副作用时仍封存并核对。未消费原文被裁剪时不启动规划，只请用户完整重述；来源已驱逐的快照不能claim。相同时间戳或inflight钉住导致窗口不能安全裁剪时明确Capacity关闭，保持有界，不静默丢证据继续写入。

聚焦证据：`live-observed-context-{red,green}.log`、`live-observed-history-{red,green}.log`、`live-source-prune-red.log`、`live-observed-context-final-green.log`；授权证据为`agent-live-{correction,payload-correction,addressed-correction,workspace-memory}-red.log`和`agent-live-observed-authorization-green.log`（14项通过）。App初轮`app-live-observed-bridge-green.log`为36项通过；后续完整回归以最新日志为准。

这些检查只约束已观察证据，不保证尚未到达的更正为零。真实云端项目工具、unknown核对恢复、全部交互、物理耳机/外放与最终workspace/release检查仍未完成。原自动Goal已由用户清除。


最新本地回归：`domain-observed-final-tests.log`退出0，387过、11 ignored；`app-observed-final-tests.log`退出0，36过。引用分句修复后完整Agent重跑见`agent-observed-quoted-final-tests.log`，域层`domain-observed-final-clippy-green.log`、App `app-observed-final-{check,build}.log`均退出0。Clippy首次needless_borrow已修正；格式检查和diff检查首次发现文件尾多余空行，移除后`observed-format-diff-green.log`两项退出0。其余机械护栏见`observed-mechanized-checks.log`，均退出0。

独立reviewer已复核引用分句、多Agent unknown、即时取消、完成快照、挂断分类和来源驱逐修复，本轮限定范围内未发现新的阻断。主Agent核实最新App build已完成。上述结果不是G6的workspace/release验收，也不是云端项目工具或声学验收。


## 物理声学与桌面字幕检查点

用户已确认DJI麦克风可用。系统实际路由为`DJI Mic Mini-6963B0`输入、`Mac mini扬声器`输出；设备快照为`logs/acoustic-audio-route.json`。耳机输出尚未确认，不能计入耳机通过。

新增`voice-live-acoustic`真实App场景：固定离线语音由`afplay`经过真实扬声器与麦克风，不向App或WS注入PCM；真人模式用`NORMA_LIVE_ACOUSTIC_HUMAN=1`显式启用。两者分别检查“你:”用户原文与“Norma:”回复，播放进程必须正常退出，不保存麦克风录音。测试后删除整份隔离数据目录；失败路径也尝试主动挂断并记录未确认情况。

真实场景v4/v5发现工作空间Host漏绘语音字幕。先保留RED截图，再在`workspace/desktop/shell.rs`两个Dock分支复用现有字幕组件。project回调绑定原project，overview绑定Host；嵌入Canvas不再重复绘制。Apple Design落地为沿用原生层级、已有全文/收起控件与即时反馈，不新增动效。`app-live-caption-tests.log`的Dock回归1项通过，`app-live-caption-{build,check}.log`均退出0。

v6已显示字幕，脚本因只等待“全文”而未接受默认已展开的“收起”失败；随后修正匹配器。v7实际进入固定扬声器激励，通话持续正常但没有用户转写，退出3记ENV，不能据此确定是AEC消除还是物理路由。服务端最终36秒，资源释放`finalized=true error=""`。已停止自动声学连跑，等待用户确认可读固定真人语句；未把这次记为声学通过。

v7证据：`artifacts/desktop-workspace-e2e/2026-10-18T07-03-21Z/`。三图均实际打开：`01-acoustic-ready.png`一份字幕位于Dock上方，通话/聆听/收起清晰；`02-acoustic-input-missing.png`仍无用户原文，不能判通过；`03-acoustic-closed.png`连接恢复、静音禁用、最终36.00秒，无Key明文。

![真实桌面的GPT-Live字幕](/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T07-03-21Z/01-acoustic-ready.png)

更早的v1/v3仅在工作空间名称输入处失败，未连接云端；声学场景随后改用隔离工作空间/项目夹具作为前置条件，不宣称完成工作空间创建流程。v2重启首帧截图失败，原脚手架将所有截图失败误标权限ENV；现先检查截图权限和锁屏，真实环境失效立即停，权限有效的未就绪窗口由既有wait重新取帧。没有换截图通道或跳过权限检查。独立reviewer已核对清理、真假输入与渲染层级，未见该增量新高风险问题。

## 服务端用量复核

只读审查确认PCM计算和产品解析没有加总或换算错误。原始WS两次用量77+81=158秒，而包含所有握手/关闭的整条测试仅134.66秒；十分钟测试首会话821秒，整条三会话测试总619.70秒。启动/关闭开销不能单独解释差异。

官方未给出输入输出分别加总、time_factor或代理导致此幅度差异的证据，不能反推系数修改原始值。当前只确认已保存服务端最终用量，不确认计量准确或账单错收。AC18保持待核对，不追加无必要长收费测试。

后台前置补充：本机`127.0.0.1:11434`当前没有Ollama服务。真实项目工具验收须另行配置可用的Norma后台；这不影响Live闲聊连接。整体Goal继续active。

## 软件闭环补齐与独立审查

用户要求先完成功能，后续统一做真人测试。本轮停止真人通话安排。

- 后台配置、超时、预算与模型错误按类别回传。草案不能作为执行事实。
- 任务ID、状态和真实回执先于正文与草案。总注入最多480 UTF-8字节。
- 无项目和缺后台立即回报。实际VAD状态约束工具准入，闲聊继续可用。
- 最终反馈遇通道背压保留重试，不再静默吞掉。
- 停止与恢复播报独立于麦克风和任务。传输与播放队列均丢弃旧代次音频。
- 支持明确语音口令。否定句与引用不触发播报控制。
- accepted关联稳定task_ref，终态来自持久任务DB。断线保留未ACK事实。
- ACK仅表示服务端接收，不能报告完整播完。查询后重查播报开关与取消。
- 慢数据库工作独立结束，通话停止等待有界，不拖住下一次连接。
- 设置新增只读核对入口，展示原命令、原回执、任务状态和当前画布。
- 核对不改原回执、不自动重做。结果按项目隔离。

离线SSE整链发现项目根接线缺陷：外壳根路径可为空，导致项目会话被当作个人会话。MemoryAdapter改用通话可信项目根后，真实画布创建一张Browser，换owner重放返回原node。该证据不代表真实云端或真实窗口验收。

两位独立reviewer已复核上述问题及修补。限定增量未发现新增阻断。52项聚焦测试见`app-live-completion-final-tests.log`，交付检查以最新日志为准。

workspace首轮已完成1493项，因Homebrew Node缺少动态库，移动端跨语言验签失败。未改该模块或全局Node；采用现有Node 22重跑，结果另记。

## 旧语音设置回归修复

完整App回归暴露模式卡片的百分比宽度问题：外层193px，内部Button却603px，覆盖相邻卡片，真实点击仍留在Legacy。另三张任务卡片宽517/571/543px。

两个中间Block wrapper改为Flex列，使子树按分配宽度布局；不改vendor或点击坐标。外层语音任务卡片统一571px，三个模式按钮均不越自身边界。

新增边界断言先命中RED，再以相同真实点击和DB断言得到GREEN（1项）。滚动fixture通过实体update+notify触发重绘；无效的额外refresh诊断已撤除。独立reviewer核验因果和限定修复，未见新增阻断。

证据：`app-live-mode-layout-diagnostic.log`、`app-live-mode-layout-outer.log`、`app-live-mode-button-diagnostic.log`、`app-live-mode-button-{red,green}.log`。默认App单独测试另发现VAD测试fixture缺dev feature，已仅在dev-dependencies声明，不影响普通产品构建。

## 交付回归结果

| 检查 | 最新结果 | 证据 |
| --- | --- | --- |
| App完整测试 | 1117过、0失败、9 ignored | `app-live-delivery-full-regression.log`，182.33秒 |
| 其他workspace crate及doc tests | 1025过、0失败、27 ignored | `workspace-live-domain-completion-node22-tests.log` |
| 合计 | 2142过、0失败、36 ignored | 两次互补运行，不是单条原子workspace命令 |
| 普通配置GPT聚焦 | 49过、0失败 | `app-live-default-focused-tests.log`；没有启用App的E2E开关 |
| 默认配置单独测试编译 | 退出0 | `app-live-default-test-compile.log` |
| 默认App严格Clippy与开发构建 | 两项退出0 | `app-live-delivery-final-{clippy,build}.log` |
| 10项机械检查 | 全部退出0 | `live-delivery-final-mechanized.log` |

最终设置页复验首次因Vision OCR的e5rt运行时错误中止，没有页面截图。脚本报告PRODUCT；错误只证明系统OCR调用失败，不能归因产品设置逻辑。

第二次在普通构建打开真实设置页，三模式同排，禁用按钮文字偏暗，OCR漏读连接，仍记PRODUCT失败。证据`artifacts/desktop-workspace-e2e/2026-10-18T08-36-35Z/01-live-dark.png`已实际打开：模式不重叠、GPT已选、缺Key提示清晰；禁用标签可读性不够。

仅将禁用标签改为主题次级文字色，组件disabled、无focus/hover与动作gate保持。独立reviewer已核验禁用和键盘路径，无新增阻断。以真实失败作为RED，等待最终构建复验。

为腾出release构建空间，只移除本任务外部`build/app/debug`缓存，先保留普通开发二进制与全部日志。未清理其他worktree或用户App。

## 最终Release与真实设置窗口

`cargo build -p app --release`退出0，9分58秒，保持仓库默认thin LTO与codegen-units=1，没有启用E2E或production-data开关。随后`cargo clippy -p app --release -- -D warnings`退出0，覆盖最终禁用文字颜色增量。上述完整测试在该纯展示增量之前通过；展示增量由最终Release、严格Clippy和真实窗口复验验证。

最终设置场景使用同一Release二进制，复制到规定的`target/debug/norma-os`路径运行。最新执行`2026-10-18T08-49-01Z`退出0，6步全通过，14.64秒，单轮通过率1/1。SQLite模式、Cedar持久化、无凭据引用均由驱动实际断言。日志为`live-delivery-settings-release-desktop-e2e-final.log`。

Vision稳定漏读深色禁用的“停止播报”，5秒重试也失败，但实际PNG中完整可见。该新按钮的存在与禁用外观改由强制逐图评审，不继续为OCR改产品。脚本仍真实点击Connect并检查未连接；不能将此扩称静音/停止播报及其键盘禁用行为均经实际点击验收。独立reviewer已看图并复核这一边界。

关键图均位于`artifacts/desktop-workspace-e2e/2026-10-18T08-49-01Z/`，以下8张已实际打开：

| 图 | 视觉结论 | 实际观察 |
| --- | --- | --- |
| 01-live-dark.png | 符合 | 三模式同排，GPT有勾选，缺Key明确，三个禁用标签完整可见。 |
| 02-live-disabled.png | 符合 | 点击Connect后仍未连接，三个按钮维持禁用外观，无Key明文。 |
| 03-audio-dark.png | 符合 | Cedar有勾选，设备行清晰，核对入口不遮挡。 |
| 03b-review-dark.png | 符合 | 刷新核对与返回主界面可见，无项目提示明确，没有成功回执或自动重做。 |
| 04-live-light.png | 符合 | 浅色GPT模式与缺Key恢复，三个禁用标签可读，无暗色残留。 |
| 05-audio-light.png | 符合 | Cedar重启后仍选中，边框和设备行可辨。 |
| 06-doubao-regression.png | 符合 | 豆包选中，原交互和连接区域可见，GPT配置未混入。 |
| 07-legacy-regression.png | 符合 | 语音指令选中，原识别引导可见，GPT没有高亮。 |

![最终Release核对入口](/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T08-49-01Z/03b-review-dark.png)

最终可运行文件及配套helper位于外部备份目录的`bin/`。`run-gpt-live.command`启动独立持久验收目录，不替换常驻App。源码快照、tracked patch与二进制SHA256由最终manifest索引。真人、云端项目、完整键盘/hover和用量准确性仍按用户安排待验，原自动Goal已由用户清除。

## 用户移除悬浮转写框（后续修订）

本节覆盖此前“真实窗口可见一份字幕”的显示要求。用户最新截图要求取消GPT-Live整块悬浮转写框，内部双向转写继续保留，用于原话关联、更正和工具授权。Dock的通话反馈、设置页的详细错误和其他语音模式字幕不变。Apple Design落地为减少画布遮挡，沿用已有状态反馈。

旧`voice-live-acoustic`依赖浮层OCR，现已在启动App和云端连接前返回ENV，不能继续用它宣称声学通过，也不会先产生云端用量再因找不到字幕失败。历史截图只代表旧版本。新版本的有转写状态由聚焦渲染回归检查，真人听说、插话与回声仍待用户统一验收。

本次定向验证：`live-caption-removal-red.log`中新增隐藏用例先实际失败；`live-caption-removal-green.log`同一用例通过。最终`cargo test -p app --release --lib -- gpt_live voice_caption feature_guide --test-threads=1`为71过、0失败、0 ignored（筛选有交集，不与单测1项相加）。`live-caption-check.log`和`live-caption-clippy.log`均退出0。测试通过App实际渲染入口检查有转写状态返回None，同时核对双向原文、通话令牌、旧模式、Dock错误、设置详细原因，以及通话中/失败态的功能引导阻断。该证据是隔离App回归，不冒充真人通话窗口验收。

`voice-live-acoustic`阻断已通过真实驱动确认：`2026-10-18T11-51-03Z/report.json`为ENV，0步、0张截图，场景未启动App或连接云端。HTML架构对比位于`docs/design-docs/2026-10-18-gpt-live-vs-doubao.html`；8个主题、2条链路图，STE检查0警告。

最终Release构建退出0（`live-caption-build.log`）。已更新外部`bin/`及worktree的desktop-e2e二进制，旧版保留于`preserved/before-caption-removal/`，用户验收数据保留。新测试进程PID为76138；激活请求成功，但CG查询未观察到可见窗口，因此不将其记为前台窗口验收。

本次真实设置回归两次整体均未通过（0/2），不复用旧版通过结果：

| 运行 | 结果与证据 |
| --- | --- |
| `2026-10-18T12-11-06Z` | 退出1，驱动记PRODUCT，38.66秒；首步因Vision `e5rtError`失败，0截图。App日志无崩溃；独立Vision探针随后识别此前图片成功。保留原报告，不擅自改判ENV。 |
| `2026-10-18T12-13-27Z` | 退出1，驱动记PRODUCT，32.75秒；前5/6步通过，最后切回旧模式步骤因窗口25073图像不可用失败；本轮真实旧模式流程未完成。SQLite模式、Cedar持久化、缺Key连接和无项目核对均通过。 |

第二轮6张关键图均已实际打开：`01-live-dark.png`与`02-live-disabled.png`符合（GPT选中、缺Key、三按钮禁用）；`03-audio-dark.png`符合（Cedar勾选、设备与核对区域清晰）；`03b-review-dark.png`符合（无项目提示与工具后台分组可辨）；`04-live-light.png`与`05-audio-light.png`符合（浅色恢复GPT/Cedar与禁用样式）。没有06/07图，不给其视觉通过结论。两次app.log与report.json均保留于对应目录。没有修改产品或测试场景来迎合失败。

![本次真实设置窗口：项目工具入口，未连接通话](/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/.worktrees/gpt-live-duplex/artifacts/desktop-workspace-e2e/2026-10-18T12-13-27Z/03b-review-dark.png)
