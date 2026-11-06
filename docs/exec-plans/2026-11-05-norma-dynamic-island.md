# Norma 灵动岛：取代左抽屉与状态胶囊，引入全局幕僚

- 日期：2026-11-05
- 状态：实施中；执行顺序调整见第 9 节
- 交互原型：[`evidence/2026-11-05-norma-dynamic-island/prototype.html`](evidence/2026-11-05-norma-dynamic-island/prototype.html)（浏览器直接打开，以其交互为准；在线版 https://claude.ai/artifact/36GvXUcVMDiPMpee5KgMvQ 为私有链接）
- 相关：[Norma 大脑设计](../design-docs/2026-10-23-norma-brain-acp.md)、[后台项目生命周期](2026-09-12-mobile-background-projects.md)

## 1. 目标与范围

**目标**：一个系统级灵动岛统一承担三件事：

1. 跟进所有工作空间的智能体运行情况，并一键定位到卡片；
2. 处理需要用户做的事（审批、提问、成果查看）；
3. 与幕僚对话、听幕僚语音汇报进度。

一次交付完成（可拆多个 PR 依次合入同一特性分支）。

**范围内**

- 灵动岛：系统级置顶窗口，贴刘海；无刘海屏在菜单栏下方显示黑色胶囊；每块屏幕各一个。
- 删除整个左抽屉（「对话」与「运行概览」）和画布顶栏状态胶囊。
- 全局幕僚：一个 brain 取代所有按工作空间的 brain，迁移旧数据。
- 设置：展开/收起岛的全局快捷键、隐藏灵动岛开关、各类主动弹出事件的开关。

**非目标**：iOS 端；Super-Agent 除对话与汇报外的能力；无刘海屏上的刘海仿真。

## 2. 用户场景

1. 在别的 App 里写东西时，某张 Claude 卡片请求运行命令 → 岛弹出审批卡片 → 点「允许一次」，Norma 不抢前台，智能体继续。
2. 展开岛，在「运行中」看到另一工作空间的 Mia 测试失败 → 点「定位」→ Norma 激活、切到该工作空间（窗口已关则重开）、画布平移到 Mia 卡片并高亮。
3. 在幕僚栏输入「让 Iris 先开始」→ 全局幕僚把任务派到 Iris 所在工作空间，回复以文字显示在岛里。
4. 点「语音汇报进度」→ 幕僚用 LLM 总结所有工作空间，朗读并逐句显示字幕。

## 3. 交互与信息架构（以原型为准）

**四种形态**

| 形态 | 内容 | 进入 / 退出 |
| --- | --- | --- |
| 收起 | 左：运行中数量与头像；右：待处理数，无待办显示绿色对勾 | 默认 |
| 完成通知 | 向下展开一行 | 3 秒后自动收回，点击进入展开 |
| 审批/提问提醒 | 弹出卡片，可就地处理 | 处理前不收回 |
| 展开 | 三栏面板 | 悬停 160ms 展开、离开 420ms 收起；图钉固定；`Esc` 收起；全局快捷键开合 |

**主动弹出**只有审批、提问、完成三类，每类可在设置中单独关闭。出错/卡住不主动弹，只在列表标出。

**展开后三栏**（有待办时默认「需要你处理」，否则「运行中」）：

- **需要你处理**：审批（允许一次 / 总是允许 / 拒绝）、提问（选项 + 自由回复）、成果（标记已看 / 在画布中查看）。
- **运行中**：按工作空间分组；每行显示当前步骤、已运行时长、进度条与「定位」；待开始折叠成一行。
- **幕僚**：语音汇报按钮与逐句高亮字幕、幕僚活动记录、对话记录与输入框。幕僚未启用或无可用后端时显示启用引导，点击打开设置的幕僚页。

**回复呈现**：幕僚回复始终显示文字；仅在开启「语音汇报」或点击汇报按钮时朗读。

**设计规范**：参考 `apple-design`、`emil-design-eng`；高频操作不加动画，形变只服务于空间连续性，减少动态效果时去掉形变只保留淡入；所有文案走 `ui::i18n`，中英双语；控件优先 `ui::component`，并实现悬停、按下、焦点、禁用状态。

## 4. 技术方案

### 4.1 岛窗口（`crates/app/src/island/`）

- `cx.open_window` 打开 `WindowKind::PopUp`。GPUI 0.2.2 在 macOS 上将其实现为 `GPUIPanel`（`NSPanel` + `NSWindowStyleMaskNonactivatingPanel`），层级 `NSPopUpWindowLevel`(101)，并加 tracking area 使 App 未激活时也收到鼠标移动。无标题栏、透明背景、不可移动。
- 用 `objc2_app_kit`（已是依赖）补充：
  - `collectionBehavior` = CanJoinAllSpaces | FullScreenAuxiliary | Stationary | IgnoresCycle，关闭阴影；
  - 由 `NSScreen.safeAreaInsets.top` 与 `auxiliaryTopLeftArea`/`auxiliaryTopRightArea` 计算刘海宽高；`safeAreaInsets.top == 0` 视为无刘海，用菜单栏高度定位胶囊；
  - 监听屏幕参数变化通知，为每块屏幕维护一个岛窗口；各窗口共享同一份状态，展开只发生在鼠标所在屏幕。
- 窗口尺寸跟形态走：展开先放大窗口再在 GPUI 中做形变动画；收起先播完动画再缩小窗口，避免透明区域拦截下层 App 的点击。
- **前置 spike**：验证非激活面板能否成为 key window，让 `Input` 在不激活 App 的情况下接收键盘。不行则退化为点击输入框时激活 App。

### 4.2 纯状态层（`runtime::island`，GPUI-free）

- `IslandMode { Compact, Activity, Alert, Expanded }` + `pinned` / `speaking` 等标志。
- 输入：概览更新、审批/提问/完成事件、指针进出、快捷键、`Esc`、定位；输出：新形态与截止时间（自动收回、悬停延时）。时间以参数传入，便于单测。
- 优先级：审批/提问 > 完成；同时多个审批时岛上只弹第一个，其余进入「需要你处理」。

### 4.3 跨工作空间数据

- 新增 App 级全局 `IslandModel` entity，按 workspace ID 汇总各工作空间（含后台保留）的运行概览。
- 现有 `capture_activity_input` / `project_activity` 只投影当前工作空间，需改为逐工作空间投影；投影仍在 `background_executor` 上进行，渲染只读缓存，不碰 SQLite。概览未变化时不 `notify`。

### 4.4 操作复用

- 审批：`decide_overview_approval`。
- 提问：`overview_questions` 的编辑器与回复链路。
- 定位：`cx.activate` → 找到或重开工作空间窗口 → 现有「镜头移到卡片并聚焦」流程。

### 4.5 全局幕僚（`brain`、`app::brain_host`）

- App 级只保留一个 `BrainHost`。运行条件：App 进程存活且至少一个工作空间打开；最后一个工作空间释放后关闭。
- 每个 `BrainEvent` 携带工作空间与 route；审批与提议按 route 回到所属工作空间。
- 逐文件 `Read` 闸门的根目录改为所有已打开项目的集合，随项目增删更新；其余闸门规则（拒绝组合命令、凭据路径、递归目录工具等）不变。
- 存储：`~/.norma/brain/projects/<hash>/` → `~/.norma/brain/global/`，目标、未处理提议、决策日志、`always_ask` 全部迁移，每条带 `workspace_id`。迁移原子、幂等，原目录保留作备份；设置合并为一份。
- 对话：岛输入 → `BrainEvent::UserRequest`；幕僚通过新增 `norma brain report --kind reply` 把文字回复送回岛。
- 汇报：按钮 → 新增 `BrainEvent::ReportRequested`，附跨工作空间概览的有界文本；幕僚 LLM 总结后经 report 通道回传，由 `tts` 朗读。

### 4.6 全局快捷键

GPUI 0.2.2 无全局快捷键。使用 Carbon `RegisterEventHotKey`（FFI，无需辅助功能权限）。在 `shortcuts.rs` 注册，并在「设置 → 键盘快捷键」支持录制与恢复。

### 4.7 删除清单

- 左抽屉：`crates/app/src/workspace/desktop/drawers/` 下 `left_*`、`chat*`、`overview_*` 渲染、`orb.rs`、`left_view_slide.rs`。
- `runtime::desktop_drawers` 的 `DesktopLeftView` 与左抽屉状态、相关快捷键与设置项、布局持久化字段。
- `crates/app/src/workspace/desktop/status_capsule.rs` 与 `route_status_capsule`。
- 相关 i18n 键（`crates/ui/locales/`）与涉及左抽屉的 desktop-e2e 场景。
- 保留概览数据与投影（`overview_rows`、`overview_layout`、`overview_attention` 等）供岛复用；渲染按岛的结构重写。

## 5. 状态、错误与边界

- 幕僚未启用/启动失败：幕僚栏显示原因与启用引导；其余两栏可用。
- 提醒期间智能体自行结束：卡片消失并提示「已失效」。
- 定位目标卡片已删除：提示「卡片已不存在」，岛保持展开。
- 屏幕插拔或分辨率变化：重算每块屏幕的岛，状态不丢。
- 用户隐藏灵动岛：保留系统通知作为兜底，否则审批没有入口。

## 6. 非功能要求

- 收起时零额外重绘；悬停与形变动画流畅；岛窗口不参与 PTY drain 的重绘路径。
- 前台不做 IO、不调 LLM；`scripts/check-performance-guardrails.sh` 通过。
- 单文件 ≤ 500 行、无 `#[allow]`；`python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py` 通过。

## 7. 风险与取舍

- **全局幕僚是最大风险**：读取范围扩到所有已打开项目，误判影响面变大；迁移必须幂等并保留备份。
- **非激活面板输入**可能不可行，退路是点击输入框时激活 App。
- **App 级单例**：现有 `MultisoulApp` 按窗口组织，岛与全局幕僚需要明确的全局拥有者。
- 一次交付评审面大：拆多个 PR 依次合入特性分支。

## 8. 验收

| 标准 | 证据 |
| --- | --- |
| 形态切换、优先级、自动收回 | `runtime::island` 单测 |
| 别的 App 在前台时审批弹出，可就地允许，Norma 不抢前台 | desktop-e2e `island` 模块场景（`--visual`）+ 断言前台 App 未变 |
| 「定位」跳到另一工作空间卡片（含窗口已关） | desktop-e2e |
| 幕僚对话与跨工作空间语音汇报 | 真实 Claude Code 后端在真实 App 跑通，日志含 report 与 TTS |
| 旧幕僚数据迁移后不丢 | brain 迁移单测（旧目录 fixture）+ 真实 App 启动检查 |
| 左抽屉与状态胶囊完全删除、无死入口 | 代码检索 + 设置页/快捷键页 E2E |
| 刘海屏、外接屏、全屏 Space | 真机验收（desktop-e2e 无法覆盖时降级 Computer Use 并记录原因） |

## 9. 用户确认的执行顺序调整（2026-11-05）

用户最新指令：“优先把所有功能都完成，使用 E2E 验收和测试，调整计划”。此指令取代先前“M0 任一项不通过即停止”的执行门槛，完整范围和第 8 节验收要求保留。

- M0 记录为技术验证证据和待完善项，不再阻断 M1–M6。非激活输入暂未通过，按 §4.1 既定退路，在点击输入框时激活 App；就地审批仍不得抢前台。
- 按 M1 状态机 → M2 全工作空间汇总 → M3 窗口与交互 → M4 全局幕僚 → M5 删除与设置 → M6 全量验收推进。每一步先补聚焦失败测试，再实现；不以假数据或演示 UI 代替最终行为。
- 每个里程碑形成可验证 PR，依次合入 dev；始终等待该 PR 最新提交的 CI（含 rust checks）通过。
- E2E 优先覆盖真实 App 的岛窗口、输入、审批、提问、定位、设置与双语；原生系统焦点、Carbon 热键、全屏 Space 和真实刘海屏不能用进程内注入或几何单测冒充。
- 详见 `2026-11-05-norma-dynamic-island-progress.md` 中的里程碑与证据映射。
