# 手机 Agent 聊天语音输入

输入框左侧用波形／键盘按钮切换模式。语音模式中央为“按住说话”：按下即反馈，松开结束收音，上滑 60pt 后松开取消。转写完成后回到文字草稿，用户检查并使用原发送／入队按钮下达任务。文字草稿和图片附件不因模式切换丢失。

## 设计依据

- 参考 `.agents/skills/apple-design/SKILL.md`：熟悉的模式切换、44pt 最小点击区域、按下与取消状态即时反馈；录音结果可编辑，发送仍由用户明确触发。
- 参考 `.agents/skills/emil-design-eng/SKILL.md`：高频模式切换不添加过场动画；按住过程直接显示状态，上滑取消有位移容差；辅助功能提供激活开始／结束及取消动作。
- 使用 SwiftUI 和系统 Speech / AVFoundation；不新增依赖或语音服务配置。转写使用系统当前语言，首次使用请求麦克风和语音识别权限。
- 已有 Norma 实时通话占用麦克风时提示先结束通话。切换卡片、离开页面、进入后台和音频中断均取消当前录音，迟到回调不得写入其他草稿。

## 自动化证据

| 验收点 | 检查 |
| --- | --- |
| 转写追加、重复结果不重复插入 | `ChatVoiceDraftTests.testReleaseAppendsTranscriptExactlyOnce` |
| 准备阶段松开、取消、迟到结果 | `ChatVoiceDraftTests.testReleaseDuringPreparationAndCancellationRejectLateResults` |
| 真实系统授权回调、拒权后再次切换 | `ChatVoiceInputFlowTests.testSystemSpeechPermissionCallbackKeepsComposerAlive`，不设置转写替身 |
| 从通知进入聊天并切换语音 | `NotificationFlowUITests.testNotificationColdLaunchAndStaleWindow`，真实独立通知窗口 |
| 切换、真实按住手势、草稿、显式发送 | `ChatVoiceInputFlowTests`，使用正式 App、隔离 TLS Mac fixture 和仅 Debug 可用的转写替身 |
| 真实页面布局、切换及状态 | 原生 UI 测试截图已检查；Computer Use 验收受工具错误阻塞，尚未通过 |

运行隔离原生流程（必须使用无用户数据的独立模拟器）：

```bash
NORMA_IOS_SIMULATOR=<isolated-simulator-id> NORMA_DECISION_UI=1 \
NORMA_NOTIFICATION_UI=1 NORMA_SWIFT_PROBE="$PWD/scripts/check-norma-ios-chat-voice.sh" \
cargo test -p e2e --test mobile_notification_native -- --ignored --nocapture
```

UI 转写替身验证手势到草稿再到原卡发送的流程，不代表真实麦克风音质或 Apple 识别服务的准确率。

## 2026-09-13 验证结果

- `swift test --package-path ios`：66 项通过；`cargo test -p e2e`：254 项通过、71 项忽略。
- 独立 iPhone 17 Pro / iOS 26.3.1 模拟器：隔离 TLS 配对与 2 项 `ChatVoiceInputFlowTests` 全部通过。覆盖往返切换、上滑取消、拒权保留草稿、转写追加、显式发送一次，以及收起并重新打开键盘。
- 原生测试发现 iOS 26 的 `.keyboard` 工具栏在恢复焦点后拦截模式按钮点击；将“收起键盘”放到导航栏后，原失败流程通过。原有轻点空白和拖动收起行为继续复用。
- Debug App 构建、生成的语音权限说明、差异格式、行数和 no-allow 护栏通过。结果包：`/tmp/norma-realtime-build/Logs/Test/Test-Norma-2026.09.13_17-44-32-+0800.xcresult`；截图：`/tmp/norma-chat-voice-final-images/`。
- Computer Use 多次遇到 `-10005 timeoutReached`；恢复读取后仍无法选择测试窗口（`element ID is no longer valid`），截图接口返回 unavailable。不能将上述原生测试当作 Computer Use 验收；真实麦克风与 Apple 识别服务仍待真机验证。

## 2026-09-14 语音切换崩溃修复

- 在真实系统语音授权中复现 `BUG IN CLIENT OF LIBDISPATCH: Block was expected to execute on queue com.apple.main-thread`。Swift 6 让旧式回调继承 `MainActor`，而 Speech 明确不保证授权回调在主线程执行。授权、音频采集和识别结果回调显式使用 `@Sendable`；UI 状态仍回到 `MainActor` 更新。
- 通知的独立 `UIHostingController` 另复现 `No ObservableObject of type VoiceCall found`；补传同一个 `VoiceCall`，保留全局麦克风占用判断。
- 两条失败路径均先复现再修复：3 项聊天语音原生 UI 测试通过（包含真实系统授权回调），通知冷/热启动和语音切换回归通过，Debug App 构建及格式/行数/no-allow 检查通过。日志：`/tmp/norma-chat-permission-red.log`、`/tmp/norma-chat-permission-green.log`、`/tmp/norma-chat-notification-green.log`。
- `cargo test -p e2e` 首轮并行运行：219 项通过、7 项失败、60 项忽略；失败集中在计划/快照/实时适配的有界等待或 route 不可用。7 项串行单独重跑均通过，首轮失败仍保留记录，未宣称全量并行测试通过。日志：`/tmp/norma-chat-crash-e2e.log`、`/tmp/norma-chat-crash-e2e-serial.log`。
- 上一轮的转写替身没有覆盖真实授权回调，本轮已补上；真实麦克风识别准确率与物理 iPhone 运行仍未验证。
