# 模块

一个桌面场景只属于一个用户可见模块。源码、视觉清单、夹具分开，避免把 Markdown 放进 Swift 编译目标。

| 放什么 | 路径 |
| --- | --- |
| 场景步骤 | `test/desktop-e2e/Scenarios/<模块>/<场景>.swift` |
| 视觉清单 | `test/desktop-e2e/checks/<模块>/<场景>.checks.md` |
| 夹具 | `test/desktop-e2e/Fixtures/<模块>/` |
| 本模块登记 | `test/desktop-e2e/Scenarios/<模块>/<模块>Module.swift`，例如 `workspace/WorkspaceModule.swift` |
| 模块名单 | `test/desktop-e2e/Scenarios/Catalog.swift` 只追加该模块的 `manifest` |

场景名用 `<模块>-<动作>`，短横线，例如 `workspace-lifecycle`。同一模块里再加一条用例，只改该模块目录和本表，不改 `Catalog.swift`。新模块才往 `Catalog.modules` 追加一行。

`bash scripts/desktop-e2e.sh <模块名>` 按 `scenarioNames` 的顺序跑完该模块。`bash scripts/desktop-e2e.sh <场景名>` 只跑一条。

## 现有模块

| 模块 | 场景 | 验收范围 |
| --- | --- | --- |
| `workspace` | `workspace-lifecycle` | 工作空间目录：创建、列出、按名称搜索、重命名、取消删除、确认删除、项目目录保留、重启后不恢复。核对 `desktop_workspaces` 与 `desktop_project_areas`。 |
| `workspace` | `workspace-home-preview` | 首页示意节点与真实保存布局对应；Sapphire Night/Graphite主题/两壁纸；目录四方向选择、跨屏滚动、⌘↓进入选中工作空间。 |
| `workspace` | `workspace-drawer-tabs` | 左栏「对话 / 运行概览」：⌘⇧E 进入运行概览，再按回到对话。只核对分段和两种视图，不覆盖概览内容重做。 |
| `workspace` | `workspace-run-overview` | 空工作空间的运行概览内容：⌘⇧E 后看到「需要你处理 / 当前项目 / 其他项目」，不出现旧空态文案和百分比，且不启动项目。 |
| `workspace` | `workspace-status-capsule` | 空工作空间的画布右上角不显示状态胶囊。有智能体时的环、名字、已完成数和紧凑态对照运行概览原型，不验收概览行内容或悬浮拖动。 |
| `workspace` | `workspace-run-overview-agents` | 项目画布里有智能体运行时：假 `claude`（经 ZDOTDIR 放进 PATH）通过真实 Hook 上报 1/3 待办进度，核对右上角胶囊、卡片标题「运行中 1/3」与 ⌘⇧E 打开的概览行（三段进度条、「第 2/3 项」）。不覆盖浅色/深色与减少动态效果。 |
| `workspace` | `workspace-overview-float` | 运行概览图钉：从抽屉弹出到画布，左栏回到对话，拖动标题后点标签收回。 |
| `workspace` | `workspace-ideas` | 想法清单：⌘⇧I 打开；手动输入添加（来源 manual）；拖到空白处与拖动中按 Esc 都保持待办；拖到智能体卡片后库里已指派并带真实 task_ref，且松手后流星从左抽屉穿过画布、底部指令栏无飞行（像素差判断）；拖到「新建智能体来完成」与点「指派」走菜单都能派发；不同想法不共用任务引用。 |
| `workspace` | `workspace-handoff-effects` | Agent 交接（`agent task assign`，即 `/ask-norma`）的派发特效：源卡片经生产 CLI 派给目标卡片；未自定义→流星，只设语音→交接跟随语音（烟花），显式设交接→保留覆盖（闪电）；浅色（亮壁纸）与深色各一轮；核对落点、起点、时长区间与结束无残留。 |
| `workspace` | `workspace-switch-shortcuts` | 切换工作空间 / 全部工作空间键位互换：浅色与深色下 ⇧⌘↑ 打开切换器、⇧⌘K 回到目录，Dock 悬浮提示与 `/help` 速查表显示新键位。键帽符号靠看图清单判定。 |
| `canvas` | `canvas-drawers`、`canvas-project-drawers`、`canvas-content-tabs`、`canvas-delete-reflow` | 桌面画布外壳：左右抽屉独立开合及拖宽自动收起、悬浮概览折叠、真实项目画布与输入位置；右侧内容标签的恢复、切换与来源移除；删除第一张卡片后其余卡片补进空出的格子。 |

| `content` | `content-workbench`、`content-home` | 右抽屉空态（`content-home`）：胶囊地址栏、六个工具磁贴与快捷键提示、推荐区、未实现入口的“即将支持”提示、文件磁贴打开工作台；左右抽屉中英即时切换、已有输入框/错误更新和草稿保留。工作台（`content-workbench`）：项目文件树、编辑保存、Git 暂存/提交（核对真实索引与 HEAD）、Markdown/HTML、图片缩放、视频播放与隐藏暂停、浏览器导航/会话保留、深浅主题。 |
| `content` | `content-terminal-path-link` | 终端里 Agent 回复的反引号路径（相对路径、含空格路径）点击后在右抽屉打开 Markdown 预览，并持久化文件标签。 |
| `settings` | `settings-theme-consistency` | 六主题在同一真实进程切换：首页、双抽屉、项目外框、选中/焦点/悬停/按下/空输入状态；SQLite 保存值与抽屉背景像素均需匹配。 |
| `settings` | `settings-navigation`、`settings-basic-preferences`、`settings-responsive-keyboard`、`settings-space-wallpapers` | 两级导航、基础设置中的语言与节能、主题切换、太空壁纸选择与预览、持久化与重启恢复，以及键盘和窄窗口。 |

| `settings` | `settings-shortcut-defaults` | 键位互换后 Settings → Keyboard Shortcuts 的默认显示（无覆盖写入、无 Restore），以及既有自定义（切换工作空间 ⌃⌥J、新建浏览器 ⇧⌘K）原样保留且优先生效：⌃⌥J 打开切换器，⇧⌘↑ / ⇧⌘K 不触发默认行为；浅色与深色。 |
| `settings` | `settings-live-configuration` | GPT-Live模式切换、缺Key禁用、音色持久化、重启恢复和深浅主题。 |
| `settings` | `settings-a1-configuration` | A1 可选识别来源的浏览（不激活、不加载模型），默认 provider 不变；切到 GPT-Live/豆包后页面保持独立。深浅主题。 |
| `settings` | `settings-orchestration-path-convention` | 编排页“文件路径输出约定”开关：默认开启（未写库），点击后保存 false，再点击保存 true，开关外观随之变化。 |
| `settings` | `settings-grok-orchestration` | Grok Build 编排安装与确认卸载；英文 Graphite、中文 Sapphire Night 下的实验性提示及 hover，隔离 home 内核对技能、原生 Hook、凭据和用户文件保留。 |
| `brain` | `brain-report` | 后台大脑：假 `claude` 通过真实 Hook 上报完成，真实 Claude Code ACP 经权限闸门执行 `norma brain report`，App 成功接收。核对模型成功结束、CLI 成功回执，并在真实幕僚活动中看见汇报正文与回执。先 `cargo build -p app -p norma`；需要 `NORMA_BRAIN_E2E_ADAPTERS` 指向真实适配器目录，使用本机 Claude 登录或网关认证。隔离数据没有 TTS，明确验收跳过语音及 `queued=false`，不代表进入队列或实际播音。 |
| `brain` | `brain-report-voice` | 沿用真实 Claude 汇报链路，在隔离数据库选择本机已安装的 Matcha Chinese-English、本地 default 音色；不读取 TTS 云端凭据、不修改用户配置或系统输出设备。要求 `queued=true`、Report 的播放开始及 macOS DataPlayedBack 完成回调、活动面板正文和回执；入队不等于播完。退出码覆盖到原生回调；要求可回放声音时须另有被测 PID 的非静音采集证据。 |
| `brain` | `brain-autonomy` | 脚本化的大脑（不用真实模型）在第一个智能体完成后，经权限闸门给第二个智能体派后续任务：概览出现“Norma 自主处理”栏和这一条，“为什么”展开理由，“以后这类先问我”把 `autonomy.assign_followups` 写成 false。 |
| `brain` | `brain-settings` | 设置 → Norma 大脑：页面内容，点开关与点策略后配置文件里的值。 |
| `voice` | `voice-live-call`、`voice-live-acoustic`、`voice-doubao-direct` | `voice-doubao-direct`：假 Key + 本机假方舟，经控制通道注入转写，验豆包按卡片名/别名直呼派任务、未命中回落模型（无需云端 Key）。`voice-live-call` 验连接/静音/用量；后者原依赖字幕OCR，用户移除浮层后在启动/连接前返回ENV，避免无效云端用量。真人声学按统一验收清单执行。 |

| `canvas` | `canvas-presentation-layout` | 固定窗口、六张 Agent 和终端下重复生产 CLI 网页/预览展示，核对抽屉回执、--new、--type、手动重排及重启布局。 |

| `brain` | `brain-claude-quota` | 无网络 Claude 0.94.0 consumer 回放：额度正文和 error 结束；设置重启保留已收到正文及 cancelled 结束；新轮隔离与 App 重开后的持久化面板。不验证服务端真实额度或语音。 |
| `agent` | `agent-startup-watchdog` | Agent 卡片启动看门狗：HOME 重定向到已安装 hook 的假目录；假 `codex` 有首屏但不发 hook，约 15 秒后标题栏出现「迟迟未收到会话启动信号」徽标，发出 SessionStart 后消失；假 `claude` 启动即发 SessionStart，等过 15 秒也不出现徽标。不覆盖会话恢复路径与浅色/深色。 |
| `boot` | `boot-sync-animation` | 冷启动开机动画（正式启动在 Sync / Awaken 间随机）：`NORMA_E2E_BOOT_ANIMATION=sync` 播 Sync（OCR 看到 HUD 文字），约 6 秒后自动淡出露出首页，播放中按键立即退场；`=awaken` 播 Awaken（9 秒），播放中首页被遮住、之后露出；未设置时（其他场景默认）不播放、首页直接可用。不覆盖「减少动态效果」（系统设置，e2e 不改）。 |

## 预留模块

下面这些目录还没有场景。第一条用例落进去时再创建目录，并在本表补一行。不要把它们的步骤塞进 `workspace`。

| 模块 | 以后验收什么 |
| --- | --- |
| `terminal` | Terminal 节点的创建、输入、输出和焦点进出 |

导航可以穿过别的模块，例如从工作空间目录打开画布。断言仍只写本模块的后置状态。跨模块的用户流程拆成各自模块里的场景，或在本表新增一行说明它为什么必须跨模块。


| `workspace` | `workspace-home-design` | Combo首页：Sapphire Night/Graphite主题×默认/Blue Horizon壁纸；12/1空间，默认/hover/Tab焦点/菜单/取消删除/搜索分组折叠/实时高亮/回车打开/Esc/无结果预填创建；命令输入隔离、面板按钮聚焦后关闭并跨帧保持关闭。 |
