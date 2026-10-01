# 桌面工作空间：dev 整合记录

## 范围与备份

- Worktree：`norma-ai-os-desktop-workspaces`；分支：`codex/desktop-multi-project-workspaces`。
- 原 HEAD：`cefa10bbc0fe7790a14558278f85083a7f5d49f0`。
- 备份分支：`codex/backup-desktop-before-dev-20261002-001946`。
- 未提交工作备份：`codex/backup-desktop-before-dev-20261002-001946-wip`，指向 stash `ec724f0d4c6abf8976670f372055135d8f5b4e62`；备份引用和 stash 均保留。
- 补充备份与本地日志：`artifacts/merge-dev/20261002-001946/`，包含补丁、未提交文件归档、原始状态与 `backup.json`。
- 合入 `origin/dev@55ca610182c6093135019587d7b7e225e8197584`，合并提交 `549eca7480f2acd8988e7fff5763cef6fc6a613b`。

## 冲突决策

用户确认采用 dev 紧凑 Command Dock 和半透明渐变材质。保留桌面项目编号、当前窗口导航、创建时命名、固定排序、Agent 默认所属项目目录及最新规格。

16 个合并冲突已解决。恢复 stash 时额外处理 Browser 模块注册、Store 和 Wry 接线冲突，同时保留 dev 加载计时、预热及圆角优化与本分支的场景变换、裁剪、输入归属。单视图生命周期移到 `browser/store/view.rs`，满足单文件 500 行限制。

旧 `crates/e2e` 的未提交改动和模型软链接保留在本地，不纳入本次恢复提交；不运行旧套件。恢复时旧 E2E 的诊断文本冲突保留原 stash 内容。

## 行为回归

- 为桌面宿主补接项目命令建议、动画调度与键盘捕获；工作空间按钮返回当前窗口目录。
- 回归测试首先复现建议面板缺失；接线修复后复现一次 Enter 创建两张 Agent。异步命令受理后立即消费草稿，阻止重复事件再次提交；目录验证及创建失败仍由宿主反馈。
- 总览 Dock 移除已过时的画布模式和失效的创建按钮，保留添加项目、项目切换、工作空间和设置入口。
- dev 的快捷键录制测试改用未占用的 `Ctrl+Alt+K`，保留 `Cmd+Shift+K` 工作空间快捷键。

## 验证证据

| 检查 | 结果 | 本地日志 |
| --- | --- | --- |
| 合并版本 `cargo test --workspace --no-fail-fast -- --test-threads=1` | 1729 passed，31 ignored | `workspace-green.log` |
| 最终代码 `cargo test --workspace --no-fail-fast -- --test-threads=1` | 1735 passed，31 ignored | `final-workspace-tests.log` |
| 恢复后 `cargo test -p app --lib -- --test-threads=1` | 883 passed，7 ignored | `restored-app-tests.log` |
| GPUI 独立 manifest 的 `scene_native` 单测 | 1 passed | `gpui-native-tests.log` |
| `cargo check -p app` | 通过，34 warnings | `restored-check.log` |
| `cargo check -p app --features chromium-browser` | 通过，34 warnings | `chromium-check.log` |
| `cargo build -p app --release` | 通过，34 warnings | `release.log` |
| 行数、禁止 allow、指南行数、存储／TTS边界、性能、视频、代理、签名护栏 | 通过 | `guards.log`、`restored-guards.log` |
| Computer Use | 连接既有隔离 App 超时，未获得本轮真实窗口证据 | `computer-use.txt` |

GPUI 是独立排除的 vendor crate，单测通过其 `--manifest-path` 执行，不能用根 workspace 的 `-p gpui`。Browser 拆文件后，同步更新了创建失败日志静态测试的源码路径，未放宽断言。

## 交付边界

普通推送当前分支，创建目标为 `dev` 的 PR，**不合并**。本次记录证明合并和列出的工程回归；统一场景 Goal 保持 ACTIVE，跨工作空间连续重定向、完整真实 App 交互、性能验收和用户原型验收仍按原计划推进。
