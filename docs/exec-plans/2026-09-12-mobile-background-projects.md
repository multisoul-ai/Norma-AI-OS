# 手机后台项目执行计划

已确认范围：手机访问已授权项目不打开桌面窗口。手机服务开启后关窗继续运行，主动退出 Norma 才停止；不引入独立 daemon 或开机启动。桌面和手机共享原卡、Runtime、PTY 与 Provider 会话。未收到任务时不批量启动历史 Agent。

## 实现与验收

- [x] 进程级项目注册表：按数据库和 workspace ID 隔离；并发请求共享加载；后台读取存储后创建无窗口实体。
- [x] 原生输入：冷卡按原权限和精确会话按需启动，手机消息仍通过原终端粘贴、Enter、Provider Hook 确认。首次 Codex 输入不依赖预先生成 session ID。
- [x] 关窗与重开：保留同一实体、路由与运行身份；重新打开窗口时附着原实例，仅恢复尚未运行的桌面卡片。
- [x] 主动退出：停止手机服务、取消待加载项目与原生启动、注销路由、保存脏状态并终止 PTY。未启用服务的项目关闭仍释放会话。
- [x] 持久化：未启动卡片的准确 session/cwd 保留；checkpoint 合并保留原网格/shell，删除或换类型节点不复活。
- [x] 后台命令：普通卡片和编排请求解除窗口依赖，真正需要显示窗口的聚焦/展示命令明确返回 unsupported。
- [x] 文案：iOS 不再要求在桌面打开项目；Mac Settings 用英文说明后台生命周期。
- [x] 完成受影响 crate 与全工作区回归、iOS 构建和机械约束检查。
- [x] 隔离真实 App 的 Computer Use 验收。

## 自动化证据

- `e2e_mobile_background`：真实回环 WSS 读取、权限/数据库隔离、无额外窗口、同一实体重开、退出路由清理及后台 CLI。
- `e2e_mobile_background_messages`：子进程隔离 HOME/ZDOTDIR，用模拟 Codex/Claude 和真实 PTY 验证首条输入及准确回执；不访问用户真实 Provider。
- `cold_native_card_restores_only_requested_session`：原卡权限与精确 resume、旁卡未启动、重复启动拒绝。
- `native_launch_cancellation_blocks_spawn_and_late_attach`：取消后的启动和删除后的晚到附着均拒绝。
- `background_flush_preserves_inactive_checkpoints_and_removes_deleted_nodes`：保留历史终端尺寸/shell，删除卡片不复活。
- `interrupted_restore_resumes_when_window_reattaches`：恢复途中关窗后重开继续；已经运行的终端不重复启动。
- `native_launch_preserves_process_observation`：后台启动原终端保留进程观察能力，结束后旧身份失效。
- `shared_desktop_owner_survives_windows_until_app_quit`：窗口全部释放后服务仍可使用，主动退出清除服务注册表和连接。

上述 GPUI 测试是 headless 测试；不将其称为真实桌面点击验收，也不将模拟 Provider 回执称为真实模型验证。

## 验收结果（2026-09-12）

- `cargo test -p runtime -p app -p e2e` 通过：App 804 项、Runtime 166 项、headless e2e 221 项及相关网络/持久化集成测试；真实云 Provider 等显式 ignored 用例未运行。
- 最终 `cargo test --workspace` 通过，退出码为 0，无失败测试；完整日志保存为 `target/norma-background-evidence/workspace-tests.log`。显式 ignored 用例仍未运行。
- Swift Core 62 项通过；`ProjectBoardFlowTests` 2 项原生 UI 测试通过，SwiftUI App 构建成功。
- `cargo check -p app` 和 `cargo build -p app --release` 通过；仅工具环境的重复 Cargo 配置提醒，无新增编译警告。
- 行数、禁止 allow、Agent 指南、存储边界、性能护栏及 `git diff --check` 通过。
- Computer Use 操作独立 `.app`：开启手机服务、创建原终端、关窗、首页重开原项目；关窗前后终端 PID 均为 80969。关窗后回环服务仍可连接；主动 Quit 后原 PTY 和监听均停止。验收二进制仅使用随机回环端口，避免与用户正在运行的 9443 服务冲突。
- Computer Use 操作独立 iPhone 模拟器：项目选择显示“点按加载项目”，不可用状态不再要求电脑开窗；fixture 发布可用状态后原页面自动显示第二项目卡片。这里使用合成项目服务，真实无窗口加载/输入由前述 WSS + PTY e2e 覆盖。测试配对已通过正式“忘记”流程清理，fixture 授权随测试结束撤销。
- 本地截图、日志和桌面生命周期记录位于 `target/norma-background-evidence/`。首次 iOS 验收 fixture 因中断超过 15 分钟过期，已通过专用清理测试移除并重建；最终验收使用独立新 fixture。

Apple 设计原则落地：保留现有原生项目选择和实时状态反馈，不引入额外“打开桌面”步骤；Mac Settings 明确区分关窗与主动退出，沿用原生窗口/Dock 操作。

## 实现边界

复用现有 `MultisoulApp` 实体承载项目，窗口是可解绑引用；关闭窗口后释放预览、视频和语音等显示资源，不创建隐藏窗口。旧协议 `open` 表示运行实例已加载，`closed` 表示尚未加载，访问失败保留明确状态。只生成本地验收构建，本轮不上传 TestFlight、不替换正在使用的桌面 App。

## 后续发布（用户另行授权，2026-09-12）

用户随后要求编译桌面并发布 TestFlight。桌面已生成 0.1.71（20260912.154818）的 Developer ID 签名 DMG，未公证、未安装；iOS 0.1.0（29）已进入原内测组并回读确认可安装。详见 [发布记录](2026-09-08-norma-ios-release-records.md#release-testflight)。
