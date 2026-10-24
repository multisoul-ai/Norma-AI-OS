# 设置通用与外观合并实施计划

状态：实施与本地验收完成。2026-10-18 在独立 worktree 的 `feat/settings-general-appearance-merge` 分支实施，目标通过 PR 合并到 `dev`。

将现有独立「通用」页面与「外观」导航分组合并为「通用」分组，下面设置「基础设置、主题、壁纸」三个页面。基础设置承载界面语言和性能与节能，整个设置导航保持两级。

## 目标结构

一级为分组标题，二级为可点击页面。分组直接展示子项，沿用现有侧栏交互。

```text
通用
├─ 基础设置
├─ 主题
└─ 壁纸

Norma
├─ 人格
├─ 记忆
└─ 语音

输入与控制
└─ 键盘快捷键

AI 与自动化
├─ 智能体
├─ 编排
└─ 手机连接
```

共 4 个分组、10 个页面入口，数量推导为 3 + 3 + 1 + 3。版本信息与检查更新继续固定在侧栏底部。

基础设置页面从上到下排列：

| 页内分区 | 内容 | 行为 |
| --- | --- | --- |
| 语言 | 界面语言，包含跟随系统、简体中文、English | 沿用现有语言控件，选择后立即应用到所有窗口并后台保存 |
| 性能与节能 | 空闲时减少动画更新 | 沿用现有 Switch，解释节能与动画响应的取舍，后台保存 |

两块内容属于同一页面，不再成为导航子菜单。主题的浅色／深色分区、壁纸的范围切换与素材管理继续放在各自页面内。

## 现状与改动范围

当前 `NAVIGATION_GROUPS` 有 5 组：独立通用、Norma、外观、输入与控制、AI 与自动化；外观下有主题、壁纸、性能。`SettingsSection::General` 只渲染语言，`SettingsSection::Performance` 单独渲染节能开关。

| 位置 | 计划改动 |
| --- | --- |
| `crates/app/src/settings/navigation.rs` | 定义通用分组及三个页面；通用页面标签改为基础设置；移除独立外观分组与性能页面枚举、元数据 |
| `crates/app/src/settings/content.rs` | 在 General 路由中组合现有语言与节能内容；移除 Performance 路由和独立副标题分支 |
| `crates/app/src/settings/general.rs` | 保留语言状态、持久化与错误反馈；仅在组合页面确有需要时调整展示边界 |
| `crates/app/src/settings/performance.rs` | 保留节能状态、后台保存与动画唤醒逻辑；页内标题调整为性能与节能 |
| `crates/ui/locales/general.en.json`、`general.zh-CN.json` | 区分通用分组与基础设置页面的名称，更新页面说明 |
| `crates/ui/locales/settings.en.json`、`settings.zh-CN.json` | 补齐性能与节能分区的中英文文案 |
| `crates/app/tests/unit/settings_navigation_layout_tests.rs`、`settings_panel_tests.rs` | 更新完整导航契约与页面接线断言 |
| `crates/app/tests/unit/language_settings_tests.rs`、`performance_settings_tests.rs` | 复用现有行为回归，按实际缺口补充独立性或页面选择断言 |
| `crates/ui/tests/i18n.rs` | 验证新增名称和说明的中英文映射 |
| `test/desktop-e2e/Scenarios/settings/`、`checks/settings/` | 新增设置模块场景和视觉检查清单 |

复用内部标识 `SettingsSection::General` 和 `settings-section-general` 承载基础设置，主题、壁纸标识继续保留。删除独立 Performance 导航项后，重新检索全部引用并清理调用与测试；`performance` 能力模块继续存在。

持久化继续使用 `general.language.v1`、`performance.idle_animation_throttling.v1` 和 `appearance.theme.v1`，沿用壁纸已有配置。无需数据库迁移或设置重置。

分组排序与默认页分别处理：首次打开仍进入 Norma 人格，关闭后再打开仍保留当前页面；语言切换后继续停留在基础设置。

## 设计与交互约定

参考 `.agents/skills/apple-design/SKILL.md`，采用清晰分组、平台熟悉性、即时反馈与可访问性；按 `ponytail` 原则复用现有控件和状态管理。

| 原则 | 本次落地 | 实现与验收方式 |
| --- | --- | --- |
| 层级清晰 | 四个分组、十个目的地；基础设置内仅有语言与性能节能两个分区 | 三种导航模式共用 `NAVIGATION_GROUPS`；单测检查分组顺序、归属与唯一性，真实 App 检查页面标题与选中项 |
| 平台熟悉性 | 保留当前侧栏、Radio、Switch 与页面切换方式 | 复用 `ui::component` 和现有字体／间距 token；验证 Tab、Shift-Tab、Enter／Space |
| 直接反馈 | 语言立即刷新；节能开关及时反映选择；保留语言保存错误提示 | 继续调用现有选择／保存接口；核对真实窗口状态与隔离库实际值 |
| 可访问性 | 深浅主题可辨、键盘焦点清晰、窄窗口控件可达 | 真实 App 检查 hover、pressed、focus、selected／toggled 及适用的 disabled 状态 |
| 视觉克制 | 两个分区纵向组合，复用 `SECTION_GAP` 和内容区滚动 | 标准与窄窗口检查无横向溢出、说明可读、开关可到达；切页保持即时 |

菜单与选择器遵守 `docs/design-docs/material-menus.md`。本轮范围集中于导航合并与内容组合，主题卡片和壁纸库沿用当前布局。

实施涉及 GPUI 元素、样式或交互时，先加载对应本地 `gpui-*` skill，再核对 GPUI 0.2.2 和 gpui-component 0.5.1 的现有写法。持久化继续后台运行，页面渲染与语言刷新不新增同步 SQLite 操作。

## 实施顺序

1. **先写失败测试。** 修改导航测试，要求首组为通用且子项严格为 General、Themes、Wallpaper，后续三组顺序与内容明确；扁平化后恰有 10 个唯一入口，独立性能、外观分组与测试专用 Runtime／Skills 不出现在生产导航。补齐基础设置中英文名称测试，运行并记录旧实现失败。
2. **合并导航与文案。** 将导航收敛为四组；保留 `general.title` 表示通用分组，新增 `general.basic_settings` 表示基础设置页面；更新副标题为语言与节能偏好。Sidebar、CompactSidebar、TopBar 从同一导航定义读取。
3. **组合基础设置页面。** 优先在 `content.rs` 的 General 分支纵向组合 `general_settings` 与 `performance_settings`，保留各模块职责、原有回调与错误处理。清理独立 Performance 的枚举、元数据和内容路由；主题、壁纸继续沿用原有入口及生命周期。
4. **完成回归与真实验收。** 更新受影响测试，新增 settings 桌面场景，执行下述验收矩阵。完成后更新 `docs/design-docs/2026-10-04-norma-settings-navigation.md` 的导航说明，并保留历史验收记录的时间与适用范围。

测试文件继续放在对应 crate 的 `tests/unit/` 或现有集成测试目录。新增测试写明场景、数据、执行过程和预期，关键断言包含说明及正负验证。导航测试直接验证生产分组数据；页面内容是否可见与可操作由真实 App 验证，避免仅凭源码字符串宣称行为通过。

## 验收映射

| 验收要求 | 自动验证 | 真实 App 证据 |
| --- | --- | --- |
| 仅有约定的四组十页，通用下为基础设置、主题、壁纸 | 导航分组、数量、唯一性与排除项测试 | 依次进入十个页面，核对选中项和页面标题；底部版本与更新入口可用 |
| 基础设置同时承载语言与节能 | General 路由接线和已有状态测试 | 两个分区同页可达，切换设置不跳页、不出现第三层导航 |
| 语言立即生效且重启保留 | 现有语言串行保存、失败提示、退出屏障测试 | 中文切英文再切回；核对导航和页标题同步刷新、其他窗口更新及重启恢复 |
| 节能开关独立保存，原有默认值不变 | 现有 missing／true／false／invalid 加载测试 | 开关往返切换，切页及重启后恢复；修改语言不覆盖开关，修改开关不覆盖语言 |
| 主题、壁纸继续可用 | 受影响的现有主题与壁纸测试 | 切换浅／深主题；进入壁纸页核对范围、跟随关系和素材入口 |
| 首次默认人格、关闭重开保留所选页 | 面板默认值及 open／close／select 回归 | 首次打开与重新打开分别核对；人格／记忆草稿保护继续可用 |
| 三种导航模式组织一致 | 现有布局阈值测试与共用导航数据测试 | 宽窗口与可达到的窄窗口验证，无裁切或遗漏；紧凑模式仍保持分组与目的地关系 |
| 控件状态和内容可读 | 对应交互回归 | 中英文、浅／深主题检查悬停、按下、焦点、选中／开关状态，逐张核对截图 |

## 桌面场景组织

在现有 workspace 模块之外，按 `.agents/skills/norma-desktop-e2e` 创建 settings 模块：

- `test/desktop-e2e/Scenarios/settings/SettingsModule.swift`：登记 `settings-navigation`、`settings-basic-preferences` 与 `settings-responsive-keyboard`。
- `SettingsNavigation.swift`：四组十页导航、标题、主题与壁纸入口。
- `SettingsBasicPreferences.swift`：同页语言／节能操作、语言刷新、切页保持、持久化与重启恢复。
- `SettingsResponsiveKeyboard.swift`：Tab／Shift-Tab、Return／Space 与三种响应式导航模式，浅深主题下控件可达。
- `test/desktop-e2e/checks/settings/`：分别建立同名视觉清单。
- `test/desktop-e2e/Fixtures/settings/`：仅放确需的隔离夹具。
- `Scenarios/Catalog.swift`：只追加 `SettingsModule.manifest`；模块登记同步到技能的 `references/modules.md`。

读写测试偏好通过模块夹具或小型只读查询 helper 核对隔离库 `settings` 表。夹具写入前停止隔离进程；真实交互后等后台写入完成，再读取语言与节能键验证结果。现有 `E2EKit/Sqlite.swift` 主要面向工作空间，按实际需要补最小查询能力，不复制 workspace 场景。

使用 `target/debug/norma-os`、独立 `NORMA_TEST_DATA_ROOT` 与 `NORMA_SOCKET`，保留用户现有 Norma 进程和配置。先用 desktop-e2e；现有驱动表达不了的窗口调整或交互再使用 Computer Use，记录具体缺口，补齐真实窗口证据。环境检查失败按 ENV 停止；产品断言失败按 PRODUCT 记录。

## 验证命令

开发中运行聚焦测试；收尾运行受影响的 app 设置测试与 ui 测试。语言状态涉及进程级 locale，测试串行执行。

```bash
cargo test -p app --lib settings:: -- --test-threads=1
cargo test -p ui -- --test-threads=1
cargo check -p app
cargo build -p app
bash scripts/desktop-e2e.sh settings
cargo build -p app --release
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
```

上面的 `settings` 模块命令需在场景登记后运行。Rust 格式检查覆盖本轮修改；若新增 E2EKit 通用能力，则同时回归使用该能力的现有 workspace 场景。

## 完成条件

- [x] 四组十页的两级结构与目标树完全一致。
- [x] 基础设置内语言与节能可独立操作，旧配置可直接恢复。
- [x] 中英文名称、页面标题与导航选中状态一致。
- [x] 主题、壁纸、默认页、关闭重开与草稿保护通过回归。
- [x] 相关测试、构建和护栏已执行，既有失败单列；desktop-e2e 有真实结果。
- [x] 记录真实验收命令、退出码、数据断言与截图绝对路径，并逐张给出视觉结论。
- [x] 更新本计划状态及现行导航说明；既有无关失败单独记录，不写成已通过。

## 实施记录

- TDD：新增中英文名称测试在旧词库下失败（缺少 `general.basic_settings`），补齐词库后通过；导航契约测试在旧结构下失败（实际 5 组、预期 4 组）。
- UI crate：`cargo test -p ui -- --test-threads=1` 通过，7 个单元测试和 6 个集成测试。
- 设置回归：247 项中 246 通过，`voice_redesign_task_layout_and_mode_persistence` 在语音卡片等宽断言处失败，实际为 517px 与 543px。单独运行同样失败；暂时将本轮修改的 `navigation.rs`、`content.rs`、`performance.rs` 恢复为 `dev` 基线后，仍在同一断言得到相同结果。本轮代码随后已恢复。对照日志：`/tmp/norma-settings-verification-20261018/baseline-voice.log`。
- 独立对抗式静态审查：未发现本轮生产改动的缺陷或当前 E2E 调用顺序中的假阳性；键盘、窄窗口和视觉状态交由真实 App 验收。


## 最终验收（2026-10-18）

- `cargo build -p app`：退出码 0（恢复工作目录后重建 16m32s）。
- `cargo build -p app --release --target-dir /Users/user/.cache/norma-settings-delivery-20261018/release-target --jobs 4`：退出码 0，29m04s。
- 正常 pre-commit：行数、no-allow、指南行数、性能、签名就绪、视频壁纸护栏、`cargo check --workspace --quiet` 与 `cargo fmt --check` 均通过；没有绕过 hook。
- UI 与设置测试沿用本任务前段已完成的 13 项 UI 通过、246 项设置通过和 1 项基线失败；目录恢复后的重复测试为优先提交而停止，不将该次未完成运行记为通过。
- `bash scripts/desktop-e2e.sh settings`：退出码 0，3/3 通过。navigation 92.84s、basic-preferences 51.11s、responsive-keyboard 28.68s；无 ENV 失败。
- SQLite 断言：语言依次保存 en / zh-CN，节能保存 true / false，互不覆盖；重启恢复 zh-CN 与 true；浅色主题保存 paper-blue。
- 第三场景首次运行在恢复 1280×800 时未达到系统窗口尺寸要求，键盘与两个窄窗口步骤已通过。浅色切换前改用已通过的 720×760 中间尺寸，最终仍严格验 560×760；完整三场景随后通过。未改产品代码或放宽最终尺寸断言。
- 独立 subagent 对新增场景及调整复审，未发现需修复问题。

证据根目录：`/Users/user/.cache/norma-settings-delivery-20261018/work/artifacts/desktop-workspace-e2e/`。逐张打开并核对以下清单，26 张结论均为符合；响应式顶部导航保留横向滚动，长内容保留纵向滚动。

| 场景目录 | 已核对截图（全部符合） |
| --- | --- |
| `2026-10-18T03-11-20Z` | `01-persona-default.png`、`02-basic-dark.png`、`3-themes.png`、`4-wallpaper.png`、`5-persona.png`、`6-memory.png`、`7-voice.png`、`8-keyboard.png`、`9-agents.png`、`10-orchestration.png`、`11-mobile.png`、`12-basic-light.png` |
| `2026-10-18T03-12-53Z` | `01-language-english.png`、`02-energy-enabled.png`、`03-language-chinese.png`、`04-reopened-basic.png`、`05-restored-basic.png`、`06-energy-disabled.png` |
| `2026-10-18T03-13-44Z` | `01-themes-keyboard-focus.png`、`02-themes-keyboard-enter.png`、`03-basic-keyboard-space.png`、`04-basic-compact-dark.png`、`05-compact-energy-dark.png`、`06-basic-topbar-dark.png`、`07-topbar-energy-dark.png`、`08-basic-topbar-light.png` |

每个目录中的 `report.json` 包含逐步结果、耗时和数据目录，原始运行日志保存在 `/tmp/norma-settings-verification-20261018/isolated-settings-e2e-final.log`。编排页的安装状态仅验证导航与说明可见，本轮没有执行安装或修复操作。
