# Norma 设置：独立板块与任务导航

## 设计依据

用户要求 Norma 位于设置最前面，记忆、语音、人设分别作为子菜单；设置导航与页面标题使用英文。

已阅读 Google 官方 [Ten things we know to be true](https://about.google/company-info/philosophy/)，主要采纳：

- Focus on the user：导航按用户要完成的事命名，而非按内部技术模块命名。
- Do one thing really well：每个页面只承担一个任务，记忆、人设、语音分别管理。
- Fast is better than slow：一个点击直达子页面，切页即时，不叠加过渡动画。

同样参考仓库 `apple-design`、`emil-design-eng`、`gpui-styling`、`gpui-actions`、`gpui-testing` 技能。采用 macOS 系统字体、组件库控件、键盘焦点和直接操作；Google 原则用于信息架构与任务组织。没有新增动画，减少动态效果偏好不会丢失操作反馈。

## 导航与布局

```text
Settings
  Norma
    Memory
    Voice
    Persona
  Appearance
    Themes / Wallpaper / Performance
  Input & Controls
    Keyboard Shortcuts
  AI & Automation
    Agents / Orchestration / Mobile Connection
```

| Before | After | Why |
| --- | --- | --- |
| Norma 混在 AI 分组里 | 首个独立 Norma 分组 | 核心伙伴配置首先可见 |
| 右侧页内“记忆/语音”标签 | 三个独立导航目的地 | 减少二次选择，明确当前位置 |
| 默认展示主题 | 默认展示记忆 | 首次打开直接进入最前面的核心板块 |
| 侧栏边框与高亮线叠加 | 单一填充选中状态与语义图标 | 减少视觉噪声，保持清晰层级 |
| 空白人设入口 | 交流风格、表达示例、补充说明与保存 | 可操作、可理解、可恢复 |

Sidebar 与 CompactSidebar 保持分组导航；极窄窗口保留现有可横向滚动的 TopBar，同样先显示 Norma 三个目的地。侧栏导航统一使用 `ui::component::Button`，覆盖 hover、pressed、键盘 focus；选中态使用持续填充色。自定义颜色使用 Theme token，深浅主题分别验收。

记忆保留已有搜索、查看来源、编辑纠正、导入和冲突保护，读写开关归为同一设置卡片。语音直接复用已有配置与后台任务，长页面可滚动。人设使用三个互斥风格选项、明确标为“表达示例”的固定样例、可选补充说明和显式保存；样例不伪装为模型实时生成。1280×800px 桌面基准下保存操作直接可见；有草稿时关闭设置会返回 Persona，滚动至页首并立即展示继续编辑/放弃修改提示。

## 人设的实现

- `agent::NormaPersona`：纯域类型、风格提示词与校验；说明上限为 2000 个 Unicode 字符。固定 Norma 身份与工具规则保留，人设只调整表达。
- `norma.persona.profile`：复用全局 `SettingsStore`，保存一个 JSON 配置；不写入长期记忆，也不按项目复制。
- `PersonaView`：后台加载/保存；保存失败保留草稿；不自动保存；恢复默认先成为草稿，点击保存才生效。
- 共享 `MemorySessionContext` 携带人设，每轮后台读取当前配置；Rig 与桌面豆包语音共用 `NormaPersona::prompt()`。关闭长期记忆仍保留人设。
- 固定提示词仅保留身份、工具规则与信息完整性；Warm 的口语、昵称与格式偏好放在人设中，Professional / Concise 不继承这些表达规则。补充说明可调整所选风格的表达偏好。
- 下一轮对话使用新风格；正在执行的回复不因编辑草稿而被重启。手机云端对话不在本次配置同步范围内。

## 交互与验收映射

| 行为 | 自动验证 | 真实 App 验收 |
| --- | --- | --- |
| 首组为 Norma，顺序为记忆/语音/人设 | `settings_navigation_layout_tests` | 点击三个子菜单确认直达 |
| 默认记忆与语音长页滚动 | panel default 与 `settings_norma_scroll_tests` | 语音页面滚动到声音配置 |
| 人设保存后重载、非法输入不覆盖 | `settings_persona_tests` | 输入说明、保存、重启查看 |
| 草稿跨页保留、关闭保护 | Persona 导航布局测试 | 改风格→切记忆→关闭→继续/放弃 |
| 人设与记忆开关独立 | Agent 真实 HTTP 集成测试 | 配置保存状态与读取开关分别显示 |
| 字符上限与表达边界 | `agent::persona` 单元测试 | 超限禁用保存与就近错误提示 |

## 实际验收记录

2026-10-04，通过 `cua_repl` 操作真实 `Norma OS Dev.app`。使用隔离数据库 `/tmp/norma-settings-redesign-20261004`，不覆盖用户配置；窗口为 1280×800px，Retina 截图为 2560×1600px。

- 英文 Memory / Voice / Persona 三个子菜单直达，当前页面标题与选中状态一致；默认进入 Memory。Tab 显示焦点环，Return 可进入 Memory。
- Voice 可滚动至 Spoken Replies，配置控件保持原有行为。
- Persona 保存专业风格与中文说明后重启，仍显示保存内容；恢复默认先成为草稿，取消后恢复已保存值。切页保留草稿，关闭保护提示在页首，继续编辑后保留草稿。
- 输入 2001 个中文字符（上限 2000 + 1）显示 `2001 / 2000` 和错误，保存按钮禁用；取消后恢复合法内容。
- Paper Blue 与 Sapphire Night 主题下检查三个页面、选中状态、文本、禁用保存与输入焦点；没有新增过渡动画。

真实截图保存在 `/Users/alan/.codex/visualizations/2026/10/04/norma-settings-redesign/`：`memory-light.png`、`voice-light.png`、`persona-light.png`、对应 `*-dark.png`，另有语音滚动、键盘焦点、关闭保护与超限输入截图。

验证结果：

- `cargo test -p agent`：90 通过，1 忽略；包含真实 HTTP 人设集成验证。
- `cargo test -p app --lib settings:: -- --test-threads=1`：191 通过。
- `cargo check -p app`、受影响生产代码 Clippy、`cargo build -p app --release` 通过。
- 最新 App 全量测试：890 通过、2 失败、7 忽略。失败位于其他桌面布局改动：`project_layout_ignores_content_changes_and_respects_reduced_motion`、`project_reflow_is_interruptible_and_keeps_project_identity`。未修改这两个测试所属的并发工作。

## 代码审查

按 implement 的 code-review 要求，对固定基线 `ec0d43cdf61b36a08c4ba70f82512f0a47facc1e` 的本轮暂存改动分别执行 Standards / Spec 审查；不包含未暂存的其他工作。

### Standards

无发现。异步 IO、域依赖、持久化校验、草稿保护、交互反馈与测试正负断言符合仓库规范。

### Spec

发现并修复 1 项：旧固定提示词的暖心表达规则与 Professional / Concise 相冲突。已把这些规则移入 Warm，保留固定身份与工具边界；真实 HTTP 测试在修复前失败，修复后通过。方舟最终请求测试同时验证简洁人设没有暖心表达或 Markdown 禁令。修复后复核无遗留发现。
