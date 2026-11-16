# 富文本点击在右侧抽屉打开

状态：已实现（真实 App 验收通过，英文/浅色待验）｜日期：2026-11-15

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

## 3. 实现结果（以代码为准）

- Spike 结论：抽屉 File Tab 已能渲染 md/html/图片/视频，无需新渲染器，只需分发入口。
- vendor `gpui-component 0.5.1`（`vendor/gpui-component-0.5.1/NORMA_PATCH.md`）：新增全局 `LinkClickHandler`，文本链接与图片链接点击先走它，无则回落 `cx.open_url`；`[patch.crates-io]` 指向 vendor。
- `crates/app/src/preview/rich_link.rs`（纯函数，单测在 `crates/app/tests/unit/rich_link_tests.rs`）：
  - `classify_target`：Web / Path / System / Reject（拒绝 `javascript:`、`data:` 等）。
  - `linkify_inline_paths`：行内代码路径 → `norma-path:` 链接；`![](src)` 套同目标链接；裸 URL → `<url>`；代码块与已有链接不动。
  - `linkify_plain_text`：用户消息，Markdown 语法按字面转义，仅裸 URL 可点。
- `crates/app/src/app/staff/open_link.rs`：按窗口登记 `MultisoulApp`（窗口根视图是 `Root`），点击转给宿主；路径依次以当前项目、其余项目、`/` 为基准在后台线程解析，复用 `plan_terminal_path` 路由；失败 toast `drawers.file_moved`；Web 开抽屉 Browser Tab，复用/新开 Tab 并自动展开抽屉。
- 入口：幕僚回复正文、用户消息（含 URL 时）。

## 4. 路由规则

| 目标 | 项目内 | 项目外 |
|---|---|---|
| http(s) | 抽屉 Browser Tab | 同左 |
| md / 图片 / 视频 / html | 抽屉 File Tab | 抽屉预览 |
| 文本/代码 | Workbench | 系统 |
| 目录 | 工作台定位 | 系统 |
| pdf/zip/office、mailto | 系统 | 系统 |

## 5. 边界与风险

- HTML 允许 JS 与外部网络（需求方已选）：沿用现有 File Tab 的 wry token 源，不新增 IPC/文件系统/Git 桥；页面内导航限制未实现，属已知风险。
- 点击时才校验路径；`:行` 后缀由 `parse_path_ref` 剥离，不跳行。
- 未做：幕僚活动摘要/Agent 卡片文本改 Markdown、画布 Agent 卡片入口、上游 PR。

## 6. 验收

- 单测：`cargo test -p app --lib rich_link`。
- desktop-e2e：`bash scripts/desktop-e2e.sh staff-rich-link --visual`（HTML、md、缺失路径 toast、外链四步）。
