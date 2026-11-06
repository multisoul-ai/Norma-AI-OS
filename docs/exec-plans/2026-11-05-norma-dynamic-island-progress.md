# 灵动岛实施进度与证据

权威范围与验收：`2026-11-05-norma-dynamic-island.md`。用户最新指令取消 M0 失败即停门槛，先完成所有功能，使用单测、E2E 和真实 App 验收。

| 里程碑 | 交付与测试边界 | 状态 |
| --- | --- | --- |
| M0 | GPUI PopUp、AppKit 屏幕与 Spaces、Carbon；原生 E2E 焦点证据 | API/几何已验证；非激活输入未通过；继续功能实施 |
| M1 | runtime::island 纯状态机：形态、优先级、160/420/3000ms、固定、输入焦点、设置 | PR #186 已合入 dev，最新 rust checks 通过 |
| M2 | 逐工作空间捕获与后台投影、App 级 IslandModel，含后台保留工作空间 | 实现与聚焦/桌面模块回归通过，待 PR/CI |
| M3 | 每屏岛、窗口尺寸、三页签、审批/提问/成果、跨工作空间定位/重开 | 待实施 |
| M4 | 全局 BrainHost、带 route 的事件、多项目 Read 闸门、幂等迁移、reply/report/TTS | 待实施 |
| M5 | 删除左抽屉/状态胶囊和死入口，新增开关/快捷键与双语 | 待实施 |
| M6 | island --visual 模块、全检查、系统焦点/全屏/屏幕与真实模型汇报 | 待验收 |

## M0 证据摘要

独立 worktree `../norma-island-m0-20261105/` 保留探针与原始日志。真实 GPUIPanel 支持 canBecomeKeyWindow，窗口层级 101，collectionBehavior 写入/读回 337；Carbon 注册返回 0。仅有 DELL P2422H，NSScreen safeAreaInsets.top=0，与无刘海判定一致；几何单测经过 red/green。

用户实际反馈“输入不了”。原生 E2E 复用生产 ns_input.rs 和 capture.rs，先确认 key=false/app_active=false；点击后 Input 接受文字且前台 PID 未变，但 app_active=true，严格非激活断言失败。激活对照组能输入。截图已实际读取。此证据不能证明全局系统键盘投递，也不等于 GPUI 所有配置均不可行。输入使用已确认方案中的点击激活退路，审批必须另验收。

## 第 8 节验收映射

| 验收要求 | 最终证据 |
| --- | --- |
| 形态/优先级/自动收回 | M1 runtime::island 单测 |
| 其他 App 前台就地审批，不抢前台 | M3/M6 island E2E --visual + 前台 PID 断言 |
| 跨空间及已关窗口定位 | M2/M3/M6 island E2E |
| 全局幕僚对话和语音汇报 | M4/M6 真实 Claude Code + report/TTS 实际播放日志 |
| 旧数据无损迁移 | M4 brain 旧目录 fixture 单测 + 真实 App 启动 |
| 删除全部旧入口 | M5 代码检索 + 设置/快捷键 E2E |
| 刘海/外接屏/全屏 Space | M6 真机/原生 E2E；环境缺失不标为通过 |

PR #185（方案及原型）已在最新 CI rust checks 通过后合入 dev，merge commit e88dd4f5。

## M1 本地验证

- `runtime::island` 的四形态、160ms 悬停、420ms 离开、3000ms 完成通知经过先红后绿；时间由调用方传入，空闲时无截止时间。
- 9 个聚焦测试覆盖提醒优先级与队列、固定/输入焦点、默认与保留页签、独立弹出开关、隐藏不丢待办、重复指针事件与迟到 timer。
- `cargo test -p runtime --no-fail-fast`：351 passed，11 ignored，0 failed。首次全量中的既有 `opencode_question_host` 回执断言失败，单独复跑及完整确认轮均通过，未修改无关 OpenCode 代码。
- `cargo tree -p runtime --prefix none` 无 GPUI。状态机文件 265 行，测试按 crate 单元模块维护。
- 原始 red/green 与全量日志位于本 worktree `artifacts/island/`。PR #186 head `0f993fc5` 最新 CI run `42149147481` 的 rust checks 通过，正常 hook 与普通 merge 完成，合入提交 `723d99b5`。

## M2 测试边界

- App 级 `IslandModel`：按数据库、工作空间、项目作用域汇总；旧缓存不重复显示实时项目。同一区域若有两个独立 Runtime，保留两个宿主的待办，操作身份必须包含 source；共享成果只显示一次。完全相同的可见数据不通知。
- 活动捕获/投影：从所有保留会话捕获运行态、审批和提问；工作空间元数据、持久化状态与成果在后台读取，逐工作空间投影；旧后台结果不能覆盖新一代状态。
- 生命周期：关窗但保留项目时贡献不消失；真正停止宿主时清除贡献，迟到任务不能复活已释放的来源。
- 用户流程的真实 App 证据在 M3 接入窗口后补充，M2 数据测试不声明为桌面 E2E。

## M2 实现与本地证据

- `crates/app/src/island/model.rs` 保存 App 级缓存：目录/已读成果使用每数据库的最新刷新版本，实时贡献使用各宿主的最新版本。旧任务不能复活已删目录或已停止来源，较慢宿主的真实待办仍会被接收。
- `activity_cache/capture.rs` 仅捕获内存；独立 500ms 监控不依赖活动窗口。`project.rs` 在后台读取全部工作空间及成果并投影。当前窗口旧概览暂由 `current.rs` 保持原有内存缓存语义；其结果不会发布到全局模型，且全局读取失败不阻断它。M5 删除旧界面时移除这一兼容入口。
- 8 条 `island_` 聚焦测试通过：跨空间/后台提问、关窗保留与释放、相同数据零通知并验证观察器有效、双实时来源同 request ID、不复活已读成果、数据库隔离、乱序/释放回调、未打开空间删除、存储故障隔离。
- 既有桌面模块 288 条测试全部通过，按每批 8 条拆成 36 个独立进程，测试内容和断言未跳过。单进程扩展回归曾在视频壁纸 AVFoundation 解码中停滞；线程采样记录 dispatch 64 线程上限，拆分进程后包含这些场景的全部测试通过。未修改无关视频解码实现。
- storage/performance guardrails、500 行、no-allow、i18n 2172 双语键与 6 条 i18n 脚本测试通过。无新增用户可见文案。
- 独立对抗式审查发现的多宿主覆盖、旧目录占优、删除不刷新均经过失败用例后修复，并补充成果去重和通知正断言；后续兼容审查补充存储故障隔离。
- 原始证据：`artifacts/island/m2-final-tests.log`、`artifacts/island/m2-desktop-batches/report.json`、`artifacts/island/m2-desktop-hang.sample.txt`，以及同目录内 red/green 日志。
