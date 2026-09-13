# 后台原卡草稿保存失败修复

状态：代码修复及回归通过；用户随后授权发布，修复已进入 TestFlight 0.1.0（30）的原内测组。旧构建 29 仍包含此问题。

## 根因

后台加载原卡时，桌面保留已存储的 `session_id`，但没有启动 PTY，因此 `run_id=null`。卡片快照和发送协议支持这种状态，iOS `ChatDraft.Scope.validate()` 却沿用“有 session 必须有 run”的旧限制，导致只有 `hi` 的草稿也不能写入。

`ChatDraftBook` 共享存储全部草稿，一条草稿验证失败会设置全局 `failure`；`ChatComposer` 为保护未保存正文而禁用发送，因此错误影响其他卡片。界面将验证和磁盘错误统一显示为“草稿尚未保存…缩短内容或重试”，没有解释真正原因。截图中的草稿列表使用内存数据，出现正文不代表已经落盘。

## 改动

- 普通草稿允许保留历史 session 而尚无 run；仍验证全部 ID 格式，并按完整机器／项目／路由／卡片／运行／会话隔离，不自动迁移旧草稿。
- 成果反馈继续要求准确 run 和 session；错误会话回执不能清空正文。保留原文件保护、容量上限、原子写入和发送前保存机制。
- 补充实际 Swift 草稿链路和原生聊天用例，弥补上一轮后台测试只验证发送协议、未覆盖历史会话草稿保存的缺口。

## 验证

- 修改前，`swift test --package-path ios --filter BackgroundDraftTests` 在 0.097s 内复现完全相同的提示；数据为临时空目录、2 字节 `hi`，排除该复现中的容量及手机文件保护因素。
- 修改后 Swift Core 63 项通过；保存、重读、其他卡片隔离、错误回执拒绝和准确回执清空均通过。
- 独立 iPhone 模拟器：正式草稿单测 6 项通过，1 项物理设备文件保护测试明确跳过；原生聊天“输入 hi → 保存 → 重启 → 恢复原文 → 点击发送 → 准确回执清空”通过。真实 TLS/WSS fixture 仅收到一条 `hi`，不调用云 Provider。
- `cargo test -p e2e`、Rust 格式、行数、禁止 allow 和 diff 检查通过。未修改桌面生产逻辑，未重打桌面或发布 TestFlight。

证据：`target/norma-background-draft-evidence/`。用户物理 iPhone 的本地文件未读取；结论来自与后台原卡契约一致、可重复触发同一提示的本地及原生流程。

后续发布证据：`target/norma-ios-testflight/draft-build30/`，见 [发布记录](2026-09-08-norma-ios-testflight.md)。桌面继续使用配套 0.1.71（20260912.154818），无需为此修复更新。
