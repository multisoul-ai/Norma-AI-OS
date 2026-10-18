# Hermes Agent 与 Norma OS 对接调研

调研日期：2026-10-18。目标：新增智能体卡片 `AgentProfile::Hermes`，在 PTY 内以 `/bin/zsh -lic "exec hermes"` 启动，支持会话恢复、状态灯（hooks）和跨 Agent 编排。总计划见 `docs/exec-plans/2026-10-18-new-cli-agent-cards-goal.md`。

证据文件位于 `docs/research/evidence/hermes/`（下文以 `ev/` 指代）。

## 0. 测试方法与隔离

- 所有交互测试都设置 `HERMES_HOME=/tmp/hermes-probe/home1|home2`，**不读写**真实 `~/.hermes`，不使用任何真实账号凭据。真实 `~/.hermes/hermes-agent` 只作为官方源码与文档只读参考（同一份安装，`git remote` 为 `https://github.com/NousResearch/hermes-agent.git`，commit `d923a5eb487a`）。
- 为得到完整的“提问 → 调工具 → 审批 → 回复 / 失败”流程而不使用真实账号，写了一个本地 OpenAI 兼容 mock 服务（`ev/mock_llm.py`，监听 127.0.0.1:18991），`config.yaml` 里 `model.provider: custom` 指向它。mock 只按提示词中的关键字返回固定回复、一次 `terminal` 工具调用、`rm -rf` 危险命令、HTTP 500/401。
- 交互界面用 tmux（独立 socket `-L hm`）+ `/bin/zsh -lic 'exec hermes'` 驱动并 `capture-pane`。
- Hermes 是 Python 程序，源码可读；凡“事件名、载荷字段、配置结构”均同时给出源码位置与实测输出。

## 1. 安装方式、版本、二进制路径（已验证）

```
$ which hermes            -> /Users/openclawd/.local/bin/hermes
$ hermes --version        -> Hermes Agent v0.22.2 (2026.9.24) · stable · local d923a5eb487a
                             Install directory: /Users/openclawd/.hermes/hermes-agent
                             Install method: git     Python: 3.14.7   OpenAI SDK: 2.24.0
$ cat ~/.local/bin/hermes -> #!/bin/sh\nexec /Users/openclawd/.hermes/hermes-agent/.hermes/bin/hermes "$@"
```

- `~/.local/bin/hermes` 是一个 `sh` 包装脚本，最终 `exec` 到 Python 3.14 进程。因此 `zsh -lic "exec hermes"` 的 PTY 子进程链是 zsh（被 exec 替换）→ sh（被 exec 替换）→ python3.14，**最终只有一个进程**（已验证：`ps` 取 pane_pid 的后代，仅返回 pane_pid 自身）。
- 全新 `HERMES_HOME` 首次运行会打印 `hermes: preparing dependencies for this data root...` / `Installing Python dependencies`，约数秒（已验证，`home1` / `home2` 首次调用均出现）。首次启动在 PTY 里偏慢，状态灯应容忍。
- 通过 `/bin/zsh -lic` 登录 shell 能找到 `hermes`（`~/.local/bin` 在登录 PATH 中）。未安装时 `/bin/zsh -lic 'exec hermesnope'` 输出 `zsh:1: command not found: hermesnope`（已验证；注意本机 zsh 还会先打印 gitstatus 初始化噪音）。

## 2. 命令行帮助（已验证）

完整输出：`ev/help-root.txt`（`hermes --help`）、`ev/help-subcommands.txt`（`chat / sessions / hooks / plugins / setup / model / config / auth / status / doctor` 的 `--help`）。与对接相关的全局选项：

| 选项 | 含义（摘自 `--help`） |
| --- | --- |
| `--resume, -r SESSION` | 按 ID 或标题续接；传 `latest` 为最近一次 |
| `--continue, -c [NAME]` | 按名称续接；无名称为最近一次 |
| `--no-restore-cwd` | 不要 cd 到被续接会话记录的工作目录 |
| `--in DIR` | 启动/续接前先进入 DIR |
| `--accept-hooks` | 自动批准 `config.yaml` 里未见过的 shell hooks |
| `--yolo` | 绕过所有危险命令审批（**只记录，Norma 不使用**） |
| `--pass-session-id` | 把会话 ID 放进 system prompt |
| `--tui` / `--cli` | 现代 TUI / 经典 prompt_toolkit REPL；默认启动的是经典 REPL（已验证界面） |
| `-z, --oneshot PROMPT` | 单次非交互模式（审批自动绕过） |

子命令里有 `hooks`（`list/test/revoke/doctor`，**没有 add/install**）、`plugins`（`list/enable/disable/install/remove...`）、`sessions`（`list/export/delete/rename...`）、`skills`、`setup`、`model`。

## 3. 启动、首次运行与未登录界面（已验证）

用 `HERMES_HOME=/tmp/hermes-probe/home1`（空目录）`/bin/zsh -lic 'exec hermes'`：

1. 先出现遥测选择页（`ev/first-launch.txt`）：`Help improve Hermes?` 三选一 `Collect and send to Nous / Collect locally only / No thanks`，默认 `No thanks`，Enter 即可。`config.yaml` 里 `telemetry.prompted: true` 可抑制，但 `home2` 里即使写了该键，首次仍停在该页（捕获为空白，按 Enter 后才进入界面）。结论：**首次运行可能需要用户在卡片里按一次 Enter**，这是用户操作，Norma 不代按。
2. 然后是横幅（`ev/first-launch2.txt`），其中直接打印当前会话：`Session: 20261018_113007_c6c880`，并提示 `no model configured — run /model or hermes setup`。
3. 未配置模型时询问（`ev/unauth-after-n.txt`）：`☤ No inference provider is configured yet — let's fix that. ... Set up a provider now? [Y/n]:`。回答 `n` 后进入 REPL，状态栏为 `☤ unknown │ ctx -- │ ...`，输入框 `❯`。
4. 因此“未登录”不是进程退出，而是**交互 REPL 内的引导**；Norma 无法用 hook 区分，只能显示 Ready/未知。
5. 真实 `~/.hermes/config.yaml` 已配置 `model.provider: auto`、OpenRouter 地址，未实测其真实登录流程（不使用真实凭据，**未验证**）。

默认 `approvals.mode` 是 `smart`（源码 `hermes_cli/config_defaults.py:1679`，同文件 `timeout: 300`）。

## 4. 会话续接（已验证）

### 4.1 会话 ID 来源

- 格式 `YYYYMMDD_HHMMSS_<6位hex>`，如 `20261018_113636_195f7f`。
- 来源 1：启动横幅打印 `Session: <id>`（终端输出，不可靠，不应解析）。
- 来源 2（采用）：**hook/插件事件的 `session_id` 字段**（见 §6），首个用户提问后才出现。
- 来源 3：退出时打印 `Resume this session with:\n  hermes --resume <id>\n  hermes -c "<标题>"` 以及 `Session: / Title:`（`pane-c.log` 实测）。同样是终端输出，不采用。
- 来源 4：`$HERMES_HOME/state.db` 的 `sessions` 表（`hermes sessions list` 也读它）。**禁止**在 PTY drain/渲染/关闭路径查询，不采用。

### 4.2 续接参数实测

| 命令 | 结果 |
| --- | --- |
| `hermes -c`（同工作目录） | `↻ Resumed session 20261018_114143_8e0087 "MOCK-REPLY to: second session text" (1 user message, 2 total messages)` + `Previous Conversation` 面板，续接的是**最近一次** |
| `hermes --resume 20261018_113636_195f7f` | 按 ID 续接成功；续接后继续提问，所有 hook 事件的 `session_id` **仍是同一 ID**（不分叉） |
| 从另一目录启动 `--resume <id>` | 横幅显示启动目录，但打印 `↻ Working directory: /private/tmp/hermes-probe/work2`（会话记录的目录）；插件 `os.getcwd()` 取到的是启动目录。`--no-restore-cwd` 时同样打印该行，**工具实际 cwd 是否改变未验证** |
| `hermes --resume nosuchid_123` | 打印 `Session not found: nosuchid_123 / Use a session ID from a previous CLI run (hermes sessions list).`，进程**不退出**（`pane_dead=0`），随后进入新会话 |
| `hermes -c` 在没有会话记录的新目录 | 仍续接了其他目录最近的会话（`resume-no-session-in-workspace.txt`）；`-c --in <新目录>` 同样如此（`resume-in-empty-workspace.txt`）。因此 `-c` 的“工作区作用域”不可靠，**Norma 不使用 `-c`** |

续接后若尚未提问，不会触发 `on_session_start`，`on_session_finalize` 的 `session_id` 为空字符串（`hook-events` 实测）。续接提问后第一个事件即带 `session_id`，且 **续接不触发 `on_session_start`**（`ev/norma-env-sink.jsonl` 之外的第二次实测：事件序列为 `PluginLoaded, UserPromptSubmit, PreToolUse, PostToolUse, Stop, SessionEnd`）。因此 Norma 续接已知 ID 的卡片，ID 由 Norma 自己持有，事件到达后校验一致即可。

## 5. 会话存储（已验证）

- `$HERMES_HOME/state.db`（SQLite，WAL），表 `sessions`，列含 `id, source, model, started_at, ended_at, end_reason, message_count, title, cwd, git_repo_root...`（`ev/state-db-sessions-schema.txt`）。交互会话 `source='cli'`、`end_reason='cli_close'`；`-z` 一次性调用 `source='oneshot'`。
- `$HERMES_HOME/sessions/` 目录只出现了 `request_dump_*.json`（失败请求转储），**不是**会话文件。
- 结论：不扫描目录、不查库，会话 ID 只能来自 hook/插件事件缓存。`session_snapshot` 的会话发现保持返回 `None`（骨架已如此）。

## 6. Hooks / 扩展 / 插件机制（已验证，重点）

Hermes 有**两套**可用机制，均在 `HERMES_HOME` 下，均实测通过。

### 6.1 Shell hooks（`config.yaml` 的 `hooks:`）

- 源码 `agent/shell_hooks.py`；官方文档 `website/docs/user-guide/features/hooks.md`。
- 配置：`config.yaml` 顶层 `hooks:`，键为事件名，值为 `[{command, matcher?, timeout?}]`。有效事件集合来自 `hermes_cli/plugins.py` 的 `VALID_HOOKS`（`pre_tool_call, post_tool_call, pre_llm_call, post_llm_call, on_session_start, on_session_end, on_session_finalize, on_session_reset, api_request_error, pre_approval_request, post_approval_response, on_human_input_request, on_human_input_resolved, subagent_start, subagent_stop, pre_command, on_stream_*` 等）。
- 脚本通过 stdin 收到 JSON：`{hook_event_name, tool_name, tool_input, session_id, cwd, profile, extra:{...}}`，样例见 `ev/shell-hook-payload-samples.json`。
- **首次使用需逐事件同意**：交互 TTY 弹出（`ev/consent-prompt.txt`）：
  `⚠ Hermes is about to register a shell hook that will run a command on your behalf. Event: on_session_start Command: ... Allow this hook to run? [y/N]:`
  同意记录在 `$HERMES_HOME/shell-hooks-allowlist.json`，格式（实测写入后 `hermes hooks list` 全部显示已允许）：
  `{"approvals":[{"event","command","approved_at","script_mtime_at_approval"}]}`；也可用 `--accept-hooks` / `HERMES_ACCEPT_HOOKS=1` / `hooks_auto_accept: true` 绕过（会放宽用户所有 hooks，Norma 不应使用）。
- 缺点：每个事件一次同意项；脚本 mtime 变化会触发漂移检查；必须编辑用户 `config.yaml` 的 YAML（仓库里没有 YAML 依赖，也无法保证保留用户注释）；每个事件新起一个子进程，且 hook 之间可并发（实测日志出现两条记录交错写入同一行），顺序不保证。

### 6.2 目录插件（采用）

- 源码 `hermes_cli/plugins.py`、`plugins_discovery.py`；文档 `website/docs/user-guide/features/plugins.md`。
- 目录：`$HERMES_HOME/plugins/<name>/plugin.yaml`（`name/version/description`）+ `__init__.py`（`register(ctx)`，`ctx.register_hook(event, callback)`，callback 接收关键字参数）。
- 插件**默认不启用**，需出现在 `config.yaml` 的 `plugins.enabled`。启用/停用用官方命令（实测）：
  `hermes plugins enable <name>` → `✓ Plugin <name> enabled. Takes effect on next session.`，`config.yaml` 变为 `plugins: {enabled: [<name>], disabled: []}`；`hermes plugins disable <name>` → 从 `enabled` 移除并加入 `disabled`；再次 `enable` 会清除 `disabled`。每条命令约 3 秒（实测 `time` 3.0 s），须在后台线程执行。`hermes plugins remove <name>` 会删除插件目录并清理列表（对象是目录名，不看标记）。
- 插件内回调是同进程调用，**无逐事件同意提示**（已验证：整个交互过程无任何弹窗）；回调拿到的是结构化关键字参数，而不是 shell hook 的信封；能读到进程环境变量。
- `register()` 在 Hermes 启动阶段就被调用（启动、尚未提问时即向本地端点 POST 成功，`ev/norma-env-sink.jsonl` 第一行 `PluginLoaded`），可用作“进程已启动 = Ready”。
- 实测 Norma 的现有环境变量协议在插件里可用：`MULTISOUL_NODE_ID / MULTISOUL_HOOK_ENDPOINT / MULTISOUL_HOOK_TOKEN` 由 PTY 继承，插件用 `urllib` POST `Authorization: Bearer <token>` 成功（`ev/norma-env-sink.jsonl`，`ev/http_sink.py` 为接收端，`ev/norma-probe-plugin.py` 为插件原型）。
- 本机 `plugins.enabled` 机制要求 `config.yaml` 有 `_config_version`；缺失时 Hermes 记录 `config migration skipped`，而 `hermes plugins enable` 能在已有合法配置上写入。**对用户真实 `config.yaml` 的行为（包括是否丢失注释）未验证**（实测只在 `home2` 的无注释文件上执行；官方 CLI 会重写该文件，注释保留情况未验证）。

### 6.3 实测事件与完整载荷样例

事件在 `home2` 以 shell hook 全部订阅后逐一触发（`ev/shell-hook-payload-samples.json` 为每种事件一条，长字段截断标注）。一次简单提问 `say hi` 的事件序列（已验证）：

```
on_session_start  -> pre_llm_call -> pre_api_request -> on_stream_start -> on_stream_end
-> post_api_request -> post_llm_call -> on_session_end(completed=true)
```

`on_session_start` 实际载荷（原文，仅 `extra` 缩写前保持原样）：

```json
{"hook_event_name":"on_session_start","tool_name":null,"tool_input":null,
 "session_id":"20261018_113636_195f7f","cwd":"/private/tmp/hermes-probe/work2","profile":"default",
 "extra":{"model":"mock-model","platform":"cli","telemetry_schema_version":"hermes.observer.v1"}}
```

工具调用（`RUNTOOL`）序列：`pre_llm_call → ... → post_api_request(finish_reason=tool_calls) → pre_tool_call{tool_name:"terminal", tool_input:{command}, extra.tool_call_id} → post_tool_call{extra.status:"ok", duration_ms:220} → pre_api_request ... → post_llm_call → on_session_end`。

危险命令审批序列（`DANGER`，默认 `smart` 模式，命令 `rm -rf` 触发）：

```
pre_tool_call -> pre_approval_request(surface="smart") -> pre_approval_request(surface="cli")
-> on_human_input_request{kind:"approval", request_id, prompt:"rm -rf ..."}      <- 终端出现选择菜单
-> (用户选择 Deny) on_human_input_resolved{kind:"approval", outcome:"deny"}
-> post_approval_response{choice:"deny"} -> post_tool_call{status:"error"} -> post_llm_call -> on_session_end
```

终端菜单（`ev/approval-prompt.txt`）：`1. Allow once / 2. Allow for this session / 3. Add to permanent allowlist / 4. Deny`，`↑/↓ to select, Enter to confirm (290s)`。

上游失败（mock 返回 401，`ev/error-screen.txt` 另含 500 重试界面）：仅 `pre_llm_call, pre_api_request, on_stream_start, on_stream_end, api_request_error{status_code:401, retryable:false, reason:"auth"}`，**没有** `post_llm_call` / `on_session_end`。HTTP 500 时 `api_request_error{retryable:true, reason:"server_error"}` 连续出现，界面显示 `Provider temporarily unavailable — retrying automatically in 17s (cycle 1/5)`。

中断（模型调用中 Ctrl+C）：`on_session_end{completed:false, failed:false, interrupted:true, turn_exit_reason:"interrupted_during_api_call"}`（延迟约 4 秒到达，与 `on_stream_end` 并发）。在“重试等待”中 Ctrl+C 则 5 秒内**未观察到** `on_session_end`（`⚡ Interrupt detected during automatic recovery wait, aborting.`）。

退出：`/exit` 触发 `pre_command{command:"quit", alias_used:"exit"}`（此时 `session_id` 为空，`extra.session_key` 才是会话 ID）然后 `on_session_finalize{reason:"shutdown"}`；Ctrl+D 同样触发 `on_session_finalize`。

### 6.4 载荷字段汇总

| 字段 | 是否有 | 说明 |
| --- | --- | --- |
| `session_id` | 是 | 顶层，除 `pre_command` 与续接后未提问就退出的 finalize 外均非空 |
| `cwd` | shell hook 有 | 插件里用 `os.getcwd()`（启动目录） |
| 工具名 | 是 | `pre/post_tool_call` 的 `tool_name`（实测为 `terminal`），工具参数在 `tool_input`/`args` |
| 审批命令 | 是 | `pre_approval_request.extra.command`、`on_human_input_request.extra.prompt`；**没有 tool_name**，需由前一条 `pre_tool_call` 的同一 `turn_id/tool_call_id` 关联 |
| 回复文本 | 是 | `post_llm_call.extra.assistant_response` |
| 失败信息 | 是 | `api_request_error.extra.{status_code,retryable,reason,error}` |
| 隐私 | **重** | `pre_llm_call/pre_api_request/post_llm_call` 带完整 `conversation_history` 与 `request_messages`；Norma 的插件必须只转发白名单字段 |

## 7. 默认权限与审批（已验证）

- 默认 `approvals.mode: smart`（先经辅助 LLM 判定，危险时升级到用户）。实测普通 `echo` 不弹审批，`rm -rf <目录>` 弹出审批菜单（见 §6.3）。
- 跳过审批：`--yolo` 或 `approvals.mode: off`（`hermes_cli/approval_mode.py`：`VALID_APPROVAL_MODES = ("manual","smart","off")`）。**只记录，Norma 默认命令不带**。
- 判断“等待审批”：`on_human_input_request` 且 `extra.kind == "approval"`（同时 `pre_approval_request` 出现两次，`surface` 分别为 `smart` 与 `cli`，只在 `cli` 才是真正等人）；解除：`on_human_input_resolved`（`outcome:"deny"` 已验证，allow 类 outcome 值未验证）。文档称同一事件还用于 `kind:"sudo"|"clarify"`，实测只覆盖了 `approval`。
- 默认超时 300 秒（配置值，超时行为未验证）。

## 8. 状态映射（Ready、Thinking、RunningTool、WaitingApproval、Done、Failed）

插件把 Hermes 事件翻译成 Norma 现有 Claude 风格事件名，使 `task_state_for_provider_event` 可原样复用（与 OpenCode 插件一致）。

| Hermes 事件（已验证触发） | 发给 Norma 的 `hook_event_name` | 状态 | 备注 |
| --- | --- | --- | --- |
| 插件 `register()` 被调用 | `SessionStart`（无 `session_id`） | Ready | 启动即触发，早于首次提问 |
| `on_session_start`（仅新会话首轮） | `SessionStart` + `session_id` | Ready | 续接不触发 |
| `pre_llm_call` | `UserPromptSubmit` | Thinking | 每轮开始，续接首轮也带 `session_id` |
| `pre_tool_call` | `PreToolUse` + `tool_name` | RunningTool | |
| `post_tool_call` | `PostToolUse`（`status=error` 同样发 `PostToolUse`） | Thinking | |
| `on_human_input_request` 且 `kind=approval` | `PermissionRequest` + 缓存的 `tool_name` | WaitingApproval | 工具名取同 `tool_call_id` 的最近 `pre_tool_call`（需插件内缓存） |
| `on_human_input_resolved` | `PostToolUse` | Thinking | |
| `api_request_error` 且 `retryable=false` | `StopFailure` + `message=reason` | Failed | 已验证 401 |
| `api_request_error` 且 `retryable=true` | 不发 | 保持 Thinking | 重试耗尽后的最终失败信号**未验证** |
| `on_session_end`（含 `interrupted=true`） | `Stop`（`message` 可带缓存的 `assistant_response` 截断） | Done | 中断也映射 Done |
| `on_session_finalize` | `SessionEnd` | Ended | `/exit`、Ctrl+D、SIGTERM 均触发 |

降级：
- 失败后（401）没有收尾事件：下一次 `pre_llm_call` 自然重置为 Thinking；不额外推断。
- “重试等待中 Ctrl+C”无事件：状态停在 Thinking，直到下一个事件或进程退出，已知缺口，不猜测补全。
- 无 hook（插件未安装/未启用）时：卡片只显示“进程已启动”，无状态灯，也无 `session_id`，恢复退化为普通启动。
- 事件由独立线程投递（`on_stream_end` 与 `post_api_request` 等并发），到达顺序不可靠；Norma 状态机应对乱序容错，插件发送时应带单调序号或接收端按到达时间处理。

## 9. 退出与清理（已验证）

- `/exit`（及别名）、Ctrl+D：正常退出，触发 `on_session_finalize{reason:"shutdown"}`，退出码 0（tmux `pane_dead_status 0`），打印 `Resume this session with: hermes --resume <id>`。
- `kill -TERM <pid>`：4 秒内进程消失，`pane_dead=1 status=0`，仍触发 `on_session_finalize`（优雅关闭）。符合 `PtyHandle::Drop`/`kill_all` 的清理预期。
- 退出后 `ps` 无 hermes 残留进程（含 Python 子进程）。未实测后台终端进程（`terminal` 工具长任务、`delegate_task`）的残留，**未验证**。

## 10. 未安装提示（已验证）

- 登录 shell 下未找到命令：`zsh:1: command not found: hermes`。
- 官方安装命令（`README.md` 第 40 行与 `website/docs/getting-started/installation.md`，且 `curl -s -o /dev/null -w '%{http_code}' -L https://hermes-agent.nousresearch.com/install.sh` 返回 200）：
  `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`
- 官方站点：https://hermes-agent.nousresearch.com ；源码：https://github.com/NousResearch/hermes-agent 。macOS 桌面包仅 Apple Silicon（文档原话 `macOS installer is Apple Silicon only`）。
- Norma 的未安装提示应展示上述命令与链接，**不代为执行**。

## 11. 官方图标来源与条款（只调查，未下载入仓）

- 仓库内 `assets/icon-master.svg`（116 KB，1024×1024 圆角方形，`~/.hermes/hermes-agent/assets/icon-master.svg`）与 `website/static/img/favicon.svg` 大小一致（116350 字节）。
- 远程地址（均返回 HTTP 200）：
  - https://raw.githubusercontent.com/NousResearch/hermes-agent/main/assets/icon-master.svg
  - https://raw.githubusercontent.com/NousResearch/hermes-agent/main/website/static/img/favicon.svg
- 条款：仓库 `LICENSE` 为 **MIT**（`Copyright (c) 2025 Nous Research`，https://github.com/NousResearch/hermes-agent/blob/main/LICENSE）。仓库内未找到独立的商标/品牌使用政策（`grep` 无结果）；MIT 授权代码，**是否覆盖 logo 的商标使用未验证**。建议：作为“指向该第三方产品的标识性使用”，在 Norma 内沿用其他 Agent 图标同样的做法，并在合并前由人工确认品牌条款。图标需转成 Norma 的 `toolbar-hermes.svg` 单色/工具栏风格，由合并方处理，本调研不下载。

## 12. 本 CLI 接入方案

### 12.1 启动命令

`/bin/zsh -lic "exec hermes"`（现有骨架已是此值，**无需修改**；默认不带 `--yolo`、`--accept-hooks`、`--tui`）。

### 12.2 恢复命令

有 hook 缓存的会话 ID 时：`exec hermes --resume '<id>'`（`<id>` 用现有 `shell_quote`，字符集仅 `[0-9a-f_]`）。
无缓存 ID 时：退化为 `exec hermes`（新会话）。**不使用 `-c` / `--resume latest`**（§4.2 实测其工作区作用域不可靠，会误续接其他目录的会话）。
是否追加 `--no-restore-cwd`：卡片已有项目目录，但该标志对工具 cwd 的实际效果未验证，**不加**；沿用 Hermes 默认行为（回到会话记录目录）。

### 12.3 Hook 安装与卸载（插件方案）

采用 §6.2 目录插件，理由：无逐事件同意弹窗；回调为结构化参数；不用手写 YAML；可复用 Norma 现有 `MULTISOUL_*` 环境协议和 OpenCode 插件的“受管文件 + 标记”模式。

- 插件目录：`<HERMES_HOME 或 ~/.hermes>/plugins/norma-agent-hook/`，文件 `plugin.yaml` + `__init__.py`；`__init__.py` 首行写标记 `# multisoul-hermes-plugin-marker`。
- 安装：
  1. 目录已存在且 `__init__.py` 不含标记 → 拒绝覆盖，返回错误（同 `install_opencode_plugin`）。
  2. 原子写入两个文件（复用 `atomic_file::replace`）。
  3. 在后台线程执行 `/bin/zsh -lic 'hermes plugins enable norma-agent-hook'`（约 3 秒，须不阻塞 GPUI 前台）；失败（hermes 未安装、命令失败）时保留文件并在设置页显示“未启用”。
- 卸载：先 `hermes plugins disable norma-agent-hook`，再仅在标记存在时删除目录；不使用 `hermes plugins remove`（它不看标记）。
- 已安装判断：文件存在且含标记，且 `config.yaml` 的 `plugins.enabled` 含该名称。后一项需要读 YAML：无 YAML 依赖时用保守的文本检查，若无法确定则显示“已写入，启用状态未知”，**具体实现需在实现阶段再评估**，不得猜测。
- 插件行为要求：
  - 仅订阅 `on_session_start, pre_llm_call, pre_tool_call, post_tool_call, on_human_input_request, on_human_input_resolved, api_request_error, post_llm_call, on_session_end, on_session_finalize`。
  - 只转发白名单字段（`session_id, tool_name, kind, retryable, reason, status, interrupted` 及截断后的 `assistant_response`），**不转发** `conversation_history / request_messages / user_message / tool_input`。
  - POST 放到守护线程、超时 2 秒、吞掉全部异常（不能拖慢或打断 Hermes）；`on_session_finalize` 发送后短暂 `join`（≤1 秒），避免进程退出丢事件。
  - 缺少任一 `MULTISOUL_*` 环境变量时静默不动作。

### 12.4 状态映射

见 §8 表。复用 `task_state_for_provider_event`，不新增状态名；仅 `hook_events_for_source` 为 Hermes 列出上述 Claude 风格事件名。

### 12.5 编排（跨 Agent）

- Hermes 技能目录：`<HERMES_HOME>/skills/<name>/SKILL.md`，frontmatter 含 `name / description / version`；实测放入 `home2/skills/norma-orchestration-probe/SKILL.md` 后 `hermes skills list` 显示为 `local ... enabled`。
- 因此 `OrchestrationInstallPaths::skill_path(Hermes)` 可返回 `<HERMES_HOME 或 ~/.hermes>/skills/norma-orchestration/SKILL.md`（`presentation` 技能同理 `norma-presentation`）。Hermes 的 `terminal` 工具实测可执行 shell 命令，因此技能内通过 `norma` CLI 的做法可行。
- **模型是否真的按该技能调用 norma CLI 未验证**（mock 模型无法验证）。
- `OrchestrationProvider::ALL` 需把 `Hermes` 加入；凭据文件 `orchestration/hermes.json` 沿用现有机制。

### 12.6 降级策略

| 能力 | 状态 | 降级 |
| --- | --- | --- |
| 状态灯 | 支持（需插件已启用） | 未启用时仅“进程已启动” |
| 会话恢复 | 支持（需缓存的 `session_id`） | 无 ID 则新会话 |
| 远程审批（Norma 代为批准/拒绝） | **不支持** | 只显示 WaitingApproval，用户在终端菜单选择 |
| 对话尾部预览（conversation tail） | 部分 | 只用 `Stop` 事件带的 `assistant_response` 截断文本；不读 `state.db` |
| 失败终态（重试耗尽、重试等待中中断） | 部分 | 见 §8 |
| 首次运行遥测页 | 用户操作 | 不代按 |

### 12.7 需要改动的文件清单

新增（以 CLI 命名，冲突最小）：
- `crates/norma/src/agent_hook/hermes.rs`：安装/卸载/已安装检查（对应 `opencode.rs`）。
- `crates/norma/src/agent_hook/hermes_plugin.py`（`include_str!`）与 `hermes_plugin.yaml`：插件源码与清单。
- `crates/norma/tests/` 下对应测试（事件解析、映射、受管文件保护、卸载）。

共享位置（保持最小，合并时需处理）：
- `crates/norma/src/agent_hook/mod.rs`：`HookSource`、`HookTarget` 增加 `Hermes`；`HookInstallPaths` 增加 `hermes_plugin_dir`；`source_from_str / target_from_str / source_name / profile_for_source` 各加一行；新常量 `HERMES_PLUGIN_*`。
- `crates/norma/src/agent_hook/event.rs`：`hook_events_for_source` 增加 `HookSource::Hermes` 分支。
- `crates/norma/src/agent_hook/install.rs`：安装/卸载/已安装的分发各加 `Hermes` 分支（仿 OpenCode 的特殊分支）。
- `crates/norma/src/orchestration/model.rs`：`ALL`、`hook_source()`、`hook_target()`、`skill_path()`、`presentation_skill_path()`、安装路径常量。
- `crates/norma/src/orchestration/installer.rs`：沿用技能安装即可，如需要拒绝覆盖非受管文件则加分支。
- `crates/runtime/src/agent_hooks.rs`：Hermes 的 hook 环境变量已有映射（第 76 行 `"hermes"`），仅补测试。
- `crates/runtime/src/session_snapshot.rs`：第 294 行 `exec hermes` 替换为带 `--resume` 的命令（有 ID 时）；会话发现保持 `None`。
- `crates/app/src/settings/hooks.rs`（及 `agent_hooks.rs`）：设置页 Hooks 行表增加 Hermes 一行及状态字段。
- `crates/app/src/settings/orchestration.rs`：已有 `Hermes` 分支，确认卡片文案。
- 图标 `crates/ui` 资源：占位 `toolbar-hermes.svg` 由合并方替换为 §11 的图标。

测试要求（下一步实现时执行）：先写失败测试再实现；覆盖插件源码包含标记与白名单字段（正负断言：不含 `conversation_history`）、受管文件覆盖保护、事件→状态映射、`--resume` 命令生成（含带引号 ID 的负例）、`PtyCommand` 默认不含 `--yolo`。

## 13. 已验证 / 未验证汇总

| 项 | 结论 | 依据 |
| --- | --- | --- |
| 版本 / 路径 / 安装方式 | 已验证 | `hermes --version`、`which hermes` |
| 子命令 `--help` | 已验证 | `ev/help-*.txt` |
| `zsh -lic "exec hermes"` 进入交互界面 | 已验证 | tmux 实测，单进程 |
| 未登录/未配置模型界面 | 已验证 | `ev/unauth-after-n.txt` |
| 真实账号登录流程 | **未验证** | 不使用真实凭据 |
| `-c` 续接最近一次 | 已验证，但工作区作用域不可靠 | `resume-no-session-in-workspace.txt` |
| `--resume <id>` 续接 | 已验证，续接后 ID 不变 | 插件事件 |
| `--resume latest` | **未验证**（仅依据 `--help`） | — |
| `--no-restore-cwd` 对工具 cwd 的实际效果 | **未验证** | 打印行与插件 cwd 不一致 |
| 会话 ID 获取（hook） | 已验证 | `session_id` |
| 会话存储位置/格式 | 已验证 | `state.db` schema |
| Shell hooks 机制与载荷 | 已验证 | `ev/shell-hook-payload-samples.json` |
| 目录插件机制与 `MULTISOUL_*` 环境转发 | 已验证 | `ev/norma-env-sink.jsonl` |
| 对用户真实 `config.yaml`（含注释）启用插件的影响 | **未验证** | 只在测试 home |
| 审批检测（deny） | 已验证 | `on_human_input_*` |
| 审批 allow 类 outcome 值、`sudo/clarify` kind | **未验证** | 仅测 deny / approval |
| 401 失败映射 | 已验证 | `api_request_error` |
| 重试耗尽后的最终失败信号 | **未验证** | 重试窗口数分钟 |
| 中断（模型调用中） | 已验证 | `interrupted=true` |
| 重试等待中中断的事件 | 已验证为“无事件” | 5 秒窗口 |
| 退出（/exit、Ctrl+D、SIGTERM）与进程清理 | 已验证 | `pane_dead`、`ps` |
| 后台工具进程的残留 | **未验证** | 未测 |
| 编排技能被发现 | 已验证（`skills list`） | `home2/skills` |
| 模型按技能调用 `norma` CLI | **未验证** | mock 模型 |
| 安装命令与链接 | 已验证（HTTP 200） | `curl` |
| 图标 URL | 已验证（HTTP 200）；品牌条款未验证 | MIT `LICENSE`，无商标政策 |

## 14. 测试环境副作用说明

- 调研初期误对默认 socket 执行了一次 `tmux kill-server`，之后全部改用独立 socket `-L hm`。该操作可能终止了同机其他任务在默认 tmux server 上的会话，已在最终报告中披露。
- 调研时发现 127.0.0.1:18765 已被其他进程占用（返回非本调研的 mock 内容），本调研改用 18991/18992，未触碰该进程。
