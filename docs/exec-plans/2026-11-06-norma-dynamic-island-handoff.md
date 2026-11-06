# Norma 灵动岛剩余任务交接

- 整理时间：2026-11-06。
- 用户最新指定接手人：Grace（已取代此前 lris；分派前须通过 Norma roster 确认 canonical name）。
- 分派状态：此前因 `unauthorized` 未发送；用户恢复授权后已确认同工作空间源为 Henry、目标为 Grace（managed，空闲），现在正式分派。最终回执另记。

## 1. 接手目标与用户最新约束

接续现有 Goal，把灵动岛、全局幕僚及验收全部做完。用户最新顺序为 **M5 → M4 剩余 → M6**；不要重新讨论范围，也不要推倒重做已完成代码。

1. **只提 PR，绝不合入 dev**，用户还要亲自测试。Goal 旧文字里的自动合入要求已被覆盖。
2. M0 缺少系统实证不再阻断功能开发；如实保留缺证。非激活 Input 采用已确认的点击激活 App 退路，就地审批仍不得抢前台。
3. 样式严格对照权威 HTML；业务测试通过不代表视觉还原完成。
4. TDD、正常 pre-commit/CI、Rust 单文件 ≤500 行、禁 `#[allow]`、前台不做 IO/LLM、所有用户文案中英 i18n。
5. 页面验收优先真实 App desktop-e2e `--visual`，逐张实际看图；不以内部状态/图片存在替代结果。
6. 在隔离 worktree 开发，不改主目录 dev；不结束用户正式 `/Applications/Norma OS.app`。
7. 所有 Cargo 命令使用 `CARGO_INCREMENTAL=0`，检查磁盘空间，勿盲目清理其他任务缓存。
8. 按 `AGENTS.md` / `CLAUDE.md` 执行。做 code review 时使用独立 subagent 对抗审查并核验发现。

## 2. 必读依据与工作位置

- 完整方案：`docs/exec-plans/2026-11-05-norma-dynamic-island.md`，尤其第 8 节逐行验收。
- 权威交互原型：`docs/exec-plans/evidence/2026-11-05-norma-dynamic-island/prototype.html`，必须用浏览器打开。
- 详细过程与证据：`docs/exec-plans/2026-11-05-norma-dynamic-island-progress.md`。
- worktree：`/Users/openclawd/Documents/code/norma-os/norma-island-state-20261105`。
- 当前分支：`feat/island-cleanup-20261106`；基点 `f1fabfe9`。
- M4 分支：`feat/island-global-brain-20261106`，已提交到 `f1fabfe9`，尚未提 PR。
- **当前 worktree 有未提交的 M5 设置、测试、E2E 和本文，须完整保留**。先运行 git status/diff，不 reset、checkout 覆盖或 stash 搬运。
- M3 PR #189：https://github.com/multisoul-ai/Norma-AI-OS/pull/189 ，未合入；已记录 head `331a193916d279843f23f62077690c12c6917078` 的 rust checks 成功。交付时重新查最新 head/CI。
- M1 #186、M2 #187 在禁止自动合入指令前已合入。后续仍拆多个 PR，注明依赖，可用叠加分支；不得代用户合并。

## 3. 已有成果，避免重复实现

| 范围 | 已有成果 | 证据与限制 |
| --- | --- | --- |
| M1/M2 | 纯状态机、跨工作空间汇总、后台投影 | 已有域层回归与 PR；复用 progress 中证据 |
| M3 | 独立岛窗口、待办/运行/成果、原生审批/问答、跨空间定位与关窗重开 | 已有 island-window、question、approval、remembered-approval、navigation、artifacts 的真实 `--visual` 证据；完整样式与系统层仍缺部分验收 |
| M4 基础 | 唯一 SharedBrain/BrainHost、带 scope 事件与状态、多根 ReadRoots、项目释放时双 lane 取消 | App/brain 单测及脚本 ACP 验证；不冒充真实 Claude/TTS |
| M4 迁移 | 后台原子幂等迁移到 `brain/global/current`，原目录备份保留；配置、提议、审批审计接入；JSONL 并发文件锁 | `island-migration --visual` 退出 0、1/1：`artifacts/desktop-workspace-e2e/2026-11-06T14-16-14Z/`；未启用 LLM/TTS |
| M5 设置（未提交） | 隐藏开关、审批/提问/完成独立开关，中英 UI；后台读取、串行保存、版本屏障；恢复隐藏时不创建岛；已有窗口原生隐藏/恢复 | App island 聚焦 28/28；settings 7/7；App build、Clippy 已通过；本轮设置 E2E 状态见第 7 节 |

M4 已有提交：`006834cc`（作用域）、`f4fda846`（唯一宿主）、`0cc63496`（精确取消）、`54793a1f`（迁移域层）、`f1fabfe9`（迁移 App 接线）。

## 4. 第一优先：完成 M5

### M5-A 设置收尾

- 本轮 `island-settings --visual` 已完成中英、精确偏好保存、重启恢复和原生窗口隐藏/出现验收，结果见第 7 节；后续按实际改动做增量回归。
- 补三类真实事件在开关关闭时不主动弹出、但手动仍可处理的证据；设置页可点击不代表事件投递已验证。
- 完成隐藏岛时的系统通知兜底，并证明仍可到达审批/提问处理入口。
- 补清除已记住精确授权的入口及回归（既有进度承诺归 M5）。

主要代码：`crates/app/src/island/preferences.rs`、`crates/app/src/island/preferences/persistence.rs`、`crates/app/src/settings/island.rs`；SQLite key 为 `island.preferences.v1`。

已修退出竞态：`crates/app/src/settings/general.rs` 的 `wait_for_all_settings` 同时复查语言与岛偏好的 revision；不要退回依次等待两个单独屏障。回归为 `quit_waits_for_language_changed_during_island_save`。

### M5-B 全局快捷键

- 生产接入 Carbon RegisterEventHotKey：注册、注销、重绑、键码映射、失败反馈。
- 在 `shortcuts.rs` 注册，并进入“设置 → 键盘快捷键”，支持录制、保存、重启恢复与恢复默认。
- 现有 shortcuts 的 `refresh_and_save` 同步写 SQLite；新增功能不得沿用前台 IO。
- 复用 spike 的窄 FFI 边界：`/Users/openclawd/Documents/code/norma-os/norma-island-m0-20261105/scripts/spikes/island-m0/src/native.rs`，不能照搬泄漏式生命周期。
- 验收必须包含别的 App 在前台时真实系统热键回调及岛开合；注册成功、GPUI 注入不能代替系统输入证据。

### M5-C 完整删除旧入口

- 删除左抽屉的对话和运行概览渲染、orb、left_view_slide、DesktopLeftView 及左侧状态/拖拽/动画。
- 删除 `status_capsule.rs` / `route_status_capsule`、相关顶栏入口、快捷键、设置、布局持久化字段、i18n 和 E2E 场景。
- 保留右抽屉、右侧内容区域、workspace 切换/ideas 刷新，以及岛复用的 overview 数据、投影、审批和问答链路。
- 检查画布留白、camera/scene/view、animation_driver、layout_save 退出行为，避免删 UI 后残留空白或无效线程。
- 库存：`artifacts/island/m5-old-entry-inventory.txt`、`artifacts/island/m5-retirement-inventory.json`。
- 重点：`crates/runtime/src/desktop_drawers.rs`、`crates/app/src/workspace/desktop.rs`、`crates/app/src/workspace/desktop/drawers.rs`、`crates/storage/src/desktop_workspace_store.rs` 及 desktop migration。
- 旧 E2E 的 WorkspaceRunOverview、WorkspaceDrawerTabs、WorkspaceOverviewFloat、WorkspaceStatusCapsule 应删除；CanvasDrawers/CanvasProjectDrawers 与 brain 场景按保留功能迁移，不能整批删除有效断言。
- 验收：精确源码检索无死入口，设置/快捷键页及画布布局真实 E2E，右侧内容区回归，旧持久化数据兼容。

## 5. 第二优先：补完 M4

1. 收口 raw wire 的单目标身份 fallback，保证 workspace/route 不能错绑或跨项目冒用。
2. 新增 `norma brain report --kind reply` 和 `BrainEventKind::ReportRequested`，贯通真实 CLI、事件、回复通道。
3. 岛内全局对话 → UserRequest → 幕僚跨工作空间执行 → 文字回复；身份沿路保留，不能绑定当前前台项目。
4. 汇报按钮捕获有界跨工作空间概览，在后台调用 LLM 总结，经 report 回传并用 tts 朗读，逐句字幕和活动记录显示在岛内。
5. 文字回复始终可见；只有开启语音汇报或主动点汇报才朗读。未启用/后端失败须有本地化原因与设置引导，其他两栏可用。
6. 将旧 brain E2E 的读盘路径迁至 `global/current`，适配显式 scope 协议，保留原功能断言。
7. 用真实 Claude Code 后端在真实 App 完成全局对话与跨空间汇报，保存 report、TTS、文字/字幕的对应证据。已有迁移场景未调用模型，不能复用为此验收。

## 6. 第三优先：视觉还原、M6 与交付

- 对照 HTML 逐形态核对：收起、完成、审批/提问、三栏展开，密度/层级/头像/语义色/按钮/圆角/内容高度/字幕；双语及适用的 hover、pressed、focus、selected、disabled。
- 已有原型图 `artifacts/island/prototype-{pending,running,staff,alert}.png` 和 M3 真机证据，不代表新幕僚页面已还原。
- 完善 `island` 模块及 `visualScenarioNames`，用最新二进制跑受影响场景；瞬时反馈与动画中间帧实际捕获，不用几何单测冒充图片证据。
- 系统层缺证：刘海屏、外接屏、全屏 Space、透明区域点击穿透、Carbon 系统按键、动画中间帧。本机此前仅确认 DELL 无刘海外屏；没有硬件时明确未验证，不能标验收通过。
- 第 8 节逐行维护“标准 → 测试/真实 App → 证据路径 → 结果”，未通过项保留。
- 依改动范围跑受影响 crate 回归、cargo check/build、必要跨 crate 测试及 Clippy；guardrails、storage/i18n/test-i18n、行数、no-allow、Agent 指南等全部通过。
- 保存正常提交，分多个 PR，并核对每个 PR 最新提交所有 CI（含 rust checks）完成且通过；交付 PR、测试方式、证据和剩余硬件缺口给用户，**不合入 dev**。
- 每个里程碑简报一次，不能说“只剩测试”，直到功能实际完成。

## 7. 本轮可复用证据与接手检查

- `artifacts/island/m5-settings-controller-green.log`：28 passed。
- `artifacts/island/m5-settings-quit-{red,green}.log`：真实退出保存竞态 RED → GREEN；最终 7 passed。
- `artifacts/island/m5-settings-build.log`：App build 退出 0。
- `artifacts/island/m5-settings-clippy.log`：app/runtime all-targets `-D warnings` 退出 0。
- `artifacts/island/m5-settings-driver-build.log`：Swift driver 编译通过。
- 已通过 fmt、performance/storage/行数/no-allow、i18n 2237 双语键、6 条 test-i18n 与 diff-check。
- 设置 E2E 已结束：`artifacts/desktop-workspace-e2e/2026-11-06T15-11-29Z/`，退出 0，result=pass，verification=model-assisted，5/5 步骤通过，927.75 秒（含看图等待）。四开关中英、独立保存、系统合成器确认隐藏、隐藏状态重启不创建岛、恢复后悬停展开均通过；未验证三类真实事件、Carbon 或通知兜底。

收到实际分派后，先确认本工作树未被其他人继续编辑，再保留现有 diff 接续。不要依赖其他会话的 exec session ID；进程、owned.json、日志和报告以磁盘现场为准。
