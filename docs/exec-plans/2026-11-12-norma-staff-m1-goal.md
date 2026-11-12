# Goal：幕僚模式 M1

状态：M1 实现已落地；真实 App 视觉验收受锁屏阻塞，尚未完成验收。基线：`dev@e555367e2`。分支：`feat/staff-m1-20261112`。

## 目标与边界

完成「创建任务 → 与 Norma 对话 → 提议 → 用户采用 → 新 Worker 卡片执行 → 结果回到任务 → 验收」闭环，并提供与画布并列的幕僚视图。

依据：`2026-11-12-norma-staff-tasks.md` 第 9 节、`../prototypes/norma-staff-mode/`、`../design-docs/2026-11-12-norma-staff-architecture.html`。交接文件从 PR #190 的未提交工作区复制，代码仅以 dev 为基础，未修改来源工作区。

M1 包含 Task/Worker、原子快照、任务对话、采用后分派、hook 结果关联、自动 phase 与手动保护、三栏 UI、双语和真实 App 验收。M2 的回收/唤起/重启对账/热会话管理，M3 的 Goal 迁移/未归属卡片/自动派活上限（默认 8，可配置）均后置。

## 已定决策

- `Interrupted` 保持静默，任务显示需要你；不改 Router/triage 的分诊规则。
- 新增 `Lane::Task(task_id)`；AgentFinished 携带宿主验证的 node_id 与本轮 dispatch_id。以任务的 Worker 绑定查归属，拒绝重复与旧 dispatch。
- Task.messages 是会话事实来源；事件持久化成功后才投递。`from_result=false` 不视为取得结果，失败/中断留下明确终态消息，不自动重试派活。
- Norma 从 M1 自动推进 phase；用户修改后自动推进不覆盖；已完成任务收到新消息自动重开并记录事件。
- 验证现有适配器的一任务一会话能力；无法证明可靠隔离时采用已授权降级：复用主会话，逐任务重放 messages，任务轮次禁止播报。
- 所有实际分派必须对应用户采用的提议；同一采用重试不能重复创建卡片。

## 实施顺序与验收映射

| 步骤 | 交付 | 验收边界与证据 |
| --- | --- | --- |
| 1 | norma 协议 Task/Worker 与 brain TaskStore | 公共类型/Store 接口：状态优先级、手动 phase、完成重开、原子保存恢复、损坏文件保护、跨任务/跨工作空间隔离 |
| 2 | 服务路由、任务消息与会话 | BrainService：先保存再投递、重复/迟到结果拒绝、静默不唤醒、任务不播报、任务摘要；适配器探测或降级证据 |
| 3 | CLI、提议采用、Worker 启动与 hook | norma CLI + 宿主集成：任务身份独立于指令文本，dispatch 环境透传、用户采用授权、新卡片与结果精确关联、无结果/失败/中断 |
| 4 | 幕僚三栏界面 | App：画布/幕僚切换、分组任务列表、新建与输入、提议采用、阶段修改、全屏/Esc、复用右抽屉与文件标签 |
| 5 | staff desktop-e2e 模块 | `bash scripts/desktop-e2e.sh staff --visual`：闭环、三栏/切换/全屏/文件、中英、当前可选主题，逐图给出模型视觉判定 |
| 6 | 收尾 | 受影响 crate 测试、app check/release、必要 lint、独立对抗式审查、正常 hook 提交、创建面向 dev 的 PR |

各逻辑切片先运行聚焦失败测试，再写最小实现；测试遵守场景/数据/过程/预期与具体正负断言规范。测试 seam 已由交接文档的域层单测、真实 App 验收要求确定。

## 设计落地

遵循 apple-design 的清晰层级与用户控制：左侧任务、中间对话、右侧既有内容抽屉保持稳定位置；切换与按下立即反馈；可退出全屏，保留任务输入与选择；优先现有 gpui-component 控件。阶段选择采用紧凑菜单，区分当前阶段和键盘焦点。状态不仅靠颜色，中英文及当前可选主题检查可读性与 hover/focus/disabled。dev 的 ThemeId::ALL 已下线浅色主题，旧浅色配置会迁移到 InkBlack；本次使用 InkBlack 与 SapphireNight，不能宣称浅色真实 App 验收。不新增装饰性动效或依赖。

## 风险与验证策略

- 旧 CLI 的 `assign --task` 已表示任务文本：保留兼容入口，为 staff 身份提供明确无歧义的新解析分支，并覆盖解析测试。
- GPUI 前台不进行快照读写与 ACP 等待；brain 持有唯一写入口，App 订阅快照与异步响应。
- 持久化失败不得发布成功快照/唤醒任务；文件损坏保留原件，不用空状态覆盖。
- 按具体改动运行 norma/brain 及其他受影响 crate 的测试，不跑全工作区套件；真实 E2E 使用隔离数据及工作树自己的二进制，不结束用户 App。
- 现有主目录有用户未提交改动，保持原状；新 worktree 独立开发。PR #190 不提交、不合并。

## 执行记录

- 已读取交接任务文档、原型与架构页；确认 dev 有 scoped_router，已创建独立 worktree。
- 适配器探测：Codex ACP 2.2.2 与 OpenCode 可返回不同 session id，Claude 适配器未安装；未证明完整 prompt 隔离，使用已授权的共享主会话＋Task.messages 重放。证据：`artifacts/staff-m1/acp-probe.json`。
- 独立审查已发现并修复：启动早到 Ready 被二次注册清空（聚焦测试先红后绿）、不同服务代写入覆盖、等待审批未进入需要你、Worker 定位未退出全屏、缓存错误未随语言切换；停用期间结算现由独立后台串行保存，保存成功后才通知模型服务，入队后立即停用的 App 回归已通过。
- 2026-11-12 首次 `staff-workflow --visual` 被驱动判 ENV：macOS 屏幕锁定，未发送键鼠；已请求用户解锁。不能据此宣称视觉验收通过。
- `rtk cargo test -p brain -p norma -p runtime -p ui`：brain、norma 全部通过；runtime 旧 OpenCode 进程测试在并发构建负载下超时，缺少 calls.txt。未修改该既有用例，随后按串行复验。
- `rtk cargo test -p runtime -p ui -- --test-threads=1`：422 通过，11 忽略；含 Worker 身份和提前 Ready 回归。
- `rtk cargo test -p app --lib disabled_brain_preserves_worker_completion_for_background_persistence`：1 通过，1785 过滤；覆盖存活服务入队后立即停用、终态落盘且模型不唤醒。
- 最新 `rtk cargo check -p app`、debug build、SwiftPM staff 场景编译通过。i18n 2258 双语键、行数/allow/资源生命周期/存储边界/性能/指南行数/SOCKS 静态护栏通过；`rtk cargo lint-resources` 通过。
- 独立对抗式审查完成两轮以上，已核验并处理发现；最后复核未发现 M1 核心阻塞，视觉结论仍待实际执行。
- release 构建与 PR 链接待补；由于真实 UI 未验收，PR 保留 Draft，不能宣称 M1 验收完成。

## 解锁后的收尾

1. 确认 Mac 屏幕解锁，运行 `bash scripts/desktop-e2e.sh staff --visual`。
2. 当前 Agent 逐一读取 visual request 与原 PNG，写匹配 SHA 的观察结果；不可预填通过。
3. 修复真实流程/看图发现，复验受影响部分，保留两种语言和两个当前主题的证据。
4. 更新本计划与 PR，全部验收通过后再取消 Draft；是否合并仍服从 dev 最新提交 CI 门禁。
