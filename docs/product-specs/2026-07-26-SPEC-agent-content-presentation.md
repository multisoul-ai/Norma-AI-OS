# SPEC：Agent 内容展示、Browser 与 Preview Master Layout

- 日期：2026-07-26
- 状态：Implemented
- 关联规格：
  [`2026-07-25-SPEC-agent-orchestration-installation-and-task-delegation.md`](2026-07-25-SPEC-agent-orchestration-installation-and-task-delegation.md)

## 1. Summary

已安装 Orchestration 集成的外部 Agent，可以通过受保护的 Norma CLI 命令把 Browser
或只读 Preview 卡片展示到当前项目。展示会创建或复用目标卡片，并立即令其成为
Master；原 Master 降为 Detail。

本能力只增加内容展示，不开放通用 Canvas 控制、批量布局、Terminal 操作或
`Cmd+M` 模拟。

## 2. User Experience

### 2.1 触发规则

- 用户明确要求打开、展示、查看或预览内容时，Agent 必须展示。
- 生成的交付物或关键证据需要直接查看时，Agent 可以主动展示。
- 用户明确提供的文件路径可直接展示；Agent 主动选择当前项目外的文件时必须先征得用户
  确认。该边界由安装的展示 Skill 执行，因为 CLI 不能判断路径的语义来源。
- 单个用户回合最多串行展示三项。超过三项时按用户原始顺序展示前三项，并报告其余
  项目未展示。
- 只有用户明确要求“再打开一个”“创建副本”或等价语义时才创建副本。

### 2.2 布局与焦点

- 每次成功展示后，目标立即成为 Master。
- 已存在的 Master 自动降为 Detail。
- 新目标成为唯一选中卡片，但应用保持 Canvas 输入焦点，不聚焦 Browser 地址栏、
  网页或 Preview。
- 用户正在输入不会延迟展示。
- 展示生成的卡片是普通持久化 Canvas 节点；项目重开后仍存在。

### 2.3 通知

- 成功展示后，工作台右上角出现可关闭通知，五秒后自动消失。
- 通知使用现有工作台的半透明面板、细边框和明确层级，符合 macOS 即时反馈习惯。
- 同一 Agent 在五秒窗口内连续展示会合并计数，例如
  `Chloe presented 3 items`。
- 不同 Agent 的新展示替换当前通知并重新计时。

## 3. CLI Contract

```bash
norma present browser [url] [--new] [--format json|text]
norma present preview <path> [--type markdown|text|image] [--new] [--format json|text]
```

- 默认格式是 JSON；人工调试可选择 `--format text`。
- Browser 省略 URL 时使用 `https://www.google.com`。
- 没有 scheme 的地址补 `https://`。
- 允许 `http:`、`https:`、`file:` 和 URL 解析器接受的自定义 scheme。
- 拒绝 `data:` 和 `javascript:`。
- Preview 相对路径以 CLI 当前工作目录解析并转换为 canonical absolute path。
- Preview 允许用户机器上的任意可读普通文件，不要求位于当前项目。
- 用户显式提供的任意可读路径不需要二次确认；Agent 主动选择的项目外路径必须先确认。
- Preview 类型只包括 Markdown、UTF-8 text 和 image；显式 `--type` 可覆盖扩展名推断。
- Preview 严格只读，不修改源文件；Canvas 自身状态仍正常持久化。

成功 JSON：

```json
{
  "schema_version": 1,
  "ok": true,
  "request_id": "42",
  "result": {
    "node_id": 17,
    "kind": "browser",
    "disposition": "reused",
    "layout": "master"
  }
}
```

成功表示卡片已经创建或复用且 Master Layout 已应用，不表示网页或文件已经加载完成。

稳定 Presentation 错误码：

- `invalid_url`
- `forbidden_url_scheme`
- `preview_not_found`
- `preview_not_file`
- `preview_unreadable`
- `unsupported_preview_type`
- `presentation_failed`

本地确定性校验错误使用退出码 5；内部展示失败使用退出码 1。App/项目不可用、授权和
来源 Agent 错误继续使用既有编排错误码与退出码。

## 4. Reuse and Failure Semantics

### 4.1 Browser

- 默认按卡片的当前规范化 URL 精确匹配。
- 用户从 A 导航到 B 后，该卡片只匹配 B。
- 复用 Browser 时不 reload，保留滚动、登录和交互状态。
- `--new` 始终创建新 Browser。

### 4.2 Preview

- 默认按 canonical absolute path 精确匹配。
- 复用 Preview 时保留 node id，采用本次显式类型并重新启动后台文件读取。
- 文件在 CLI 预检成功后被删除或变为不可读时，卡片显示现有异步加载错误。
- `--new` 始终创建新 Preview。

### 4.3 多项与部分失败

- Agent 通过最多三次独立 CLI 调用串行展示，不提供 batch 命令。
- 确定性失败不回滚之前已经成功的项目，并继续处理后续项目。
- 副作用结果未知时立即停止，不重试。
- 每次调用返回独立 request id 和展示回执。

## 5. Authorization and Installation

- `present` 与 `agent` 高层命令使用同一个 Provider Token。
- 请求必须带仍然存在、启用且 profile 匹配的来源 AgentCard node id。
- App 在任何卡片或布局副作用前校验项目、Token 和来源 Agent。
- Grok Build 继续不支持 Orchestration 安装和内容展示。

一次 Orchestration 安装管理两个独立能力文件：

- `norma-orchestration`：只描述明确授权的任务委派。
- `norma-presentation`：描述 Browser/Preview 展示决策和最多三项规则。

Settings 继续显示一个 Skill 聚合组件。两个文件都是当前版本时才 Ready；任意一个
缺失、过期、读取失败或冲突时显示 Needs repair/Install failed。旧安装在升级后因
缺少 `norma-presentation` 显示 Needs repair，用户点击 Repair 后只补齐缺失内容。
现有 Agent session 必须重启后才能发现新 Skill。

卸载只删除带对应 Norma managed marker 的两个文件，并保留用户自定义文件。

## 6. Persistence, Logging and Privacy

- 创建或复用后的 Canvas 与 Master Layout 通过现有持久化调度保存，不新增 migration。
- `norma_presentation` 日志记录 request id、provider、source node、command、
  node id、created/reused、结果和耗时。
- Browser 目标只记录 scheme、host 与完整目标的 SHA-256；不记录完整 URL 或 query。
- Preview 目标只记录文件名与 canonical path 的 SHA-256；不记录完整绝对路径。
- Provider Token 不进入 Skill、stdout、普通日志或 UI 状态。

## 7. Acceptance Evidence

| 验收行为 | 自动化证据 |
|---|---|
| CLI 命令、默认 JSON、URL/路径预检和稳定错误码 | `crates/norma/tests/presentation_cli_tests.rs` |
| 双 Skill 安装、旧安装修复和安全卸载 | `crates/norma/tests/orchestration_installer_tests.rs` |
| URL/path 复用、`--new` 与 Master 切换 | `crates/app/tests/unit/norma_presentation_command_tests.rs` |
| Token/来源校验、Preview loader 与通知合并 | `crates/app/tests/unit/norma_presentation_lifecycle_tests.rs` |
| Browser A→B 导航后的当前 URL 复用 | `crates/app/tests/unit/browser_location_tests.rs`；`crates/e2e/src/e2e_12_agent_presentation.rs` |
| WireRequest 到 Browser/Preview/Master/通知的用户流程 | `crates/e2e/src/e2e_12_agent_presentation.rs` |
| 真实 `.app` 的通知、Master/Detail 和焦点视觉状态 | Computer Use 验收截图与报告 |

## 8. Out of Scope

- PDF、Office、视频或新的 Preview 内容类型。
- 通用 `layout`、batch present、等待页面加载、查询加载状态。
- 修改 Preview 源文件。
- 自动启动 Norma OS、自动选择最近项目。
- 通用 Browser 自动化、Terminal 控制或完整 Canvas API。
