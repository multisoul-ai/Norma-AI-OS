# iOS 工作空间实时活动：实现与验收

状态：代码已接通；推送通道尚未发布。当前物理 iPhone 11 在 `devicectl` 中为 `unavailable`，后台 APNs 真机验收未完成。

## 已实现

- `norma::mobile::WorkspaceActivitySummary` 从桌面工作空间、项目区域、具名 Agent 的卡片与审批事实、冻结交付记录生成统一摘要。先计算全量计数，再稳定排序和截取三个项目；计划的完成步数、总步数、当前动作保留在所属 Agent 上。
- 仅完成、已冻结、可读取、无冲突且未验收的成果进入验收队列。历史交付使用原节点、Provider、运行、会话、回合和指纹；已移除的 Agent 明确标识，不计作运行 Agent。
- Mac 通过现有保留项目会话读取内存事实，不因查看摘要加载项目或启动 PTY。数据库、冻结成果读取、网络发送与回源等待均在后台执行。
- 实时活动登记与普通通知登记独立，按设备授权版本和订阅版本校验；启动前持久预约周期，迟到周期不能覆盖当前活动。权限撤销阻止后续读取和发送。
- `workspace_activity` RPC 复用已有认证、局域网/加密中继、命令账本及工具审批/交付验收入口。未知字段拒绝；提交前再次核验原目标。响应丢失后查询原操作，不重放决策。确认成功的回执必须匹配原操作、区域、节点、运行、会话、回合和原决策/成果指纹。
- iOS 增加 ActivityKit 管理、WidgetKit 扩展、工作空间跟踪开关、锁屏项目深链与回源详情。token 观察从 UIApplicationDelegate 启动，覆盖无 SwiftUI 场景的后台启动。跟踪设置未知响应只回查恢复，不重复写入。
- iOS 17.2 起使用 push-to-start；17.0/17.1 仅在前台启动，随后使用活动更新 token。结束并观察真实空闲后可开始新工作周期；八小时连续工作不自动循环。
- 普通进度合并 30 秒、每 60 秒保活、120 秒过期；审批/失败/成果验收立即更新并通过普通通知提醒。新旧通知入口共享持久事项去重。完成卡片本身不生成成果提醒。结束结果保留 600 秒，收件箱入口独立于跟踪开关。
- Node 服务复用 APNs 连接、授权和 HTTP 入口，使用独立 `liveactivity` push type/topic。远程 start 携带一次 alert，update 无 alert/sound；字段、身份、时间、内容和 4 KiB 预算均校验。

## 自动化证据

| 验证 | 本轮结果/证据 |
|---|---|
| Norma + Storage | 314 项通过；覆盖三项目筛选、隐藏计数、稳定顺序、Agent 步骤归属、审批消项、完整身份、交付资格、启动预约、迟到 token、撤权、八小时抑制和过期恢复。 |
| 跨 crate 单元与集成测试 | `cargo test --workspace --exclude e2e --lib --tests`：1665 项通过，13 项忽略；禁用的旧 e2e 未运行。 |
| 完整 workspace 测试 | 编译受既有 `crates/agent/examples/norma_live_smoke.rs` 阻塞：缺少 `AdapterCommand::WorkspaceMemory` 分支；未修改其他任务的示例。 |
| App API | `cargo test -p app workspace_activity --lib`：2 项通过，覆盖解析与授权范围；已有派工交付身份、推送拒绝分类聚焦测试各 1 项通过。 |
| App 全量单元测试 | 最新一轮 944 项通过、7 项忽略、4 项失败：人格英文布局、导航英文标题、豆包反馈、MiniMax 反馈测试读到了中文全局语言状态。四项分别在独立进程复验均通过；现有语言测试会切换进程全局 locale，本轮未修改该任务的语言实现或测试。该全量结果仍记为未通过。 |
| Mac 编译 | `cargo check -p app` 与 `cargo build -p app --release` 均通过。 |
| Swift Package | 111 项 XCTest（2 项跳过）及 23 项 Swift Testing 通过。 |
| 原生 iOS | 8 项测试中 7 项通过，1 项仅供显式视觉验收的样本测试跳过；包括真实 ActivityKit request/update/end、扩展安装、登记丢响应后的只读恢复。 |
| APNs 服务协议 | Node 4 项通过；本地 HTTP/2 检查签名、topic、静默更新、远程启动提示、去重、复合 scope、4 KiB 和过期预算。 |
| 工程护栏 | 存储依赖、热路径、行数、禁止 allow、Agent 指南行数检查通过。 |

## 原生界面证据

Computer Use 实际操作独立 iOS 26.3 模拟器上的 Norma App 与系统锁屏，未用 HTML 或静态设计图代替原生界面。

- 三项目卡片实际出现，项目、具名 Agent、动作、审批/验收计数可见。
- 本地 ActivityKit 样本超过两分钟后，系统 Widget 显示“状态已过期”。
- 点击项目打开工作空间详情；测试 Mac 身份不在配对记录中时明确停止连接，无批准或验收动作。该样本不具有真实 Mac 权限。
- 工作空间设置中的跟踪开关独立存在，离线时禁用；遵循现有 iOS 浅色原生布局、语义色、动态字体与标准控件。

截图：`docs/exec-plans/evidence/ios-workspace-activity/lock-screen-stale.png` 与 `detail-identity-rejected.png`。样本由显式 `NORMA_ACTIVITY_VISUAL=fresh|stale|cleanup` 原生测试生成，没有使用真实任务或 APNs 发送；验收结束后 cleanup 原生测试通过，已结束本测试 generation 的活动。

## 发布门槛与证据分层

目前只有本地服务协议接受与模拟器系统实际展示的证据。本轮没有新实时活动请求经线上服务或 Apple 接受的证据，模拟器本地 request 不能证明真机后台自动启动。

连接并解锁可用 iPhone 后，使用签名构建与相同环境的私有推送服务验证：

1. App 在后台/未运行时，指定 Mac 工作开始，远程 start 自动出现且仅提示一次；记录服务/APNs 接受响应和设备展示分别对应的 event/cycle ID。
2. push-to-start 与更新 token 回调、token 轮换、重启和重复启动抑制；普通通知关闭时实时活动仍更新，关闭实时活动时收件箱和普通通知继续工作。
3. 普通进度合并、审批/失败/验收立即更新；断网两分钟过期、恢复后刷新；权限撤销后停止发送。
4. 点击项目、审批与冻结成果回源核验；响应丢失只查原操作；已处理或身份变化的旧目标不能再次操作。
5. 最后事项处理后结束，结果保留十分钟；连续八小时结束不循环启动，收件箱仍可访问事项。

真机全部通过后才部署推送服务；部署必须同时包含 `relay.mjs` 与其导入的 `activity.mjs`。不把线上旧普通通知的 accepted 回执作为新实时活动验收证据。
