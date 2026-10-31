# 桌面 E2E

真实 Norma 窗口上的用户流程由 `scripts/desktop-e2e.sh` 驱动。先按[执行规范](agent-execution-policy.md)确定本次差异与可复用证据；需要新增页面、布局或样式证据时，选择覆盖该差异的最小场景；场景表达不了的交互再降级 Computer Use。headless GPUI、单测和“截图文件存在”不能代替真实窗口结果。

## 跑什么

```bash
cargo build -p app
bash scripts/desktop-e2e.sh workspace
bash scripts/desktop-e2e.sh workspace-lifecycle --repeat 5
```

被测二进制是 `target/debug/norma-os`，必须是 debug 构建。`production-data` 构建会忽略 `NORMA_TEST_DATA_ROOT`，release 构建不含测试控制通道，都不能拿来做这套验收。

驱动直接运行 SwiftPM 产物，不打包、不签名，也不需要辅助功能或屏幕录制授权。点击、拖拽、滚动、按键和截图都经 App 内的测试控制通道完成（见下文“测试控制通道”）。

旧命令 `scripts/desktop-workspace-e2e.sh` 只转发到 `workspace-lifecycle`。

## 模型直接看图

`bash scripts/desktop-e2e.sh canvas-presentation-layout --visual` 复用同一真实 App 场景，将定位和画面判断交给当前多模态 Agent。此模式不调用 Vision，不需要额外 API Key；隔离启动、真实输入、CLI 回执与数据断言保持。支持名单在各模块 `<模块>Module.swift` 的 `visualScenarioNames`（含 `canvas-presentation-layout`、`settings-theme-consistency`、`workspace-ideas-delete-undo`），未登记的场景明确拒绝，不能静默回退。新增页面场景默认同时提供看图版；OCR 的 Vision/e5rt 故障属 ENV，改跑 `--visual`，不降级 Computer Use。

驱动逐项发布原始 PNG 与问题，等待 Agent 实际查看并响应。缺项、错图、越界、看不清和无响应都不能通过；尚在加载可有界重截，不重放操作。报告标记 `verification=model-assisted`，区别于 `ocr-scripted`；需要 Agent 参与，不是无人值守脚本。协议见 `.agents/skills/norma-desktop-e2e/references/model-vision.md`。

## 一次运行

1. 前置检查：未锁屏、测试二进制存在。发现其他 Norma 只记录，不结束；`owned.json` 里上次由本脚手架留下的 PID，且可执行文件就是本次测试二进制时，才会被结束。
2. 用独立的 `NORMA_TEST_DATA_ROOT`、`NORMA_SOCKET` 和本次随机生成的控制通道 socket / token 启动，避免碰到用户数据和默认 socket。输入只进入本次隔离窗口，不占用系统键鼠，因此不同 git worktree 可以同时跑。窗口默认开在驱动创建的私有虚拟显示器上，不出现在用户屏幕、不抢前台（见下文“不打扰用户”）。
3. 场景步骤都是“动作 → 期望的后置状态 → 有限次重试”。点击前重取窗口尺寸，先移动鼠标再按下；OCR 模式按文字定位，模型模式按当前截图定位。
4. 硬断言同时看画面文字、`desktop_workspaces` / `desktop_project_areas`，以及 `keep-me.txt`。
5. 证据写到 `artifacts/desktop-workspace-e2e/<时间戳>/`，含截图、`report.json` 和 `app.log`。该目录被仓库根 `.gitignore` 的 `artifacts/` 忽略。

退出码：`0` 通过，`1` 产品断言失败，`3` 环境失败。环境失败应停下来处理锁屏、构建或控制通道问题，不要换一种输入方式绕过。

OCR 模式的识别和输入驱动故障也使用退出码 `3`，报告标记 `kind=env`、`failureLayer=driver`。Vision 默认精确识别执行失败时，依次尝试同进程 CPU 精确识别和独立进程 CPU 精确识别；独立进程读取当前原始 PNG，20 秒超时后收尾。全部精确路径失败时，快速 OCR 只留作诊断，不能支持 present / absent 断言，并保存 `failure-ocr-*.png`。原始识别错误与恢复结果保存在 `report.json` 的 `driverEvents` 和控制台日志。

点击证据按同一个 `id` 串联：`not-sent` 表示定位或准备失败、尚未调用鼠标传输；`dispatch-attempted` 表示开始调用；`acknowledged` 表示控制通道确认；`delivery-unknown` 表示调用失败、无法确认送达。完整 move/down/up 确认后仍必须检查产品后置状态。动作抛错时不会等待该动作的后置条件，每次重试的原始错误独立留存，不能用后置超时覆盖未发送点击的原因。

## 谁判什么

| 层 | 判据 |
| --- | --- |
| 画面 | 模型逐项读原图并写观察理由；OCR 模式按 present / absent 定位与检查文字 |
| 数据 | 隔离库 `app.sqlite` 的工作空间名称和项目区行 |
| 文件 | 项目目录里的 `keep-me.txt` 在删除和重启后仍在 |
| 视觉 | Agent 查看本次验收范围内的关键图，按 `checks/<模块>/*.checks.md` 给出符合、不符合或不确定；读图边界见 [visual-checks.md](../../.agents/skills/norma-desktop-e2e/references/visual-checks.md) |

项目区通过停掉隔离进程后写入 SQLite 来准备。

## 测试控制通道

`crates/app/src/e2e_control/` 只编进 macOS debug 构建（`debug_assertions` 且未启用 `production-data`），release 与打包产物里没有这段代码。启动时同时设置 `NORMA_E2E_CONTROL_SOCKET` 和不短于 32 字符的 `NORMA_E2E_CONTROL_TOKEN` 才会监听；socket 权限为 `0600`，只接受同一用户的连接，每条请求都要带 token。

通道只提供原始输入与画面：鼠标移动/按下/抬起、滚轮、按键、文字、窗口尺寸、激活和导出本窗口 PNG；语音专用操作 `doubao_transcript` 把一句转写当作豆包 ASR 最终结果注入当前项目会话（ASR 无法在真实 App 里稳定复现，其余回复、直呼路由和工具执行仍走生产路径）。它不读取产品内部状态，断言仍靠 OCR、隔离 SQLite 和文件。输入经 GPUI `Window::dispatch_event` / `dispatch_keystroke` 进入窗口，截图用 macOS 允许进程读取自身窗口的接口，所以都不需要系统授权。

本地语音指令用 `voice_transcript` 提交一句 ASR 最终转写，之后走生产语音指令、Agent 和回复路径；`voice-command-budget` 场景同时核对本机模拟模型收到的完整请求和真实对话回复。此入口不覆盖麦克风采集或识别质量。

默认键盘投递必须在每个 KeyDown、KeyUp 之间结束一次 `App.update`，让输入控件的 Change 订阅先更新模型，再允许下一事件触发重绘。不能把整段文字和按键抬起塞进同一个更新，否则受控输入可能在重绘时被旧模型值清空。通道 ACK 只证明命令处理完成；焦点、输入框实际文字和过滤后的列表须分别取证，不能只凭点击 ACK 将输入后置失败判为产品故障。

### 与旧路径对比

| 维度 | 之前（系统键鼠 + screencapture） | 现在（debug 测试控制通道） |
| --- | --- | --- |
| 输入方式 | `CGEvent` 发到整机 | App 内 `dispatch_event` / `dispatch_keystroke` |
| 截图方式 | `screencapture` / ScreenCaptureKit | App 截自己的窗口 |
| 系统授权 | 每次（或换签名后）要辅助功能 + 屏幕录制 | 不需要 |
| 驱动形态 | 打包 `NormaE2E.app` 并签名 | 直接跑 SwiftPM 产物 |
| 多 worktree | 全机键鼠锁，必须排队 | 可并行（各打各的隔离窗口） |
| 适用构建 | debug / 有权限即可 | 仅 debug（release / `production-data` 无通道） |

### 可选开关

| 环境变量 | 作用 |
| --- | --- |
| `NORMA_E2E_INPUT=ns` | 鼠标、文字和按键改走 AppKit：App 内构造 `NSEvent` 并交给 `NSWindow.sendEvent`，所以内嵌的 WKWebView 也能收到事件。默认仍是 GPUI 路径。事件必须在 `cx.update` 之外发送，否则 GPUI 的原生事件回调拿不到窗口，事件被静默丢弃。 |
| `NORMA_E2E_FOREGROUND=1` | 恢复旧行为：每次输入前把 Norma 激活到前台，窗口留在用户屏幕上。会抢焦点，仅用于排查后台模式是否影响结果。 |
| `NORMA_E2E_VISIBLE=1` | 不建虚拟显示器、不移动窗口，隔离窗口留在用户屏幕上便于实时观察；仍不抢前台与键鼠。 |
| `NORMA_E2E_VIRTUAL_DISPLAY=0` | 不建虚拟显示器，窗口停靠在当前屏幕右下角外、只留 4pt 在屏内。 |
| `NORMA_E2E_NO_ACTIVATE=1` | 连 App 内的 active 标记也不做，仅用于验证非活动窗口行为。焦点路径缺失时部分输入会被丢掉，正常验收不要设置。 |
| `NORMA_E2E_AUDIO_FIXTURE=<wav>` | debug 构建里用这段 WAV 代替麦克风（按实时节奏回放，播完后输出静音），并报告麦克风已授权，不会触发系统授权。release 构建不含这段代码。本地 Sherpa 听写走的 `AudioCapture` 路径不支持，会直接报错。诊断示例：`cargo run -p voice_input --example e2e_fixture_probe`。 |

`voice-live-call` 的云端部分仍需要 `NORMA_LIVE_TEST_KEY_FILE`。`voice-doubao-direct` 不需要：debug 构建下 `NORMA_DOUBAO_CHAT_URL` / `NORMA_DOUBAO_TTS_URL` 可指向本机假方舟（release 与 `production-data` 构建忽略这些变量）。

### 不打扰用户

默认运行不抢屏幕、前台、键盘或鼠标，测试期间可以照常使用电脑：

- **虚拟显示器**：驱动用私有 API `CGVirtualDisplay` 创建一块 1440×900 HiDPI 虚拟屏，只在主屏右下角点相接，通过 `NORMA_E2E_DISPLAY_ID` 告诉 App；窗口创建时就开在这块屏上，驱动进程退出时系统回收。每个场景各自创建一块，整个模块会增减多次显示器。创建失败（例如未来 macOS 移除该 API）时退回“停靠屏幕角落”，不判为失败。
- **不成为前台应用**：控制通道启用后 App 切为 `Accessory` 激活策略；`activate` 只在 App 内把窗口标为 active（`Window::set_active_for_e2e`，vendored GPUI），并等窗口画出 active 帧后才应答，之后的输入才有焦点路径。
- **不接真实指针**：隔离窗口 `ignoresMouseEvents`，用户指针误入虚拟屏也点不到它；注入事件不受影响。
- **观察画面**：看证据目录里的逐步截图；需要实时看时设 `NORMA_E2E_VISIBLE=1`。

### 关闭后台模式：人工观察与验收

需要人亲眼看着窗口操作（例如人工验收、排查动效）时，任何人都可以自己用环境变量关掉后台模式，不需要 Agent 代为启动：

```bash
cargo build -p app   # 先确保 target/debug/norma-os 是最新 debug 构建

# 窗口出现在自己的屏幕上实时观看；仍不抢前台和键鼠（推荐）
NORMA_E2E_VISIBLE=1 bash scripts/desktop-e2e.sh canvas-drawers

# 完全恢复旧行为：每步把 Norma 激活到前台，会抢焦点，运行期间不要操作电脑
NORMA_E2E_FOREGROUND=1 bash scripts/desktop-e2e.sh canvas-drawers

# 只关掉虚拟显示器：窗口停靠在当前屏幕右下角外，只留 4pt 在屏内
NORMA_E2E_VIRTUAL_DISPLAY=0 bash scripts/desktop-e2e.sh canvas-drawers
```

`VISIBLE` 与 `FOREGROUND` 都不会创建虚拟显示器。只看结果不看过程时，直接打开证据目录 `artifacts/desktop-workspace-e2e/<时间戳>/` 里的逐步截图与 `report.json`。

### 能测到 vs 不能测到

| 验收对象 | 之前 | 现在 |
| --- | --- | --- |
| 画布 / 卡片 / Dock（GPUI UI）点击、拖拽、滚动 | 能 | 能 |
| 快捷键、文字输入（GPUI 处理路径） | 能 | 能 |
| 删卡后补位、布局、抽屉等页面样式 | 能 | 能 |
| OCR 读窗口文字 + 视觉看图清单 | 能 | 能 |
| 隔离 SQLite / 夹具文件断言 | 能 | 能 |
| 原生输入法候选、合成输入 | 能 | 未验证（`ns` 路径经过 `interpretKeyEvents`，但没有场景覆盖） |
| Browser / 文件预览 WebView 内的点击、输入与视频播放 | 能（真实指针） | 设 `NORMA_E2E_INPUT=ns` 后能（见下） |
| 系统菜单栏、系统对话框 | 能 | 不能 |
| 「真实系统键鼠路径」本身 | 能 | 不能（注入在 App 内） |
| 窗口完全被挡时的画面刷新 | 依赖前台 | 窗口在虚拟显示器上不被遮挡，持续重绘 |

一句话：GPUI 画出来的交互，现在都能测且不用授权；系统层和原生嵌入视图（尤其 Browser）仍要 Computer Use 或旧路径。窗口被完全遮挡时 GPUI 会停止重绘，所以隔离窗口放在没有其他窗口的虚拟显示器上，而不是另一个 Space 或用户屏幕的背后。

## 场景按模块放

场景源码在 `test/desktop-e2e/Scenarios/<模块>/`，视觉清单在 `test/desktop-e2e/checks/<模块>/`，夹具在 `test/desktop-e2e/Fixtures/<模块>/`。模块里有哪些场景，写在该目录的 `<模块>Module.swift`。`Scenarios/Catalog.swift` 只保留模块名单。边界说明在 `.agents/skills/norma-desktop-e2e/references/modules.md`。

`bash scripts/desktop-e2e.sh workspace` 跑完 workspace 模块；`bash scripts/desktop-e2e.sh workspace-lifecycle` 只跑这一条。

`workspace` 模块的 `workspace-lifecycle` 覆盖创建、目录列出两个工作空间、搜索框按名称过滤、重命名、取消删除、确认删除、项目目录保留、重启后不恢复已删除工作空间。每步都核对两张表。

新增场景、前置失败和分诊步骤见 `.agents/skills/norma-desktop-e2e/`。
