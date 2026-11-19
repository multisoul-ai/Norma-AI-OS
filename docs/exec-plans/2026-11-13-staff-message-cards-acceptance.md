# 幕僚对话消息重构验收记录

## 范围与基准

- 计划：`/Users/openclawd/.claude/plans/staff-message-cards-v2.md`，原型：`docs/prototypes/norma-staff-material/index.html`。
- worktree：`/Users/openclawd/Documents/code/norma-os/pr-235`，分支：`feat/staff-m1-20261112`，实施基准：`54d92fc6`。
- 保留接手时已有改动，按计划七步提交。权限卡不实施，ACP gate 维持自动拒绝越权调用。
- #235 已在本轮进行期间合并（2026-11-13 18:33:55 UTC）；新增提交通过关联它的后续 PR 交付，不自动合并。

## 七步交付与验收映射

| 步骤 | 提交 | 验收证据 |
| --- | --- | --- |
| 1. 消息重叠 | `c69e3a8e` | 真实生产消息列表布局，长文本、提议、Worker 结果共九条消息；外层与气泡 bounds 无重叠。仅添加 flex_none 不足，修正宽度和用户行布局后通过。 |
| 2. tokens / 渲染目录 | `61bc5183` | messages 的穷举注册表、text/card 分层与统一 tokens；保留原 debug_selector，复用布局回归。 |
| 3. 协议与持久化 | `8e1223b2` | 旧 JSON 默认字段、问答终态、重启过期、单条忽略与仅最新提议可恢复、TaskStore 往返。 |
| 4. brain 提问 | `18e51bb7` | ACP 并发提问解析与 stdio 测试，125 秒预算、取消；回答写用户消息供下一轮读取，原工具只释放、不回传回答。 |
| 5. 工具活动 | `489c7cf4` | toolCallId 配对、交错完成、拒绝、计时与下一轮清空；活动不持久化。 |
| 6. 卡片与 Worker 镜像 | `cbf2207a` | 四种问答状态渲染、完整性检查、忽略/恢复、原卡与镜像抢答互斥。 |
| 7. 收尾 | 本记录所在提交 | 双语键、真实 App fixture、逐图视觉验收、连续录像及验收中发现的修复。 |

设计依据 apple-design：对话作为阅读主体，用户消息右对齐且自然宽度；统一圆角与间距；采用按钮即时 hover/pressed/disabled 反馈；忽略可恢复且不自动派发。Material 菜单原则用于紧凑筛选和任务列表入口。没有新增非必要动效。

## 缺陷与修复证据

1. Markdown 列表只有项目符号、正文不可见：GPUI TextLayout 在自然宽度测量时错误复用零宽换行缓存。改为严格比较 wrap_width；`staff_markdown_text_recovers_intrinsic_size` 先红后绿，真实 App 检查粗体、两条列表正文和行内代码。
2. 英文短用户消息被百分比最大宽度压成竖排：外层确定可用宽度，气泡保留自然宽度，布局测试和真实 App 通过。
3. 英文任务筛选芯片：固定等宽且不压缩；`staff-workspace-streaming-en` 已确认三个标签完整。
4. 独立对抗式审查发现的文本表单、Merging 待答状态和原生接管状态问题已补回归；最终另修复 Worker 镜像过早显示已回答，使用公开 question_snapshot 等待 transport 真正结束。`staff_mirror_waits_for_transport_before_reporting_submitted` 先红后绿。
5. E2E 收起工具详情后过度上滚，导致修改后发送按钮离开视口：脚本回到底部后定位，中英完整流程通过。英文跳过按钮原先复用中文固定坐标，改为视觉定位后通过。
6. 600px 顶栏任务按钮空白：`IconName::Menu` 引用的 SVG 未嵌入。资源加载测试复现缺失，补菜单 SVG、AssetSource 映射与目录枚举。另有顶栏边界回归保证入口不出窗、不重叠。

7. 窄窗任务浮层进入全屏后 Escape 无效：组件库按钮不抢焦点，搜索框卸载后焦点路径断开。聚焦回归测试复现，切换全屏时把焦点交回稳定页面根节点，保留未发送草稿。独立审查核验根因，中英真实 App 均通过。
8. 重开回复和 OpenCode 送达提示有时位于保留视口下方：当轮通过同一 E2E 控制通道向下滚动，重新取图后逐图通过；把等价滚动补入两处脚本。重开存储状态为 Active，无运行中 Worker 时显示 Waiting，已修正双语看图判据。

红绿日志保存在 `artifacts/staff-message-cards-v2/logs/`。布局探测的首版测试宿主缺少 size_full 容器导致负坐标，已修正测试宿主；该次失败不作为产品缺陷证据。

## 自动化验证

- `cargo test -p norma -p brain` 通过；App 的 `app::staff` 与 `staff_` 聚焦用例通过，最终 `cargo test -p app --lib staff_` 为 19/19。
- `cargo check -p app`、debug 与 release 构建通过，包含菜单资源及 Escape 焦点修复。
- `cargo fmt --all -- --check`、i18n（2313 双语键、6 条脚本测试）、文件行数、no-allow、performance、resource-lifecycle 和 diff 空白检查通过。
- Runtime 域回归有一条 OpenCode foreground 瞬态失败，同一文件两用例定向复跑通过，记录于 `artifacts/staff-message-cards-v2/logs/opencode-rerun.log`。
- 本轮未跑 App 全量或 workspace 全量测试；正常提交 hook 仍完整执行。

## 真实 App 验收

执行入口：`bash scripts/desktop-e2e.sh staff --visual`。模型逐张读取真实窗口原图，响应绑定 requestID 与 SHA256。运行独立 debug App，不操作用户安装的 App。外部模型和 Worker 使用受控 fixture，App、CLI、PTY、hook、TaskStore 和权限闸门走生产路径。

| 场景 | 结果 | 证据目录（相对 artifacts/desktop-workspace-e2e） |
| --- | --- | --- |
| staff-workspace-streaming | PASS | `2026-11-13T23-01-07Z` |
| staff-workspace-streaming-en | PASS | `2026-11-13T23-10-09Z` |
| staff-workflow | PASS | `2026-11-14T00-33-54Z`，1241 秒 |
| staff-workflow-en | PASS | `2026-11-14T01-02-32Z`，1573 秒 |
| staff-opencode-dispatch | PASS | `2026-11-14T01-28-52Z` |

五份 report.json 均为 pass。结果为模块初跑加修复后定向复跑汇总 5/5，不宣称单次模块执行 5/5。末尾新增两行视口滚动复用当轮同通道滚动后的原图证据；脚本另做 Swift 编译检查。

流式两场景复用同实现证据：partial/final、双任务计数、草稿隔离、项目文件抽屉跟随、错误停止等待和跨 workspace 不泄漏。补充菜单 SVG 不改变这些行为。

最终中英完整流程均通过：两张并发问答（单选、多选、secret 掩码、跳过）、原工具仅释放一次、快照无密码、Markdown 正文、提议忽略/展开/恢复、拒绝工具详情、编辑保留草稿、hover/pressed、Worker 镜像回答及 Hook 恰好一次回执、已派发禁用、搜索筛选、900/760/600px 布局、600px 搜索焦点→全屏→单次 Escape、产物文件、完成→重开、全屏恢复和画布两张卡。OpenCode 验证首次粘贴未确认、15秒后需要你、重新投递保持 card_id / dispatch_id 且由一次 UserPromptSubmit 确认。

保留失败记录：`2026-11-13T23-20-20Z` 滚动定位失败；`2026-11-13T23-33-33Z` 窄窗菜单资源缺失；`2026-11-14T00-06-22Z` 全屏 Escape 失焦；英文首轮因 Skip 固定坐标失败。上述轮次不计整场通过。

## 录像

`StaffRecording` 每 250ms 采集真实 debug 窗口，按单调时间实时编码 H.264；不拼接检查点截图。每条 workflow 都输出 `staff-conversation.mp4` 和统计 JSON。

磁盘耗尽导致 `2026-11-13T23-33-33Z` 的录像采集中断，已明确排除为交付录像。仅删除本 worktree 的 debug incremental 缓存回收约 28GB，未删除源码、证据或录像。最终交付使用后续完整通过轮次，已用 ffprobe 核验编码、时长、帧数，并从视频实际抽帧检查正向画面与关键流程。

| 语言 | 连续录像路径 | 编码与时长 | 采样 |
| --- | --- | --- | --- |
| 中文 | `artifacts/desktop-workspace-e2e/2026-11-14T00-33-54Z/staff-conversation.mp4` | H.264，2560×1600，1235.33 秒（20分35秒） | 2654 帧，最长采集间隔 1.115 秒 |
| 英文 | `artifacts/desktop-workspace-e2e/2026-11-14T01-02-32Z/staff-conversation.mp4` | H.264，2556×1600，1569.70 秒（26分10秒） | 3161 帧，最长采集间隔 1.014 秒 |

请求采样为4fps，实际平均约2fps；不是60fps动效录像。中文抽帧确认160秒多选/跳过、324秒Markdown、380秒忽略、420秒恢复入口、457秒恢复后的提议、513秒拒绝工具详情、1234秒画布；英文抽帧确认520秒恢复入口与1568秒最终画布。抽帧文件在 `artifacts/staff-message-cards-v2/video-frames/`，完整视频覆盖全部流程。

## 验证边界

- 现行产品只提供深色主题；paper-blue 已迁移为 InkBlack。中英分别使用 InkBlack / Sapphire Night，不能声称浅色通过。
- 真实 Claude ACP 0.94.0、Codex ACP 2.2.2 已捕获 elicitation/create 表单：`artifacts/staff-message-cards-v2/acp-probes/`。该证据验证实际消息形状，未证明所有外部模型问答组合的完整 App 闭环。
- 125 秒真实等待后的超时与取消尚无真实 App 全程证据；已有逻辑/协议测试。
- Worker 镜像先答已验收；Worker 原卡先答后幕僚镜像更新仅有互斥/传输单测，尚未录制真实 App 该方向。
