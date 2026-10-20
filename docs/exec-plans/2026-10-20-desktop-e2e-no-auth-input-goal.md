# 桌面 E2E 免授权输入 · Goal 验证文档

更新时间：2026-10-20 · 版本：7 · 状态：X1 到 X6、X10、X12 已完成，X9 完成了不需要云端 Key 的部分，X11 被阻塞；方案 A 成立但有两个前提（见第 7 节）；X7、X8 不需要做

目标：在不需要辅助功能、屏幕录制、麦克风等系统授权的前提下，让现有 14 个 desktop-e2e 场景都能跑。本文规定要验证的候选方案、判定标准和选型规则，最终选出一个最优先实施的方案。

背景：[PR #132](https://github.com/multisoul-ai/Norma-AI-OS/pull/132) 的免授权通道在分支 `e2e/in-app-control-channel`（`8c09bc74`），用 GPUI `dispatch_event` 注入输入。静态核对结论：10 个场景不受影响；`content-workbench` 中 WebView 内的点击和输入送不到；`voice-live-call` 需要真麦克风授权；`voice-live-acoustic` 本来就因转写框移除而报 ENV。

## 1. 待解决的缺口

| 编号 | 缺口 | 涉及场景 | 现有路径为何失败 |
| --- | --- | --- | --- |
| G1 | 内嵌 WebView（Wry/WKWebView，CEF 为可选 feature）里的点击与键入 | `content-workbench`（图片放大、视频播放、网页会话保留、网页内链接、回退/前进/刷新） | GPUI 事件不经 AppKit hit-test，到不了 WKWebView 子视图 |
| G2 | 麦克风授权 | `voice-live-call` | TCC 授权按二进制签名记录，重新构建后会再弹窗 |
| G3 | 声学场景断言依赖已移除的转写框 | `voice-live-acoustic` | 场景开头直接抛 ENV，需要改写断言来源 |

## 2. 候选方案

### G1 输入（WebView）

| 方案 | 做法 | 预期优点 | 主要风险 |
| --- | --- | --- | --- |
| A. `NSWindow.sendEvent` | App 内构造 `NSEvent`（鼠标、滚轮、`keyDown` 带 `characters`），交给 `NSApp.sendEvent` 走正常 hit-test | 同时覆盖 GPUI 与 WebView；输入法路径（`interpretKeyEvents`）也可走通；驱动接口不变 | 非主窗口或遮挡时 WKWebView 是否响应；坐标系换算；`isTrusted` 与用户手势（视频播放） |
| B. WebView 内执行脚本 | 用 `evaluate_script` 在页面里派发 DOM 事件 | 实现简单，不依赖 AppKit 事件 | 事件 `isTrusted=false`，视频播放常被手势策略拦住；只验证 DOM，不是真实输入；等于给网页开测试后门 |
| C. CEF 远程调试（CDP） | 启用 chromium-browser feature，用 `Input.dispatchMouseEvent` | 事件为真实输入 | 默认引擎是 Wry，CEF 为可选 feature；改变被测引擎，覆盖面与线上不一致 |
| D. A + B 混合 | 点击优先走 A，A 对某类控件失效时用 B 补 | 兜底覆盖 | 复杂度高，选型后需说明每个降级点 |
| E. 降级 Computer Use | 保持现状 | 零开发 | 需要授权，与目标冲突；仅作对照基线 |

### G2/G3 音频

| 方案 | 做法 | 风险 |
| --- | --- | --- |
| M1. 夹具音源 | debug 构建下 `NORMA_E2E_AUDIO_FIXTURE=<wav>`，在 `start_pcm_stream`、`start_duplex_pcm_stream`、`AudioCapture` 三个入口按实时速率输出 PCM，并跳过 `ensure_microphone_authorized` | 必须只编进 debug；绕过了真实设备链路 |
| M2. 虚拟声卡 | 用系统级虚拟设备路由音频 | 需要安装驱动，且麦克风授权仍在 |
| M3. 保持真麦克风 | 现状 | 授权问题未解 |

## 3. 实验设计

每个实验独立运行，记录退出码、`report.json`、截图路径和耗时。实验在 `e2e/in-app-control-channel` 分支的隔离 worktree 上做，不改主分支。

| 编号 | 验证对象 | 步骤 | 通过标准 | 结果 |
| --- | --- | --- | --- | --- |
| X1 | 方案 A：WebView 点击 | 打开网页标签，点击页面里的链接 `NEXT PAGE E2E` | 页面跳到 `BROWSER SECOND E2E`，由 OCR 和 SQLite 标签地址双重确认 | **通过**（2026-10-20）。点击网页里的 `NEXT PAGE E2E` 后页面进入 `/two`，OCR 读到 `BROWSER SECOND E2E`；后退、前进、刷新也成功，SQLite 里的标签地址为 `/two`。证据：`wt-e2e-nsevent/artifacts/desktop-workspace-e2e/2026-10-20T09-28-46Z/` |
| X2 | 方案 A：WebView 键入 | 点击 `Session marker` 输入框后输入 `RETAINED E2E` | 输入框里出现该文字；切换标签再回来仍保留 | **通过**（2026-10-20）。`09-browser-first.png` 里网页输入框有焦点光标和 `RETAINED E2E`；切到文件标签再回来后（`10-browser-retained.png`）文字仍在 |
| X3 | 方案 A：GPUI 回归 | 用方案 A 路径重跑 `workspace-lifecycle`、`canvas-delete-reflow` | 与 GPUI 路径结果一致，全部通过 | **通过**（2026-10-20）。`workspace-lifecycle` 9 步全过（40s）；`canvas-delete-reflow` 4 步全过（10s，需带上 PR #132 的补位修复）；`content-webview-spike` 完整通过，含拖拽。证据：`wt-e2e-nsevent/artifacts/desktop-workspace-e2e/2026-10-20T10-08-39Z`、`…T10-09-20Z`、`…T10-09-37Z` |
| X4 | 方案 A：视频 | 点击 `<video controls>` 播放，间隔 650 ms 截两帧 | 两帧进度不同，说明解码确实在推进 | **通过**（2026-10-20，连跑 3/3）。点击播放前播放器没有计时；点击后 OCR 读到播放时间 `1s → 2s`，播放器出现暂停图标。判据由「两帧像素不同」改成「OCR 读播放时钟」，原因见第 7 节。证据：`wt-e2e-nsevent/artifacts/desktop-workspace-e2e/2026-10-20T10-24-33Z`、`…T10-24-58Z`、`…T10-25-10Z` |
| X5 | 方案 A：窗口状态 | 窗口被其他应用部分遮挡、非前台时重做 X1、X2 | 仍通过；记录失败条件 | **有条件通过**（2026-10-20）。窗口**非 key**时，点击会被丢掉，全屏遮挡时更糟；驱动照常激活 Norma 后，两种遮挡下 `content-webview-spike` 都通过（23s）。结论是必须让 Norma 成为 key 窗口，不能躲在别的窗口后面跑。详见第 7 节 |
| X6 | 授权检测 | 在从未授权过的新用户账户或重置 TCC 后重做 X1、X2 | 辅助功能、屏幕录制、输入监听/发送事件均无授权；但**出现过一次「文档文件夹」弹窗** | **部分通过**（2026-10-20）。新身份探针（从未授权）6 次运行 5 次通过；未出现辅助功能、屏幕录制、麦克风等弹窗，驱动和 Norma 没有发起这些请求。但第 1 次运行触发了一个 `kTCCServiceSystemPolicyDocumentsFolder` 弹窗，该次运行失败。详见第 7 节 |
| X7 | 方案 B：脚本事件 | 用脚本派发点击和键入，重做 X1、X2、X4 | 记录哪些步骤成功；重点看视频是否被手势策略拦截 | |
| X8 | 方案 C：CDP | 以 `chromium-browser` feature 构建，重做 X1、X2、X4 | 记录构建成本与结果；并记录与默认 Wry 引擎的行为差异 | |
| X9 | 方案 M1：夹具音源 | 注入一段 WAV，跑 `voice-live-call` 的连接和启用麦克风步骤 | 无麦克风弹窗；`app.log` 显示识别到夹具内容；用量记录增加 | **部分通过**（2026-10-20）。无弹窗和夹具音频内容经过同一采集接口这两项已验证；云端识别和用量增加**没有验证**（需要 `NORMA_LIVE_TEST_KEY_FILE`）。详见第 7 节 |
| X10 | 方案 M1：发布隔离 | `cargo build -p app --release` 后检查符号和字符串 | release 产物不含夹具音源代码和环境变量名 | **通过**（2026-10-20）。`cargo build -p app --release`（5 分 23 秒）后，release 二进制里 `NORMA_E2E_AUDIO_FIXTURE`、`NORMA_E2E_CONTROL_SOCKET`、`NORMA_E2E_CONTROL_TOKEN`、`e2e_control`、`ns_mouse` 等 8 个字符串全部为 0；debug 二进制里同样的字符串存在（正向对照）。没有运行 release 二进制 |
| X11 | G3 断言改写 | 用夹具音源 + `app.log` 断言重写 `voice-live-acoustic` 的核心步骤 | 步骤可重复通过 `--repeat 5` | **阻塞，未做**（2026-10-20）。App 里没有可断言的输出：GPT-Live 的日志只有通话结束时的 `resources released`（时长、是否定稿、错误），没有任何转写或回复的日志，界面上的转写框也已移除。要做需要先改产品代码，见第 7 节 |
| X12 | 全量回归 | 选定方案后依次跑 `workspace`、`canvas`、`content`、`settings`、`voice` 五个模块 | 14 个场景中，除明确标注为人工验收的项外全部通过 | **未达标，但不是 NS 路径引入的**（2026-10-20）。16 个场景：7 个通过、7 个失败、2 个环境错误。7 个失败在 GPUI 路径基线上完全相同，且 10 月 19 日主 worktree 的旧路径完整运行中也都是失败。详见第 7 节 |

执行顺序：X1、X2 最先做，它们决定方案 A 是否成立；失败时才做 X7、X8。X9、X10 与前面互不依赖，可并行。

## 4. 选型规则

按以下顺序判定，前一条不满足则淘汰：

1. 不需要任何系统授权（X6 必须通过）。
2. 事件走真实输入路径，而不是改写页面状态。
3. `content-workbench` 的全部步骤能通过，含视频与网页会话保留。
4. 被测引擎与线上一致（默认 Wry）。
5. 同时满足以上的方案里，选实施成本和新增 App 侧代码最少的。

预设优先级：A 通过则选 A 且不引入 B/C；A 只有个别控件失败，用 D 并在文档中逐项记录降级点；A 完全失败再评估 B 与 C。

## 5. 约束

- 通道代码只能编进 macOS debug 构建，release 与 `production-data` 不含（见 [desktop-e2e.md](../quality/desktop-e2e.md)）。
- 通道只提供原始输入和本窗口画面，不暴露内部状态；断言仍靠 OCR、隔离 SQLite 和文件。
- 夹具音源不读取用户麦克风，也不读取用户已有 Key；`voice-live-call` 仍需要 `NORMA_LIVE_TEST_KEY_FILE`。
- 单个 Rust 源文件不超过 500 行，不使用 `#[allow(...)]`，新增行为先写失败测试（TDD）。
- 仍然测不到的内容必须写入文档：真麦克风硬件链路、扬声器到麦克风的回声消除、系统菜单栏与系统对话框。

## 6. 产出

| 产物 | 要求 |
| --- | --- |
| 实验记录 | 回填第 3 节「结果」列，附 `artifacts/desktop-workspace-e2e/<时间戳>/` 路径 |
| 选型结论 | 本文末尾新增「结论」一节：选中的方案、被淘汰方案及依据、降级点清单 |
| 实施计划 | 结论确定后另起 exec-plan，按模块列实施步骤与验收；不在本文直接实施 |
| 文档更新 | 实施后同步改 `docs/quality/desktop-e2e.md` 的「能测到 vs 不能测到」表和 `norma-desktop-e2e` skill 的 preflight |

## 7. 实验记录与结论

### 已完成：X1、X2（方案 A 成立）

实现：分支 `e2e/nsevent-spike`（基于 `8c09bc74`，未提交）在控制通道新增 `ns_mouse`、`ns_text`，App 内构造 `NSEvent` 后调用 `NSWindow.sendEvent`；驱动用环境变量 `NORMA_E2E_INPUT=ns` 切换。聚焦场景 `content-webview-spike` 只含「建立项目、打开右抽屉、浏览器步骤」。

跑通过程中的发现，实施时必须保留：

1. **AppKit 事件必须在 `cx.update` 之外发送。** 最初在 `update` 里调 `sendEvent`，hit-test 命中了 `GPUIView`，但点击完全无效：此时窗口被借出，GPUI 的原生事件回调再访问窗口时静默失败。改为先在 `update` 里取出 `NSWindow`，返回后再发事件，点击立即生效。
2. 坐标原点用 `contentView().frame().height` 换算，不用 `contentLayoutRect`（后者在全尺寸内容视图下会扣掉标题栏）。
3. 事件时间戳用 `NSProcessInfo.systemUptime`。

### 已完成：X3（NS 路径回归）

X3 补了两处：

- 拖拽：`ns_mouse` 带 `pressed` 时发 `LeftMouseDragged`，否则发 `MouseMoved`。补上后 `content-webview-spike` 的拖拽右栏和窄栏地址输入步骤通过。
- 按键：新增 `ns_key`（虚拟键码 + 字符 + 修饰键）。带 Cmd/Ctrl 的组合先走 `performKeyEquivalent:`，没处理再 `sendEvent`，与 `NSApplication` 对真实按键的流程一致。原先只有文字走 NS、按键仍走 GPUI，导致 `workspace-lifecycle` 的「清空搜索」（Cmd+A、Backspace）失败。

两点说明：

- `canvas-delete-reflow` 在 spike 分支（基于 `8c09bc74`）上，GPUI 与 NS 两种路径都在「删除第一张后卡片补位」失败，因为这个分支没有 PR #132 的修复。把 `ce189c47` 不提交地应用进来后，NS 路径通过。这也是该修复在真实 App 上的一份证据，但我只核对了硬断言，没有逐张看 `CanvasDeleteReflow.checks.md` 的截图。
- `workspace-lifecycle` 的搜索步骤在 GPUI 基线上，输入搜索词没有生效（界面显示还是 2 个工作空间）；NS 路径下输入正常。我没有深究 GPUI 路径为什么没输入进去。
### 已完成：X4（视频播放）

场景 `content-video-spike`：打开 `ClipE2E.mp4`，等 2 秒让播放器加载，确认暂停态没有计时，点击中央播放按钮，再间隔 1.3 秒读两次播放时间。

- 判据改动：原设计用两帧像素差，实测不可靠。夹具视频内容接近纯黑，像素差几乎为零；而且第一张截图抓到的是播放器还没加载的瞬间。改为 OCR 读播放器的当前时间，并要求播放前没有计时作为对照。
- 点击位置：暂停态只显示中央大播放按钮，没有时间标签，所以不能等 `00:12` 出现。点击位置是按 1280 宽窗口标定的（距右缘 322px，y=443），和原 `content-workbench` 里的点击位置（距右缘 236px，y=497）不同，窗口尺寸变化时需要重新标定。
- 偶发：第一次跑时建库阶段出现过一次 `SQLite 写入失败：CHECK constraint failed: kind IN ('browser', 'file')`（0.58s 就失败），之后 6 次同样的前置步骤都正常。我没有查出原因，怀疑是应用迁移尚未跑完时夹具就写库的竞态，实施时需要排查。

### 已完成：X5（窗口被遮挡或非前台）

用自写的遮挡窗口（`cover`：小窗放在屏幕角落，或全屏黑窗）和 `NORMA_E2E_NO_ACTIVATE=1`（驱动不主动激活 Norma）做对照。

| 条件 | 结果 |
| --- | --- |
| Norma 是 key 窗口 | 通过 |
| 非 key、窗口仍可见（小遮挡 + 不激活） | 点击被丢掉，悬停仍有效。抽屉按钮的处理函数没被调用；打开工作空间那次点击反而有效 |
| 全屏遮挡 + 不激活 | 打开工作空间的点击也没生效，界面停在列表页 |
| 驱动正常激活，小遮挡或全屏遮挡 | 都通过（23s） |

要点：

- `makeKeyWindow()` 在应用未激活时不会让窗口变 key（`isKeyWindow` 仍为 false），不能当作绕过办法。
- 非 key 时画面仍会重绘（悬停提示能切换），所以不是画面冻住，是部分点击被丢。点击为什么会被丢我没有查清根因。排除了 `first_mouse` 判断（仓库里没有地方使用它）和 gpui-component 的 Button（没有按活动状态判断）。
- 驱动的 `activate` 不需要任何系统授权，但必须能把 Norma 抢到前台。这意味着「多个 worktree 并行跑」只在不同时抢前台时才成立，同一时刻只有一个 Norma 窗口能是 key。
- 持续抢焦点的遮挡（每 0.4 秒一次）会让驱动失败，这是真实限制：测试期间用户在别的应用里操作，会打断测试。

### 已完成：X6（全新身份的授权检查）

做法：造一个从未授权过的新应用身份（临时 bundle id、ad-hoc 签名的无界面探针），由它作为「负责进程」运行驱动和 Norma，二进制放在 `/tmp` 下。探针先自检：`AXIsProcessTrusted`、屏幕录制预检、发送/监听事件预检全部为 false，确认它没有任何授权。然后用 `/usr/bin/log` 查系统 TCC 日志。

| 项目 | 结果 |
| --- | --- |
| 场景结果 | `content-webview-spike` 连跑 6 次，5 次通过 |
| 辅助功能、屏幕录制、麦克风等 | 日志里没有驱动和 Norma 发起的请求。日志里的辅助功能请求全部来自探针自己的自检调用 |
| 「文档文件夹」弹窗 | 第 1 次运行开始后 4 秒出现过一次 `AUTHREQ_PROMPTING`（`kTCCServiceSystemPolicyDocumentsFolder`）。弹窗约 2 分钟后才有记录创建，该次运行失败；之后 5 次运行不再提示且全部通过 |

### 文档文件夹弹窗的来源（已定位）

TCC 日志里，这次请求的访问方（`accessing`）是 `stage/norma-os`，也就是 Norma 本身，不是驱动。被访问的路径在日志里是 `<private>`，所以用进程内拦截库（`DYLD_INSERT_LIBRARIES` 注入 `open`、`stat` 等调用，记录以 `/Users/openclawd/Documents` 开头的路径，附调用栈）在完整场景里复现：

- 整个场景里 Norma 只访问了一个受保护路径：`<仓库>/crates/app/assets/home/earth-horizon.png`（首页壁纸）。驱动 `norma-e2e` 没有访问任何 `~/Documents` 路径。
- 调用栈一：`MultisoulApp::render` → `render_workspace_backdrop` → `wallpaper_img` → `WallpaperSettingsState::image_source_for_selection` → `wallpaper_resolvers::resolve_builtin_asset` → `Path::is_file`（`stat`）。一次场景里约 340 次，即每次渲染都会在前台线程 `stat` 一次。
- 调用栈二：后台任务里 `std::fs::read` 读取同一文件，用于加载图片。
- 原因：`resolve_builtin_asset` 先找 App bundle 资源，找不到才回退到编译时记录的 `CARGO_MANIFEST_DIR/assets`。运行未打包的 `target/debug/norma-os`（E2E 用的就是它）没有 bundle，所以必然回退到仓库路径，而仓库在 `~/Documents` 下。该函数的注释本来就写着要避免这件事，但只覆盖了「有 bundle」的情况。
- 弹窗的时间点（Norma 启动后约 4 秒）与首次渲染一致，且这是整个场景里唯一的受保护路径访问，所以我判断它就是触发源。

连带发现：

- 前台渲染路径里每帧做文件系统调用，和项目约束「前台不做文件 IO」冲突，这与 E2E 无关，单独值得修。
- 此前把 `crates/app/assets` 临时设为不可读后，壁纸看起来没变，说明文件不可读时有回退（`cached_asset` 或内置图）。所以让 E2E 跳过仓库资源路径是安全的，但回退的具体来源我没有逐一确认。

已修复（2026-10-20）：

- `WallpaperSettingsState::image_source_for_selection` 对默认内置壁纸直接返回内置资源（`Resource::Embedded`），不再 `stat` 或读取仓库路径。
- 抽屉毛玻璃解码新增 `decode_frosted_selection`：默认壁纸从内置字节解码（`image::load_from_memory`），不读文件系统。
- 各加一个单元测试，先确认旧实现失败（返回了仓库绝对路径），修复后通过；`wallpaper` 相关 69 个测试通过。
- 验证：用同一个拦截库在 4 个场景（`content-webview-spike`、`workspace-lifecycle`、`canvas-delete-reflow`、`content-video-spike`）里统计，`~/Documents` 访问为 0（修复前每次场景约 340 次 `stat` 加读取）；4 个场景全部通过。再用一个全新身份（未授权过的临时 bundle id）首次运行 `content-webview-spike`：通过，系统 TCC 日志里没有 `AUTHREQ_PROMPTING`。
- 剩余：选择非默认的内置壁纸（`wallpapers/*.webp` 等）时，未打包构建仍会回退到仓库路径。`settings-space-wallpapers` 会走到这条路径，在从未授权的宿主上首次运行仍可能弹一次。这些壁纸没有内置，需要另行处理（例如 E2E 构建里改用下载缓存或打包资源）。

因此 X6 的准确结论是：**辅助功能和屏幕录制确实不再需要，但「完全无弹窗」不成立**，首次由新宿主运行会出现一次文件夹访问弹窗，来源见上一节。对你现在的使用方式（在终端里运行，终端已被授权访问仓库）这个弹窗不会出现；只有换成从未授权过的宿主应用才会。

### 已完成：X9、X10（夹具音源）

实现（分支 `e2e/nsevent-spike`，未提交）：

- 新增 `crates/voice_input/src/fixture.rs`，仅在 `debug_assertions` 下编译。设置 `NORMA_E2E_AUDIO_FIXTURE=<wav>` 后，`start_pcm_stream` 和 `start_duplex_pcm_stream` 不再打开麦克风，改为按实时节奏（每 100 ms 一块）回放该 WAV，播完后持续输出静音，直到会话被销毁。支持 16 位整数和 32 位浮点 WAV，自动混为单声道并重采样到 16 kHz。
- `microphone_authorization_status` 和 `ensure_microphone_authorized` 在夹具模式下直接返回已授权，不再查询系统。
- `AudioCapture::start*`（本地 Sherpa 听写直接使用）在夹具模式下返回错误而不是去打开真实麦克风，避免夹具模式下意外触发授权弹窗。这条路径因此不支持夹具。
- 单元测试 5 个（解码混音重采样、缺失文件、100 ms 切块、实时节奏加静音、环境变量判定），`voice_input` 全部测试通过；四个约束脚本通过。
- 诊断示例 `crates/voice_input/examples/e2e_fixture_probe.rs`：经过 App 同一个采集接口和 Silero VAD 输出统计。`status-only` 模式只查权限状态，因为不带夹具直接启动会触发真实麦克风弹窗。

X9 验证（全新身份，从未授权的临时 bundle id，二进制放在 `/tmp`）：

| 检查 | 结果 |
| --- | --- |
| 不带夹具、只查权限 | `NotDetermined`（确认该身份没有授权） |
| 语音夹具（`say` 合成，3.1 秒） | 权限 `Authorized`；57 块音频，31 块非静音；VAD 检测到 1 次语音、1 个语音段；退出码 0 |
| 静音夹具（4 秒） | 57 块音频全为静音；VAD 0 次语音（对照成立） |
| 系统 TCC 日志 | 麦克风相关记录共 5 条，全是 `preflight=yes` 的状态查询；没有真实授权请求，没有 `AUTHREQ_PROMPTING` |

X9 没有验证的部分：

- `voice-live-call` 的「真实连接、识别到夹具内容、用量增加」需要真实云端测试 Key，我没有运行。目前只证明夹具音频进入了处理链并被本地 VAD 正确区分，没有证明云端能识别它。
- 本机没有语音识别模型（只有 Silero VAD），所以没做本地识别。
- `voice-live-acoustic` 的断言改写（X11）没做。

### X11 阻塞：`voice-live-acoustic` 无法改写

原场景靠悬浮转写框 OCR 断言「听到固定语句、回复苹果香蕉」。转写框已移除后，换成夹具音源加日志断言，需要 App 输出可断言的内容，但现状是：

- `crates/app/src/app/gpt_live.rs` 里和通话相关的日志只有一条通话结束记录：`GPT-Live call resources released`，带时长、是否定稿和错误。
- `live_voice` 会收到 `session.input_transcript.delta` 和 `session.output_transcript.delta`，但没有打日志，界面也不显示。

所以现在只能断言「连接成功、夹具播放期间通话不报错、挂断后资源释放且无错误」，断言不了模型是否听到、回复了什么，而且仍然需要真实测试 Key，每次产生云端用量。要恢复原来的内容断言，需要在 debug 构建里给转写事件加一条日志（改产品代码），需要你决定。

### 已完成：X12（全量回归，NS 路径）

用 `NORMA_E2E_INPUT=ns` 逐个运行 16 个场景：

| 模块 | 场景 | 结果 |
| --- | --- | --- |
| workspace | `workspace-lifecycle` | 通过（27s） |
| canvas | `canvas-delete-reflow` | 通过（10s，需带上 PR #132 的修复） |
| canvas | `canvas-drawers` | 失败：抽屉未到达预期状态 |
| canvas | `canvas-project-drawers` | 失败：等不到「当前对话」 |
| canvas | `canvas-content-tabs` | 失败：等不到「当前对话」 |
| content | `content-home` | 通过（9s） |
| content | `content-webview-spike`（新增） | 通过（23s） |
| content | `content-video-spike`（新增） | 通过（12s） |
| content | `content-workbench` | 失败：OCR 把 `LOCAL CHANGE E2E` 读成 `EZE`，在编辑步骤停下，到不了浏览器步骤 |
| settings | `settings-basic-preferences` | 通过（9s） |
| settings | `settings-space-wallpapers` | 通过（33s） |
| settings | `settings-navigation` | 失败：「进入 persona」等不到 `Communication Style` |
| settings | `settings-responsive-keyboard` | 失败：「浅色顶部导航」等不到 `Paper Blue` |
| settings | `settings-live-configuration` | 失败：找不到「按住说话」 |
| voice | `voice-live-call` | 环境错误（退出码 3）：缺 `NORMA_LIVE_TEST_KEY_FILE` |
| voice | `voice-live-acoustic` | 环境错误（退出码 3）：场景自身阻断，原因见上 |

对 7 个失败场景用 GPUI 路径（不设 `NORMA_E2E_INPUT`）跑了基线：全部在同一步以同样的文案失败，所以 NS 路径没有引入新的失败，也没有修复它们。另外，主 worktree 在 10 月 19 日 22:38 到 22:42 的一轮完整运行（旧的系统输入路径）里，这 7 个场景的结果也都是 `fail`。这个旁证只能说明它们当时就在失败，我不知道那一轮的代码状态。

没有查失败的根因。只能确认「当前对话」是产品里写死的中文（`drawers/chat.rs:130`），不受系统语言影响，所以 canvas 三个场景大概率是抽屉没有打开，而不是文案问题；其余几个要看具体步骤。这些失败需要在主分支的当前代码上复查，spike 分支基于较早的提交。

### 已知未解决

- **建库阶段的偶发失败**（与授权无关）：`SQLite 写入失败：CHECK constraint failed: kind IN ('browser', 'file')`，已出现 2 次（第 1 次通过脚本运行，第 2 次直接运行驱动），都发生在 1 秒左右。夹具在应用刚创建数据库文件时就停掉应用并写入，若此时迁移还没跑完，会写进旧约束的表。需要让夹具等迁移完成（例如检查表结构或 `user_version`）再写。

- X6（全新环境确认无授权弹窗）尚未做。目前只能说这几次运行没有出现任何授权弹窗，辅助功能和屏幕录制没有被用到。

### 最终选型

待你确认后填写。当前倾向：方案 A，不需要 B 和 C，前提是：Norma 必须能成为 key 窗口，且首次运行要消除「文档文件夹」弹窗。
