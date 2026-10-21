# 工作空间想法清单：对话记下、拖给智能体、流星投递

状态：S0–S6 已实现并提交到 `dev`；真实 App 的拖拽投递 e2e 在本机环境恢复前未取得通过结果，见 §11。基线为本地 `dev`。交互与视觉以原型为准：[`docs/prototypes/norma-assistant`](../prototypes/norma-assistant/README.md)「想法清单」一节与 `tests/ideas.test.mjs`。本文只定义生产实现的分层、切片、验收，不重复原型细节。

## 1. 目标与非目标

目标：用户对 Norma 说一句话就能把想法记进当前工作空间；左栏「想法」标签列出待办；把一条想法拖到画布上的智能体卡片（或「新建智能体」）即可派发，派发成功有流星落点反馈；也可用菜单、键盘、对话完成同一件事。

非目标（一期不做）：想法分组/标签/优先级排序、跨工作空间移动、手机端编辑与新增、重复派发给多个智能体。

## 2. 现有能力与复用（已核对代码）

| 需要 | 已有 | 用法 |
| --- | --- | --- |
| 派给已有智能体 | `AdapterCommand::AssignWorkspaceTask`、`assign_workspace_task` 工具、`desktop/dispatch.rs::norma_assign_workspace_task` | 想法指派直接走这条路径，不另造派发通道 |
| 新建智能体并带任务 | `AdapterCommand::CreateAgent { profile, task }`、`desktop/creation.rs` | 「新建智能体来完成」走此路径 |
| 派发幂等与回执 | `NormaToolRuntime` 的 `operation_id` 回执、`agent_dispatch_tasks.task_ref` | 以想法 ID 作 `operation_id`；任务状态用 `task_ref` 关联 |
| 流星飞行与落点 | `app/effects/dispatch_receipt/{flight,impact,geometry}`、`motion/dispatch_receipt.rs`（`DispatchVisualOrigin::{CommandDock, AgentNode}`，已含 `reduce_motion`） | 新增起点类型 `IdeaRow`，其余飞行/落点/减少动态效果复用 |
| 左栏分段 | `runtime::desktop_drawers::DesktopLeftView{Chat,Overview}`、`drawers/left_tabs.rs`、`left_view_slide.rs` | 加 `Ideas`，三段标签，复用 10px 滑入规格 |
| 本地持久化 | `storage` 迁移体系（`migrations.rs`） | 新增一张表，见 §4 |

## 3. 分层与依赖（遵守 AGENTS.md 依赖方向）

```text
workspace::ideas   纯类型 + 状态流转（零 GPUI、零存储）  Idea / IdeaState / transitions
storage            ideas_store.rs：SQLite 读写 + 乐观并发     -> workspace
agent              三个 typed tool + AdapterCommand 变体        （GPUI/Storage-free）
app                左栏视图、拖拽、流星起点、适配器实现、快捷键、i18n
```

- 想法领域类型放 `workspace`（已是 GPUI-free 元数据层），`storage` 已依赖它；`agent` 不依赖 `workspace`，tool 参数用本 crate 内的纯字符串/数字类型，由 `app` 适配器转换。
- 原型里的正则“记一下/交给”解析不进生产：意图判断交给模型调用 tool，生产不做关键词匹配。

## 4. 数据模型

表 `workspace_ideas`（app DB，迁移写在 `storage/migrations/desktop.rs`，`workspace_id` 外键指向 `desktop_workspaces(id)` 并级联删除；注意不是 `workspace_identities`，后者按项目根目录。工作空间可含多个项目，所以不放项目库）：

| 列 | 说明 |
| --- | --- |
| `id` TEXT PK | UUID，也是派发 `operation_id` |
| `workspace_id` TEXT | 工作空间 ID，建索引 |
| `body` TEXT | 想法全文，trim 后拒绝空串，上限 500 字符，超出时拒绝并提示（不静默截断）；列表标题取首行并按宽度截断显示 |
| `source` TEXT | `chat` / `voice` / `manual` |
| `state` TEXT | `todo` / `dispatching` / `assigned` / `dismissed` |
| `assignee_node_id` / `assignee_area_id` / `task_ref` | `task_ref` 在派发**前**预先生成并写入（见下方派发协议），其余派发成功后写入 |
| `position` INTEGER | 待办顺序，间隔 1024 递增，撤销删除回原位时取前后均值，间隙耗尽才整体重排 |
| `created_at_unix` / `updated_at_unix` | 现有时间辅助函数 |

状态流转由 `workspace::ideas` 纯函数定义，非法流转返回错误：`todo → dispatching → assigned → done`（任务完成）；`dispatching → todo`（派发失败回退）；`todo → dismissed`（删除，可撤销回 `todo`）。`todo → dispatching` 用 `UPDATE … WHERE state='todo'` 做比较并交换，双击/双重放手只会成功一次。

派发协议（崩溃安全）：`NormaToolRuntime` 的 `operation_id` 回执只在内存里，重启即失效，不能单独保证幂等。因此：① CAS 置 `dispatching` 的同一事务里写入预生成的 `task_ref`；② 调用现有派发路径并把该 `task_ref` 传下去；③ 成功后置 `assigned`。启动恢复时不能把遗留 `dispatching` 直接回 `todo`（可能已发出），而是按 `task_ref` 去 `agent_dispatch_tasks` 查：查到则补成 `assigned`，查不到才回 `todo`。需先确认现有派发路径能接收外部给定的 `task_ref`，否则 S2 要小改派发接口。

## 5. 切片与顺序

每片独立可合并，先测试后实现（TDD），单文件 ≤ 500 行。

**S0 技术探针（先做，半天以内，产出结论写回本文）**
画布节点不是 GPUI 子元素树，命中由 `canvas::hit_test` 与 `app/canvas/interaction.rs` 管理，GPUI `on_drag/on_drop` 在 0.2.2 下不一定能直接落到节点上。探针要回答：想法拖拽采用 (a) GPUI `on_drag` 起拖 + 画布层 `on_drop`/自定义命中，还是 (b) 应用级指针状态 `IdeaDrag`（`MultisoulApp` 持有，沿用节点拖动的 viewport 换算与鼠标移动合并重绘）。默认假设 (b)，因为目标是画布节点而非 GPUI 元素，且必须满足 `check-performance-guardrails.sh`（拖动 mouse move 合并 repaint、渲染前先剔除）。结论不确定前不开 S3。

**S1 领域与存储**
- `workspace::ideas`：类型、`transition()`、标题规整；单测覆盖合法/非法流转、空标题、超长截断。
- `storage::ideas_store`：迁移、`add/list/transition/remove/restore(position)`；单测覆盖 CAS 只成功一次、撤销回原位、重启后顺序、损坏/缺失列迁移兼容。
- `scripts/check-storage-boundaries.sh` 通过。

**S2 Norma 工具与对话入口**
- `agent`：`capture_idea(title)`、`list_ideas()`、`assign_idea(idea_id, target)`，target 为 `{agent_card_id}` 或 `{new_agent: profile}`；新增 `AdapterCommand` 变体与 `parse_adapter_command` 分支、`tool_policy` 条目、`master_prompt` 的一两行用法说明（“用户说记下/别忘了时调 capture_idea，不要自己当待办回答”）。
- 提示词与 tool 的幂等：`assign_idea` 以 `idea_id` 作 `operation_id`。
- `app` 适配器实现：`capture_idea` 写库并通知左栏刷新；`assign_idea` 复用 `norma_assign_workspace_task` / `CreateAgent`，成功后把想法置 `assigned` 并写 `task_ref`，失败回退 `todo` 并给出可读原因。
- 测试：`cargo test -p agent`（tool 参数校验、策略、幂等回执）；适配器层用现有假适配器模式。

**S3 左栏「想法」视图（无拖拽）**
- `DesktopLeftView::Ideas`、`left_tabs.rs` 三段（对话 / 想法 / 运行概览），角标为计数且非琥珀（琥珀仍只表示“在等你处理”）。
- `drawers/ideas_list.rs`（必要时拆 `ideas_row.rs`、`ideas_menu.rs`）：待办/已指派两组、行 hover/按下/键盘焦点/选中四态、「指派 ▾」菜单（可搜索浮层，按 `material-menus.md`）、删除 + 6 秒撤销、空状态、已指派行点击定位卡片（复用 `norma_locate_workspace_agent`）。
- 新增可配置快捷键：在 `shortcuts.rs` 注册并支持 Settings → Keyboard Shortcuts 录制/恢复；先查现有快捷键冲突（`⌘⇧E` 已用于概览切换）。
- 文案进 `crates/ui/locales/*.zh-CN.json` 与 `*.en.json`。
- 以菜单指派为首个端到端闭环：记下 → 菜单指派 → 卡片收到任务。

**S4 拖拽与流星**
- 依 S0 结论实现 `IdeaDrag`：行按下 5px 阈值起拖、预览条 1:1 跟手并保持抓取偏移、悬停卡片/新建区即时高亮、Esc/落空弹回原行、触屏式输入仅手柄起拖。
- 拖动期间非智能体节点变淡，底部出现「新建智能体来完成」投放区；鼠标移动只更新拖拽状态并合并重绘，命中测试用画布缓存矩形，不在 mouse move 里查库。
- `DispatchVisualOrigin::IdeaRow(Point)`：从放手点（菜单/对话触发时从行手柄或输入框）出发，复用现有飞行与落点；“新建智能体”需先拿到新节点 ID 再起飞，节点入场与落点同步；`reduce_motion` 下沿用现有回退（无飞行，落点用无位移高亮）。
- 派发成功才写卡片与落点；派发失败则预览条弹回并 toast 原因，想法保持 `todo`。

**S5 状态联动与收尾**
- 已指派行显示任务状态（运行中/排队/失败），来源是 `agent_dispatch_tasks`，经现有活动缓存（`activity_cache.rs`）读取，不在渲染路径查库。
- 自动完成（决定 2）：`agent_dispatch_tasks` 在**项目库**，想法在 app 库，二者无外键，且后台/未加载项目的库可能暂不可读：完成回写由任务状态变更时的应用层事件驱动，另在项目加载时按 `task_ref` 补算一次，读不到则保持 `assigned`。状态映射：`done`→完成；`failed`/`interrupted`→失败可重试；`cancelled`→回待办；`accepted`/`submitted`/`queued`→进行中/排队；`untracked`（无法追踪完成）→保持已指派并显示“无法追踪”，不自动完成。任务到达完成态时把想法置 `done`，行在「已指派」组内淡出折叠进「已完成 N」（默认收起，可展开，保留 7 天后清理）；失败不自动完成，行显示“失败·重试”并可回到待办。完成判定只认 `agent_dispatch_tasks` 的终态，不靠终端文本猜测。测试：状态机（完成/失败/重启后补算）、存储、概览不受影响。
- 运行概览与顶栏状态已由真实任务驱动，无需像原型那样手动同步。
- 文档：更新 `docs/references/keyboard-shortcuts.md`、`crate-file-map.md`；必要时在 `ARCHITECTURE.md` 增一行。

**S6 iOS 只读清单（决定 3，最后做）**
- 范围：iOS 伴侣只读展示当前已配对 Mac 的工作空间想法（标题、状态、指派给谁），不新增、不编辑、不指派。
- 注意命名冲突：`ios/Sources/NormaCore/IdeaStore.swift` 已有手机本地“个人想法”（文字/语音、本地 SQLite）。工作空间想法使用独立类型 `WorkspaceIdea` 与独立界面入口，不混入个人想法库，文案区分“我的想法”和“工作空间想法”。
- 通道：沿用现有 mobile 会话/快照通道（`crates/app/src/mobile/`），Mac 侧只暴露只读快照，字段限定为标题（≤ 80 字符）、状态、指派对象名；不含项目路径与任务正文。推送中继不新增事件类型。
- 测试：`cargo test -p app` 的 mobile 快照序列化测试；`swift test --package-path ios`；真实设备验收按 `ios/README.md`。

## 6. 设计与动效（CLAUDE.md §8 必填）

- 参考：`apple-design`、`emil-design-eng`；动效完成后用 `review-animations` 审一遍并记录结论。
- 采纳：直接操作（1:1 跟手、保持抓取偏移、可中断：Esc 或指针取消即回弹）；按下即反馈；流星只在“派发成功”这一次出现，约 0.7–0.9 秒，服务于“从清单到卡片”的归属理解，高频的行 hover/选中不加动画。
- 拒绝：常驻漂浮粒子、列表项弹簧过冲、拖动时的卡片缩放；标签切换沿用已有 200ms 规格，不新增曲线。
- 减少动态效果：无飞行、无回弹动画，落点为边框高亮，投放区只淡入淡出。
- 平台与可访问性：列表为 macOS 习惯的键盘浏览（↑↓、回车打开指派菜单、Delete 删除、Esc 返回）；拖拽之外必须有菜单与对话两条等价路径；状态不只靠颜色（文字 + 圆点）；最小字号 11px、点击区 ≥ 28px 高。
- 圆角容器内的不透明块自行绘制外圆角（GPUI 0.2.2 矩形裁剪），并以测试保护。
- GPUI 部分先加载 `gpui-*` skills，并对照钉死的 `gpui =0.2.2` / `gpui-component =0.5.1` 验证 API；优先用 `ui::component` 的 Tab/List/Popover，不满足时再用裸 div。

## 7. 验收映射

| 行为 | 证据 |
| --- | --- |
| 状态流转、CAS 只成功一次、删除撤销回原位、重启保留 | `cargo test -p workspace -p storage` |
| tool 参数、策略、幂等回执、失败回退 | `cargo test -p agent`，适配器假实现测试 |
| 对话“记一下…”落库并出现在清单 | 适配器测试 + desktop-e2e `ideas` 模块（文字指令，不依赖麦克风） |
| 菜单指派 / 键盘操作 / 撤销删除 | `app` 单测（视图状态机）+ desktop-e2e |
| 拖到已有智能体、拖到新建、落空弹回、Esc 取消 | `app` 单测（`IdeaDrag` 状态机纯函数部分）+ desktop-e2e 真实 App 拖拽 |
| 流星起点/终点、减少动态效果回退 | 复用 `motion/dispatch_receipt` 测试补 `IdeaRow` 用例 + `review-animations` 记录 |
| 性能护栏（拖动合并重绘、渲染先剔除、无同步 SQLite） | `scripts/check-performance-guardrails.sh` |
| 其他约束 | `check-max-file-lines.sh`、`check-no-allow.sh`、`check-storage-boundaries.sh`、`cargo check -p app` |
| 浅色/深色外观 | desktop-e2e 两种主题各看一次，看图清单放 `checks/ideas/` |

desktop-e2e 按模块组织：`test/desktop-e2e/Scenarios/ideas/IdeasModule.swift`，`Catalog.swift` 只追加 `manifest`；夹具放 `Fixtures/ideas/`。场景：记下并列出、菜单指派给空闲智能体、拖到忙碌智能体（排队）、拖到新建、落空弹回。

## 8. 风险

| 风险 | 缓解 |
| --- | --- |
| 画布命中与 GPUI 拖放不匹配 | S0 探针先行，S3 与 S4 解耦，菜单路径先交付 |
| 拖动路径触发重绘或查库造成卡顿 | 只更新拖拽状态；命中用缓存矩形；护栏脚本兜底 |
| 模型不调 `capture_idea`、把想法当聊天回答 | prompt 明确 + 工具描述写清触发语；在评估集里加“记一下”类用例 |
| 双重派发 | `todo→dispatching` CAS + 预写 `task_ref`；重启后按 `task_ref` 对账，不盲目回退 |
| 新建智能体失败后想法状态卡在 `dispatching` | 失败路径回退 `todo`；启动对账见 §4 派发协议，并有崩溃点测试（发出前/发出后、标记前） |
| Realtime 语音通道工具集独立（`rig_tools/gpt_live.rs`、`direct_address_turn.rs`） | 三个想法工具需同时登记到文本主会话与 GPT Live 通道，并各有一条测试，否则语音“记一下”无效 |
| 语音转写错误直接成为派发给智能体的指令 | `capture_idea` 不需确认；`assign_idea` 经语音触发时沿用现有 `tool_policy` 的确认策略，回复里复述想法全文与目标，S2 明确哪些来源需要二次确认 |
| 新建智能体归属哪个项目区域 | 默认当前展开的项目区域；`assign_idea` 的 `new_agent` 参数必须带 `area_id`，对话触发时取当前区域 |
| 面板跨项目、画布只显示当前项目 | 拖拽目标只含当前画布上的智能体；其他项目的智能体用「指派」菜单（按项目分组）或对话指派 |
| desktop-e2e 是否支持拖拽 | 已有 `Input.swift` 与含 drag 的场景（`CanvasDrawers`），S4 前确认支持按住-移动-松开的指针事件，不支持则先补 E2EKit |
| iOS 只读快照泄露想法内容 | 仅经已配对的直连会话通道，不进推送中继；字段沿用截断规则，关闭 Mobile Connection 时不暴露 |
| 新建智能体的流星要等节点存在 | 先创建、拿到节点 ID 再起飞，节点入场与落点同帧 |

## 9. 待用户拍板

1. 归属：按桌面工作空间存。已确认，§4 不变。
2. 完成后自动标记完成并折叠。已确认，见 S5。
3. iOS 伴侣只读展示。已确认，见 S6。
4. 快捷键：默认 `⌘⇧I`（Ideas），在 `ShortcutCommandId` 新增 `ToggleIdeas`，语义同 `ToggleRunOverview`：抽屉收起时先展开并切到「想法」，已在想法时切回对话。已核对现有 `⌘⇧` 组合（A/E/K/L/N/O/方向键）无冲突；仍需在 Settings 可改，并在录制时做冲突检测。

## 10. 建议顺序与规模

S0 → S1 → S2 → S3 → S4 → S5 → S6。S1–S3 交付后即有可用闭环（对话记下 + 菜单指派，无拖拽）；S4 增加拖拽与流星；S5 收尾联动。每片合并前按 `docs/quality/agent-execution-policy.md` 选最小充分验证，代码 review 使用独立 subagent 做对抗式审查。

## 11. 实施记录（2026-10-21）

| 切片 | 结果 |
| --- | --- |
| S0 | 画布节点命中走应用层指针状态（方案 b）：全窗口捕获层 + 拖拽开始时缓存的窗口坐标矩形；desktop-e2e 已有 `drag` 原语，补了 E/I 键码。 |
| S1 | `workspace::ideas` 状态机、`storage::IdeasStore`（CAS、撤销保位置、级联删除、手机视图）。 |
| S2 | `capture_idea / list_ideas / assign_idea` 工具与 App 适配器；task_ref 无法预注入，改为占位 ref + 派发成功后替换为协调器真实值；对账改为按停留时长与真实任务正文匹配。 |
| S3 | 左栏「想法」视图、⌘⇧I、就地指派菜单、删除撤销、键盘浏览。 |
| S4 | 拖拽到卡片/新建区、`DispatchVisualOrigin::IdeaRow` 流星；顺带修正左抽屉展开时飞行终点偏左。 |
| S5 | 已指派想法联动任务阶段：成功→完成、取消→回待办、失败可重试、已完成折叠并 7 天后清理。 |
| S6 | 手机只读：`workspace_ideas` 动作（同项目计划读取的授权闸门）、`WorkspaceIdea`（与本机“个人想法”分离）、只读列表页。 |

与原方案的偏差：删除撤销靠保留排序值（不取前后均值）；新建智能体目前只支持当前展开的项目区域，默认 Codex；拖拽落点仅限当前展开项目的卡片，其他项目用「指派」菜单；预览条不做弹回动画。
