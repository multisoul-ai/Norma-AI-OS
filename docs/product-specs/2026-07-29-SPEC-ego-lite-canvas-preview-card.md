# SPEC：Ego Lite 画布预览卡片

> 状态：已实施（画布预览 + Ego 原生交互）
>
> 日期：2026-07-29
>
> 范围：Norma OS 内部 Beta / 单用户
>
> 关联：[citrolabs/ego-lite](https://github.com/citrolabs/ego-lite)

## 1. 决策与边界

Norma 的画布新增一张单实例 Ego Lite 卡片，用来显示专用 Ego Task Space 的当前页面、状态、
所有权和交接动作。卡片以 **1 FPS 只读页面快照**显示 Ego 的完整页面；点击“打开 Ego”后仍在
Ego Lite 原生应用中交互。Norma 不把第三方浏览器视图嵌入 GPUI，也不模拟点击、键盘或滚动。

这是当前 Ego API 下的最小实现：Agent 继续通过精确 `task_space_id` 控制浏览器；App 只在后台
调用 Ego helper 的 `useOrCreateTaskSpace(task_space_id)` 和 `captureScreenshot()` 读取该空间的
当前页面像素，不执行 claim、takeover、导航或输入。这个路径可以读取隐藏 Task Space 的真实
页面，不依赖 macOS 是否为该空间保留可见的窗口合成画面。

macOS 窗口捕获只作为 helper 不可用或当前所有权不允许内部截图时的降级路径。新 Task Space
使用 `Norma · Ego · <node_id>` 唯一标题，降级窗口选择必须同时匹配 Ego bundle id 与精确标题。
旧版同名窗口只在恰好一个候选含非空页面像素时兼容；多个非空候选则拒绝猜测，避免显示用户的
其它浏览内容。

helper 调用、窗口枚举、系统抓图和图片解码全部在后台执行。每个节点最多一个捕获任务，刷新
间隔至少 1,000ms；卡片只保留最近一帧，并显式释放被替换的 GPUI 图像。主路径不要求 Screen
Recording 权限；只有 macOS 窗口捕获降级路径需要该权限。两条路径都失败时，占位区显示恢复
提示。

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

- GPUI 内嵌 Ego `NSView`、可交互镜像、视频帧渲染或高帧率预览。
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
- 主预览按精确 ID 进入既有 Task Space 并调用 `captureScreenshot()`；不 claim、不 takeover、
  不导航，也不改变 Agent 控制权。
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
| `app` | `ego-browser` 精确 Task Space 截图、macOS 窗口降级捕获、1 FPS 后台刷新、单帧渲染、Master/交接 UI。 |
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
5. 任务结束报告 `complete` 或 `fail`，不在用户控制阶段偷偷重试浏览器动作。

受管 `norma-presentation` skill 必须向已安装的 Claude、Codex、Cursor 与 OpenCode Agent 提供
这套规则。应用升级后，设置中的 Orchestration Repair 会重装缺失或版本过期的受管 skill。

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
- `crates/app/tests/unit/ego_preview_tests.rs`：helper 截图路径协议、全部已就绪卡片调度、精确
  窗口选择、旧同名空窗兼容、1 FPS 调度，以及默认忽略的真实 Ego Task Space 非白屏 L3。
- `crates/e2e/src/e2e_21_ego_card.rs`：真实 GPUI New 菜单、受控 helper 结果、预览帧渲染、
  复用、非自动 Master 和忙碌关闭阻断。

### 真实 App 检查

页面或样式改动必须在真实 macOS App 中通过 Computer Use 检查：确认真实 Ego 页面出现在卡片
内容区、状态 badge 与按钮不遮挡页面、底部圆角正确、窗口切换后预览继续更新，以及错误状态
可读。交互仍在 Ego 原生窗口完成。

2026-07-30 的真实 App 验收中，恢复后的 `task_space_id=8` 即使对应的隐藏 macOS 窗口只返回
空白合成画面，画布仍通过 Ego 内部截图显示了该 Task Space 当前的 Chrome 证书错误页；后台
调度以 1 秒间隔持续产生下一帧。

## 10. 后续升级闸门

只有同时满足以下条件，才可升级为可交互、高帧率的完整 Ego 画布镜像：

1. Ego API 能稳定返回某 Task Space 的专属 macOS window ID，或直接提供连续安全图像流。
2. 能证明该 ID 不会指向用户日常窗口，并在任务创建、交接、重启和关闭后保持一致。
3. 连续图像流的低拷贝 `surface()` 渲染和圆角帧裁剪都有 L1/L3 证据；若仍依赖
   ScreenCaptureKit，也必须覆盖权限恢复。
4. 用户接管时窗口定位与失焦恢复可逆、可访问，且不会移动非 Norma 创建的窗口。

在此之前，1 FPS 只读预览 + Ego 原生交互是最小实现成本和安全边界。
