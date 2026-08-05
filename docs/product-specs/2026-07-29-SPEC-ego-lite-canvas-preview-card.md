# SPEC：Ego Lite 画布预览卡片

> 状态：已实施（画布预览 + Ego 原生交互）
>
> 日期：2026-07-29；实时预览修订：2026-07-31
>
> 范围：Norma OS 内部 Beta / 单用户
>
> 关联：[citrolabs/ego-lite](https://github.com/citrolabs/ego-lite)

## 1. 决策与边界

Norma 的画布新增一张单实例 Ego Lite 卡片，用来显示专用 Ego Task Space 的当前页面、状态、
所有权和交接动作。卡片以 **最高 24 FPS 的只读实时预览节奏**显示 Ego 的完整页面；点击
“打开 Ego”后仍在 Ego Lite 原生应用中交互。Norma 不把第三方浏览器视图嵌入 GPUI，也不模拟
点击、键盘或滚动。

当 Runtime 同时确认 `AgentWorking` 和 `ownership: Agent` 时，主路径保持一个持久
`ego-browser nodejs` 子进程，通过精确 `task_space_id` 选择已有 Task Space，再用 CDP
`Page.startScreencast` 持续接收页面 JPEG。每个 CDP 帧都立即
`Page.screencastFrameAck`，脚本把最新内容写入 App 创建的本机 Unix socket；App 只保留最新
序号，后台解码，并由本地帧节奏器以 24 FPS 向 GPUI 呈现最近有效的 `RenderImage`。Chromium
没有产生新 JPEG 的节拍点复用同一张已解码帧，不重复解码，也不暴露空白间隙。该路径没有逐帧
进程启动、PNG 临时文件或磁盘轮询，并能读取被 Ego 暂停原生窗口合成的隐藏 Task Space。

CDP 是 Agent 工作阶段的唯一主源。`ReadyForUser`、`AwaitingUser`、`UserControlling` 等
非 Agent 控制阶段不启动 CDP，避免把 Ego 的用户控制硬停误判为可重试故障；这些阶段直接使用
ScreenCaptureKit，其 420f `CVPixelBuffer` 通过 GPUI `surface()` 低拷贝显示。源切换采用
make-before-break：只有新 CDP 源交付第一张有效帧后才释放旧窗口帧；失败、重试或所有权切换
期间继续保留上一张有效帧。两种渲染分支共享同一媒体边界，因此不会因 `surface()` /
`RenderImage` 切换而改变页面缩放。低频 `captureScreenshot()` / macOS 窗口截图仅是两种持续
源均不可用时的最后恢复路径。新 Task Space 使用 `Norma · Ego · <node_id>` 唯一标题；窗口降级
选择必须同时匹配 Ego bundle id 与精确标题，不得按窗口顺序猜测。

所有 helper、窗口枚举、图片解码和系统捕获都在后台执行。持续流使用容量为一的最新帧邮箱；
消费者落后时覆盖未呈现旧帧，拒绝乱序帧，避免积压造成延迟。纯白或低信息过渡帧只被丢弃，
不会替换上一张有效帧，也不会把健康的 CDP session 降级为窗口捕获。Agent CDP 路径不要求
Screen Recording 权限；用户控制阶段的 ScreenCaptureKit 路径需要。全部路径不可用且从未得到有效帧时，
占位区显示恢复提示。持续流 session 释放时先发出取消，再把 JoinHandle 交给专用 reaper
线程；捕获线程在后台 kill/wait helper 并删除 Unix socket，不阻塞 GPUI 前台。Node helper
同时监听 socket close/error，宿主进程结束后会停止 screencast 并自行退出，防止重复
`cargo run -p app` 后留下孤儿进程或 socket。

## 2. 用户结果

用户可在 New 菜单创建 Ego 卡片；有可用帧时内容区显示完整 Ego 页面，没有帧时显示当前阶段、
交接信息或捕获错误。“画布预览 · 在 Ego 中交互”和“打开 Ego”持续说明交互边界。创建不会
自动把卡片设为 Master。

受授权的 Agent 可通过 `norma present ego` 创建或复用同一张卡片。用户或 Agent 都不能制造
第二张并行 Ego 卡。Agent 需要人工输入时显式交接，用户先手动设为 Master 再点击“接管”；
完成后点击“交还 AI”，原 Agent 才收到恢复提示。Norma 从不替 Agent 调用
`takeOverTaskSpace`。

```text
Agent present ──> PreparingAgent ──> AgentWorking ──> AwaitingUser
                                                        │
                                                        ▼
                                                    UserControlling
                                                        │  用户交还
                                                        ▼
                                                 ResumeRequested
                                                        │  Agent 显式 takeover + running
                                                        ▼
                                                   AgentWorking
                                                        │
                                              complete / fail
                                                        ▼
                                                Completed / Failed
```

## 3. 非目标

- GPUI 内嵌 Ego `NSView`、可交互镜像，或把画布内的鼠标/键盘事件转发给 Ego。
- 自动移动、缩放、停靠 Ego 原生窗口，或从画布位置推断窗口位置。
- 多张 Ego 卡片、多 Agent 抢占、队列、强制交接或多用户协作。
- 在用户控制期间自动 `takeOverTaskSpace`、自动关闭用户拥有的 Task Space。
- 仅按模糊标题、窗口顺序或页面 URL 猜测目标。

## 4. 状态与所有权规则

### 4.1 单实例与关闭

- Runtime 最多维护一张 Ego 卡片；New 菜单的重复点击只选中既有卡片。
- `AgentWorking`、`AwaitingUser`、`UserControlling`、`ResumeRequested` 都是忙碌状态。
- 忙碌状态不能关闭卡片或执行 `present ego --new`。
- `ReadyForUser`、`Completed`、`Failed`、`Disconnected` 可以关闭。关闭只移除 Norma
  卡片关联，不会冒险关闭外部浏览器空间。

### 4.2 Agent 绑定

- 第一个 `present ego` 的受授权来源 Agent 被写入 `bound_agent_node_id`。
- 只有这个 Agent 能报告 `running`、`handoff`、`complete` 或 `fail`。
- 第二个 Agent 会收到结构化 `busy` 或 `unauthorized` 错误，不能抢占。
- 任务恢复只通知原绑定 Agent；若卡片或 Agent 已不存在，Norma 只记录警告，不转交给其他
  Agent。

### 4.3 用户交接

- Agent 先在 Ego helper 中显式 `handOffTaskSpace` 并确认成功，再调用
  `norma ego handoff <node_id> --message <原因>`。
- 只有 `AwaitingUser` 显示接管按钮。接管前必须由用户手动将卡片设为 Master；否则给出
  清晰提示。
- “接管”只更新 Norma 的交接状态并打开 Ego Lite，不调用自动 takeover。
- “交还 AI”只进入 `ResumeRequested` 并给原 Agent 发送提示。Agent 必须在用户点击交还后
  自己调用 `takeOverTaskSpace`，成功后报告 `norma ego running <node_id>`。

## 5. Task Space 规则

- `task_space_id` 是唯一可信重连和主预览标识；名称用于显示和 macOS 降级窗口定位。
- 控制器用唯一名称调用 `useOrCreateTaskSpace` 创建/复用专用空间并将精确 ID 写回卡片。
- 仅当 Runtime 确认 Agent 工作且持有所有权时，主预览才按精确 ID 进入既有 Task Space 并启动
  `Page.startScreencast`；不 claim、不 takeover、不导航、不给页面输入，也不改变控制权。
- 用户控制或等待用户阶段不尝试 CDP；直接保留/更新 ScreenCaptureKit 帧，直到 Runtime 再次
  确认 Agent 工作且新的 CDP 首帧已经可用。
- 降级窗口捕获只使用 bundle id + 精确唯一标题定位像素来源；它不改变 Agent 控制所依赖的
  `task_space_id`。
- `present ego --new` 仅可替换终态卡片，并复用同一个 Canvas node。
- 替换清理仍在进行时，持久化层保留旧精确 ID/名称，但协议状态暂不返回它；只有新的
  `TaskSpaceReady` 到达后才原子写入新 ID。这样 Agent 不会误用旧空间，而所有权查询失败时
  也不会遗失用户仍可手动打开的唯一可信标识。
- 替换前控制器先独立查询 `listTaskSpaces`；只有仍为 `ownership: "agent"` 的精确旧 ID
  才会进入另一个、独占的 `completeTaskSpace(..., { keep: false })` helper 调用。
- 找不到空间、空间为 `user` / `agentDelegatedToUser` 或查询失败时，替换失败并进入可见的
  `Disconnected` 状态；Norma 不会为了清理而 claim、take over 或关闭该空间。

## 6. 数据和分层

| 层 | 责任 |
| --- | --- |
| `canvas` | `BrowserSurfaceKind::EgoLite(EgoLiteSpec)`、JSON 兼容和卡片持久化字段。 |
| `runtime` | 纯 Rust 状态机、所有权守卫、关闭/替换守卫、`RuntimeEvent::EgoTaskStateChanged`。 |
| `norma` | `present ego` 与 `ego` 生命周期协议、JSON CLI 输出和受管 presentation skill。 |
| `app` | 所有权感知的 CDP / ScreenCaptureKit 源选择、make-before-break 切换、低频快照恢复、最新帧渲染、Master/交接 UI。 |
| `e2e` | 菜单、单实例、预览帧渲染、非自动 Master、忙碌关闭守卫。 |

`EgoLiteSpec` 持久化以下字段：精确 `task_space_id`、显示名、绑定 Agent、最后阶段和消息。
恢复时有已知 Task Space 的卡片先进入 `Disconnected + Unknown`；不从持久化阶段猜测外部
控制权，更不按名称重连。

## 7. CLI 协议

所有 Agent 自动化命令默认 JSON 输出。

```text
norma present ego [--new]
norma ego status <node_id>
norma ego running <node_id>
norma ego handoff <node_id> --message <message>
norma ego complete <node_id>
norma ego fail <node_id> --message <message>
```

推荐 Agent 顺序：

1. `norma present ego`，读取返回的 `node_id`；若 `task_space_id` 尚未出现，轮询
   `norma ego status <node_id>`。
2. 使用返回的精确 ID 调用 Ego helper；确认可由 Agent 控制后，运行
   `norma ego running <node_id>`。
3. 需要用户操作时，先 `handOffTaskSpace` 并确认 `{ done: true }`，再运行 `norma ego handoff`。
4. 用户点击“交还 AI”后，才调用 `takeOverTaskSpace`；成功后再次 `norma ego running`。
5. 任务成功结束时，在独立的最终 Ego helper 调用中执行
   `completeTaskSpace(task_space_id, { keep: true })` 并确认 `{ done: true }`，保留页面供画布
   持续显示，再报告 `norma ego complete`；不可恢复失败时保留 Task Space 后报告 `fail`。
   Ego 卡片仍关联任务期间禁止 `keep: false`，也不在用户控制阶段偷偷重试浏览器动作。

受管 `norma-presentation` skill 必须向已安装的 Claude、Codex、Cursor 与 OpenCode Agent 提供
这套规则。应用升级后，设置中的 Orchestration Repair 会重装缺失、版本过期或同版本内容漂移的
受管 skill。

## 8. UI 与可访问性

- 卡片标题和状态 badge 使用当前卡片 chrome，清楚区分“Agent 正在浏览”“请求你接管”与
  “你正在控制”。
- 内容优先：页面预览占据主体；交互边界和下一步动作始终可见，状态不只依赖颜色。
- “设为 Master”“接管”“交还 AI”仅在相应状态显示，避免不可用的伪按钮。
- 所有动作使用 `gpui-component::Button`：可通过 Tab 获得焦点，并用 Space/Return 触发；组件的
  焦点态、tooltip 和至少 24 px 的点击高度让键盘与指针反馈一致。卡片不引入非必要动画，因而
  不会违背系统的减少动态效果偏好。
- Ego surface 不注册 Native Browser view，故预览不会拦截 Canvas 输入或渲染空白 WebView。
- 预览图自身绘制底部圆角，不能只依赖 GPUI 0.2.2 的矩形 `overflow_hidden()`。
- `node-close` 对忙碌任务拒绝后以工作区错误横幅反馈，保留卡片和状态。

## 9. 验收证据

### 已自动化

- `crates/canvas/tests/ego_browser_spec.rs`：旧 Browser 数据兼容、Ego spec 序列化。
- `crates/runtime/tests/ego_task_state.rs`：状态转换、原 Agent 恢复、恢复安全态、终态替换。
- `crates/app/tests/unit/norma_ego_lifecycle_tests.rs`：单实例、授权、`--new` 终态守卫。
- `crates/app/tests/unit/ego_controller_tests.rs`：helper 结果解析、用户拥有/缺失空间的清理拒绝。
- `crates/app/tests/unit/ego_preview_tests.rs`：全部已就绪卡片调度、CDP 主源优先、helper 截图
  路径协议、精确窗口选择、旧同名空窗兼容，以及默认忽略的真实 Ego Task Space 非白屏 L3。
- `crates/app/tests/unit/ego_preview_live_tests.rs`：24 FPS/420f/双缓冲配置、最新帧邮箱、
  应用侧 24 FPS 呈现节奏器、screencast 协议与 ack、持久 Unix socket 脚本，以及白帧拒绝
  不终止 session、前台非阻塞取消与后台 reaper 回收。
- `crates/app/tests/unit/ego_preview_quality_tests.rs`：纯白过渡帧拒绝和最后有效帧保留。
- `crates/app/tests/unit/ego_preview_source_tests.rs`：只有 Agent 工作且持有所有权时才进入 CDP；
  源切换和失败重试期间按全部预览节点保留上一张窗口帧。
- `crates/app/src/bin/ego_live_capture_l3.rs`：对真实动态 Ego Task Space 连续采集 60 帧，报告
  实际 FPS、序号缺口、来源和输出尺寸。
- `crates/e2e/src/e2e_21_ego_card.rs`：真实 GPUI New 菜单、受控 helper 结果、预览帧渲染、
  复用、非自动 Master 和忙碌关闭阻断。

### 真实 App 检查

页面或样式改动必须在真实 macOS App 中通过 Computer Use 检查：确认真实 Ego 页面出现在卡片
内容区、状态 badge 与按钮不遮挡页面、底部圆角正确、窗口切换后预览继续更新，以及错误状态
可读。交互仍在 Ego 原生窗口完成。

2026-07-31 的动态 L3 中，真实 Ego Task Space 连续得到 60/60 个应用呈现帧、24.00 FPS、
`max_sequence_step=1`、1440×900，来源为 `cdp-screencast-jpeg`。这里的 24 FPS 是稳定的 UI
呈现节拍；页面内容有新合成帧时立即替换，没有新内容时复用最近有效帧。同日真实 App +
Computer Use 最终验收中，页面内 `requestAnimationFrame` 计数在 1.5 秒内从 1651 更新到
3946；随后导航到纯白过渡页并等待 2.2 秒，卡片仍保留 `frame 5726` 的最后一张彩色有效帧，
没有切换到白屏或原生窗口后备源。L3 结束后没有新增 helper 子进程或
`norma-ego-screencast-*.sock`。

同日补充的 release Dev App 退化验收中，CDP 与 ScreenCaptureKit 均持续失败重试时，通过
Computer Use 在 0、3、6 秒保存的三张 `1228×768` 截图像素 RMSE 均为 0；卡片保持同一媒体
边界和占位/上一帧，没有因重试或源切换出现缩放跳动。

## 10. 后续升级闸门

当前 24 FPS 预览已经成立，但升级为画布内可交互 Ego 仍需同时满足：

1. Ego 提供稳定、明确归属单个 Task Space 的输入路由或可嵌入视图，不依赖坐标猜测。
2. 能证明输入目标不会指向用户日常窗口，并在任务创建、交接、重启和关闭后保持一致。
3. 交互视图的焦点、IME、快捷键、滚动、无障碍和圆角裁剪都有 L1/L3 证据。
4. 用户接管时窗口定位与失焦恢复可逆、可访问，且不会移动非 Norma 创建的窗口。

在此之前，24 FPS 只读预览 + Ego 原生交互是当前安全边界。
