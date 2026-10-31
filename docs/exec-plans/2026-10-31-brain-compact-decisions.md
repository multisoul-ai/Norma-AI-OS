# 幕僚紧凑卡片与简洁决策文案实施计划

日期：2026-10-31。状态：已实施，本次功能验收完成；全仓检查的剩余限制见验收记录。

将用户已认可的紧凑原型接入真实 macOS App：详情默认收起，审批和下一步操作始终显示在外层；AI 决策文案简洁明确、直击要害，最多 50 个字符。提示词、共享文案校验和原生折叠界面已接入，聚焦测试、发布模式构建与本次五轮真实 App 场景均已通过。最终证据和检查限制记录在 `artifacts/brain-compact-decisions/verification/summary.md`。

设计基准：`docs/prototypes/brain-readable-cards/index.html` 的外置审批版本。说明见 `docs/prototypes/brain-readable-cards/README.md`，原型验证见 `docs/prototypes/brain-readable-cards/screenshots/compact-result.json`。该结果证明 HTML 原型的交互，不代替原生 App 验收。

## 1. 已确认的产品行为

- 默认只显示行动标题、类型、Agent、必要状态和适用的操作按钮。保持当前侧栏宽度，不增加独立详情面板。
- **允许 / 拒绝始终在外层，收起即可审批**；下一步的采纳 / 拒绝同样常驻外层。点击操作不触发展开、收起或定位。
- 点击标题区域原位展开补充信息，同一时间只展开一条；再次点击收起。展开不重复绘制审批按钮。
- 问答在展开后展示原题、分行选项、推荐理由和编辑入口。推荐、选择、发送是不同状态，未选择不能发送；不把问答当作权限审批。
- 长任务按目标、行动要点和验收条件组织，原文按需查看。切换和折叠保留回答草稿；失效、已提交的问题不能被迟到结果覆盖。
- 忙碌、等待确认、不可用或提交中沿用当前禁用规则。失败保留待办；成功由真实回执驱动，不承诺真实派发或审批可以撤销。

## 2. 50 字符内容契约

限制的是**面向用户的 AI 决策文案**。完整任务指令、命令、路径、原始题目、答案载荷及用户输入保留完整；不能为了短文案删掉执行约束或改变授权。

| 生成入口 | 长度范围 | 保留的完整数据 |
| --- | --- | --- |
| 下一步 `BrainOp::Propose` | `title` 与 `why` 合计，加一个分隔字符，不超过 50 | `task` 原样用于真实派发 |
| 问答建议 `BrainOp::ProposeAnswer` | `reason` 不超过 50 | 原题、选项 key、答案数组及自定义回答 |
| 审批判断 `BrainOp::Decide` | `reason` 不超过 50 | 原请求、精确命令、目标身份和授权范围 |

计数使用 Rust `str::chars().count()`，按 Unicode 标量值计算，不使用 UTF-8 字节长度；中文、字母、数字、标点、空格均计入。去除首尾空白后校验，文案字段不得为空或包含换行。下一步建议必须满足 `title.chars().count() + 1 + why.chars().count() <= 50`，不是每个字段各允许 50。

示例：

- 下一步：`修复父子会话串扰；避免子任务误改主状态。`
- 问答建议：`建议先补回归测试，便于复现并验证修复。`
- 审批理由：`需确认清理文件，目标包含项目外路径。`

不把常规进度播报和用户要求的详细回答统一压成 50 字符。已有长文案继续兼容读取，不批量重写或删除待处理数据；历史记录在折叠态可使用本地化中性标题，展开后完整查看。新写入执行新契约。

## 3. 提示词改动

主入口是 `crates/brain/src/lane/prompt.rs`：`primer`、`NEXT_STEP`、`question_request` 和 `approval`。公共文案要求同时覆盖主会话与审批会话，并在具体事件提示中明确对应字段，避免只约束首轮。

已接入的核心要求：

```text
面向用户的决策文案必须简洁明确、直击要害。
先说明要做什么或需要用户决定什么，再给出一个关键理由。
最多 50 个字符，中文、英文、数字、空格和标点均计入。
不要寒暄、复述上下文、堆砌背景或加入“建议考虑进一步”等空话。
保留影响判断的否定、风险、作用范围和不确定性；无法确认就明确说需确认。
输出前检查长度，超出时重新组织，不能直接截断句子。

propose：title 写具体行动，why 只写关键理由，两者加分隔符合计不超过 50 字符。
propose-answer：reason 不超过 50 字符，不改写题目、选项 key 或答案载荷。
decide：reason 不超过 50 字符，保留决定 allow / deny / escalate 的关键依据。
task 不受短文案预算限制，须保留完整目标、分点步骤和验收条件。
```

保持现有策略：下一步仍提议给刚完成任务的原 Agent；未获授权不能直接派发或回答；信息不足时先核验上下文，审批拿不准就 escalate。简洁要求不能改变这些判断。

## 4. 实施步骤与代码位置

### 步骤一 先补聚焦失败测试

- 扩展 `crates/brain/tests/unit/prompt_tests.rs`：主/审批 primer、完成事件、问答和审批提示均包含长度与文案要求；完整 task、原 Agent、上下文核验和授权要求仍存在。
- 在 `crates/norma/tests/brain_decision_copy.rs` 新增决策文案契约测试。覆盖 49/50/51 字符、中英混排、标点空格、空白和 Unicode 换行；下一步组合长度也覆盖临界值。
- 新增 `crates/app/tests/unit/overview_details_tests.rs`、`crates/app/tests/unit/overview_card_interaction_tests.rs`，扩展 `crates/app/tests/unit/overview_question_keyboard_tests.rs`，以及提议与问答动作测试：收起时操作可见、操作不触发展开、一次只展开一条、草稿保留、禁用与回执正确。

### 步骤二 接入提示词和长度校验

- 在 `crates/norma/src/protocol/brain_copy.rs` 增加纯逻辑的文案限制与校验入口，由 `crates/norma/src/protocol.rs` 导出。共用一处计数规则，避免 CLI 和宿主各自实现。
- `crates/norma/src/cli.rs` 尽早返回超长字段与实际长度；`crates/app/src/app/brain_host/ops.rs` 在副作用前再次校验，直接协议调用也不能绕过。
- 在 `crates/app/src/app/brain_host/proposal_actions.rs` 和 `crates/app/src/app/brain_host/questions.rs` 的直接写入入口复用校验，保护内部调用；校验必须早于删除旧建议、保存、播报或执行审批。
- 不合格文案返回可修正的结构化错误，包含字段、上限和实际计数。使用现有会话与超时机制处理，不新增同步 LLM 调用或无限重写循环。审批保持待处理，不能因为文案校验失败默认放行。
- 不使用 `.take(50)` 截断决策；不把长度错误当作动作已成功，也不重放已经生效的动作。展示边界提供中英文错误，纯协议层不依赖 UI。

### 步骤三 接入原生紧凑布局

- `crates/app/src/workspace/desktop/drawers/overview_proposals.rs`：保留已有外层按钮和阻塞语义，整理标题/理由层级及按需详情，消除标题被按钮挤成省略号的问题。
- `crates/app/src/workspace/desktop/drawers/overview_questions/view.rs` 与 `crates/app/src/workspace/desktop/drawers/overview_questions/editor.rs`：从常驻题面改为折叠展示；使用完整问题身份保存草稿，不以列表下标绑定答案。
- `crates/app/src/workspace/desktop/drawers/overview_list.rs` 与 `crates/app/src/workspace/desktop/drawers/overview_brain.rs`：审批按钮沿用现有直接授权入口，明确独立于详情触发器。折叠的只是内容，不能隐藏审批。
- 统一展开状态按提议、问题或审批的稳定身份管理；切换项目、请求失效时清理相应状态，普通刷新不清空有效草稿。已有 `proposal_open` 与问题编辑状态要统一协调，不另建一套并行真相。
- 替换问答固定预留 180/260px、提议展开固定 132px 等估高方式，按当前展开内容布局，短窗口仍能滚动到操作与下方分组。约 62px 是常规收起态目标，不通过裁字保证高度；50 字符标题允许自然换行。
- 新提议要求完整 `task` 分点包含验收条件；展示层复用经钉死版本验证的只读段落/列表能力。历史无格式 task 按段落展示，不在 UI 猜测含义、调用模型改写或改变派发原文。
- `overview_list.rs` 当前 490 行、`questions.rs` 当前 455 行。新增职责先拆到同目录小模块，不能突破 Rust 单文件 500 行。

### 步骤四 定向验收

优先扩展现有 `brain` 模块场景，不改其他模块或他人的语音验收任务：

- `test/desktop-e2e/Scenarios/brain/BrainPropose.swift`：现有 `brain-propose` 已检查收起可采纳、阻塞与原文派发。补紧凑布局、详情格式、50 字符样例及外层反馈证据。
- `test/desktop-e2e/Scenarios/brain/BrainQuestions.swift`：`brain-questions-ui` 补折叠、草稿保留与单条展开；继续核验原 Hook 精确消费答案，没有普通 PTY 派活。
- 新增 `test/desktop-e2e/Scenarios/brain/BrainApprovals.swift` 的 `brain-approvals-ui` 场景，并在 `test/desktop-e2e/Scenarios/brain/BrainModule.swift` 登记；验证真实原生 PTY 与生产观察 Hook 的审批消费和回执。
- 按 `norma-desktop-e2e` 的前置检查使用隔离测试 App；不终止用户常驻 App。已覆盖中文 Sapphire Night、英文 Ink Black 审批，两种主题的问答及约 400–446px 侧栏。现行主题只保留深色，旧 `paper-blue` 会映射为 Ink Black，本次不宣称浅色验收。

## 5. 验收矩阵

| 要求 | 必须取得的证据 |
| --- | --- |
| 详情收起时即可允许/拒绝或采纳/拒绝建议 | 聚焦交互测试 + 真实 App 收起态点击及真实回执 |
| 点击审批不展开、不定位、不重复执行 | 正负状态断言 + 一次动作仅产生一次接收记录 |
| 收起态紧凑且标题可理解 | 原生关键图对照认可原型；三条常规短标题参考原型 187px，允许字体差异与长标题换行 |
| 默认折叠、单条展开、答案草稿不丢失 | 状态测试 + `brain-questions-ui` 实际编辑/切换/发送 |
| AI 文案不超过 50 字符 | 字符边界测试、CLI/直接协议拒绝测试；真实 UI 显示合格样例 |
| 内容直击要害且不丢风险语义 | 提示词约束 + 人工核对行动、理由、否定、范围与待证措辞；不能只用长度断言证明 |
| 精简不改任务或授权 | 完整 task 的接收记录、原题答案 key 和审批目标一致性测试 |
| 失败、忙碌、过期或重复点击正确 | 任务保留、禁用、明确错误及无重复副作用的聚焦证据 |
| 双语、主题、键盘可用 | 可辨 hover/pressed/focus/disabled，Enter/Space、回执焦点及语言切换的原生证据 |

## 6. 验证顺序与完成定义

先运行新增聚焦测试证明失败，再实现并恢复通过；随后运行 `cargo test -p brain`、`cargo test -p norma`、受影响 App 测试和 `cargo check -p app`。集成确实形成跨 crate 行为后按仓库要求运行 `cargo test --workspace`；GPUI 变更完成后补 `cargo build -p app --release`。不在每个中间步骤重复全仓验证。

运行 `python3 scripts/check-i18n.py`、`python3 scripts/test-i18n.py`、`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`，再执行覆盖差异的最小 desktop-e2e 场景。具体 E2E 选项以执行时的模块清单和技能为准。当前 i18n 检查有既有的 `crates/app/src/settings/voice/view/gpt_live.rs:70` 文案报错，执行时先确认状态并区分既有问题，不把它混作本次新增失败。

设计按 `apple-design` 的层级、直接操作、即时反馈与可访问性原则落地；实施前加载适用的 GPUI skills，以 `gpui =0.2.2` 和 `gpui-component =0.5.1` 验证实际 API。折叠与审批保持即时，不为此新增装饰动效。

完成交付需包含：提示词与文案契约、原生卡片实现、聚焦测试和真实 App 对照证据，以及明确的剩余限制。全部代码走正常验证与提交钩子；不清理、覆盖或夹带其他 Agent 当前的语音预算及测试控制通道改动，不自动发版或推送。

## 7. 最终落地与验收

- 外层审批接入真实待处理请求的内存缓存投影，不依赖幕僚正在运行；完整请求身份在点击时再次校验，异步回执前显示提交中并防重复。
- 修复 Enter/Space 重复切换和操作按键冒泡定位，均有先失败后通过的聚焦回归。
- 五轮最终 desktop-e2e 均通过：两轮审批、两轮问答、一轮下一步建议。关键图已人工核对；完整证据目录、构建和测试日志见 `artifacts/brain-compact-decisions/verification/summary.md`。
- 全仓测试与全仓 i18n 检查仍有本次范围外失败，未标记为全仓通过；串行重跑结果及未解决项见同一记录。本次仅提交到本地 `dev`，不推送或发版。
