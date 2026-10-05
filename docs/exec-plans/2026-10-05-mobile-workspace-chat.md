# 手机工作空间群聊改造

## 已确认边界

手机与 Mac 共用 DesktopWorkspace / DesktopProjectArea / Agent 卡片。消息直接归属桌面空间，想法独立，通过引用送入群聊。普通讨论交给 Mac Norma；明确执行意图才允许工具派工。首期一台 Mac、多项目，局域网与中继共用业务入口。本地验收，不部署。

手机长期保存历史、草稿、操作 ID 与同步游标；Mac 持久保存接收与执行事件。旧线程只读，旧待发操作暂停，不按名字迁移到桌面空间。

## 开发与验收矩阵

| 顺序 | 实现 | 验收 |
|---|---|---|
| 1 | 持久操作与事件、目录及配置版本 | 去重、重启回执、错误空间隔离、配置冲突 |
| 2 | LAN / relay 相同接口 | 直连不依赖模型凭据、查回执后重试 |
| 3 | 精确卡片与 Mac Norma | 同名卡片不误路由、切换空间不影响目标、讨论不派工 |
| 4 | iOS 群聊 / 设置 / 想法引用 | Swift 核心、原生模拟器、旧记录只读 |
| 5 | 清理与真实闭环 | Rust / 本地中继、Computer Use、独立 UI 复核 |

## 实施记录

- 基线：dev；现存桌面改动已备份到 target/mobile-workspace-chat-2026-10-05/baseline，保留原样。
- 首个失败测试：`cargo test -p storage --test workspace_chat`，缺少 WorkspaceChatStore，符合预期。
- 新协议保持 version=1 和原配对；使用 `desktop_workspace_id`，不改变旧项目 workspace_id。
- 真实扫码与蜂窝网络验收待用户执行，不计入本地通过证据。
