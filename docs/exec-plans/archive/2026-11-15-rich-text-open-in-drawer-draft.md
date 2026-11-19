# 富文本点击在右侧抽屉打开（实现前草案）

归档状态：SUPERSEDED，实现版见[主计划](../2026-11-15-rich-text-open-in-drawer.md)（PR #249 已合入）。本文件保留实现前的范围、架构、选型与实施顺序，仅作历史参考，不是当前施工指令。

状态：草案（范围与沙箱边界已澄清，渲染方案待 spike）｜日期：2026-11-15

## 1. 目标与范围

第一期范围：幕僚内全部文本（Norma 回复、用户消息、Agent 卡片/活动摘要）。画布 Agent 卡片另立项（尚未找到其富文本渲染点）。以下目标，点击后在右侧抽屉打开：

- Markdown 链接 `[text](url)`、裸 URL
- 行内代码中的绝对/相对路径（如 `/Users/x/snake.html`）
- Markdown 图片 `![](src)`
- 文件类型：md、html、图片、视频、文本/代码、目录

非目标：画布 Agent 卡片；原生 AVPlayer；HTML 访问文件系统/Git 桥；渲染时预检路径是否存在；`:line` 跳行（只打开文件）。

## 2. 现状

- 幕僚消息用 gpui-component `TextView::markdown` 渲染（`crates/app/src/app/staff/messages/text/prose.rs`）。
- `TextView` 0.5.1 的链接点击写死 `cx.open_url`（`inline.rs`），无回调；行内代码、图片不可点。
- 仅 `prose.rs` 用 `TextView`；`user.rs`、活动/卡片文本为纯文本渲染，需改为 Markdown 渲染（用户消息行为变化需评估：用户输入的 `*`、`#`、`_` 会被渲染）。
- 抽屉 File Tab 目前只渲染 `Kind::Text` 纯文本，md 富渲染、图片、视频、HTML 的 wry 加载尚不存在（`drawers/content/render.rs`）。
- 画布与幕僚均已有右侧内容抽屉与 Tab 体系：`DesktopContentKind::{Browser, ProjectFiles, File}`，入口 `open_desktop_content_tab`（`crates/app/src/workspace/desktop/drawers/content/tabs.rs`）。
- 终端点路径已有路由：`crates/app/src/preview/terminal_route.rs`、`local_content.rs`（含 wry token 沙箱）。

## 3. 架构

| 层 | 职责 | 位置 |
|---|---|---|
| 点击入口 | `TextView` 增 `on_link_click`；行内代码、图片产生可点击区域，选择文本时不触发 | vendor 的 `gpui-component 0.5.1` + 补丁（估计 150 行以上，涉及 `inline.rs`、`format/markdown.rs`、图片节点），`[patch.crates-io]` 指向 `vendor/`，同步向上游提 PR |
| 识别与路由（纯函数） | 点击目标分类为外链 / 本地路径 / 图片 src；路径沿用 `route_terminal_path` 规则 | `crates/app/src/preview/rich_link.rs`，复用 `inspect_path`、`in_project` |
| 抽屉分发 | 路由结果交给 `open_desktop_content_tab`；同资源聚焦已有 Tab，否则新开；抽屉关闭则展开 | `workspace/desktop/drawers/content/` |

## 4. 技术选型

| 项 | 选择 | 理由 |
|---|---|---|
| Markdown 渲染 | 保留 gpui-component `TextView` | 保留选择、表格、代码块 |
| 升级 gpui-component | 不升级 | 0.9.x 依赖 `gpui-pre =0.3.12`，与 vendored `gpui =0.2.2` 不兼容 |
| md/文本/图片 | 原生 GPUI（现有 preview） | 主题、i18n、选择一致 |
| HTML/视频/外链 | wry（现有依赖） | 不新增原生绑定；视频用 WKWebView `<video>` |
| 行内代码路径 | 补丁暴露行内 code 点击；路径判定在纯函数层 | 不误伤代码块 |

## 5. 路由规则

| 目标 | 项目内 | 项目外 |
|---|---|---|
| http(s) 链接、裸 URL | 抽屉 Browser Tab | 同左 |
| md / 图片 | 抽屉 File Tab | 抽屉预览 |
| html | Workbench（沿用终端规则） | 抽屉预览（wry，token 沙箱，无文件系统/Git 桥） |
| 视频 mp4/mov/m4v/webm | 抽屉 File Tab | 抽屉预览 |
| 文本/代码 | Workbench | 系统 |
| 目录 | 项目文件 Tab | 系统 |
| pdf/zip/office 等 | 系统 | 系统 |

## 6. 行为与边界

- 相对路径以任务所属项目根解析（与 `open_desktop_artifact` 校验一致）。
- HTML：允许 JS 与外部网络（需求方已选）。缓解：导航锁定本地 token 源，页面内跳转/`window.open` 的外链改走 Browser Tab；使用非持久 data store；不注入任何 IPC/文件系统/Git 桥。已知风险：本地 HTML 可外联，须在 UI 上不暗示其可信。

- 渲染不碰磁盘；点击时在后台线程 `canonicalize` + `is_file` 校验，失败 toast「文件已移动或不存在」（复用 `drawers.file_moved`）。
- 路径与链接统一样式（下划线/链接色），含 hover、按下、键盘焦点状态。
- 新增文案同步 `crates/ui/locales/` 中英文，跑 `scripts/check-i18n.py`、`scripts/test-i18n.py`。
- 安全：只信任点击时规范化后的路径；拒绝 `javascript:`；非 http(s)/本地路径 scheme 交给系统或拒绝；不新增 wry 桥。

## 7. 开放点（实现前确认）

1. 现有 hook 约定 Agent 用 `` `path` `` 写路径并由 Norma 转链接：需查该转换的实现位置，避免与识别层重复。
2. 抽屉渲染方案由第 0 步 spike 决定（见 §9）。
3. `:line` 后缀只剥离不跳行。
4. 用户消息改 Markdown 的行为变化：是否对用户消息仅启用链接/路径识别、不渲染其它 Markdown 语法。

## 8. 验收

- 单测（TDD 先行）：`rich_link.rs` 识别与路由表，覆盖项目内外、相对路径、`:line` 后缀、`~`、`javascript:` 拒绝。
- desktop-e2e：幕僚模块新增场景并登记看图版（`--visual`），覆盖 md、html、图片、视频、外链；中英文、浅色深色各一遍。
- 约束：每个 Rust 文件 ≤ 500 行；无 `#[allow]`；`scripts/check-resource-lifecycle.py` 通过。

## 9. 实现顺序

0. Spike：在抽屉里用 wry 加载本地 html（含 JS）与 mp4，原生渲染 md 与图片；验证层级、尺寸、焦点、token 沙箱与导航锁定，产出结论后更新本文 §3、§4。
1. `rich_link.rs` 纯函数 + 单测。
2. vendor gpui-component 补丁（回调、行内 code、图片）。
3. 幕僚 Prose 接入并分发到抽屉；补 i18n。
4. 幕僚活动/卡片、用户消息改 Markdown 并接入。
5. desktop-e2e 场景与上游 PR。
