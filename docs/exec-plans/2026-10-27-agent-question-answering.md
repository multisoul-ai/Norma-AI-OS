# 幕僚代答：Codex、Claude Code、OpenCode、Cursor 适配方案

状态：方案已进入实施，四家整体尚未完成。日期：2026-10-27。本文保留初始能力调查与设计依据，当前生产能力及验收结论以 `docs/exec-plans/2026-10-27-agent-question-answering-implementation.md` 为准。尤其 Cursor 原生前台调用归属不能可靠核验，实验逐键路径未开放到生产，不能将原始探针结论理解为已适配。

实施顺序、代码范围、失败测试与交付门槛见 `docs/exec-plans/2026-10-27-agent-question-answering-implementation.md`。

## 1. 要解决的行为

Agent 的问答是在等待当前交互的选择、文字和提交。幕僚应生成一个绑定原问题的建议答案，用户点“采用此回答”后，Norma 完成这次问答，确认答案被接收，再让原 Agent 继续。

明确区分三种动作：`AnswerQuestion` 回答当前问题、`DecideApproval` 批准/拒绝操作、`DispatchTask` 派发后续任务。不能把答案塞进新任务，不能将 allow 权限决定当作回答，也不能仅凭 `WaitingApproval` 判断存在问答。

本期覆盖四家 **CLI Agent 卡片**；Cursor 指 Cursor Agent CLI，不包含 Cursor 编辑器内独立的聊天面板。已有运行中卡片保持原会话，不通过另起进程 resume 历史来假装接管当前问题。

默认交互：

```text
Agent 当前问题：采用哪种缓存？
  ○ 内存缓存   ○ SQLite   ○ 自定义

幕僚建议：SQLite
理由：项目需要重启后保留缓存。

[采用此回答]  [编辑回答]  [在原卡片处理]
       ↓
正在提交 → 已回答，Agent 继续执行
```

- 问题与对应建议合为同一个待办，不再出现一条问答加一条无关“下一步”争抢操作入口。
- 问答期间产生的真正后续任务保留为后续建议；回答结束不会自动派发它。
- 采用或编辑后的答案绑定问题与版本；问题变化、用户已回答、会话结束后，旧建议立即失效。
- 用户说“这题你直接回答”，只授权当前明确问题的代答。长期自动代答另设规则，不因一次授权推导为全局自动处理。
- Apple 设计原则落地：建议紧邻原问题，选中值与提交动作明确分开；提交中禁重复操作，错误就地可见；保留原卡片入口，不加模态确认层。

## 2. 适配矩阵与本次证据

| Provider / 本机版本 | 推荐提交路径 | 问题形态 | 本次验证 |
| --- | --- | --- | --- |
| Codex 0.162.1 | app-server `item/tool/requestUserInput`；响应同一个 RPC ID | 题目 ID、单选、自由文本、多题；当前 schema 不声明原生多选 | 真实隔离模型问答成功：选择 Green，模型确认收到并完成 |
| Claude Code 2.1.296 | 原生卡片优先 `PreToolUse: AskUserQuestion` 桥接；SDK 会话使用 `canUseTool` | 单选、多选、自由文本、多题 | 真实隔离 stream-json + Hook 成功：tool_result 有 Green、source=hook，模型确认收到 |
| OpenCode 1.19.3 | 同一 server/instance 的 `GET /question`、`POST /question/:id/reply` | 单选、多选、自由文本、多题；按题序对应答案 | 隔离服务 `GET /question` 返回 200；未做真实模型回答 |
| Cursor Agent 2026.10.18-ea085cb | ACP `cursor/ask_question` 扩展，同一个连接/RPC 请求回包 | 问题 ID、选项 ID、单选、多选；当前扩展无自由文本 | 隔离 ACP 握手及真实安装包 handler 的 5 项映射检查通过；未做真实模型回答 |

**证据边界：四家均查到可适配路径，但只有 Codex、Claude 做过真实新会话单选往返；尚未验证用户当前卡片、四家原生终端点击、多选、重连和人工抢答。** 当前工作不是“四家功能已经上线”。

本次探针只创建临时目录与新进程；已关闭回收，没有替换或重启用户 Norma，也未操作用户现有会话。没有修改生产代码。协议问答实测不等于真实 Norma App UI 验收。

### 2.1 Codex

- 接收 `item/tool/requestUserInput` 的 `threadId/turnId/itemId/questions` 与 RPC ID；回包为 `{"id":原RPC_ID,"result":{"answers":{"题目ID":{"answers":["Green"]}}}}`，不调用新的 `turn/start`。[1]
- **先确定调用来源再分类**：官方说明 connector 有副作用的调用也会借这类 user-input 请求呈现 Accept/Decline/Cancel。必须关联 itemId 的工具/调用来源，将此类请求分流到审批；不能仅凭方法名或选项文案认作普通问答。无法核验来源时交回原卡片。[1]
- RPC ID 可为数字或字符串，必须原样保存，并绑定 server/connection 代数；不能仅按题目文字找目标。
- `serverRequest/resolved` 也会用于取消清理，不能单独算回答成功。需关联本次答复、工具或后续 Agent 消费答案的证据。[1]
- 新版原生 TUI 支持共享 daemon 和 `--remote`。官方源码有同一服务器 live `thread/resume` 重放 pending request 的实现，因此可在保留原终端体验的同时接入问答。[2]
- 这条双客户端路径尚未本机实测；实施前验证 endpoint 归属、线程绑定、请求重放、人工先答与断连。另起 app-server 再恢复历史不是同一活请求。旧版、`--no-daemon` 或不可取得 endpoint 的会话走终端适配。
- 以本机生成 schema 为准：不从 answers 数组推断多选支持；`autoResolutionMs` 在该版本已 deprecated，不按旧网页描述硬编码超时。

### 2.2 Claude Code

- 在卡片启动前注册专用 `PreToolUse` Hook，只捕获 `AskUserQuestion`，读取 `tool_use_id` 和完整 `tool_input`；后台等待答案，前台仅更新 UI。[3]
- Hook 返回 `permissionDecision: "allow"` 和 `updatedInput: {...原输入, answers}`。`answers` 的键为原问题全文；必须保留原 questions 及其它输入字段，不能只回 allow。[3]
- 内部用 `tool_use_id + 题序 + 文本指纹` 标识子题。重复问题全文会使外部 answer map 歧义，应明确拒绝自动提交。多选映射为已验证版本接受的 labels 表示，自由文本保留用户原文。[3][4]
- SDK 托管会话使用 `canUseTool` 的同一控制请求，保留 requestId/toolUseID 与取消信号。先按工具名分流，不能把所有权限 callback 当问答。[4]
- Hook 超时、进程退出或用户取消后关闭旧句柄并使建议失效。command/http Hook 通常回正常权限流程，但 onFailure 配置或 SDK Hook 可能阻断；受控 stream-json 也没有原生终端弹题。须按启动模式确认实际后继状态，再提供存在的人工入口，不能保证超时一定恢复原问答，不能接受迟到答案。
- 已经出现在原终端、未被 Hook 捕获的问题不能靠迟装 Hook 重放；这类题仍需操作原终端。官方当前 SDK 的 subagent 问答限制需单独处理，不承诺全覆盖。

### 2.3 OpenCode

- 新卡片由 Norma 管理 loopback server，TUI 用 `opencode attach` 连接同一服务，或在启动 TUI 时显式暴露受控 endpoint。记录 server/process 代数、instance 目录、sessionID。[5]
- 现有插件增发 `question.asked/replied/rejected`；重连时 `GET /question` 对账，再按 sessionID 过滤。HTTP 的 instance/workspace 路由必须与原题一致。[6]
- 回答体为 `{"answers":[["第一题选项"],["第二题选项一","第二题选项二"]]}`；每个子题没有独立 ID，答案按原题序映射，选项按 label 映射。[6]
- 提交前自行校验题数、单选数量、多选、自定义允许与重复 label；上游当前 handler 不替 Norma 做完整题目校验。
- `question.replied` 包含 requestID、sessionID、answers，可与本次提交比对；再检查关联工具或模型继续执行。HTTP 200 本身不作为 Agent 已消费答案的全部证据。
- 默认普通 TUI 使用内部 worker RPC，不一定暴露 HTTP。不得另起 `serve` 或只按历史 sessionID 恢复来回答原 pending。没有可绑定的原服务就走原 PTY。OpenCode ACP 不能假定具备 Cursor 的专有问答扩展。

### 2.4 Cursor Agent

- 托管 ACP 会话接收 `cursor/ask_question`，绑定 connection/session、RPC ID、toolCallId；回包 `outcome: answered`，每题提交 `questionId + selectedOptionIds`。请求本身没有 sessionId，首版每张卡片/run 独占一个 ACP connection，且只承载一个 session；以后复用连接必须先验证 toolCallId 到 session 的唯一映射，不能猜最后活跃会话。[7]
- 直接实现此扩展，不静默降级成通用 `session/request_permission`。当前官方兼容 fallback 仅处理单选，会过滤多选问题，可能造成“看似回答、实际漏题”。
- 当前公开扩展没有自由文本字段，安装包也未转发 `freeformText`。不能把文字塞进选项 ID；自由输入只有在同一运行实例中仍承载该问题的原 TUI 确实提供输入控件时才走终端适配。纯 ACP 会话没有这种 TUI 时显示该题型不可用，不能另起 resume 会话作降级。[7]
- `skipped/cancelled` 与 `answered` 分开保存；skip/cancel 在当前实现里都映射为 rejected，不能显示为已回答。
- `session/load` 恢复历史，不代表能接管已运行 TUI 的 pending promise；目前没有已确证的双客户端接管路径。
- 因 Norma 默认卡片是原生 PTY，Cursor 第一版必须包含原终端适配；ACP 可作为明确的托管会话能力。不能为了拿到 ACP 在后台复制一份现有会话，也不能把纯 ACP 输出伪装成原终端。

## 3. 统一模型、状态与终端兼容

建议协议模型：

```text
QuestionTarget
  workspace / project / node / provider / run generation
  provider session + turn（若供应商提供）
  connection-or-server identity / request identity / question revision

PendingQuestion
  target + 原始题目 + 规范化子题与选项
  single / multiple / free_text / secret 等显式能力

SuggestedAnswer
  target + per-question answers + 理由 + 依据的 context references

Delivery
  Pending → Submitting → Submitted → Answered
                    ↘ Failed / Unknown
  任意未完成状态 → Cancelled / Expired / AnsweredElsewhere
```

Provider 未提供 turn 或子题 ID 时保留缺省值，通过本次调用代数和原题序补本地身份，不能伪造为上游真实 ID。对不支持的题型关闭采用入口，具体说明原因。

新增 `norma brain questions` 与 `norma brain answer --target <引用> --answers-json <JSON>`，沿现有 `brain -> norma CLI -> app -> runtime` 方向执行。`questions` 返回结构化来源引用；所有内容视为待处理数据，不能让题目文字成为额外授权。

“成功”分两层：Submitted 表示已发出，Answered 表示有与该回答对应的消费证据。只见 UI 消失、状态变忙、写入 PTY 成功或连接断开，都不足以单独判成功。若只能确认提交而不能确认消费，保留“已提交，等待确认”；失去确认能力则标“结果待确认”，不盲目重试。

终端适配不是给普通消息输入框粘贴答案，而是有限的原控件操作：

1. 读取同一 run 的实时 terminal grid，识别当前问题、题序、选项、焦点和提交阶段；不从滚动历史识别待答。
2. 使用各 Provider/版本验证过的方向键、空格、文字输入、确认动作；逐步读回，提交前复核问题指纹与当前选中值。
3. 借用现有输入租约、screen_signature、input_sequence、run/session 校验；用户开始操作即让出控制，避免两方交错输入。必要时增加显式 Space 按键，不能用 bracketed paste 冒充多选切换键。
4. 读取最终选择摘要、关联工具结果或后续消费证据；无法确认时停在 Unknown。不能自动 Escape 掉原问题再派一条新消息。

现有终端控制在 `crates/runtime/src/runtime/mobile_cards/terminal_view.rs`，依赖 `MobileRun/mobile_control`，原生跟踪名单不包含 OpenCode。实施时抽出可供桌面/幕僚共用的终端交互能力，不能仅改 provider 白名单、强迫用户开启手机服务。

## 4. 代码边界与交付顺序

| 层 | 负责内容 |
| --- | --- |
| `crates/norma/src/` | Question/Answer 协议、CLI 命令、Provider wire 类型与纯转换；扩展专用 Hook 转发 |
| `crates/runtime/src/` | 问题生命周期、回答校验、身份/并发/取消、Provider 待答句柄与终端输入协调；保持零 GPUI |
| `crates/brain/src/` | 问题事件与上下文生成答案；只经 norma CLI 行动，不直接读写 PTY |
| `crates/app/src/` | 后台连接与 Hook 等待、项目路由、建议展示、状态反馈、持久化副作用 |
| `crates/storage/src/` | 按既有上下文/活动机制保存问题与回答决策，遵守既有依赖方向 |

采用四个可验收增量，不一次重写全部 Agent 接入：

1. **统一问题与回答契约**：问题和审批分流；`WaitingQuestion` 明确阻塞派发；推荐动作类型化；覆盖身份、幂等和过期。旧持久化提议保持 DispatchTask，不猜测转换成 AnswerQuestion。
2. **协议最小闭环**：接 Codex 受控问答、Claude Hook/SDK；补 OpenCode 真问题 reply 与 Cursor ACP 真问题 answered 试验，再实现两个适配器。四家按实际能力开放单/多选/文本。
3. **原生卡片闭环**：验证 Codex 同 daemon 双客户端、Claude 原生 TUI Hook、OpenCode 同 server attach；实现 Cursor 和无结构化入口的旧会话终端适配。此项未过不能宣布默认卡片已支持代答。
4. **统一交互与发布**：建议合并入问题卡片，采用/编辑/原卡片入口；验证手工回答与幕僚提交互斥、断连对账、关闭项目回收、重新打开不复发旧答案。

记录可持久化，活的回答句柄不可直接持久化复用。重启后必须向原 Provider 对账；不能证明仍是同一活问题时，旧建议过期。

IO、模型推理、Hook 等待和协议重连均后台执行；前台只更新状态和路由输入。禁止每帧扫描 transcript/SQLite；终端识别只在相关事件或有界待答轮询时运行。

## 5. 验收矩阵

每条先补可失败的聚焦测试，再实现。真实 Agent 测试与 fixture 测试分别记录，不能用发送一个假的 WaitingApproval 事件替代真实问答验收。

| 场景 | 必需证据 |
| --- | --- |
| 四家单选采用 | 真 Agent 发题 → Norma 提交 → 同题选项被消费 → 原 Agent 继续；不能增加新任务 |
| 多选 / 多题 / 自定义 | 按各家能力逐项测试；不支持时明确不可用，不能漏题或假成功 |
| 审批与问题同时出现 | 答题不会批准 Bash/写文件；审批 allow 不会标记问题已回答；Codex 同方法承载 connector 审批时必须按来源分流 |
| 原问题与下一步冲突 | 建议答案在原问题内；真正下一步仍独立，回答后不自动派发 |
| 人工先答 / 双击 / 桌面与手机竞争 | 一个有效提交；后到者显示已处理；不多发字符或 Enter |
| workspace/session 替换、同 ID 重用 | 旧结果不作用于新问题；按 run/connection 代数隔离；Cursor 无 sessionId 时不能把请求归到另一张卡片 |
| 中断 / 超时 / 断连 / 重启 | 对账或过期；提交结果不明时不自动重试；原卡片仍可接管 |
| 原生 TUI 版本、语言、缩放与滚动 | 各 Provider 真快照和真实按键路径；读不到当前问题时不误输入 |
| UI 状态 | 真实 Norma App 定向 desktop-e2e；待答、编辑、提交、失败、已回答的关键图实际查看，浅/深色分别覆盖 |

Provider 转换与状态机用 norma/runtime 单元和集成测试；生命周期用测试进程验证退出回收；新增 UI 用 app check、受影响 crate 测试、真实 App 与必要 release 构建。方案文档阶段不重复跑无关全仓构建。

## 6. 本次证据与一手来源

本机原件目录：`artifacts/agent-question-adapters-20261027/`（ignored，不随源码提交）。

- Codex 实测脚本/结果：`artifacts/agent-question-adapters-20261027/probes/codex_question_probe.py`、`artifacts/agent-question-adapters-20261027/probes/codex-question-result.json`；约 10.9 秒，question/response/resolved/模型确认/turn completed 全部出现。
- Claude 实测脚本/结果：`artifacts/agent-question-adapters-20261027/probes/claude_question_probe.py`、`artifacts/agent-question-adapters-20261027/probes/claude-question-result.json`；约 8.5 秒，Hook、tool_result 与模型确认均出现；记录成本 $0.021468。
- OpenCode/Cursor 接口探针：`artifacts/agent-question-adapters-20261027/provider-interface-probe.json`；Cursor 安装包 handler 映射：`artifacts/agent-question-adapters-20261027/cursor-question-handler-probe.json`。
- 逐条研究与更多引用：`artifacts/agent-question-adapters-20261027/codex-claude-research.md`、`artifacts/agent-question-adapters-20261027/opencode-cursor-research.md`。

官方网页会更新；本地版本、生成 schema 和固定源码版本优先用于实现核对。

1. [OpenAI Codex App Server](https://developers.openai.com/codex/app-server)；本机 `codex app-server generate-json-schema --experimental`，包括 ToolRequestUserInputParams/Response、ServerRequestResolvedNotification。
2. [Codex running thread 订阅与 pending replay](https://github.com/openai/codex/blob/479f7046ebbb5039c587b72a33af4e3776eac281/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L964)；这是官方源码证据，不假设与本机发行物完全一致。
3. [Claude Code AskUserQuestion Hook](https://code.claude.com/docs/en/hooks#tools-that-require-user-interaction)；[PermissionRequest 边界](https://code.claude.com/docs/en/hooks#permissionrequest-input)。
4. [Claude Agent SDK 用户输入](https://code.claude.com/docs/en/agent-sdk/user-input)；[TypeScript canUseTool](https://code.claude.com/docs/en/agent-sdk/typescript#canusetool)。
5. [OpenCode v1.19.3 TUI transport](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/opencode/src/cli/cmd/tui.ts#L273)；[server 官方文档源](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/web/src/content/docs/server.mdx)。
6. [OpenCode question schema](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/schema/src/v1/question.ts)、[pending 生命周期](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/opencode/src/question/index.ts)、[官方 TUI 回答控件](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/tui/src/routes/session/question.tsx)。
7. [Cursor ACP 与 cursor/ask_question 官方文档](https://cursor.com/docs/cli/acp)；本机官方发行物 2026.10.18-ea085cb 的 ask-question-handler 与 TUI/headless handler。
