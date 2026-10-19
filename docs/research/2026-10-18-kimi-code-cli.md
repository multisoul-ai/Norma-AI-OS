# Kimi Code CLI 与 Norma OS 对接调研及接入方案

> 调研日期：2026-10-18
> 分支：`feat/new-cli-cards/kimi`（基线 `45b42992`）
> 被测对象：Kimi Code CLI **2.2.0**（`/Users/openclawd/.kimi-code/bin/kimi`，Mach-O arm64 单文件可执行）
> 证据目录：[`docs/research/evidence/kimi/`](evidence/kimi/)
> 规则：每条结论标「已验证」（附命令与关键输出）或「未验证」（附原因，对应能力写成“不支持”并给出降级）。官方源码仅作为解释“为什么”的旁证，状态结论以真机输出为准。

## 0. 探测方法与限制

本机**没有 Kimi 账号凭据**，未登录状态下 CLI 不会创建会话（见 §3）。为了不使用任何真实凭据又能真实驱动会话、hooks 与续接，采用如下隔离环境（官方文档确认 `KIMI_CODE_HOME` 可整体迁移数据根目录，已验证它确实生效）：

- `KIMI_CODE_HOME=/tmp/kimi-iso/home`：独立配置与会话目录，**不触碰** `~/.kimi-code`。
- 本地假 OpenAI Chat Completions 服务 `127.0.0.1:18765`（脚本 [`mock_openai.py`](evidence/kimi/mock_openai.py)，只回放固定文本、一次 `Bash` 工具调用或 HTTP 400），配置里的 `api_key = "dummy-not-a-credential"` 不是任何真实凭据。
- 真实交互用 `tmux` 承载 PTY，启动命令与 Norma 完全一致：`/bin/zsh -lic "exec kimi ..."`，工作目录均为 `/tmp/kimi-iso/work*` 的全新空目录，没有在仓库内运行该 CLI，没有用任何跳过审批的参数。
- 注意：模型层是假的，所以“模型怎样思考/调用工具”不是被测对象；被测的是 CLI 自身的会话存储、hooks 触发、审批界面、续接与退出行为，这些与模型无关。
- 已知探测假象：新会话的**第一条**消息，假服务看到的最后一条 user 消息是 CLI 追加的 system-reminder，所以触发词在首条消息不生效；这不影响 CLI 行为结论。
- 官方源码克隆自 `https://github.com/MoonshotAI/kimi-code`（`4a82a07f…`，晚于 2.2.0 发布，可能与二进制有差异），只用于旁证。

## 1. 安装方式、版本、二进制路径（已验证）

```text
$ which kimi            -> /Users/openclawd/.kimi-code/bin/kimi
$ kimi --version        -> 2.2.0
$ file $(readlink -f ~/.kimi-code/bin/kimi) -> Mach-O 64-bit executable arm64
```

- 安装器把 PATH 追加进 `~/.zshrc`；因此 Norma 用的 `zsh -lic` 能找到 `kimi`（已验证，§3）。
- 官方安装命令（来自官方文档 `https://code.kimi.com/kimi-code/install.sh` 与 npm 包名，**安装命令本身本次未重新执行**，机器已安装）：
  - `curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash`
  - 或 `npm install -g @moonshot-ai/kimi-code`（Node.js >= 22.19.0）
- 数据根目录 `~/.kimi-code/`（可被 `KIMI_CODE_HOME` 覆盖）：`config.toml`、`tui.toml`、`sessions/`、`session_index.jsonl`、`credentials/`（登录后出现）、`logs/`、`updates/`。已验证：未登录时目录里只有 `config.toml`（仅三行注释）、`tui.toml`、`device_id`、`region`、`logs/`、`updates/`、`telemetry/`、`bin/`，没有 `sessions/`。
- 自动更新默认开启（`tui.toml [upgrade] auto_install = true`，已验证文件内容）。

## 2. 帮助输出（已验证）

完整输出见 [`help-all.txt`](evidence/kimi/help-all.txt)。顶层要点（逐字摘录）：

```text
Usage: kimi [options] [command]
  -V, --version
  -S, --session [id]   Resume a session. With ID: resume that session. Without ID: interactively pick.
  -c, --continue       Continue the previous session for the working directory.
  -y, --yolo           Start in Ask When Needed mode: routine edits and commands run automatically; risky actions, questions, and plans still ask.
      --auto           Start in Never Ask mode: never interrupts you; everything runs and is decided automatically.
  -m, --model <model>  -p, --prompt <prompt>  --output-format <text|stream-json>
  --skills-dir  --agent  --agent-file  --add-dir  --plan
Commands: export fork provider session acp web server rc|remote login doctor vis install-desktop app migrate upgrade|update
```

- **没有旧版的 `--resume`/`-r` 列在帮助里**：帮助只列 `-S, --session` 与 `-c, --continue`。但 2.2.0 的退出提示与实际行为都接受 `-r`（见 §4）。旧版文档的 `--yolo` 语义已变：2.2.0 的 `--yolo` 是 “Ask When Needed”，真正的“从不询问”是 `--auto`。
- 子命令 `session list [--cwd <path>] [--all] [--archived] [--limit n] [--json]`、`fork`、`export`、`doctor config` 等均已读到帮助；`kimi session list --json` 已实跑，输出字段为 `id,title,lastPrompt,workDir,sessionDir,createdAt,updatedAt,archived,metadata,lastTurnReason`（Norma 不应在热路径使用，仅作为调研与手工兜底）。
- 互斥规则（已验证）：`kimi -c -S <id>` -> `error: Cannot combine --continue, --session.`

## 3. 用 `/bin/zsh -lic "exec kimi"` 启动与首次运行（已验证）

证据：[`launch-unauth.txt`](evidence/kimi/launch-unauth.txt)。

1. 全新目录首次启动先出现**目录信任对话框**（`Trust this folder?  ❯ Trust and continue / Exit`）。信任结果按目录记忆（在 `KIMI_CODE_HOME` 下）。这意味着 Norma 卡片第一次在某个项目目录启动时，用户需要在卡片里按 Enter 选择一次，不属于异常。
2. 信任后进入完整 TUI 欢迎页：

   ```text
   Welcome to Kimi Code!   Run /login or /provider to get started.
   Directory: /private/tmp/kimi-probe.0ELU   Session:   Model: not set, run /login or /provider   Version: 2.2.0
   ```

   未登录时 TUI 照常进入，输入框可用，状态栏 `Model: not set`，无任何会话（`No session yet — one will be created on your first message.`）。`kimi session list` 输出 `No sessions found.`，`kimi provider list` 输出 `No providers configured.`。
3. 登录方式：TUI 内 `/login`（设备码 OAuth，`kimi login [--region mainland-cn|global]` 为非交互等价命令）或 `/provider`（API Key / models.dev / 自定义 registry）。凭据写入 `~/.kimi-code/credentials/<name>.json`（0600）。**本次未执行登录**（无账号），登录后的真实 Kimi 模型行为为“未验证”。
4. 配置完成后（隔离环境）：欢迎页 `Model: mock-model`，输入框 `>`，可直接对话。

结论：启动命令无需任何参数即可进入交互界面；登录/配置是用户在卡片内完成的一次性操作。

## 4. 会话续接（已验证）

数据：隔离环境下在 `/tmp/kimi-iso/work2` 创建会话 A（`session_e9029623-7275-4b57-8758-0e8cb054234f`，交互式 TUI），再用 `kimi -p second` 创建会话 B（`session_42c254f5-…`）。

| 命令（均经 `kimi ... -p <prompt>` 或 TUI 实测） | 结果 |
| --- | --- |
| `kimi -c -p viaC`（work2，A、B 并存，B 更新） | 续接 **B**（目录内最近更新的会话），输出 `To resume this session: kimi -r session_42c254f5-…` |
| `kimi -S <A> -p viaS` / `kimi -r <A> -p viaR` / `kimi --session <A> -p viaLong` | 全部续接 **A**；`-r` 是未在 help 列出的别名，实测可用 |
| TUI：`/bin/zsh -lic "exec kimi --continue"`（work2） | 回放会话 A 全部历史，`SessionStart` 钩子 `source:"resume"`、同一 `session_id` |
| TUI：`/bin/zsh -lic "exec kimi --session session_e9029623-…"` | 同上，`source:"resume"`，同一 `session_id` |
| TUI：`kimi --session`（无 ID） | 打开 `Sessions (type to search)` 选择器，Enter 选中后续接（[`session-picker.txt`](evidence/kimi/session-picker.txt)） |
| 在**别的目录**执行 `kimi -S <A> -p x`（work3） | `error: failed to run prompt: Session "session_e9029623-…" was created under a different directory.`（退出码非 0） |
| 在没有历史会话的目录执行 `kimi -c -p noPrev`（work3） | **不报错**，直接新建会话 |
| `kimi -S session_nope -p x` | `error: ... Session "session_nope" not found.`，退出码 1 |

会话 ID 的来源（已验证）：
- 格式为 `session_<uuid>`（含 `session_` 前缀，续接时必须整串传入）。
- 新会话在**用户发出第一条消息时才创建**，此前没有 ID；创建后 `SessionStart`（`source:"startup"`）与后续每个 hook 载荷都带 `session_id`。恢复的会话在**启动时立刻**触发 `SessionStart`（`source:"resume"`）。
- 退出时终端输出 `To resume this session: kimi -r <id>`（[`exit-output.txt`](evidence/kimi/exit-output.txt)），但该输出不适合作为 Norma 的 ID 来源（要抓取 PTY 文本）。

结论：恢复命令用官方帮助中记载的 **`kimi --session '<id>'`**；缺少缓存 ID 时按 Norma 既有策略退化为普通启动（不使用 `--continue` 猜测，因为 `--continue` 取“目录内最近更新”而非“这张卡片的会话”，多张 Kimi 卡片同目录时会串会话）。

## 5. 会话存储与不扫描目录取得 ID（已验证）

证据：[`storage-layout.txt`](evidence/kimi/storage-layout.txt)、`kimi session list --json` 输出。

```text
$KIMI_CODE_HOME/
  session_index.jsonl      每行 {"sessionId","sessionDir","workDir"}
  workspaces.json          wd_<slug>_<sha256前12位> -> {root,name,created_at,last_opened_at}
  sessions/wd_work2_e9557769360f/session_<uuid>/
      state.json           {id,version,cwd,createdAt,updatedAt,archived,lastPrompt,title,lastTurnReason,...}
      agents/main/wire.jsonl   事件流（含完整 system prompt、请求轨迹）
      notify/  logs/
```

- 会话目录按工作目录分桶（`wd_<目录名>_<hash12>`），目录内按会话 ID 命名。Norma 理论上可以列目录得到“最近会话”，但这违反 Norma 约束（关闭/渲染/PTY drain 路径禁止扫描目录）。
- **无需扫描即可得到当前会话 ID**：每个 hook 载荷都带 `session_id`（§6），Norma 现有机制把它缓存进项目数据库。`find_agent_session_id_for_profile` 对 Kimi 保持返回 `None`（不支持目录发现），只依赖 hook 缓存。
- 同一会话被 `--session` 恢复后 ID 不变（已验证：`SessionStart source:"resume"` 的 `session_id` 与创建时相同）。

## 6. hooks / 扩展机制（已验证）

### 6.1 机制

- Kimi Code 2.2.0 有原生 hooks：在**用户级** `~/.kimi-code/config.toml` 里写 `[[hooks]]` 表数组。官方文档说明且已用源码核对（`packages/agent-core-v2/src/features/externalHooks/configSection.ts`）：schema 为 `.strict()`，**只允许** `event`、`matcher`、`command`、`timeout`（1–600 秒，默认 30）四个字段，多余字段会让配置加载失败。
- 官方文档明确：**没有项目级配置文件**（“The CLI currently reads a single user-level config file and has no project-level config file mechanism”）。因此 Norma 无法做到像 Claude 的项目级 hook，只能修改用户级 `config.toml`（或使用插件机制，见 §6.5）。
- hook 以 `shell: true` 子进程运行，stdin 收到一行 JSON，cwd 为会话工作目录。已验证：hook 子进程**继承 Kimi 的环境变量**，包括 Norma 注入的 `MULTISOUL_NODE_ID`/`MULTISOUL_HOOK_TOKEN` 等（在 `tmux new-session -e MULTISOUL_NODE_ID=7 -e MULTISOUL_HOOK_TOKEN=tok` 下，探针脚本 [`hook-probe.sh`](evidence/kimi/hook-probe.sh) 记录到 `MULTISOUL_NODE_ID=7`、`MULTISOUL_HOOK_TOKEN=tok`）。所以 Norma 现有的 `multisoul-agent-hook forward --source kimi` 模式可以原样工作。
- 退出码 0 放行；2 阻断（仅 `PreToolUse`/`Stop`/`UserPromptSubmit`）；其余 fail-open。Norma 的转发器应始终 `exit 0`。

### 6.2 事件名与实测载荷

所有事件公共字段：`hook_event_name`、`session_id`、`cwd`、`client_type:"kimi_code_cli"`，以及（会话已命名后）`session_title`。完整样例见 [`hook-payload-samples.jsonl`](evidence/kimi/hook-payload-samples.jsonl)，下表是**真实触发过**的事件：

| 事件 | 触发场景（实测） | 独有字段 |
| --- | --- | --- |
| `SessionStart` | 新会话在首条消息提交时；恢复会话在启动时 | `source: "startup"|"resume"`、`model`、`profile:"agent"` |
| `UserPromptSubmit` | 用户提交消息 | `prompt`（**数组** `[{"type":"text","text":"..."}]`，不是字符串）、`is_steer` |
| `TurnStarted` | 一轮开始 | `turn_id`（数字）、`origin_kind:"user"`、`prompt`（字符串） |
| `PreToolUse` | 工具调用前（权限检查之前） | `tool_name`、`tool_input`、`tool_call_id` |
| `PermissionRequest` | 弹出审批框前 | `tool_name`、`tool_input`、`action`、`display`、`id`、`turn_id` |
| `PermissionResult` | 审批完成后 | 同上加 `decision`；实测 `"approved"`（选 1/2）、`"rejected"`（选 3） |
| `PostToolUse` | 工具成功 | `tool_name`、`tool_input`、`tool_output`、`tool_call_id` |
| `PostToolUseFailure` | 工具失败（实测：调用不存在的工具 `NoSuchTool`） | 事件名已实测触发；载荷字段未单独留存，按源码含 `error`（未验证） |
| `Stop` | 一轮正常结束 | `stop_hook_active:false`（**不含**最后回复文本） |
| `StopFailure` | 一轮因错误结束（实测：模型接口 HTTP 400） | `error_type:"APIStatusError"`、`error_message:"400 mock failure"`（**不是** `message`/`error`/`reason`） |
| `Interrupt` | 用户在回复中途按 Esc | `turn_id`、`reason:"cancelled"`；代替 `Stop` 触发 |
| `SessionEnd` | `/exit` 或双击 Ctrl+C 退出 | `reason:"exit"` |

配置里声明了但**本次未触发过**的事件（原因：需要子智能体/后台任务/压缩/排队等额外场景，假模型难以稳定构造）：`UserPromptQueued`、`SubagentStart`、`SubagentStop`、`TaskStarted`、`PreCompact`、`PostCompact`、`Notification`、`SessionHeartbeat`。这些标记为**未验证**，Norma 不订阅它们。`kimi doctor config` 对包含全部 19 个事件名的配置输出 `All checked config files are valid.`，说明事件名本身被接受。

### 6.3 一个必须记录的异常：PostToolUse 偶发不触发

- 配置全部 19 个事件时，三次真实工具调用（含“批准一次”“批准本会话”“无需审批”三种路径）都**没有**收到 `PostToolUse`，但收到了 `PreToolUse`、`PermissionRequest/Result`、`Stop`。
- 换成仅含 `PostToolUse/PostToolUseFailure/PreToolUse`，以及再加 `Stop`，以及加到 11 个事件（`SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionResult Stop StopFailure Interrupt SessionEnd`）时，`PostToolUse` **都正常触发**。
- 根因**未验证**（源码里 PostToolUse 是 fire-and-forget；未做进一步二分）。对 Norma 的影响：状态机不得依赖 `PostToolUse` 才能离开 `RunningTool`；Norma 只订阅上述 11 个事件，这个组合下实测可用，同时 `Stop`/`Interrupt`/`StopFailure`/`PermissionResult` 也会把状态拉回，不会卡死。

### 6.4 安装/卸载而不覆盖用户配置（已验证可行性）

- 方案：在 `config.toml` 末尾追加一个带标记注释的区块，卸载时按标记删除整块：

  ```toml
  # >>> multisoul-agent-hook managed (do not edit) >>>
  [[hooks]]
  event = "SessionStart"
  command = "'/path/to/multisoul-agent-hook' forward --source kimi"
  timeout = 10
  ...（每个事件一条）
  # <<< multisoul-agent-hook managed <<<
  ```

- 已验证：在含这种标记区块、含一个无关的 `[tui_user_section]`、含一行自定义注释的 `config.toml` 上，让 Kimi 自己改写配置（TUI `/model` 切换 Thinking 开关），结果只在文件末尾追加了 `[thinking]\nenabled = false`，**注释、标记、`[[hooks]]` 全部原样保留**（`diff` 仅 2 行，见 [`config-after-rewrite.toml`](evidence/kimi/config-after-rewrite.toml)）。
- 文件格式允许 `[[hooks]]` 与其它表交错，`kimi doctor config` 验证通过。
- 约束：`norma` crate 没有 TOML 依赖，所以采用**文本级**编辑（标记行定位）；检测到用户自己写了行内 `hooks = [...]` 形式时无法安全合并，应报错而不是覆盖（该分支未做真机验证，属实现时用单元测试覆盖的边界）。
- 尊重 `KIMI_CODE_HOME`：默认路径为 `$HOME/.kimi-code/config.toml`，若进程环境设置了 `KIMI_CODE_HOME` 则用其下的 `config.toml`（官方文档与实测均确认该变量决定配置位置）。
- 多条 `command` 完全相同的规则在**同一事件**内只运行一次（官方文档）；我们每个事件只有一条，不受影响。

### 6.5 备选：插件携带 hooks（未验证，不采用）

官方文档称插件清单可声明 `hooks`，且 hook 进程会额外收到 `KIMI_PLUGIN_ROOT`。需要走 `/plugins` 安装流程与 `plugins/installed.json`，本次未实测，复杂度也高于上面的区块方案，**不采用**。

## 7. 权限模式与等待审批的判断（已验证）

- 默认模式：对 `Bash`、`Write`、`Edit` 等工具先询问（TUI 底部状态提示 `Ask When Needed mode: OFF` 表示未开启免审批）。审批界面（[`approval-prompt.txt`](evidence/kimi/approval-prompt.txt)）：

  ```text
  ▶ Run this command?   cwd: /private/tmp/kimi-iso/work2   $ echo hello-from-tool
  ▶ 1. Approve once   2. Approve for this session   3. Reject   4. Reject with feedback
  ```
- 跳过审批的参数（**只记录，Norma 不使用**）：`-y/--yolo`（Ask When Needed：常规编辑与命令自动执行，风险操作仍询问）、`--auto`（Never Ask：完全不打断）；TUI 内 `/yolo`、`/auto`、`/permission` 切换。
- 判断“在等待审批”：`PermissionRequest` 事件（弹框前触发，带 `tool_name` 与 `display.command`）；用户操作后 `PermissionResult` 带 `decision`。实测：选 1 -> `decision:"approved"`，选 3 -> `decision:"rejected"`；选 2（本会话批准）界面显示 `Approved for session`，`decision` 取值未单独留存，之后同类调用不再弹框，也不再有 `PermissionRequest`。拒绝后**没有** `PostToolUseFailure`，模型继续回复，最终 `Stop`。
- 注意：会话内“批准本会话”的规则在 `--continue` 续接后仍然有效（实测续接后同样的 `Bash` 调用不再弹框），这是 Kimi 的行为，Norma 不干预。

## 8. 事件到 Norma 状态的映射

Norma 状态：Ready / Thinking / RunningTool / WaitingApproval / Done / Failed / Ended（`AgentHookTaskState`）。基于 §6.2 的真实事件：

| Kimi 事件 | Norma 状态 | 现有 `task_state_for_provider_event` 是否已覆盖 |
| --- | --- | --- |
| `SessionStart` | Ready | 已覆盖 |
| `UserPromptSubmit` | Thinking | 已覆盖（但提示词文本为数组，需 Kimi 适配，见 §11） |
| `PreToolUse` | RunningTool(tool_name) | 已覆盖 |
| `PermissionRequest` | WaitingApproval(tool_name) | 已覆盖 |
| `PermissionResult` | Thinking（无论 approved/rejected，模型随后继续） | **未覆盖，需新增** |
| `PostToolUse` / `PostToolUseFailure` | Thinking | 已覆盖 |
| `Stop` | Done | 已覆盖 |
| `StopFailure` | Failed(error_message) | 状态已覆盖，但 `message` 字段取值需扩展到 `error_message` |
| `Interrupt`（reason=cancelled） | Done（用户中断，回到可输入） | **未覆盖，需新增** |
| `SessionEnd` | Ended | 已覆盖 |
| `TurnStarted` | 不映射（`UserPromptSubmit` 已覆盖） | 返回 None |

降级说明：
- Kimi 没有等价 `SubagentStart/Stop` 的**已验证**事件，不订阅；子智能体期间卡片会显示 Thinking/RunningTool（由 `PreToolUse` 驱动），不显示 “subagent”。
- `Stop` 不含最后回复文本，所以 Norma 的“完成摘要/推送正文”对 Kimi 取不到助手输出，降级为不带摘要（或后续读 `wire.jsonl`，本次未验证格式，不做）。
- 新会话在用户发送第一条消息前没有 hook（`SessionStart` 懒触发）：启动后到首条消息之间卡片状态由 Norma 的“进程已启动”逻辑决定，不依赖 hook；该期间没有 session_id，保存时无缓存 ID 即退化为普通启动。
- 终端内容兜底：没有 hook（未安装）时，Norma 照常把卡片当作无状态灯的终端卡片，不报错。

## 9. 退出与清理（已验证）

- `Ctrl+C`：第一次提示 `Press Ctrl+C again to exit`，约 1 秒内再按一次退出；输出 `Shutting down… (running shutdown hooks)` -> `Bye!` -> `To resume this session: kimi -r <id>`，进程退出码 0，`SessionEnd` 钩子 `reason:"exit"` 被触发。
- `/exit`（别名 `/quit`、`/q`）：同样退出（已实测 `/exit`，`SessionEnd reason:"exit"`）。
- 残留进程：三次退出后 `ps aux | grep kimi`（排除假服务与 tmux）均为 0 个；`exec kimi` 使 PTY 子进程就是 kimi 本身。Bash 工具的子进程（`echo`）已随命令结束。后台任务（`TaskStarted`）场景**未验证**；官方文档称默认退出时停止后台任务（`KIMI_CODE_BACKGROUND_KEEP_ALIVE_ON_EXIT` 可改变）。Norma 的 `PtyHandle::Drop` 仍负责杀进程。
- `Esc`（回复进行中）：中断当前轮，TUI 显示 `Interrupted by user`，触发 `Interrupt`（§6.2）。
- 未安装时：`/bin/zsh -lic "exec kimi_nonexistent_x"` -> `zsh:1: command not found: kimi_nonexistent_x`，退出码 127（已验证对一个不存在命令的行为；对 `kimi` 本身未卸载验证）。

## 10. 未安装提示与官方资源

- 安装命令（官方文档）：`curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash`；备选 `npm install -g @moonshot-ai/kimi-code`。官方站点 `https://code.kimi.com`，文档 `https://moonshotai.github.io/kimi-code/`，仓库 `https://github.com/MoonshotAI/kimi-code`（MIT，已核对 `LICENSE`）。
- 官方图标来源（**只调查，未下载进仓库**）：
  - 仓库内 `apps/vscode/resources/kimi-icon.svg`（24x24 线框，`currentColor` 描边，已读取文件头确认）与 `apps/vscode/resources/kimi-icon-storefront.png`、`docs/public/favicon.ico`、`apps/vscode/webview-ui/public/kimi-logo.png`。
  - URL：`https://github.com/MoonshotAI/kimi-code/blob/main/apps/vscode/resources/kimi-icon.svg`。
- 使用条款：仓库为 MIT 许可（`Copyright (c) 2026 Moonshot AI`），MIT 覆盖代码与随仓库分发的资源副本的版权许可；**商标/品牌使用没有单独声明**（在 `README.md`、`CONTRIBUTING.md`、`LICENSE` 中检索 trademark/brand 无结果）。结论：品牌使用条款**未验证**。建议 Norma 使用与现有 Claude/Codex 等占位同风格的自绘或 Kimi 配色（代码里已有占位色 `0x1f6feb`）图标，并在合并前由人决定是否直接使用官方 logo。

## 11. 本 CLI 接入方案

### 11.1 启动命令（已验证）

`/bin/zsh -lic "exec kimi"`（骨架已是该命令，无需修改）。不带任何跳过审批参数。

### 11.2 恢复命令

有 hook 缓存的会话 ID 时：`exec kimi --session '<id>'`（ID 经现有 `shell_quote`）；无 ID 时仍为 `exec kimi`。**不使用** `--continue`（见 §4 结论）。`-S/--session` 在 help 中有文档，`-r` 虽可用但未文档化，不采用。

### 11.3 hook 安装与卸载

- 新增 `HookSource::Kimi` / `HookTarget::Kimi` / `HookInstallPaths.kimi_config`（默认 `<KIMI_CODE_HOME 或 ~/.kimi-code>/config.toml`）。
- 新文件 `crates/norma/src/agent_hook/kimi.rs`：文本级区块安装/卸载/检测（§6.4），事件列表 `hook_events_for_source(Kimi)` = 11 个事件：`SessionStart UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PermissionRequest PermissionResult Stop StopFailure Interrupt SessionEnd`；每条 `command = "<build_hook_command(path, Kimi)>"`、`timeout = 10`。
- 幂等：重复安装先删旧区块再写；卸载只删除标记区块，保留其余内容；配置文件不存在时创建；无 `[[hooks]]` 冲突的行内 `hooks =` 形式时返回错误。
- 状态检测 `agent_hooks_installed_for_source(Kimi)`：区块存在且 11 个事件齐全。

### 11.4 载荷适配（新增 Kimi 专属解析，不改其他 CLI 行为）

- `provider_session_id`/`tool_name` 通用逻辑已能读 `session_id`/`tool_name`，无需改。
- `message`：`StopFailure` 使用 `error_message`；`Interrupt` 的 `reason` 只在映射到 Done 时忽略文案。
- `extract_prompt_text`：Kimi 的 `UserPromptSubmit.prompt` 是 `[{type,text}]` 数组，需拼接 `text` 后截断到 60 字；放入 `kimi.rs` 的函数并由调用点按 `HookSource::Kimi` 分派。
- `task_state_for_provider_event`：新增 `PermissionResult -> Thinking`、`Interrupt -> Done`。这两个事件名在其它 provider 的 `hook_events_for_source` 中不存在，不会影响其它 CLI。

### 11.5 会话发现与保存

`find_agent_session_id_for_profile(Kimi, _)` 保持 `None`（不支持目录发现）；会话 ID 只来自 hook 缓存，热路径不扫描目录、不查库，满足性能护栏。

### 11.6 降级策略

| 能力 | 不可用时 |
| --- | --- |
| hook 未安装 | 卡片为普通终端卡片，无状态灯；设置页显示“未安装”可一键安装 |
| 没有缓存 session_id | 重新打开时普通启动（新会话） |
| 未登录/未配置模型 | 卡片内显示 Kimi 自己的欢迎页，由用户 `/login` |
| 拿不到最后回复文本 | 完成通知不带摘要 |
| 子智能体/后台任务状态 | 不支持（未验证） |
| `PostToolUse` 偶发缺失 | 由 `Stop`/`PermissionResult`/下一事件纠正状态 |
| CLI 未安装 | PTY 内显示 `command not found`；Norma 可在设置页提示安装命令（§10） |

### 11.7 需要改动的文件清单

共享位置（需在合并时处理冲突，改动保持最小）：
- `crates/norma/src/agent_hook/mod.rs`：`HookSource`、`HookTarget` 各加 `Kimi`；`parse_hook_source`、`parse_hook_target`、`hook_source_arg`、`profile_for_source` 各加一行；`HookInstallPaths` 加 `kimi_config`；`default_install_paths` 加路径。
- `crates/norma/src/agent_hook/event.rs`：`hook_events_for_source` 加 Kimi 分支；`task_state_for_provider_event` 加两条（或下沉到 `kimi.rs` 由分派调用）；`parse_hook_event` 的 `message` 取 `error_message`（仅对 Kimi）。
- `crates/norma/src/agent_hook/install.rs`：`install_agent_hooks`/`uninstall_agent_hooks`/`agent_hooks_installed*` 各加 `HookTarget::Kimi` 分支。
- `crates/app/src/settings/hooks.rs`、`crates/app/src/settings/agent_hooks.rs`：设置页 hooks 行表加 Kimi 一行及开关处理函数。
- `crates/norma/src/orchestration/`：`OrchestrationProvider::ALL` 加 `Kimi`（其余 Kimi 的 match 分支骨架已有）。
- `crates/runtime/src/session_snapshot.rs`：恢复命令 `AgentProfile::Kimi` 分支改为带 `--session`。

CLI 专属新文件：
- `crates/norma/src/agent_hook/kimi.rs`（区块安装/卸载/检测、prompt 数组解析、测试）；如超 500 行则拆 `kimi/` 目录。
- 测试：区块幂等、卸载保留用户内容、含无关表时不破坏、`KIMI_CODE_HOME` 路径、事件映射含 `PermissionResult/Interrupt/StopFailure(error_message)`、恢复命令带/不带 ID、`prompt` 数组解析；设置页行存在。
- 真实 App 验收（后续阶段）：在 Norma 里新建 Kimi 卡片，用本调研的假服务配置运行，检查状态灯在 Thinking/WaitingApproval/RunningTool/Done 间切换，关闭后重开能 `--session` 恢复。

## 12. 已验证 / 未验证汇总

| 项 | 状态 | 依据 |
| --- | --- | --- |
| 版本 2.2.0、路径、Mach-O arm64 | 已验证 | §1 命令输出 |
| 完整 help 与子命令 help | 已验证 | `help-all.txt` |
| `-S/--session`、`-c/--continue` 语义 | 已验证 | §4 表（含 TUI 与 `-p`） |
| `-r` 别名可用但未文档化 | 已验证 | §4 |
| 旧版 `--resume`、`--yolo` 语义 | 已验证为不同 | §2 |
| `zsh -lic "exec kimi"` 可进入 TUI；首次目录信任对话框 | 已验证 | §3 |
| 未登录界面与 `/login` 入口 | 已验证（界面）/ 未验证（登录流程，无账号） | §3 |
| 真实 Kimi 模型的对话行为 | 未验证 | 无凭据，使用假 OpenAI 服务 |
| 会话 ID 格式、来源、续接后不变 | 已验证 | §4、§5 |
| 会话存储位置与格式 | 已验证 | §5 |
| hooks 机制、配置位置、strict 4 字段 | 已验证（实测+源码） | §6.1 |
| 12 种事件的真实载荷 | 已验证 | `hook-payload-samples.jsonl` |
| `UserPromptQueued/Subagent*/TaskStarted/Pre|PostCompact/Notification/SessionHeartbeat` | 未验证 | 难以在假模型下构造；不订阅 |
| `PostToolUse` 在 19 事件配置下缺失的根因 | 未验证 | §6.3；11 事件组合下可用 |
| hook 继承 `MULTISOUL_*` 环境变量 | 已验证 | §6.1 |
| 区块安装不被 Kimi 改写破坏 | 已验证（一条写入路径：`/model`） | §6.4；其它写入路径未逐一验证 |
| 行内 `hooks = [...]` 冲突处理 | 未验证 | 实现时单元测试覆盖 |
| 插件携带 hooks | 未验证 | §6.5，不采用 |
| 默认权限模式与审批界面 | 已验证 | §7 |
| 审批 approve/reject 的 hook 序列 | 已验证 | §7 |
| `--yolo`/`--auto` 语义 | 已验证（help），未实跑 | 只记录，不使用 |
| Ctrl+C、`/exit` 退出与无残留进程 | 已验证 | §9 |
| 后台任务期间退出的残留 | 未验证 | 未构造后台任务 |
| 未安装提示 | 部分验证（zsh 对缺失命令的行为）；安装命令来自官方文档 | §9、§10 |
| 官方图标来源 | 已验证文件存在 | §10 |
| 品牌/商标使用条款 | 未验证 | 仓库无声明 |
