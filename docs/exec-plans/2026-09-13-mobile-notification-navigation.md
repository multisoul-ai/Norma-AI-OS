# 手机通知定位与完成摘要

完成/失败通知核验电脑授权、项目、路由、节点、运行与会话后，直接进入原卡片对话。Provider 完成时会清空活动回合，因此只读会话导航不要求活动回合仍等于通知回合；审批保留精确回合/请求核验。换窗口、换运行、换会话仍拒绝旧通知。

通知使用系统 APNs 标题显示卡片名称、编号和状态，副标题显示项目名称，正文显示该回合冻结的最终输出摘录。摘要沿用 Runtime 已有 TaskSummary，不额外调用模型。终态与摘要一起后台持久化，缺失时明确提示打开卡片查看，不读取下一回合的摘要。项目/卡片/正文 UTF-8 上限为 160/240/640 字节；所有身份字段和引号转义取上限时请求为 3846 字节，小于 4096 字节。

参考 `.agents/skills/apple-design/SKILL.md`：采用直接导航、明确名称和系统通知层级；复用 SwiftUI 对话及系统按钮/加载反馈、Dynamic Type，无新增装饰动效。APNs 预览与 `norma` 路由身份分离，旧 Mac 不带预览仍兼容。

| 验收 | 自动化证据 |
| --- | --- |
| 完成后活动回合为空、下一回合开始仍能进入原会话；旧审批/其他会话拒绝 | `ios/Tests/NormaCoreTests/PushNotificationTests.swift` |
| 冻结原回合输出，不被下一回合覆盖 | storage `mobile_turn_outcomes`、runtime `mobile_task_summary_tests` / `mobile_codex_message_tests` |
| 项目、卡片、结果摘要从 Mac 经 relay 进入系统 alert | `crates/e2e/tests/mobile_push_delivery.rs`、`services/push/relay.test.mjs` |
| UTF-8、转义、长度、无摘要兜底、未知字段拒绝 | `mobile_push_preview_tests`、relay 边界测试 |
| 完成清空回合后，冷启动/已有弹窗点击通知直接显示对话；错路由拒绝 | `mobile_notification_native` fixture + `NotificationFlowUITests` |

发布顺序：先更新 push relay，再更新 Mac 和 iOS。初始开发阶段仅改代码；2026-09-14 按用户后续授权完成 47 的服务部署和入口切换，详见下文。本次部署未替换正在运行的桌面进程，未上传 TestFlight。

已通过：Swift 全套64项（最新通知聚焦测试另通过）；push relay 3项（含HTTP/2与最大转义边界）；Mac通知worker真实SQLite/HTTP集成；Runtime 168项（另2项ignored，包含原生Codex摘要、受控Codex/Claude冻结摘要回归）；iOS Simulator应用构建；行数/no-allow/指南/存储/性能护栏。`cargo test --workspace` 和 `cargo check -p app` 均已通过；workspace包含完整受影响crate及常规E2E（显式ignored的真机/Provider测试不在此结论内）。Computer Use 连接 Simulator 两次超时，全局状态查询也超时，当前无法作为界面验收证据；XCTest 不替代 Computer Use 或物理 iPhone/APNs 验收。

原生测试补充限制：测试target已编译，但工作区同时更新Xcode工程并加入WebRTC依赖，测试过程停在Resolve Package Graph超过5分钟，已停止自己的测试进程，未改动其他任务的工程配置。未获得通知点击的XCTest/Computer Use完成证据。

2026-09-14 线上排障：安装中的 Mac 二进制含 `push/preview.rs`，旧 relay 对带预览的合法诊断事件返回 `400 invalid_event`；不带预览的无效测试 token 探针通过入口并返回 `422 token_invalid`。Mac 的错误分类此前把协议拒绝掩盖成 `delivery_unknown`，现已修复，并验证未知正文不会进入日志、伪造成功回执不会报成功、400 不引发重复投递。代码修复验证通过：`cargo check -p app`、通知单测 4 项、relay 3 项及 `cargo test -p e2e`（270 passed、75 ignored），行数/no-allow/diff 检查通过。

2026-09-14 部署完成：经用户授权，通过 `47.251.242.24:2222` 将现有支持 preview 的 `services/push/relay.mjs` 安装到 `/opt/norma-push/`，复用 Node 20.18.1（`/usr/local/bin/node`）和原 relay/APNs 凭据。systemd 服务 enabled、active/running，限制为 CPU 20%、内存 128 MiB、Tasks 32，仅监听 `127.0.0.1:8788`。线上原 `/norma/ → https://118.31.118.73` 已改为本机 relay；本轮未登录或修改 118。公网 URL 保持 `https://soulvoiceai.com/norma/v1/notifications`，配置与回滚备份见 `services/push/README.md`。

部署验证：47 的现有 Node 运行 relay 3 项测试通过，部署文件哈希一致；`nginx -t`、reload、服务状态、本机/公网 health 均通过。网站首页切换前后 SHA-256 一致，Nginx 主 PID 未变。公网实际 remote IP 为 47，无凭据请求为 401，正确凭据加非法事件为 400。向已登记的 production iPhone 发送单条带 preview 的诊断事件 `cc167f44-112b-4ae9-9a9c-4c0b85f49919`，Apple 返回 `200 accepted`；使用当前卡片 #507 的定位身份与新事件 ID，未重放历史任务。此结果证明 Apple 接受投递，尚未取得手机展示、锁屏点击或新任务自动触发的真机证据。
