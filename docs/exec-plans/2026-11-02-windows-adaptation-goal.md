# Windows x64 适配 Goal

状态：ACTIVE。2026-11-02 用户要求重新规划，并将测试放到 GitHub runner。
本计划接替此前按编译错误逐轮推进的安排；历史证据继续保留。

## 目标与交付

完成当前默认产品能力的 Windows x64 原生适配，修复 PR #170，正常提交并推送代码，交付经过验证、可下载的 Windows 候选包。首个发行目标为 Windows 11 x64；测试执行环境使用标准 GitHub hosted `windows-2025`。Runner 的 Windows Server 2025 结果不写成 Windows 11 用户设备实测。

最终交付包含同一源码版本的便携 ZIP、SHA-256、逐文件 manifest、能力说明及 producer/consumer 两个 job 的运行证据。便携包保留主程序、三个 helper、必要 DLL 与 Resources。签名安装器和公开 Release 需要各自的发布条件，当前不创建公开 Release。

所有功能测试、Clippy、默认/release 构建和真实 App 验收在 GitHub runner 执行。macOS 回归使用 GitHub macOS runner。本地负责编辑、读取日志和整理证据，不主动运行测试、Clippy 或构建。唯一的本地检查例外是正常 `git commit` 自动执行的既有 hook（含 Rust 改动时的 `cargo check --workspace` 与格式检查）；它属于仓库强制提交完整性要求，不关闭、不替换，也不将其当成 Windows 验收证据。

## 当前事实与检查点

- Mac完整回归已GREEN：`02942cb1` run `41583624505`、attempt 1，完整App 1739过/0败/10既有ignored（212.03秒），加强后的原scope读失败与Failed期间poll重试测试另连续10次通过。没有新增ignore或放宽5秒/同delivery/原Hook唯一响应要求；这是收窄夹具后的通过，不反推旧活SQLite替换时那一次具体错误已唯一定位。日志`/private/tmp/norma-mac-full-green-02942cb1`；artifact `13207546973` SHA256 `e780118274adc51b6e59a1dc69dcf8e0aeeaae0b90d368d8e0484d496af2fac7`。后续退出快捷键改动不在这个全套基准中。
- Quit注册表GREEN：`2884b587` run `41584360020`两平台PASS（Windows7/0/0，Mac6/0/0），Windows默认Ctrl+Shift+Q、正式录制与恢复均通过；Mac保留Cmd+Q。日志`/private/tmp/norma-quit-registry-green-2884b587`。Win artifact `13206744957` SHA256 `0e9e7340ac4905a6b6c080f75d75307b44c654d1a539bddd1ddea46a1b3e3e03`；Mac `13206997892` SHA256 `53e546a83c522a89ba928fbbe05e454c90de05f85d388b1768fa7990bdd488f2`。
- Quit仍有已确认静态反例：InputState键图先于root capture消费Ctrl+A，录制为退出键后在输入框内无效。`025bb77f`/`286f1d9a`补真实Root/InputState聚焦与正式GPUI派发反例（Mac用Cmd+A仅注入路由策略）；当前尚未改成pre-binding interceptor，等待RED。独立审查确认实际焦点、键图与全局回调读实体的前提成立。现有`App::intercept_keystrokes`可在键图前运行，但`KeystrokeEvent`不携带is_held；后续须同时保留长按抑制，不可从单次键测试推断重复键已覆盖。
- 真正App默认Quit验收`7706fed2` run `41584529677`进行中：正常WM_CLOSE、Ctrl+Shift+Q、请求Settings后Ctrl+Shift+Q三次独立进程；只用原生键盘输入，无退出失败后的WM_CLOSE fallback。Settings原图尚需看图确认，不把“发送了打开Settings键”先写成已进入设置页。不是发行包验收，也未验证自定义键、后台服务和PTY退出树。
- App-only Clippy诊断run `41583956356`（`23453202`）仍进行中，原严格依赖门禁保留。此前native退出策略GREEN artifact `13206434902` SHA256 `2935cb772582d187d813de20d445a8bdb4c0439e631796831fe1ac3410dc182d`；旧Mac10次诊断artifact `13206870035` SHA256 `9bb5e6c2ee40d1ce773c121d1e9aa5369d3627634233a10bd7edf0671d66f96c`；Windows11编译错误artifact `13206761290` SHA256 `71d44351386a071cae31cb4abceb4a617b5ea87e62a3ee85eaefb0169c722c54`。均为日志archive，不是候选ZIP。

- W07平台退出策略GREEN：`3f257cdc` run `41581943206`、attempt 1、job `125738555114`五模式全部exit0。无keeper模式确认close_count=1、drain_finished=true、原生窗口销毁、keeper_present=false、errors=0；原四模式继续PASS。证据`/private/tmp/norma-background-last-close-green-3f257cdc`。仅平台策略通过，Norma尚未启用；单实例重开/ACK、可达Quit和真实服务/PTY保活复用仍未完成。
- 原W07 RED日志artifact `13206177551`的SHA256为`50b40159d7deb877344c720f03aa775db2ecedb3641c4de56e55d5a02c1353e5`（日志archive，不是发行包）。
- Mac诊断`a0714e14` run `41580160481`整体PASS，原测试10次均通过，但没有复现原全套第三次失败，不能将其视为修复。`eaca5a41`把故障改为原scope的真实checkpoint解码失败，先断言无旧state/events、防止覆盖状态；加强具体错误、唯一lease/receipt和非空delivery断言。独立审查另发现原测试只在Submitting显式poll，`02942cb1`补了Failed等待期的生产poll。`eaca5a41`全套run `41583403268`在缓存恢复阶段因缺这项覆盖被新来源取代并取消，非PASS；新来源完整+10次定向run `41583624505`进行中。原全套失败仍未关闭。
- Windows完整App门禁`95fc57ab` run `41581512783`、attempt 1、job `125737198998`已FAIL：App测试harness是11条编译错误（Unix socket/symlink/shell夹具、Mac视频槽和Windows测试Noop sink），尚未执行测试。Clippy先失败于workspace clone runner的Windows未使用metadata，尚未到App本体；补充App-only诊断以读取后续lint，但保留原严格依赖门禁，不能用诊断替代。原始日志`/private/tmp/norma-windows-app-gates-95fc57ab`。
- Windows可配置退出入口RED测试已提交`69200ded`，run `41582834327`进行中：生产registry必须包含`quit_norma_os`、默认Ctrl+Shift+Q、支持正式录制与恢复；保留Ctrl+C用于PTY。尚未实现或宣称实际App退出通过。参考apple-design的可发现性、用户控制和熟悉性，复用现有Settings状态反馈；实际App键盘与保存/进程清理链仍待验证。

- W07平台候选`3f257cdc`已正常提交推送，复验run `41581943206`进行中，尚无GREEN。独立只读对抗审查核验默认true、先注销窗口再检查策略及无跨App回调借用；Norma仍未启用此开关。原RED日志在`/private/tmp/norma-background-last-close-red-5b772aa3`。
- Mac诊断run `41580160481`在2026-11-03 05:11 UTC仍为in_progress，job `125732862258`的首个测试步骤自04:49:38未结束；job日志API此时返回BlobNotFound，尚不能区分构建/测试执行。继续等待同一job，不将观察超时当进程终止，也不重启覆盖失败。完整Windows门禁run `41581512783`也仍进行中。

- W07平台策略真实RED：`5b772aa3` run `41580285891`、attempt 1、job `125733259551`的原有四模式全部exit0；唯一新增无keeper的`background-last-close`为exit101，准确失败于`drain_finished`断言。Windows最后窗口自动PostQuit覆盖保活决定已由真实窗口证实；最小候选实现增加默认true的退出策略，仅显式false时继续消息循环，Norma产品仍暂不启用。
- 完整Windows App门禁已补为独立手动workflow，`95fc57ab` run `41581512783`、job `125737198998`运行完整App单测harness及生产App严格Clippy。该run尚未完成，不以独立probe代替全套结果。

- 最新Mac完整App结果：`c11c9ce3` run `41576561435`、attempt 1、job `125721310668`为1738过/1败/10既有ignored（154.81秒）。唯一失败是`question_prepared_lease_survives_poll_and_context_read_failure`第三次恢复后仍Failed；前两次及delivery身份断言已过，不能据此断言poll丢lease。独立只读审查提出活SQLite路径替换可能与额外Tokio初始读取竞争；尚未证实。`a0714e14`仅增强错误/lease诊断，run `41580160481`进行中，不视为修复或重跑覆盖失败。
- W07先补无keeper的真实`background-last-close`反例：窗口关闭后必须在零窗口状态执行100毫秒后的UI任务，再显式quit。新增平台策略API暂为默认no-op以保留既有行为，等待Windows runner证明RED；Norma产品尚未启用保活策略，单实例重开/ACK与主动退出入口仍待完成。

- 产品feature下native关闭契约最终GREEN：`dc82c76e` run `41575727720`、attempt 1、job `125718662353` PASS。原生关闭、迟到激活、排队activate/resize/fullscreen后关闭、App移除四模式均exit0；每项明确验证close_count=1、drain_finished=true、原窗及keeper HWND均销毁、errors=0。没有过滤shutdown报错或放宽断言；真实App的同类关闭证据见下条。
- 本批验收日志artifact的GitHub SHA256：native `13204400934`为`a1f3b2667fa69444e1969d9ceb99f617dd3e3acafc6f1cf3b800b29caf84af24`；实际App `13202859951`为`dfde3085d2ff748646965522327dbe9bcc0e1a093ccfbd77434537ffde8950fc`；Mac定向 `13203083772`为`2ea51a116204271d418aad3325df9a5def1fa7b376b3d3484000a8d77811c58d`。这三个是日志/截图archive校验和，不是可交付Windows发行包的校验和。

- 实际Windows App清理复验GREEN：`8e2a1b3a` startup run `41575429677`、job `125717706162` PASS；真实WebView2 ready约6.57秒，三张client原图已逐张看图，完整欢迎页无先前桌面/任务栏夹带，正常WM_CLOSE退出0，stdout/stderr未出现ERROR或panic。原图/日志在`/private/tmp/norma-startup-clean-8e2a1b3a`。这是开发App启动与关闭证据，不代表实际项目/终端/网页操作、手机后台或同包fresh-runner验收完成。

- Mac六项剩余失败已获得定向GREEN：`f902836c` run `41574377560`、job `125714371971`整体PASS。Browser 2/0/0、桌面光晕2/0/0、Master首帧2/0/0、失活会话1/0/0；原QR/OpenCode/Azure分别1/2/1项继续PASS。无新增ignore，原业务断言保留；全套新源复验已dispatch `c11c9ce3` run `41576561435`，未提前写成完整App PASS。

- 产品feature锁冻结：`8e2a1b3a` run `41575256058`，Windows/macOS的`cargo update --workspace`均只移除测试feature专用依赖、升级0包，product dependency identity检查均PASS；两端Cargo.lock逐字节一致，SHA256 `41f79d5e1da16f78301970737c4fd1bfe5a35549d49dbc679c75ed59a42e7feb`。`dc82c76e`提交相同锁文件；新的native run `41575727720`及shortcut run `41575727697`进行中。`a7a23002`的失败是旧锁与feature不匹配，未到行为断言，不冒充产品功能失败或成功。
- 实际App复验已dispatch `8e2a1b3a` startup run `41575429677`：采用产品feature和修正后的client捕获，正常关闭后拒绝ERROR/panic日志；当前进行中，不以最小GPUI探针代替App验收。Mac剩余回归run `41574377560`（`f902836c`）也仍在执行。
- W07确定缺口：App层`on_window_closed`已按mobile/后台项目决定是否退出，但WindowsPlatform最后原生窗口关闭仍直接PostQuit，覆盖了保活决定。Windows `on_reopen`只存回调、未驱动，`activate`为空；第二实例endpoint失败后仍继续启动。固定Cmd+Q在Windows为Win+Q，主动Quit入口不可达。不能只取消自动退出而制造无法重开的无窗进程；后续需同时完成App接管最后关窗决策、单实例reopen/ACK、原宿主/Runtime复用及可达Quit入口，并以真实服务/PTY跨关窗保活→重开同会话→主动退出验证。当前四模式native关闭契约不等于W07已通过。

- `61a6e64b` combined shortcut run `41568535811`两平台均6/0/0 PASS；Ctrl+V focused/selection、默认registry和badge策略/提示均通过。实际App键盘/重绘/焦点验收仍待执行。
- native窗口真实RED：`67b2086c` run `41570402448`三模式都已通过close计数、明确drain和两个原HWND销毁前提，最后在errors断言失败。日志精确定位`window.rs:1130`到`handle_activate_msg`迟到任务，及Windows `window.rs:510/511`重复Revoke/Destroy。`1c6db17d` run `41571554479`补第四模式queued activate/resize/fullscreen后同步关闭，实际捕到旧HWND操作失败。
- `ef748246`/`e68e2e84`实现逻辑脱离、原生销毁、拖放注册及平台注册各自所有权；先撤销callbacks/IME handler，在借用外drop和COM/native清理。异步操作执行前、回调后及默认proc续执行前校验closed，WM_DESTROY仍完成平台注销；构造失败的未登记窗口不发平台注销。静态独立审查未见前述反例残留；构造失败注入尚未验证。
- `274fbf50` run `41572369133`四模式原三类native错误均不再出现，但全部仍因GPUI shutdown报错失败。查明独立probe误启用`test-support`，`executor.rs`遇真实dispatcher会直接返回Err（连空future也如此）。`f902836c`去掉该feature以匹配产品；没有过滤错误或放松断言。产品features下的最终native GREEN尚待执行。
- Mac完整App `f24b9d08` run `41568796539`：1732过/6败/10既有ignored，256.37秒；QR/OpenCode/Azure继续分别1/2/1项PASS。剩余6项分为过时源文本断言、2项丢失根TestAppContext所有权、2项自指宿主Entity、及豆包假凭据后台Disconnected与失活测试竞争。`9057dd0f`保留fixture根owner、独立宿主、生产失活绑定，并新增真正BrowserStore队列→App→Runtime选中/删除迟到事件断言替代旧处理代码字符串；定向复验待执行，尚未声称Mac全绿。

- 最新启动结果：`6183569d` startup run `41566226993`、job `125688326459` PASS。真实WebView2 native创建完成（约6.64秒）；三张原始PNG已逐张看图，欢迎页实际渲染；WM_CLOSE退出码0。截图存在初始窗口越界、夹带桌面的问题，不能宣称整窗布局验收通过。关闭日志仍有`window not found`和两条invalid-HWND，独立审查定位到既有GPUI重复销毁及迟到active回调候选；须增加无Wry真实窗口反例后修复，不把exit0写成无错误清理。
- 剪贴板独立RED：`923ab1fe` run `41566913219` Windows 2过/2败，focused粘贴与keyboard-selected外层准入分别失败；Mac 3过/1败，额外修饰组合被误识别。`8ef9b815` run `41567873627`中两平台的两条clipboard均GREEN；整个Windows job仍因两条badge RED失败（4过/2败），Mac为6/0/0。不可将该run整体称PASS。
- Badge独立RED：`c4b5521d` run `41567577876` Windows实际失败于Alt不显示卡片序号、提示仍为macOS符号。`61a6e64b`修默认平台策略、完整Modifiers快照触发重绘，以及picker内实际支持的裸数字提示；combined run `41568535811`进行中。提示继续遵守文档中“仅默认键位”的既有范围；真实Alt重绘、焦点和完整键盘路径尚待App验收。
- 截图driver RED `93313269` run `41568237185`：640×360 client被旧函数捕为656×399。`682acc36` run `41568614384`整体PASS，真实WinForms client尺寸和越界拒绝两项均通过，readiness四项继续PASS。生产driver会先将App放入桌面工作区，再捕获client；修正后的实际App截图待下一轮，不用fixture代替产品截图。
- `f24b9d08`已显式dispatch Mac完整App回归run `41568796539`，复用定向workflow与缓存，先保留QR/OpenCode/Azure，再执行原App全套；当前进行中，Goal仍ACTIVE。Windows App全测试harness的10个Unix/macOS夹具编译问题、完整Windows Clippy、默认能力缺口、PR接回与最终同包验收仍未完成。

- 2026-11-03 UTC 最新增量：`6183569d` script run `41562853503` PASS，readiness会逐个读取stdout/stderr；`50141620`的真实native WebView已完成创建，旧probe仅查stdout导致超时，未获得正常截图/退出证据。修复后startup dispatch `41566226993`正在Windows runner构建，尚不记PASS。
- `e94d9663` shortcut run `41561550263` 两平台PASS：生产registry默认键唯一、Windows不使用Win键、保留PTY控制输入；焦点纯策略run `41561550286`两平台PASS。上述为软件契约，不替代真实窗口焦点/键盘验收；录制与恢复及Ctrl+V两路径仍待补齐。
- GPUI根TestAppContext清理的第四条callback-panic反例修复后，`8f28f047` run `41557240079`为4/0/0 PASS；尚未以整套App回归确认原74项线程耗尽失败全部消除。
- `6183569d` macOS定向run `41562853525`、job `125677266162`全部PASS：QR 1/0/0、OpenCode foreground/history 2/0/0、Azure设置真实滚动后点击1/0/0。原始artifact下载至`/private/tmp/norma-mac-fixes-green-6183569d`。保留生产1500毫秒TTL与原4秒测试时限、原持久化与Provider断言，未新增ignore；不声明整个Mac App已通过。
- clipboard RED候选`64650637`/`923ab1fe`抽取focused与keyboard-selected原有生产判定，并分别检测平台粘贴、外层准入和额外修饰键；runner runs `41566662009`/`41566913219`进行中。独立只读审查确认三个原调用点均已接入，后续须取得独立RED→GREEN及真实程序验收。

- 工作树：`/private/tmp/norma-windows-feasibility-20261101`。
- 分支：`codex/windows-feasibility-20261101`，PR #170。
- PR基准 `c26f0071` 的标准CI run `41536051278` PASS；Windows build run `41536039634` 的默认check、browser/helper及App/norma release均PASS，但最后媒体准备失败，没有候选ZIP。真实失败为Windows checkout把可信README的1951字节变为1983字节（32个CRLF），不是编译或哈希保护错误。
- 上一轮 `7a1a1c4c` 的默认App check PASS，build run `41530644679` 在过滤browser单测前编译整个App测试harness时失败：10条Unix/macOS专用测试夹具编译错误。`c26f0071`把同一生产browser模块及原四项测试接入既有native probe，保留真实storage依赖和锁定版本，等待复验；不宣称整个Windows App单测harness已迁移。
- Windows原生终端、helper、文件/附件与传输契约已有通过证据，见后续记录；真实App用户流程、WebView2及其余平台能力仍未完成。
- follow-up分支 `codex/windows-audio-20261102` 在独立worktree `/private/tmp/norma-windows-audio-20261102` 开发音频；只在验证完成后接回PR，避免持续取消长构建。
- `5399afbd` run `41534211348`：原生麦克风枚举/权限/失效选择3项PASS。`bd4ab0f8` run `41534408042`：新增采集2项RED（仍是unsupported）。`fb66cd17` run `41535530484`：WASAPI实现后5项平台契约、voice_input 24/0/6 PASS；无物理端点，真实录音/AEC为NOT_RUN。
- `f418389a` run `41536602913` Windows job `125589915321`：新增包解析/线程收口4项PASS，voice_input 28/0/6、voice_sherpa 26/0/8；Clippy唯一失败是capture timer可改成while-let，`1b1ac491`已修待复验。忽略数均来自原有模型/硬件测试，没有新增ignore。
- 最新音频绿灯 `5973078a` run `41544919078`：Windows job `125618049728`及macOS job `125618049366`整体PASS。Windows原生平台5/0/0、voice_input 29/0/6、voice_sherpa 26/0/8、生产App播放10/0/0、生产App ASR5/0/0；域层和实际App适配器Clippy均PASS。macOS复验受影响crate及相同生产ASR调用链。物理输入/输出端点为0，真实录音、扬声器及AEC效果仍NOT_RUN。
- 运行时错误不是只测私有wrapper：`9353b105` run `41537434166`实际返回截断文本成功，修复后User/DurationLimit/Cancel/EOF四条通过；`6365cc73` run `41541100661`明确暴露Provider错误被SessionClosed覆盖，`96ff20bc`保留原始结构化错误。capture失败仍需join forward的回归使用受控退出门，错误不能跳过清理。
- 播放ACK/Stop的真实RED：`cd92b9da` run `41542429408` job `125609667207`为7过/2败。`5366d83f`使用零容量ACK交接，并按Stop冻结游标、读取padding、Reset、收口队列的顺序保留实际启动证据；上述5973078a已包含两条绿灯，无放宽断言。独立对抗复核未发现新的确定问题。
- 媒体RED `7f6efff6` script run `41543998643`两项失败：可信README被checkout改字节；catalog报告398字节但实际412字节。`d7b76613`固定可信README为LF，显式UTF-8/LF读写catalog/report，run `41544534254` job `125616762322`两项GREEN，原报告/包验证亦通过。主构建已把资源测试及真实LFS媒体准备移至长构建前；没有改写可信哈希或归一化损坏源。
- 重规划冻结证据仍保留在 `artifacts/pr170-goal-handoff/checkpoint.json` 与 `working-tree.patch`；历史失败不改写为成功。

## 执行顺序和完成门槛

| 阶段 | 工作 | GitHub runner 证据与完成条件 |
|---|---|---|
| G0 Runner 门禁 | 冻结输入；整理已有未提交改动；将规范工具链、日志、超时、缓存、source/run/attempt 校验接入 Actions；修复汇总遗漏与恒定失败 | Windows/macOS 工作流报告实际 rustc 版本；失败、缺报告、来源不符均不能假绿；Canvas 与所有必需原生检查进入汇总；已通过的可行性检查可通过 CI，完整迁移仍单独记录 |
| G1 传输与文件安全 | 关闭背压下的 socket；确认 WinSock 新通知是否适用于 AF_UNIX；完善目录能力与原子替换失败恢复 | 保留原 3 秒关闭契约、真实连续 WouldBlock 与全部 source 句柄释放见证；测试慢端、半关闭、取消、双向拥塞；附件交换/原地 reparse 反例、DACL、替换失败与恢复路径均有聚焦测试 |
| G2 原生终端和 Agent | PowerShell/ConPTY；Codex/OpenCode/Cursor 的实际生产启动包装器；版本判断、同一 TUI、私有 server/relay、原会话和 hook；Windows 进程树所有权 | 无模型 CLI 夹具经实际 controlled prepare 路径启动；真实 Agent 另按 W05/W06 必过；支持/不支持版本、中文空格/单引号路径、resume、提问、取消、恢复均验证；取消和关卡片后按 PID+创建时间确认父子孙全部退出，树外进程存活 |
| G3 完整 App 构建 | 逐个关闭默认产品构建前沿；WebView2、内容编辑/预览、手机网络与 QR、生命周期、资源定位 | 默认 `cargo check -p app` 和默认二进制通过；WebView2 真加载页面、导航和本地资源；内容冲突保护与附件能力通过；保留默认功能和依赖边界 |
| G4 其余默认能力 | 盘点并实现语音输入/输出、模型安装、媒体及 Ego 等 Windows 平台缺口；逐项保留结构化能力状态与双语提示 | 平台实现、真实 API/受控输入回归及能力清单一致；未实现项保持未完成，不能用隐藏入口或删功能关闭 Goal；硬件限制单独给出 ENV/NOT_RUN 和可执行验证入口 |
| G5 真实 App 与回归 | 在 hosted desktop 操作真正 Norma 程序；项目、画布、终端、Agent 夹具、保存重开、退出、手机后台生命周期；双语/主题关键状态 | Windows 的真实窗口、输入、最终状态、截图与进程证据；macOS 受影响测试与必要真实 App 回归通过；GPUI hello-world、TestAppContext、CLI version 均只作补充 |
| G6 候选包交付 | 从通过的同一 SHA 构建 release，打包资源/DLL/helper，上传 artifact，在新 job 下载并验证同一个包 | ZIP/逐文件 hash、必需资源/运行库、source SHA 校验通过；新 job 不重编译，源码位于不同目录；真实启动、关键用户流程、退出及遗留进程检查通过，产出截图与报告后交付 |

G1、G2 的纯协议夹具可以与 G3 的编译调查交错进行；发布包的成功判定必须等必需门槛通过。每轮先取得一个能失败的聚焦验证，再实现并复验原失败；发现新失败时只扩大到有证据的范围。

## Actions 编排

保留普通 PR CI，规范工具链固定 Rust 1.91.1。Windows 工作流按验证职责组织，避免一个长串命令阻塞所有日志：

1. `windows-contracts`：norma、Canvas、存储/文件边界、ConPTY、实际原生 Agent 启动与关闭。
2. `windows-app`：默认 App 编译、App adapter、WebView2 与真实桌面场景。
3. `macos-regression`：受影响 crate、workspace Clippy 和必要真实 App 回归。
4. `live-agents`：用户于2026-11-02明确回复“这个我自己测”，承接真实Claude/Codex模型验收。交付时提供冻结CLI版本、任务/提问/回答/取消/恢复的手动步骤与回执入口，不再等待GitHub模型凭据；生产控制链的无模型runner验收仍由Agent完成。可选受保护environment job仅在用户后续提供环境时运行，不以协议夹具冒充真实模型通过。
5. `package`：必需 job 成功后构建 release、校验资源并上传候选包。
6. `verify-candidate`：独立新 runner 下载同一 artifact，校验、启动和检查最终结果。

工作流必须监听源码、manifest/lock、验证脚本、打包脚本、资源准备及能力说明的变更。已被具体编译错误阻塞的旧轮次可取消，保留取消事实；取消不作为通过记录。缓存按源码/依赖/工具链区分，不能以缓存成功代替执行。

## 已知阻塞，按优先级处理

1. **候选包尚未生成**：资源和音频修复已获聚焦绿灯，接回PR后从同一源码重建。产物上传后在新runner消费同一ZIP并读取原图；窗口检查不能代替核心用户流程。
2. **真实App核心流程未验收**：工作区/项目/终端输入、保存重开、快捷键、双语主题、WebView2及手机后台生命周期均需真正Windows程序证据。源码中的cmd修饰键在GPUI Windows映射Win键，需按Windows键盘语义聚焦复现，不能仅以终端输入编码测试代替App快捷键验收。
3. **其余默认能力矩阵未关闭**：模型真实受控推理、媒体/Ego/移动链路按下表逐项实现与验收。已实现WASAPI软件契约和错误收口，不把无物理设备的API绿灯写成真实录音/播放/AEC效果。

## 默认能力与验收矩阵（G0 冻结）

以下自动化软件门槛要求本轮来源一致的 PASS；FAIL、ENV、NOT_RUN 阻止相应自动化阶段完成。用户已明确承接W05/W06的真实模型调用验收，这两项真实调用记录为USER_VALIDATION_PENDING，交付验收步骤，不要求配置GitHub模型凭据。生产Agent启动和控制实现、无模型runner验证仍不可省略；不能把软件缺口降为设备例外。

| ID | 必需能力 | runner 验收证据 | 当前状态 |
|---|---|---|---|
| W01 | 默认 App/helper、release、依赖边界 | MSVC 默认目标；护栏；macOS CI | FAIL |
| W02 | 文件/附件、DACL、原子替换、恢复与持久化 | 真实 NTFS 并发/reparse 反例；保存重开 | FAIL |
| W03 | PowerShell/ConPTY、中文路径、环境、输入与进程树 | 原生 PTY 及 PID+创建时间；树外 sentinel | NOT_RUN |
| W04 | Codex/OpenCode/Cursor 启动、relay、权限、提问和取消 | 生产 prepare 路径无模型夹具；严格关闭契约 | FAIL |
| W05 | Claude、Codex 各执行真实任务 | 用户在交付包上以自己的账号验收；提供步骤、CLI版本和session/产物记录入口 | USER_VALIDATION_PENDING |
| W06 | 两个真实 Agent 的提问、回答、取消和同会话恢复 | 用户验收真实模型行为；Agent仍完成runner生产控制链和迟到动作反例 | USER_VALIDATION_PENDING |
| W07 | 项目、画布、终端、退出与手机后台生命周期 | 真正 Norma 窗口输入、最终状态、退出树 | NOT_RUN |
| W08 | WebView2、内容编辑/预览、资源和浏览器认证 | 真实页面导航、本地资源、编辑冲突及回调 | NOT_RUN |
| W09 | ASR/VAD、TTS、模型安装、音频软件管线 | 真实引擎与受控 PCM；安装完整性；错误状态 | NOT_RUN |
| W10 | 媒体/视频壁纸、Ego、图形及移动端网络/QR | 真正平台 API、帧推进、连接和权限生命周期 | NOT_RUN |
| W11 | 中英文、亮暗主题、焦点和键盘 | 真实 App 双语/主题；Ctrl+C 中断、截图 | NOT_RUN |
| W12 | 同一候选包可消费且 macOS 无回归 | ZIP/hash/manifest；fresh runner 核心流程；Mac 定向回归 | NOT_RUN |

仅物理麦克风/扬声器、摄像头、蓝牙和实体手机链路可单列设备例外：标准 hosted runner 缺设备时保留 ENV，并交付可执行验证入口。例外只限制物理设备结论，不能替代 W09/W10 的驱动/API 实现、受控输入、资源及生命周期证据；不把 Server 2025 结果写成 Windows 11 实机覆盖。

## 证据与状态

每次记录 source SHA、run ID、attempt、runner OS、实际工具链、命令、退出码、通过/失败/忽略数量、artifact 与 SHA-256。日志下载缺失单独记录，不补写成功状态。详细原始输出放 artifact，仓库保留简短索引和关键诊断，避免大量原始日志膨胀 PR。

`PASS` 表示该门槛真实通过；`FAIL` 表示产品或断言失败；`ENV` 表示运行条件不足；`NOT_RUN` 表示没有执行。可行性 CI 的绿色仅对应其列出的原生技术检查，`goal_complete` 必须与完整验收矩阵一致。

不把图形样例、协议夹具、编译通过或有截图单独等同产品验收。真实模型调用已由用户明确承接，不复制本机凭据、不要求用户在聊天中提交密钥。可选自动化真实模型调用只使用已授权GitHub environment/secrets；人工待验收项准确列出。

## Goal 完成条件

- W01–W04、W07–W12必需软件门槛均PASS；W05/W06相关实现和无模型runner控制验证通过，并提供用户明确承接的真实模型验收步骤。用户未反馈结果时保留USER_VALIDATION_PENDING，不宣称真实模型通过，也不再因缺GitHub模型密钥阻止包交付。
- 所有已确认的数据安全、生命周期、会话/权限边界阻塞均关闭；既有失败不得以重跑或放宽断言替代修复。
- 默认能力适配矩阵逐项有实现与证据；设备相关限制清楚归类，未实现项仍属未完成。
- 正常提交并推送；PR 的状态、描述和证据入口反映最终实现。
- 用户得到可下载的 Windows 包、SHA-256、来源提交、验证链接和准确的剩余限制。

## G0 执行记录

- `999b39a7`：计划冻结及 Canvas 门禁回归。GitHub Windows run `41475394387`，attempt 1，job `125379500444` 执行 `python scripts/test-windows-probe.py`：18 tests，三条 Canvas 反例全部如预期暴露错误（missing/FAIL/PASS+101 均被错误判绿）。原始日志保留在该 job，未在本地执行测试。
- 下一提交将 `canvas_tests` 纳入独立必需门槛；只有本轮明确 PASS 且退出码为 0 才可通过。修复结果等待下一轮 runner，不将红灯提交记成通过。

- `a0616877`：Canvas 汇总修复。Windows run `41475988222`，attempt 1，job `125384598403`：Rust 1.91.1，18 tests 全过；完整 run 仍在执行，不宣称其余原生门槛通过。前一个红灯 run `41475394387` 已取消剩余编译任务，红灯 job 的日志保留。
- `deae23ec`：独立只读审查未发现 Clippy/fixture 候选中的调用或隔离回归；正常 hook 的 check/fmt 已通过，Windows/macOS runner 仍待执行此提交。下一批增加独立原生契约 workflow，保留冲突恢复和关闭原断言，仅增强失败诊断；Windows App 的配对名称改用系统环境名称，移除未受平台条件保护的 Objective-C 引用。

- `2c315b29`：Windows native contracts run `41477762062`，attempt 1，script job `125387736138` 的 report 18 项通过；package 3 项中 1 项失败，确认仅四个 MZ 文件和自洽 manifest、完全缺 Resources 的目录会被旧 verify 错误接受。先修必需资源和实际二进制检查，再继续 CRT/目录清单完整性；native job 尚在执行。

- 原子替换共享句柄修复候选：`NamedTempFile::into_temp_path()` 先关闭可写句柄，仍保留路径的清理/keep 所有权。Microsoft `ReplaceFileW` 文档明确 replacement 打开“不指定 sharing mode”；独立审查未发现本次路径/回滚所有权回归。真实旧错误详情及修复后结果以 runner 为准，不以 API 文档代替测试。文档来源：https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew 。
- 增加 GitHub macOS 定向域层与 App 单测回归，固定 Rust 1.91.1；并保留当前原生诊断 run 完成，后续推送不再自动取消它。两套 runner 的编译及测试不在本地重跑。

## G1/G2 最新诊断（`2c315b29`）

Windows native run `41477762062`，attempt 1，job `125387736616` 已完成失败：

- workspace lib 23 过/1 败：冲突恢复明确为 OS error 32（sharing violation）；`content_platform` 同样失败。Python launcher/专属 Job 子孙回收本轮通过，不能据单轮结果宣称既有时序问题永久消失。
- norma lib 30 过/1 败，严格 3 秒关闭仍失败；独立 `codex_control_shutdown` 也失败（方向=true，关闭写端=true），前轮通过不覆盖本轮失败。
- 双向非零写探测导致 AF_UNIX 原未读内容消失并返回 10054，数据完整性断言确实失败。因此该探测被撤回，不能用于产品“检测关闭”；恢复只读诊断及原正文完整性断言。
- 原始 ConPTY 测试未像App处理协议查询，10秒超时。portable-pty 0.9 的 Windows实现启用了 INHERIT_CURSOR；改为真正 TerminalSession 的 drain/Grid/DSR 路径验证，仍保留原10秒、可见唯一标记和正常退出断言，不新增生产绕过。
- `52a3aed5` 的 sharing violation 修复已正常提交；将与上述夹具修正及 CRT 必需门槛红灯测试一起推送复验。恢复文件 keep 失败风险、严格 socket 关闭及真实 Agent 仍未关闭。

- `bbaacab9` script job `125403140004`：report 18 项仍通过，package 正向夹具暴露一个构造错误（误把嵌套的扩展 manifest 也排除）；本轮尚不能据此证明 CRT 反例。先修夹具，仅排除根 manifest，再复验 CRT 反例。为保证脚本 RED→GREEN 不等待长 Rust/App 回归，把小型脚本检查移至独立 Windows workflow；原生和 Mac 验收仍独立保留。
- 本地临时文件创建曾报磁盘空间不足；检查时可用空间已恢复到 7.9 GiB，未删除代码、缓存或证据，测试已用 apply_patch 写入。正常 hook 已执行，其余测试仍只在 runner 运行。

## 修复后的聚焦结果与后续红灯

- `bbaacab9` 的 native job `125403139978`：普通冲突恢复和 `content_platform` 均通过，sharing violation 修复获真实 Windows 绿灯；只读 socket 诊断的正文完整性恢复通过。
- 同轮 ACL 回归真实触发 keep 的 OS error 5，验证了恢复清理缺口；候选修复在冲突/部分替换后禁自动清理，并以结构化恢复路径返回 keep 错误。
- 同轮 TerminalSession 已收到 PowerShell 可见标记，但状态仍 Running：DSR修正仅解决了启动，ConPTY 的进程退出/输出EOF仍有生命周期缺口。保留10秒与正常退出断言，不能记为通过。Python进程夹具本轮也失败（1.56秒时连启动标记都未生成），仍需消除启动速度对清理断言的干扰。
- `0edc0149`，script run `41482927165`，job `125405674109`：报告18项通过；包正向及Resources反例通过，CRT缺失反例单独失败（5项中1败）。新增三项MSVC CRT文件作为必需清单，等待下一轮。
- Windows 与 macOS 长测试改用各自的job并发锁，避免Mac完整App测试阻塞Windows原生反馈；已启动的验收保留完成。脚本检查继续独立Windows runner。

## ConPTY 生命周期回归

- `863a40b9` script run `41484967761`，job `125412767292`：报告18项和包5项通过。native job `125412769903` 仍有文件ACL角色断言、严格socket、ConPTY退出红灯；不得把脚本绿灯当全量通过。
- `efc48392` native run `41486346012`，job `125417589042`：正常退出与1024条记录/唯一尾部/exit7两个真实TerminalSession测试均收到正文，但10秒后仍Running。
- Windows候选实现独立后台child.wait，锁外关闭ConPTY，reader排空后发布退出码；有界发送可由取消信号打断，关闭时不在前台等待输出。新增旧关闭语义的Windows2022 runner回归，主平台仍Windows2025。
- Microsoft文档确认build26100(24H2)以前ClosePseudoConsole可能无限等待未关闭/未排空输出端；portable-pty0.9隐藏原读端的析构顺序需专门反例。引入同版本原始vendor及真实满管道/reader释放测试，先记录旧顺序结果，再改最小顺序；不升级依赖或缩减Windows11范围。
- ACL夹具已确认同时阻止rollback。`SaveConflictRecovery`契约允许目标为draft、旧版本在recovery；现测试错误地假定已恢复。将区分rollback失败与仅keep失败两个严格场景，并补准确的双语副本角色提示。

- `35d296ec` native run `41491899815`：TerminalSession 正常退出/1024行尾部+exit7两个案例在 Windows2022 job `125436762496`、Windows2025 job `125436762939`、主原生job `125436763338` 均真实通过（0 ignored）。ConPTY常规生命周期修复获得绿灯，不能延伸成取消/整树验收。
- vendor回归原命令被Cargo拒绝：excluded依赖包含dev-dependencies，不能由root `-p`测试。实际未执行，不计成旧析构顺序红灯；改用独立manifest及原始vendor lockfile。退出码/实际test result双重门禁拒绝了空测试假绿。

## 退出事件与恢复副本角色回归

- `4c9a86ae` native run `41493369517`，job `125441857509`：真实ACL的rollback失败分支通过；“已回滚、仅keep失败”独立案例按预期暴露Recovery/Restored状态混淆。ConPTY两条正常路径继续通过，新kill案例失败为Disconnected，证明已安装PTY不能取消Exited转发。
- 候选修正 `0d5221a5` 保留正常kill的事件转发；构造失败仍可停止有界发送并排空。Restored显式携带可选warning，副本角色和磁盘状态不再被keep元数据错误覆盖；独立审查未发现新数据保留反例。双语展示测试故意先保留红灯，随后补准确文案。
- vendor独立manifest已启动编译，但fixture的`Box<dyn MasterPty + Send>`不能直接downcast；已在同候选中显式擦除冗余Send标记。尚无真实满管道反例结果，不能记为旧析构顺序红灯。
- macOS旧source `bbaacab9` job `125403139224`：受影响域层1469 passed /0 failed /19既有ignored；App 1659 passed /76 failed /10既有ignored。App包含Vision VM的E5RT输出污染及后续线程资源耗尽，不能据此宣称Mac全绿。按差异补独立文案边界测试，单测opt-level0及workspace缓存缩短编译；release仍单独验收。

## 真正的旧API反例与基础CI结果

- `4c9a86ae` CI run `41493382935`、`796cc43c` CI run `41496616544` 均通过，正常check/fmt/Clippy和既有护栏已获得GitHub证据；不代表Windows功能与发布验收全绿。
- `796cc43c` native run `41496606263`，job `125453090768`：workspace lib 26/0/0，content_platform 1/0/0，Review 1/0/0，scoped directory 4/0/0；ConPTY三个真实案例均通过（启动退出、1024行尾部+exit7、主动kill返回真实Exited）。严格socket关闭仍失败。
- 同run Windows2022 job `125453090817` 的vendor反例确实执行，满管道、外部reader释放、写入者存活前提均满足，master销毁3秒未返回而失败；Windows2025 job `125453090682` 同反例1项通过、产品PTY3项通过。修复将隐藏原读端排在con之前析构，保持正常外部reader排空；下一轮必须同时通过两平台，不能用较新API的绿灯覆盖旧API红灯。

## ConPTY 双平台复验及整树反例

- `9931eadd` native run `41500959311`：Windows2022 job `125467996763` 与 Windows2025 job `125467997460` 均通过真实满管道关闭1项、产品PTY3项，全部0 ignored。前一轮2025失败发生在启动握手前提；夹具改为逐个响应真实DSR查询，保留10秒启动和3秒关闭预算，未修改产品关闭断言。
- 同源socket诊断2项通过：source/receiver复制句柄及128KiB/4MiB正文的8个组合均保留全部正文并观测到HUP。零长send、WSASend及peer-state ioctl不能区分连接状态，不能作为产品关闭判据。真实relay原3秒关闭仍失败；尚未定位普通pair与真实relay之间的关键差异。
- 同源workspace 25过/1败：文件恢复两条DACL反例继续通过，失败为Review Python launcher未在1.5秒内生成启动标记。此前单轮全过不能覆盖本轮这个既有时序问题。
- `b13fdadf` 已补Restored warning的准确中英文展示，并复用现有GPT-Live翻译key。run `41500714761` job `125467156521` 的i18n检查通过，App测试在Sherpa原生静态库缺失时编译失败，未到断言。`c81432cc` 在runner缓存中显式保存原生预构建库，并重新执行该依赖build script；修复后的App测试仍待结果。
- `c3b3599d` run `41501997882` 两平台的新增进程树测试真实红灯：root已退出，脱离控制台的middle/leaf在3秒后均存活。独立只读审查指出预算必须包含kill调用、等待API失败不得当退出、失败采集后代身份也要准确清理；已在`e2eb5e21`补强。
- `e2eb5e21` run `41502962168`，Windows2022 job `125474882146`：3项中1过/2败，主动kill及自然root exit7均保留两个存活后代。自然退出码7断言通过，后代清理断言失败。下一补丁使用非继承私有Job，在CREATE_SUSPENDED期间绑定后才Resume；主动取消及自然root完成均收口后代，后台等ActiveProcesses归零并保留root退出码。等待两平台复验，不能将此候选记作W03完成。

W01–W12的整体门槛仍未全部满足；没有生成候选包或同包新runner消费证据，Goal保持active。真实模型验收现由用户承接，按上表保留人工待验收。

## 整树修复绿灯与用户验收分工

- `ef9eb454` run `41503576948`：Windows2022 job `125476999072`、Windows2025 job `125476999364` 的真实满管道1项、PTY3项、进程树3项全部通过，0 ignored。两条实际树测试覆盖主动取消、自然root退出及退出码7，树外sentinel存活；独立审查未发现本次API或所有权阻塞，Future取消的上游等待线程行为单独保留说明。
- `c81432cc` run `41501587650`，macOS job `125470144921`：i18n护栏及真实App文案边界1项通过，0 ignored。该差异的展示实现和catalog后续未改变，可复用此证据；不是实际GUI验收。
- `20d775f9` run `41504359152`，Windows job `125479704697`：workspace lib 27/0/0，content/review/scoped directory分别1/0/0、1/0/0、4/0/0。Review夹具使用同一个原生Rust测试exe派生真实后代，保留原1.5秒deadline及回收断言，消除Python冷启动这个无关前提。严格relay关闭仍失败，整个native job没有通过。
- `fb8a2d05`实验把通知注册句柄从标准OwnedSocket clone改为uds_windows clone，真实relay严格关闭仍失败（run `41504024395`，job `125478533916`）；该候选已在`9f1c0152`撤回，不能记为修复。
- `9f1c0152` run `41505010677`，socket job `125481956397`：诊断2项通过，实际逐次512KiB发送在累计1048576字节时遇到真实WouldBlock。此时全部source释放后、仍有131072字节未读，WSAPoll只报可读、不报HUP；非背压对照报HUP。该最小重现不含WebSocket、relay或IOCP；完整正文仍可排空，不能靠丢正文制造关闭。下一轮延长仅观察窗口到原3秒，随后排空并比较HUP状态，不放宽产品3秒关闭断言。
- 后续macOS缓存复验曾再次缺Sherpa静态库，`cargo clean -p`报告移除0文件。调整为独立原生库cache、在缺库时按test profile清理包；不因缓存问题反复重编译已通过的文案行为，也不把失败写成通过。
- 用户2026-11-02回复“这个我自己测”，明确承接真实Claude/Codex模型任务、提问和恢复验收；不再索要模型环境或密钥。runner生产启动/控制验证、软件适配与候选包消费仍由Agent完成，真实模型结果不得冒充PASS。

## 原生缓存与命令边界的新证据

- `d61364a1` run `41506482883`，socket job `125486989342`：四种clone组合的真实饱和场景均等完整3秒无HUP，排空全部原正文后立即有HUP。后续`94eab557` run `41509564695` job `125497574899` 证实SO_CONNECT_TIME与getpeername在alive/closed/after-drain均成功，不能充当关闭判据。纯pair仍未包括IOCP，下一诊断增加生产一致的OwnedSocket、私有port、LEVEL|PERSISTENT HANGUP注册及队列对照；不从WSAPoll结果推断IOCP保证。
- `7db4be3d` run `41507397341` job `125490089087`：先实际构建Sherpa，再只裁剪原生lib目录和Rust库文件、保留build-script输出；第二次构建确实缺`sherpa-onnx-c-api`失败。`78e89638`增加原生目录rerun-if-changed后，run `41508405287` job `125493549823` 两次实际构建通过，裁剪后原生库恢复。旧App缓存仍有构建脚本二进制，需要在恢复缓存后触碰build.rs以引导新脚本；App消费者复验仍待结果，不能据独立库绿灯称全App构建通过。
- `8c083fc6` run `41508587588` job `125494185669`：命令边界4项3过1败，明确OPENCODE_BIN本命令路径被错误换成默认opencode。`94eab557`修正命令环境大小写/最后覆盖/空值默认及Cursor变量后，两平台job `125497575127`、`125497575295` 均4/0/0，PTY及树回归继续通过。全套terminal默认测试尚未验证，不能以定向测试替代。
- 独立对抗审查发现ASCII-only PowerShell单引号编码遗漏U+2018–U+201B，及全局Cursor模板替换会改写Claude正文。`4c2dd0dd` run `41510462982` job `125500693291` 实际6项4过2败：正文被拆成两个错误路径参数，真实PowerShell5.1解析器在U+2018路径处得到额外/畸形语句。`0b1c6ceb`限制只替换开头程序模板、逐字符重复五种单引号，等待两平台六项及真实解析器复验。
- 原有zsh profile顺序测试依赖Unix权限API，已仅限Unix编译；未增加ignore。下一门禁执行Windows完整`cargo test -p terminal`，暴露并逐项迁移其余Unix shell夹具，同时保留原平台行为和Windows原生对等验收。

## 原生终端全套与关闭修复（2026-11-02 后续）

- `409a7bbe` run `41516020469`：Windows2022 job `125519668728`、Windows2025 job `125519668644` 的完整 terminal 均为79项单测、两组各3项集成测试通过，0失败、0新增忽略；原生shell夹具保留了时间、输出、退出、目录和尺寸断言。Mac PR CI 的 Clippy 随后指出测试辅助模块重复加载；`8161f6f1` 改为crate根单次注册，没有增加allow。
- `00c3e7b0` run `41518749959`，Windows job `125528981629`：norma lib 31/0/0，严格实际输出关闭反例保留真实连续8次WouldBlock和原3秒预算，已通过。两个方向的24MiB慢端完整性均通过，来源各实际收到3个合法Ping。探针仅在原反向flush成功、来源读取因背压暂停时发送；server还要求没有pending answer，不扩大缓存或绕过票据锁。这证明全部source关闭，不宣称已覆盖仅Shutdown::Write的半关闭。
- 同job：workspace lib 27/0/0，content/review/scoped directory分别1/0/0、1/0/0、4/0/0；原生PTY3/0/0。原始日志保留在对应job；不可将其延伸为真实模型或完整GUI验收。
- `00c3e7b0` 同源增加真实App资源路径模块及Windows中文空格路径回归，先验证原macOS布局在便携包下失败，再修Windows sibling Resources解析。候选包生产/消费尚未执行；Goal保持ACTIVE。
- 取消旧source的重复待执行任务 `41515182043`、`41516020469`、`41516020544`，释放runner；其已完成失败日志保留，取消不记为通过。
- Mac旧source run `41513185652` 的域层完成后，App在链接阶段缺`sherpa-onnx-c-api`，不是断言通过。仅touch旧build.rs未重建保留输出；后续在原生库缺失时精确清理该依赖的build/fingerprint/Rust输出以强制重新下载，不删除其他crate或系统工具。
- `00c3e7b0` build run `41518749964` job `125528981830`：App adapter 17/1/0，唯一失败正是新增中文空格Windows路径的Resources断言（实际None，预期exe旁Resources）。默认App check仍通过；`c78787fe`修复平台布局并接入release/同包消费，等待绿灯。
- 独立交付审查发现helper名称和无HOME路径缺口。`c78787fe` native run `41521019369` job `125536743947` 的三项真实Windows回归均为RED：已编译.exe无法解析；源/托管文件名无.exe；无HOME子进程得到相对`.claude`目录。`0e3e7e87`使用原生EXE_SUFFIX和dirs::home_dir修复，旧Unix行为保持；同包smoke新增真实hook帮助与browser helper拒绝未授权caller检查。复验待执行，不能提前标PASS。


## 独立 follow-up 分支与最新绿灯（2026-11-02）

为避免诊断推送不断取消候选构建，后续聚焦验证使用`codex/windows-audio-20261102`，worktree为`/private/tmp/norma-windows-audio-20261102`；虽然分支名包含audio，本批先处理真实构建和终端阻塞，音频后端尚未实现。通过后以fast-forward接回PR分支，不合并dev/main。

- 主分支`7eb9b85e`的标准CI run `41522506966`，attempt 1因macOS ARM hosted容量不足、15分钟未分配runner而取消；仅对该ENV失败重试一次，attempt 2 check/fmt/Clippy及护栏全部通过。断言失败没有据此重跑覆盖。
- 同源native run `41522490922` job `125541769305`：helper三项RED→GREEN（真实PE同目录发现/执行、源和托管.exe名、无HOME子进程用户目录）。norma31项与慢端/关闭、workspace全部继续通过。但其PTY三项10秒无输出失败；两个compatibility job在同一`windows-2025-vs2026/20261030.294.1`镜像却均通过，不能认定ENV或靠预热跳过。
- 同源build job `125543493792`实际暴露Windows同样缺Sherpa native lib，没有release包。Mac content copy job `125541769616`也在同一链接前置失败。旧仅删除整个lib目录的回归未覆盖“保留空目录”的真实裁剪。
- cache RED：`7b26a409` run `41524676555` job `125549239559`，初始真实构建及删除整个lib目录后重建均通过；仅删所有native库文件、保留目录及build-script输出后，第三次真实build失败缺`sherpa-onnx-c-api`。`5019710d`改为检查全部必需非空库文件；run `41526454004` Windows job `125555320774`、Mac job `125555321528`均通过完整目录删除、空目录及保留C API但缺另一库三个场景。
- 同步输出RED：`ced42697` run `41526089514`，idle同步批次1败、显式ESU1过。四项补强后`10176f24` run `41528850944`全部通过，完整terminal为83项单测及两组各3项集成测试通过，0 ignored。计时测试锚定真实vte deadline，覆盖到期前/到期、显式ESU、Exited(7)及已kill后仅channel EOF；不sleep，也不向死后端回复DSR。独立审查未发现新计时/重复回写反例。该job原始ConPTY首帧是CSI6n且已正确回复，不包含2026，原三项同时空白的因果仍保留未确认，不能把同步修复冒充该次故障已定位。
- CRT RED：同`10176f24`探针确认VS18默认CRT元数据为14.51.36231/VC145，而旧选择得到14.44.35112/VC143；独立中文空格路径、ARM64排除及版本测试明确失败。`e774d0f4`依据实际`Microsoft.VCRedistVersion.default.txt`选择匹配的完整x64 CRT，不向旧版本静默回退；run `41529631242`整体PASS，四项同步、完整terminal和真实SDK选择一并通过。生产workflow在长构建前验证CRT，再复制同一目录。

候选构建尚未完成；真实App窗口、保存重开、WebView2、双语/主题、音频等完整软件矩阵仍未关闭，Goal保持ACTIVE。真实Claude/Codex调用继续由用户承接。标准CI保持原有macos-latest检查环境，不以换OS跳过失败。


## 路由 ID 原生随机源与真实包流程（2026-11-03 UTC）

- 生产 App 原路由 ID 无条件读取 `/dev/urandom`；在 Windows 上工作区/手机/Agent 路由生成失败。RED `6a306e0f` run `41547549765` Windows job `125626724778` 得到 OS error 3，macOS 对照通过。
- `e5ffcb2b` 复用 norma 已有 ring::SystemRandom，保留可失败结果，不提供弱随机降级；App 调用相同实现。GREEN run `41549108614` Windows job `125631888863`、macOS job `125631888653` 均通过真实 App helper 的1000次生成、UUID版本/variant与批内唯一性测试；批内唯一性不作为熵质量证明。
- PR `626debf2` 标准CI `41545807436` PASS；native run `41545800521` 的Windows原生、2022/2025 ConPTY、socket、Mac文案均PASS。Mac域回归有OpenCode history/receipt迁移超时，App回归1662过76败（主要线程创建EAGAIN，另有Vision VM警告混入QR结果、Azure输入框断言），保留原失败并定位，不重跑覆盖。
- `0888e50b` 增加独立Windows桌面consumer：显式下载producer run的同一ZIP，验证hash与candidate源码；记录probe SHA和candidate SHA。使用真实SendInput触发Ctrl+N及中文输入，再关闭重开；只读SQLite观察是补充证据，必须读取对应窗口原图，不能冒充完整项目/终端/WebView2验收。
- Windows release run `41545800566`仍在构建，尚无已验证候选包。本次route修复尚未接回PR；待当前构建取得诊断包后合入，避免取消已有长构建。

## 首个包与真实启动阻塞（2026-11-03 UTC）

- PR `626debf2` build run `41545800566` 的默认check、adapter、App/helper release及打包全部PASS，首次得到ZIP：95,136,908 bytes，SHA-256 `48e47c32cfa715869be50bcb6e321bcc83e240e1bd8d5c9201c881bd28f22c06`，manifest 59个文件。该包是诊断输入，尚不能交付为可用版本。
- fresh consumer job `125643924340`、独立desktop run `41552688583` job `125643929068` 均失败：窗口句柄短暂出现，随后App在AsyncApp::update重复借用处abort。未进入Ctrl+N；没有可验收的正常窗口截图。
- 真实开发App诊断 `2ce592f6` run `41553489141` 得到完整符号栈：`norma/endpoint.rs:120`的前台任务，在`WarmWebView::create → build_as_child → webview2_com::wait_with_pump`期间重入；外层是`settings/general.rs:73`持有App借用的开窗任务。原图frame-1已读，主要为黑屏/桌面任务栏，不能算产品渲染通过。由此确认WebView2同步消息泵触发本次启动崩溃；不再把它当GPU或缺DLL猜测。
- `04449a5a`、`04e09dc6`把Windows WebView2创建、命令和释放放到同UI/STA线程的独立前台任务，所有native调用前移出RefCell借用；32条命令后让步。创建前后校验AnyWindowHandle，关窗/删view撤销；retained项目换host时重建native view。导航/cookie保持FIFO，仅在顺序屏障后的尾段合并布局。失败结束loading、使Pending cookie失败，并以双语错误展示。
- `8086eea0`新增真实`native_webview_ready`见证，startup probe必须等真实native构造完成再取三帧并正常关窗。占位返回时改记creation_requested，不把排队时间写作native构造成功或耗时。startup工作流改为显式dispatch，防止每次TDD辅助推送重复做长App构建；后续必须在冻结源码上dispatch并复验。
- `3fff84c4`的startup run `41555972612`未执行App构建，前置Python i18n默认CP1252解码失败。`0383e51c`显式UTF-8读取catalog/source，script run `41557077334`整体PASS，新增`python -X utf8=0 scripts/check-i18n.py`亦PASS；不是对App启动修复的成败结论。

## 本轮聚焦回归与未完成项

- ZIP命名RED `616ce0f3` run `41552880320`：真实package函数的资源循环覆盖name，产物误为browser-auth-extension.zip。`eaed7ce1`仅改循环变量，run `41553535034` package 6项GREEN、原report/media继续通过；未改hash保护。
- Ctrl+N RED `81d22c4f` run `41550774986` Windows job `125637515950`：生产registry返回None，0过1败；Mac job `125637515518`对照PASS。`3fff84c4`仅修Windows NewWorkspace默认Ctrl+N，待GREEN；其余Windows快捷键、终端Ctrl+C/D、复制粘贴和焦点仍需逐项处理，不能以这一项代替完整键盘验收。
- 测试资源RED `9f487297` run `41551742282`：离开根TestAppContext作用域仍保留真实窗口实体。`1ccc2e47` run `41554212453`三项GREEN，覆盖根退出、clone不接管/显式quit幂等、原断言panic清理。第四项RED `3fff84c4` run `41555972614`为3过1败：on_quit回调自身panic会跳过其余清理；`8f28f047`逐个捕获回调异常、继续shutdown、最后保留首个错误，等待四项复验。没有把减小并行数当生命周期修复。
- 独立审查发现旧native事件先入队、换host后再消费仍可污染新view。RED `8086eea0` run `41556862483` 两平台均0过2败，保留旧URL/失败及窗口撤销反例；`630376e1`把production generation owner/lease接入native事件信封，App逐项消费时校验，包含同一drained batch前一个事件撤销后一个事件的情况。新代际必须有正断言；待两平台GREEN。
- 尚待真实验证的审查项：延迟focus_parent可能覆盖用户后续的焦点意图，需要原生焦点所有权和迟到命令反例；当前不能声明焦点生命周期完成。
- Mac QR修复及Azure独立复现曾因30分钟冷编译job上限取消；当前将App编译job预算扩至60分钟，测试内部4秒/3秒契约不变。保持原失败/取消记录，尚无整套Mac回归GREEN。
- 当前Windows实现仍在follow-up分支；PR未接回上述新提交，没有包含本轮完整修复的最终release/新runner消费包。W01–W12未全部完成，Goal保持ACTIVE。真实Claude/Codex继续由用户承接。
