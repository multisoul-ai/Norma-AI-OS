# 灵动岛：幕僚/提问输入可键入，幕僚提议回到「需要你处理」

日期：2026-11-09
分支：`fix/island-input-proposals-20261109`
工作树：`/Users/alan/Documents/mybook/norma-island-fix-20261109`
基线：`b66f54059`（与 `dev` 相同，实施前工作树干净）

本文件是实施方案和验收记录。实现只在上述工作树进行，经 PR 合并到 `dev`。主目录检出保持不动。

## 设计技能与取舍

参考技能：`.agents/skills/apple-design`（页面基线）、`.agents/skills/emil-design-eng`（动效与手感，本轮拒绝新增动效）。GPUI 仍以钉死的 `gpui = 0.2.2` 和现有岛代码为准，不照搬技能里的 Web API。

采纳：

- 信息层级沿用现有岛：紧凑胶囊只给数量和头像，展开后「需要你处理」里审批、提问、提议使用同一张卡片。提议低于审批和提问，不自动弹 Alert。
- 平台熟悉性：岛是 `WindowKind::PopUp`（NSPanel + NonactivatingPanel，`canBecomeKeyWindow`）。点击输入框只把这块面板变成 key window，不调用 `cx.activate(true)` 把整个 App 拉到前台。
- 直接操作：提议卡上就地放「派发」「忽略」。派发受阻时按钮禁用，原因写在同一张卡上，不用模态。
- 即时反馈：按钮沿用 `style::action` 的 hover / pressed / focus / disabled。进行中的动作用现有 `pending` 集合去重，两枚按钮一起禁用。成功或失败用岛内 `LocalizedText` 消息。
- 可访问性：按钮有明确文案；禁用态给出可读原因；Esc 仍收起岛；点到输入框外再点回来可以继续输入。浅色和深色工作空间都要看一遍。岛表面继续使用 `style.rs` 里已有的深色材质，不跟随工作空间浅色主题改色，但按钮状态在这块深色表面上必须可辨。

拒绝：

- 不为提议卡加弹簧、入场或独立遮罩。这些只是装饰，邻卡也没有。
- 不在输入框点击时激活整个 App。这正是键入丢失的原因。
- 不把提议放进 `IslandOverview.alerts` / `current_alerts` / `notify_hidden`。那会让提议抢过审批和提问的 Alert。
- 不改 `navigation.rs` / `fallback.rs` 里为定位卡片而激活 App 的路径。

## 问题 1：幕僚和提问输入框键入无效

### 根因（源码，待真实事件核对）

`staff_view.rs` 与 `question.rs` 的输入框 `mouse_down` 先 `cx.activate(true)`，再 `window.activate_window()`。

- 岛面板已经可以成为 key window（`vendor/gpui-0.2.2/src/platform/mac/window.rs` 的 `canBecomeKeyWindow`）。
- `cx.activate(true)` 是异步的 `activateIgnoringOtherApps`。激活完成后 AppKit 把 key 交回工作空间主窗口。岛上看起来有焦点，按键进了主窗口。另一个 App 在前台时最容易出现。
- 这对调用是 2026-11-05 岛方案的 M0 回退，当时只用注入的 NSEvent 检查过。
- 现有 IslandStaff / IslandQuestions 的 `session.type()` 走控制通道，不经过窗口服务器的 key 路由，所以没抓住这个问题。`NORMA_E2E_NO_ACTIVATE=1` 只跳过测试工具自己的激活。

没有已打开的工作空间时，幕僚页是引导而不是输入框（`staff::availability` 失败）。复现必须先打开工作空间，并且大脑设置已启用，否则输入框根本不出现。

### 步骤

1. RED：Norma 不在前台。展开岛，用 `CGEvent` 的 `.cghidEventTap` 真实点击幕僚输入框并键入。输入框应为空，字符落到别处。日志和截图放在 `artifacts/island/`（该目录被 gitignore，结论写回本文件）。若现象与根因不符，停下来重新定位，不按本方案改焦点。
2. 删掉这两处 `cx.activate(true)`。两处只通过 `island/focus.rs` 的 `focus_panel_input(window)` 调用 `window.activate_window()`。注释写明禁止再调用 `cx.activate(true)`。提问输入框保留 `editing.prepare`，只替换激活这一对调用。
3. GREEN：同一套真实事件。幕僚输入框和提问「其他答案」输入框都能输入中文和英文；能测的输入法组合如实记录。回车发送。Esc 收起。点到框外再点回仍可输入。前台 App 可以仍不是 Norma；若 GPUI 点击本身仍会激活 Norma，成功标准是「能输入，且主窗口不再抢走 key」。
4. 若 desktop-e2e 能走真实 key 路由，补一条岛场景；否则在本文件写明原因，并以真实事件证据为准。`keyboardSetUnicodeString` 能证明 key window 路由，不一定能证明系统输入法组字。

## 问题 2：幕僚下一步提议不出现在「需要你处理」

这不是回归。左抽屉删除时，提议列表和派发/忽略界面被一起拿掉，数据和动作还在。见 `docs/exec-plans/2026-11-05-norma-dynamic-island-progress.md`。

### 行为

- 在「需要你处理」里，审批和提问旁边增加「提议」卡。复用 `pending.rs` / `question.rs` 的卡片和 `style.rs`。
- 展示 `title`、建议的 agent、一行截断的 `why`。按钮是「派发」和「忽略」，使用新的 `island.*` 文案，不用 chrome 的「采纳 / 拒绝」。
- 派发和忽略都走 `IslandTarget.session()` / `host()`，也就是提议所属的项目会话，不是当前看得见的工作空间。
- 派发复用删除前的 `dispatch_brain_proposal`：`execute_with_state(DispatchTask)` 里 `assign_task(..., AutoKind::UserDispatched)`，然后 `proposals.take` 并保存。忽略复用 `decline_brain_proposal`：取走、保存、`BrainEventKind::ProposalDeclined`。
- 无卡片不是阻断。`WaitingConfirmation`、`Busy`、找不到 agent、agent 已停用才禁用派发，并显示本地化原因。忽略在派发被禁用时仍然可用。阻断的派发不删除提议。
- 用 `pending` 集合去重。成功后用 `resolved_proposals` 先从投影里拿掉，避免下一帧快照把卡刷回来。
- 计数、紧凑头像的待处理侧、队列都包含提议。队列点击提议是展开并切到待处理页，不是 `SelectAlert`。
- 默认不弹 Alert。`render_pending(..., alert_only: true)` 跳过提议卡。审批确认后的下一张 Alert 仍只来自 `overview.alerts`。
- 只有提议、没有审批/提问时，展开应落在待处理页。这个切换放在 `IslandController::event` 进入 Expanded 的那一次，之后用户仍可手动切回运行中。不给 `IslandOverview` 增加字段，避免改爆 runtime 里的结构字面量。

### 数据

- `Proposals::take` 按 id 取走并返回。
- `ProposalBlock` 用 `Missing` / `Disabled` 替换带硬编码中文的 `Unavailable(String)`。展示边界再翻译。
- `LiveProject` 带上该区域会话里的提议快照（含 block、是否会新建卡片）。未加载区域清空提议，和审批、提问一样。重复发布相同数据不通知。
- `load_brain_proposals` 完成和 `propose_next_step` 之后调用 `note_workspace_activity_changed`。活动刷新前如果提议还没装载，触发一次装载；已在装载或已就绪则立即返回。不在鼠标移动、PTY drain、输入路由上读盘。
- 稳定 id：`proposal:{source}:{workspace}:{area}:{id}`。合成头像的 node id 用 `(1 << 48) | proposal.id`，避开画布节点 id。

## 验收对照

| 标准 | 证据 |
| --- | --- |
| 另一 App 在前台时，真实点击并键入，修复前幕僚输入框为空且字符落在别处 | `artifacts/island/` 的 RED 日志和截图，结论写入下方进度 |
| 修复后幕僚输入框接受中文、英文，回车发送 | 同一套真实事件的 GREEN |
| 提问的其他答案输入框同样可输入并回车 | 真实事件 GREEN |
| Esc 收起；点出再点回仍可输入 | 真实事件 GREEN |
| 已加载工作空间的提议出现在对应岛上，其他工作空间和未加载缓存没有 | `cargo test -p app` 中岛模型用例 |
| 只有提议时不进 Alert，展开落在待处理页 | 岛控制器用例 |
| 无 host 时派发给出 `island.expired` 且不把提议标成已处理 | 岛控制器用例 |
| 派发把任务交给目标 agent 卡并移除提议；忽略后重启不再出现 | `scripts/desktop-e2e.sh` 的 IslandProposals `--visual` |
| 派发被阻断时按钮禁用且原因可读，忽略仍可用 | 同上，中文和英文各一次 |
| 计数、头像待处理侧、队列包含提议，Alert 弹层不渲染提议卡 | 控制器 / faces 用例，外加看图 |
| 用户可见文案中英都有 | `python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py` |
| 浅色和深色工作空间下按钮状态可辨，岛本身保持现有深色材质 | 真实 App 看图，结论写入进度 |

完成前还要跑：受影响的 app 岛测试、`cargo clippy -p app --all-targets -D warnings`、`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`、`bash scripts/check-performance-guardrails.sh`。runtime 只有在真的改了 `crates/runtime` 时才跑 `cargo test -p runtime`。本方案不改 runtime。

desktop-e2e 使用工作树 `target/debug/norma-os`（debug，无 `production-data`）。不使用 `/Applications` 里的包，也不使用主仓库的 release 二进制作为验收被测程序。RED 若借用同一提交的本地二进制，只用于修复前的对照，并在进度里写明路径。

## 提交

可以分成两个提交、一个 PR：输入焦点、提议恢复。PR 正文写根因、修复和证据路径。合并前等待该 PR 最新提交的 CI（含 `rust checks`）全部通过。不直接推 `dev`，不用管理员权限强行合并。

## 进度

- 2026-11-09：工作树已在 `b66f54059`，方案写入本文件。Rust 尚未修改。
- 2026-11-09 RED 受阻：`artifacts/island/cgevent` 仍是 `untrusted`。未改 `staff_view.rs` / `question.rs` 的 `cx.activate(true)`。
- 2026-11-09 提议：模型、快照、派发/忽略、待处理卡、计数、头像和队列已落地。聚焦 app 测试与 i18n、行数、no-allow、性能脚本已通过。桌面场景是 `island-proposals`（`scripts/desktop-e2e.sh island-proposals --visual`）。会话身份用 `desktop:{workspace}:{area}`，提议文件在 `$NORMA_BRAIN_DIR/global/current/proposals/{sha256}.json`。输入焦点仍等辅助功能授权后的真实事件。
- 2026-11-09 `island-proposals --visual` 第 1 次：证据目录 `artifacts/desktop-workspace-e2e/2026-11-09T05-51-06Z`，日志 `artifacts/island/e2e-proposals.log`，退出码 3，通过率 0/1。看图已确认：Alan 终端出现 `PROPOSAL AGENT READY`；深色画布；展开后「需要你处理」橙色 2；Alan「整理日志」浅色派发可辨且悬停更亮；Nobody「未知目标」派发发灰，原因「未找到 Agent：Nobody」；按下 Nobody「忽略」的那一帧比普通深色按钮更深，Alan 卡仍在。步骤「悬停可点的派发，按下 Nobody 的忽略」在 273.72s 被驱动记为 ENV：启动日志含 `Address already in use`（手机连接，用户的 Norma 仍在），进程随后在 `cocoa` `appkit.rs:3407` 空指针 abort。按 desktop-e2e 规则，ENV 停止，不把同一次失败改记成产品缺陷，也不自动重跑。派发、重启消失、英文浅色都还没有看图证据。
- 2026-11-09 用户改为自己做真实操作验收，不再重跑 desktop-e2e。输入焦点已改：幕僚和提问的按下不再调用 `cx.activate(true)`，只通过 `island/focus.rs` 的 `window.activate_window()` 让这块面板成为 key window。`navigation.rs` 和 `fallback.rs` 里为定位卡片而激活的路径没动。辅助功能仍是 `untrusted`，真实键入没有机器证据。
- 派发：找不到或被阻断时直接返回，不打开共享上下文，提议留在列表里。真正派发前在共享上下文锁里再查一次，避免点下去之后目标变忙仍发出去。
- 工作树单测 `cargo test -p app --lib -- --test-threads=1 brief_why_keeps_forty_two block_key_maps proposal_click_while_pending resolved_proposal_hides classify_dispatch_block dispatch_keeps_blocked island_proposals_open_pending proposal_moves_a_known_agent`：8 passed。覆盖文案截断和四条阻断文案、进行中点击不再派发也不标已处理、模型删掉提议后裁掉处理记录、阻断优先级（等确认优先于正忙）、找不到或停用时派发不删提议、忽略只删被点的那条、队列步进跳过提议。`cargo clippy -p app --all-targets -- -D warnings` 退出 0。`check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh`、`check-i18n.py`、`test-i18n.py` 退出 0（2169 个双语键）。随后 `cargo build -p app` 退出 0，可运行二进制是工作树 `target/debug/norma-os`。未改 runtime，未跑 workspace 全量。
- 2026-11-09 交叉审查：规格轴无偏差。标准轴的七项逻辑没有被推翻。派发失败原因保持宿主原始诊断，不另做翻译。处理记录的裁剪补了一条正断言：提议还在模型里时 `prune_resolved_proposals` 必须留下记录。`resolved_proposal_hides_until_the_model_drops_it` 再跑通过。真实 App 仍由用户自己验收，本轮没有重跑 desktop-e2e。
