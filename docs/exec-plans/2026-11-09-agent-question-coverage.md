# 任务：把问答工具（AskUserQuestion 类）适配到 Grok / Kimi / ZCode / InfCode / Hermes，使其问答出现在灵动岛“需要你处理”并可在岛内作答

仓库：/Users/alan/Documents/mybook/norma-ai-os。先读 AGENTS.md、CLAUDE.md。必须在 worktree 开发：
  git worktree add -b feat/agent-questions-coverage-20261109 ../norma-agent-questions-20261109 dev
不要在主目录 dev 上改；通过 PR 合并到 dev，CI 全绿前不合并；禁止 --no-verify、禁止 #[allow]；单个 Rust 文件 ≤500 行；用户可见文案中英双语（crates/ui/locales/），跑 python3 scripts/check-i18n.py。先把本方案存为 docs/exec-plans/2026-11-09-agent-question-coverage.md 并随 PR 提交，进度/证据/每家结论写在里面。
并行注意：Chloe 正在分支 fix/island-input-proposals-20261109 修灵动岛输入框（crates/app/src/island/question.rs、staff_view.rs）与提议卡片。本任务尽量只改 runtime / norma 层，不改岛 UI；若必须改岛文件，先与 Chloe 对齐避免冲突。

## 现状（Bella 已核对）
问答链路只认 4 家：Claude、Codex、Cursor、OpenCode。
- crates/runtime/src/runtime/questions/lifecycle.rs:18 —— 只为这 4 家 reserve_native_run 预留问答通道
- crates/runtime/src/runtime/questions/lifecycle.rs:93 —— 其他 profile 的 hook 事件直接 return true，不进问答逻辑
- crates/norma/src/agent_hook/question.rs:89 —— target.provider != Claude 时拒绝（Cursor 另走 cursor_host）
- crates/runtime/src/agent_hooks/question.rs —— 只有 Claude 在启动时注入 PreToolUse(AskUserQuestion) + `--request-question`（timeout 125）+ PostToolUse/PostToolUseFailure 的 per-process settings
- 现有实现参考：runtime/questions/{claude.rs, claude_receipts.rs, fallback.rs, delivery.rs, codex_host.rs, cursor_host.rs, opencode_host.rs}；norma/src/agent_hook/{question.rs, native_question.rs, question_forward.rs}；norma/src/{codex,cursor}/questions.rs

各家问答工具（本机核对）：
| Agent | 工具 | 证据 | 当前 Norma hook |
|---|---|---|---|
| Grok (grok 1.0.54) | AskUserQuestion，schema 与 Claude 同（questions[]/options/multiSelect/preview）；另有 ACP ext_method ask_user_question；env GROK_ASK_USER_QUESTION_TIMEOUT_SECS | ~/.grok/bin/grok strings | ~/.grok/hooks/norma-hook.json 只有生命周期转发，timeout 5，无 matcher、无 --request-question |
| Kimi (~/.kimi-code) | AskUserQuestion | 会话 wire.jsonl 出现 | 见 install.rs 中 Kimi 分支 |
| ZCode (~/.local/share/zcode-source) | AskUserQuestion | zcode.cjs bundle | 见 install.rs ZCode 分支 |
| InfCode (@infcode/cli) | AskUserQuestion | bin/*.js.map | 见 install.rs InfCode 分支 |
| Hermes | 疑似 clarify 工具，未核实（~/.local/bin/hermes 指向的 ~/.hermes/hermes-agent 不存在） | — | Python 插件（install.rs Hermes 分支） |
| Pi | 本机未发现问答工具（Pi 内置工具极少） | — | TS extension |

## 实施步骤

### 阶段 0：逐家可行性 spike（每家必须给出证据结论，写进方案文档）
对每家确认：
1. hook/插件能否在问答工具调用前同步拦截（阻塞等待，超时可配到 ≥120s）；
2. 能否把用户答案回传给 Agent —— 例如像 Claude 那样 PreToolUse 返回 deny/updatedInput/reason 携带答案，或插件 API 直接返回 tool result；
3. 工具调用后/失败后是否有事件可用来收口（对应 PostToolUse/PostToolUseFailure）；
4. 用户直接在终端里回答（不经岛）时，Norma 能否感知并撤销岛上的问题。
做法：最小 hook 脚本 + 真实 CLI 跑一次触发问答，记录 stdin 事件 JSON、返回格式和 Agent 实际收到的答案；保存到 docs/exec-plans/evidence/2026-11-09-agent-question-coverage/<agent>/。不确定的不要猜，用证据说话。
- Grok：先查官方 hooks 文档（https://docs.x.ai/build/features/hooks）PreToolUse 返回值是否支持 deny/修改输入；不支持则评估 ACP ask_user_question 路线并写明代价，不在本 PR 硬上 ACP。
- Hermes：先找到真实安装与问答工具名；Pi：确认确实无问答工具则标“不适用”并结束。
某家确认不可行 → 文档写明原因与证据，保持终端原生作答（岛上不出现半成品），不阻塞其他家。

### 阶段 1：Grok（优先，预计可复用 Claude 路径）
1. TDD：先写失败测试——
   - runtime：Grok 节点被 reserve_native_run 预留；Grok 的 PreToolUse(AskUserQuestion) 事件登记为 PendingQuestion，provider=Grok，出现在 questions() 快照中；PostToolUse/Failure 收口后移除；非问答工具的 PreToolUse 不登记（负断言）。
   - norma：question.rs 对 provider=Grok 不再拒绝；答案格式化成 Grok 能接受的返回 JSON（以 spike 证据为准）。
   - hook 安装：Grok hook 配置新增 matcher=AskUserQuestion 的 PreToolUse 项（--request-question，timeout 125），保留原生命周期项且不覆盖用户其他 hooks；重复安装幂等；卸载干净。
2. 实现：优先把 Claude 专用分支泛化为“兼容 AskUserQuestion 的 hook 型 provider”列表（Claude、Grok，及后续通过 spike 的家），不要为每家复制一份 claude.rs。若 Grok 支持 per-process 配置则与 Claude 一样启动时注入，否则走 ~/.grok/hooks 安装文件。
3. native_first 开关（设置 → 灵动岛“问答交给 Agent 卡片回答”）对 Grok 同样生效。

### 阶段 2：Kimi / ZCode / InfCode / Hermes
按阶段 0 结论逐家接入，每家同样：失败测试 → 实现 → 验收；复用阶段 1 的泛化路径，只在事件字段/返回格式不同处做适配（放各自 agent_hook 子模块，不堆在一个文件）。每家可独立提交，便于回滚。

### 阶段 3：验收
- 单测：cargo test -p runtime -p norma，及 app 中问答/岛相关测试；测试注释与断言遵循 docs/quality/test-requirements.md（场景/数据构造/执行过程/预期结果，正负断言带说明字符串）。
- 真实 App：在 test/desktop-e2e/Scenarios/island/ 扩展 IslandQuestions（或按 Agent 新增场景，夹具 Fixtures/island/，看图清单 checks/island/，登记 IslandModule.swift 与 visualScenarioNames），用 --visual 验收每个已接入 Agent：
  1) Agent 发起问答 → 岛“需要你处理”出现卡片，计数+1；
  2) 岛内选项作答 → Agent 实际收到答案并继续（检查终端后续输出）；
  3) 自定义答案作答（注意：岛输入框真实键盘问题由 Chloe 修，若未合入则用控制通道注入并注明）；
  4) 在终端里直接回答 → 岛上卡片消失；
  5) 打开“交给 Agent 卡片回答”开关 → 岛不拦截，终端原生作答。
  中英文各一遍。可用真实 CLI（无账号时参考 Hermes 调研的本地 OpenAI 兼容 mock 思路）。
- 通过：cargo clippy -p runtime -p norma -p app --all-targets -D warnings；bash scripts/check-max-file-lines.sh、check-no-allow.sh、check-performance-guardrails.sh；python3 scripts/check-i18n.py、test-i18n.py；python3 scripts/check-resource-lifecycle.py。
- 代码 review 用独立 subagent 做对抗式审查（重点：答案串到错误会话/run、hook 超时与 Agent 卡死、重复安装覆盖用户 hooks、终端作答与岛作答竞态）后再开 PR。

## 完成标准与回报
- 方案文档里有每家一行最终结论：已接入 / 不可行（原因+证据）/ 不适用。
- 一个 PR（每家独立提交），描述写清根因、每家结论、证据路径。完成后回报 PR 链接、各家结论表和未验证项，不得把未验证说成通过。

## 阶段 0 结论（2026-11-09 实机）

证据目录：`docs/exec-plans/evidence/2026-11-09-agent-question-coverage/<agent>/`。用户真实 hook 配置未改，spike 只用 `/tmp` 隔离环境。

| Agent | 结论 | 岛内作答 | 依据 |
|---|---|---|---|
| Grok 1.0.54 | 接入 | PreToolUse `{"decision":"deny","reason":"..."}` | `grok/stdout.txt` 以 `SPIKE_ANSWER=Blue` 结尾；`grok/pre-tool-use.json` 工具名 `ask_user_question`，双 casing。`permissionDecision:allow` 的 updatedInput 不改写问答。不能用顶层 `grok --plugin-dir`。 |
| Kimi 2.2.0 | 接入 | PreToolUse `hookSpecificOutput.permissionDecision=deny` + `permissionDecisionReason` | 隔离 `KIMI_CODE_HOME` 下 deny 后模型 stdout 为 `SPIKE_ANSWER=Blue`（`kimi/deny-stdout.txt`）。无 updatedInput。`-p` 会禁用该工具，但 deny 发生在禁用检查之前。 |
| ZCode 0.16.9 | 接入 | allow + 完整 `updatedInput.answers` | `zcode/allow-stdout.txt` 与 35 秒 hook 的 `zcode/slow-stdout.txt` 都是 `SPIKE_ANSWER=Blue`。PostToolUse 带回 answers。`hooks.enabled` 必须为 true。 |
| InfCode 1.0.4-beta.48411 | 接入 | PreToolUse deny+reason，工具名 `question` | TUI `infcode --yolo` 的 tool error 与 `infcodeHookBlocked.reason` 都是 `The user answered Spike color? with Blue.`（`infcode/question-tool-error.json`）。`infcode run --auto` 会永久 deny question，不能代表产品。 |
| Hermes | 不可行 | 无 | 安装树缺失，且 human-input hook 是 observer，返回值被忽略。见 `hermes/README.md`。 |
| Pi | 不适用 | 无 | 本机未安装，且调研中的内置工具没有问答。见 `pi/README.md`。不安装、不接第三方扩展。 |

未验证，不能说需求 4 已通过：

- Grok / Kimi / ZCode / InfCode：用户在终端原生问答 UI 里作答后的 PostToolUse 成功载荷，都没有干净实机。ZCode headless 不注入 answers 时工具不执行（`No permission client configured for AskUserQuestion`，`zcode/native-stdout.txt` 为 `SPIKE_ANSWER=MISSING`，事件只有 PreToolUse）。
- Kimi 现存 `kimi/events.jsonl` 是后来的 log 对照，不是 deny 那次；对照里的失败是 `-p` auto 禁用，error 为对象 `{code,message}`，字段是 `tool_call_id`。
- Grok 交互 PTY 曾得到 `User declined to answer`，二进制里的超时文案这次没观察到。
- Grok ACP `ask_user_question` 不在本 PR 实现。
- 英文壳层只在 `island-hook-questions` 里对 Grok 写了一步；Kimi / ZCode / InfCode 的场景是中文。
- `native_first` 的「不拦截」以 hook 空 stdout 且 15 秒内返回为准。题在 Post 之前仍可能是 Pending，岛仍可能显示。

## 验收记录（2026-11-09）

脚本化 desktop-e2e 已写好，但没有跑通，不能当作真实 App 通过：

- `island-hook-questions --visual` 第一次退出码 3：看图 `point` 写成了对象，驱动要求数组。ENV，不是产品失败。
- 第二次同样退出码 3：300 秒内没有写入响应。ENV。
- 第三次启动后用户接手真实 App 验收，脚本进程已停。停之前只确认了：能点进 Hook questions 工作空间、进入 E2E Project、点到命令框，并出现一张 Grok 卡片。卡片画面当时还没有 `grok question fixture ready` 文本，这一步没有下结论。
- `island-hook-providers` 没有跑。

真实窗口验收由用户进行。单测、clippy 和机械化检查见 PR 说明，不以本节代替。
