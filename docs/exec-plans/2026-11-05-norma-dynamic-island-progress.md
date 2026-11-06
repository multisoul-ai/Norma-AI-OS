# 灵动岛实施进度与证据

权威范围与验收：`2026-11-05-norma-dynamic-island.md`。用户最新指令取消 M0 失败即停门槛，先完成所有功能，使用单测、E2E 和真实 App 验收。

| 里程碑 | 交付与测试边界 | 状态 |
| --- | --- | --- |
| M0 | GPUI PopUp、AppKit 屏幕与 Spaces、Carbon；原生 E2E 焦点证据 | API/几何已验证；非激活输入未通过；继续功能实施 |
| M1 | runtime::island 纯状态机：形态、优先级、160/420/3000ms、固定、输入焦点、设置 | 实现及本地验证完成，待 PR/CI |
| M2 | 逐工作空间捕获与后台投影、App 级 IslandModel，含后台保留工作空间 | 待实施 |
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
- 原始 red/green 与全量日志位于本 worktree `artifacts/island/`；下一步提交正常 hook，等待 PR 最新 CI 后合并。
