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
| `workspace` | `workspace-drawer-tabs` | 左栏「对话 / 运行概览」：⌘⇧E 进入运行概览，再按回到对话。只核对分段和两种视图，不覆盖概览内容重做。 |
| `workspace` | `workspace-run-overview` | 空工作空间的运行概览内容：⌘⇧E 后看到「需要你处理 / 当前项目 / 其他项目」，不出现旧空态文案和百分比，且不启动项目。 |
| `canvas` | `canvas-drawers`、`canvas-project-drawers`、`canvas-content-tabs` | 桌面画布外壳：左右抽屉独立开合及拖宽自动收起、悬浮概览折叠、真实项目画布与输入位置；右侧内容标签的恢复、切换与来源移除。 |
| `content` | `content-workbench`、`content-home` | 右抽屉空态（`content-home`）：胶囊地址栏、六个工具磁贴与快捷键提示、推荐区、未实现入口的“即将支持”提示、文件磁贴打开工作台。工作台（`content-workbench`）：项目文件树、编辑保存、Git 暂存/提交（核对真实索引与 HEAD）、Markdown/HTML、图片缩放、视频播放与隐藏暂停、浏览器导航/会话保留、深浅主题。 |
| `settings` | `settings-navigation`、`settings-basic-preferences`、`settings-responsive-keyboard`、`settings-space-wallpapers` | 两级导航、基础设置中的语言与节能、主题切换、太空壁纸选择与预览、持久化与重启恢复，以及键盘和窄窗口。 |

| `settings` | `settings-live-configuration` | GPT-Live模式切换、缺Key禁用、音色持久化、重启恢复和深浅主题。 |
| `voice` | `voice-live-call`、`voice-live-acoustic` | 前者验连接/静音/用量；后者原依赖字幕OCR，用户移除浮层后在启动/连接前返回ENV，避免无效云端用量。真人声学按统一验收清单执行。 |

## 预留模块

下面这些目录还没有场景。第一条用例落进去时再创建目录，并在本表补一行。不要把它们的步骤塞进 `workspace`。

| 模块 | 以后验收什么 |
| --- | --- |
| `terminal` | Terminal 节点的创建、输入、输出和焦点进出 |
| `agent` | Agent 卡片的创建、状态和可见回复 |

导航可以穿过别的模块，例如从工作空间目录打开画布。断言仍只写本模块的后置状态。跨模块的用户流程拆成各自模块里的场景，或在本表新增一行说明它为什么必须跨模块。
