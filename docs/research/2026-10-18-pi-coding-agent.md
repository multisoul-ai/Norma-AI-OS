# Pi Coding Agent 接入调研与方案

日期：2026-10-18。工作分支：`feat/new-cli-cards/pi`。目标分支：`feat/new-cli-cards-base`。

## 结论与证据边界

**已验证**：本机官方 npm 包 `@earendil-works/pi-coding-agent@1.3.0` 可以在 zsh PTY 中交互运行，TypeScript 扩展可以实时取得 session ID、cwd、工具调用及结束事件，具备接入 Norma PTY 卡片的基础。建议新启动 `exec pi`，恢复 `exec pi --session-id '<UUID>'`，会话绑定只读 hook 缓存，禁止扫描会话目录。

**未验证，因此当前不支持宣称完成**：真实托管模型登录、真实外部模型任务、Norma App 的 Pi 卡片、Pi 到其他 Agent 的编排、PR。原因分别是没有 Pi 可用模型凭据、尚未收到“基线骨架已提交”及基线已推送通知。第二步不得提前改枚举或其他 CLI 分支。

本轮没有把其他智能体作为依赖。研究中的模型响应由本机 `127.0.0.1` OpenAI SSE 协议夹具提供；运行的是已安装的真实 Pi，文件写入、shell 执行、会话文件和扩展回调全部来自真实 CLI。夹具结果只证明 Pi 的协议与生命周期，不能代替真实模型质量、OAuth 或完整 Norma 产品验收。

证据目录：[assets/2026-10-18-pi/](assets/2026-10-18-pi/)。临时根目录为 `/var/folders/pf/k4j7_s8d0yv6z3jbtc7sfv2c0000gn/T/norma-pi-probe-7f1l_gyq`，下文用 `$PROBE` 表示；这是说明性变量，没有设置 HOME。所有 Pi 会话测试都在这个新建临时目录的空工作目录开始，未在仓库中运行 Pi。

## 1. 安装、版本与二进制

**已验证**，执行：

```bash
/bin/zsh -lic 'command -v node; node --version; command -v npm; npm --version'
env PATH=/opt/homebrew/opt/node@22/bin:$PATH npm view @earendil-works/pi-coding-agent version engines bin repository license --json
env PATH=/opt/homebrew/opt/node@22/bin:$PATH npm install --global --prefix "$HOME/.local" --ignore-scripts @earendil-works/pi-coding-agent
/bin/zsh -lic 'command -v pi; pi --version'
```

关键输出：

```text
/opt/homebrew/opt/node@22/bin/node
v22.23.3
10.9.9
version: 1.3.0
engines.node: >=22.19.0
bin.pi: dist/bundle/cli.js
repository: https://github.com/earendil-works/pi.git (packages/coding-agent)
license: MIT
added 118 packages in 43s
/Users/openclawd/.local/bin/pi
1.3.0
```

**已验证**：本次不用 sudo；包位于 `~/.local/lib/node_modules/@earendil-works/pi-coding-agent`，入口位于 `~/.local/bin/pi`。安装时保留 `--ignore-scripts`。当前 zsh 原本能找到 `~/.local/bin`，本任务没有执行写入 shell 配置的命令。

**已验证的环境问题**：普通工具 shell 的默认 `/opt/homebrew/bin/node` 会报 `Library not loaded: ... libsimdjson.30.dylib`，而交互式 zsh 选择 Node 22。探测时显式给 npm/Node 使用 Node 22，正式启动沿用登录交互 shell。

**未验证**：`curl -fsSL https://pi.dev/install.sh | sh` 的实际安装副作用。本轮仅下载并查看该官方脚本，未执行，不能宣称已验证该安装路径。

**已验证的配置变化核对**：[install-cleanup.json](assets/2026-10-18-pi/install-cleanup.json) 记录前后 SHA-256。同期 `.zshrc` 哈希从 `444d803c…ddc` 变为 `b5337563…722`，来源未验证；其余被检查的 shell 文件仍不存在。不能据此声称所有 shell 配置未变，也没有回滚用户文件。本任务没有执行 installer 或修改 shell 配置。

Pi 首次实际启动后新增 `~/.pi/agent/{settings.json,models-store.json,auth.json,bin/fd}`；临时扩展已删除。`fd` 是 Pi 首屏实际提示下载到 `~/.pi/agent/bin/fd` 的辅助程序。未读取或提交凭据内容。

## 2. 完整帮助与子命令

**已验证**：`pi --help` 及全部列出的子命令帮助保存在 [cli-help.md](assets/2026-10-18-pi/cli-help.md)，是完整输出，无省略。

执行的顶层子命令：`install/remove/uninstall/update/list/config/auth/mcp --help`。还执行了 `auth print-api-key/print-bearer-token/check --help` 与 `mcp add/remove/list/login/logout --help`；嵌套命令返回各自命令族的同一完整帮助，退出码均为 0。没有调用凭据打印命令本身。

本机输出明确列出：

```text
--continue, -c              Continue previous session
--resume, -r                Select a session to resume
--session <path|id>         Use specific session file or partial UUID
--session-id <id>           Use exact project session ID, creating it if missing
--approve, -a               Trust project-local files for this run
--no-approve, -na            Ignore project-local files for this run
PI_CODING_AGENT_DIR         Config directory (default: ~/.pi/agent)
PI_CODING_AGENT_SESSION_DIR Session storage directory (overridden by --session-dir)
```

## 3. zsh PTY、首次启动与未登录行为

**已验证**：在 `$PROBE/work` 执行 `/bin/zsh -lic 'exec pi'`，真正进入 24 行 PTY 的 TUI，显示 `v1.3.0`、输入区、cwd、`unknown` 模型和以下提示：

```text
Warning: No models available. Use /login to log into a provider via OAuth or API key.
```

输入 `hello-pi-probe` 后：

```text
Error: No API key found for the selected model.
Use /login to log into a provider via OAuth or API key.
```

输入 `/login` 后出现 `Select authentication method:`，选项为 `Sign in with an account`、`Sign in with an API key`、`Sign in with Radius • not configured`。没有自动打开登录浏览器；本轮在选择前取消。

再次用 PTY 记录器运行相同行为（仅增加 offline 和关闭技能/上下文发现以减少噪声），证据：[unauth.txt](assets/2026-10-18-pi/unauth.txt)、[unauth.meta.json](assets/2026-10-18-pi/unauth.meta.json)。无凭据情况下依然触发 `session_start`，但提交失败没有 `before_agent_start/agent_start/agent_end`，因此不能仅凭用户输入声称模型正在思考。

**已验证**：执行 `pi auth check --provider anthropic --json --no-refresh`，退出 1，输出 [auth-check.txt](assets/2026-10-18-pi/auth-check.txt)：

```json
{"status":"not_ready","provider":"anthropic","reason":"credentials_not_configured"}
```

**未验证、不支持自动登录**：OAuth 完成、有效 API key 外部请求。降级：保留 CLI 自己的 `/login` 和错误界面；Norma 不代填、不读取其他工具的凭据。

## 4. 会话创建、退出与恢复

**已验证**：使用 `$PROBE/agent/models.json` 配置本机协议夹具；`PI_CODING_AGENT_DIR=$PROBE/agent`，会话工作目录 `$PROBE/model-work` 最初为空。真实命令完整 argv、stdout、stderr 和退出码见 [command-results.json](assets/2026-10-18-pi/command-results.json)。通用探测前缀为：

```bash
pi --offline --no-skills --no-context-files --no-prompt-templates \
  -e ~/.pi/agent/extensions/norma-probe-7f1l_gyq.ts --model norma-probe/fixture
```

这是探测隔离参数，不是 Norma 默认参数。未使用 `--approve` 或任何绕过审批参数。第一次 `-p 'NORMAPI_TOOLS remember norma-pi-token-20261018'` 后，Pi 真实执行 write/bash/read，落盘 `probe.txt`，输出 `PI_FIXTURE_DONE prior_tool_history=true` 并退出 0。

首次 session ID：`01a14cb5-4b48-71eb-af74-53296bd903ff`。以下命令依次运行，最后都添加 `-p 'NORMAPI_RESUME prove prior history'`：

| 参数 | 实测结果（已验证） |
|---|---|
| `-c` | 退出 0；hook ID 不变；下一次请求含原工具历史 |
| `--session 01a14cb5-4b48-71eb-af74-53296bd903ff` | 同上 |
| `--session <hook 给出的完整 JSONL 路径>` | 同上 |
| `--session-id 01a14cb5-4b48-71eb-af74-53296bd903ff` | 同上 |
| `--session 00000000-0000-0000-0000-000000000000` | 退出 1；`No session found matching '…'` |
| `--session-id 11111111-1111-4111-8111-111111111111` | 退出 0；提示未找到项目会话、以该确切 ID 新建；没有原历史 |

[verification.json](assets/2026-10-18-pi/verification.json) 的断言核对了前五次 `session_start` ID 全相同，以及磁盘工具输出、工具失败、Provider 失败和取消事件。

**已验证**：`-r` 打开 `Resume Session (Current Folder)`；输入目标 ID 再 Enter 可以进入原会话。`-r <ID>` 仍先打开选择器，Esc 后退出 0 并显示 `No session selected`，不是直接按 ID 恢复。证据：[resume-selector.txt](assets/2026-10-18-pi/resume-selector.txt)、[resume-id-positional.txt](assets/2026-10-18-pi/resume-id-positional.txt)。Norma 自动恢复不使用 `-r`。

**已验证**：在真正 `/bin/zsh -lic 'exec pi ... --session <ID>'` 的 TUI 中恢复并再次提交，显示旧回复，夹具证实 `prior_tool_history=true`，退出时打印：

```text
To resume this session: pi --session 01a14cb5-4b48-71eb-af74-53296bd903ff
```

证据：[zsh-resume.meta.json](assets/2026-10-18-pi/zsh-resume.meta.json)、[zsh-resume.txt](assets/2026-10-18-pi/zsh-resume.txt)。

**已验证的空会话边界**：仅启动后 `/quit`，hook 已给出 ID，但对应 JSONL 不存在。接着 `--session-id <同一 ID>` 能进入 TUI，提交后创建文件并保持同一 ID。证据：[empty-session-verification.json](assets/2026-10-18-pi/empty-session-verification.json)。所以方案选 `--session-id`：已有文件恢复，空会话同 ID 新建。若用户手动删除过历史，也会按该 ID 新建空会话，不能宣称历史仍在。

**未验证、不支持承诺**：仅用短 UUID、跨 cwd 自动查找、自定义 session-dir 改动后仍能恢复。Norma 只使用 hook 给出的完整 ID 和保存的启动 cwd；不猜最近目录或替换为其他会话。

## 5. 存储格式与无需扫描的当前 ID

**已验证**：默认无凭据启动时，扩展 context 给出：

```text
cwd=/private/var/folders/…/norma-pi-probe-7f1l_gyq/work
sessionId=01a14cb3-5411-7655-a4bf-9e266417ec61
sessionFile=/Users/openclawd/.pi/agent/sessions/--private-var-folders-…-work--/2026-10-18T01-49-43-314Z_01a14cb3-5411-7655-a4bf-9e266417ec61.jsonl
```

完整未改写 context 见 [unauth-default-events.jsonl](assets/2026-10-18-pi/unauth-default-events.jsonl)。`PI_CODING_AGENT_DIR` 改到临时目录后，文件也落在该目录的 `sessions/<编码 cwd>/` 下。

**已验证**：实际文件是 JSONL v3，不是 SQLite。首行如下，后续为带 `id/parentId` 的 message/model/thinking 等记录；原文件见 [session-file.jsonl](assets/2026-10-18-pi/session-file.jsonl)。

```json
{"type":"session","version":3,"id":"01a14cb5-4b48-71eb-af74-53296bd903ff","timestamp":"2026-10-18T01:51:52.138Z","cwd":"/private/var/folders/pf/k4j7_s8d0yv6z3jbtc7sfv2c0000gn/T/norma-pi-probe-7f1l_gyq/model-work"}
```

**已验证**：回调 `ctx.sessionManager.getSessionId()`、`getSessionFile()`、`ctx.cwd` 在 `session_start` 时已可用，不需要扫描、读取会话文件或查库。注意 `event` 原始对象不自带这些字段，必须由扩展包装。

方案：扩展转发 `session_id/cwd` → Norma 已有 hook ingress → `AgentSessionBindings` 缓存 → checkpoint。Pi 的 `find_agent_session_id_for_profile` 分支返回 `None`，不新增扫描器；PTY drain、渲染和关闭路径只用已有缓存。

**未验证、不支持**：从 Pi session-ID 环境变量直接获取当前会话。没有使用终端标题、退出文本或“最近文件”作为绑定来源。未安装扩展或被禁用时，降级为无可靠会话绑定，新开 Pi，不自动用 `-c` 猜会话。

## 6. 扩展机制与完整真实载荷

**已验证**：在 `~/.pi/agent/extensions/norma-probe-7f1l_gyq.ts` 写入最小 TypeScript 扩展，默认 `pi` 首屏列出扩展名称并实际执行。扩展本体作为非执行文本留档：[probe-extension.ts.txt](assets/2026-10-18-pi/probe-extension.ts.txt)。核心为：

```typescript
import { appendFileSync } from "node:fs";
export default function (pi) {
  for (const name of ["session_start", "agent_start", "agent_end", "tool_call", "tool_result", "turn_end"]) {
    pi.on(name, (event, ctx) => {
      appendFileSync(process.env.NORMA_PI_PROBE_LOG, JSON.stringify({
        event,
        context: {
          cwd: ctx.cwd,
          sessionId: ctx.sessionManager.getSessionId(),
          sessionFile: ctx.sessionManager.getSessionFile(),
          hasUI: ctx.hasUI
        }
      }) + "\n");
    });
  }
}
```

实际探针还订阅了输入、开始规划、turn_start、settled、shutdown 和 UI prompt，以区分最终状态。全量选中事件载荷保存于 [payload-samples.json](assets/2026-10-18-pi/payload-samples.json)：每个 `event/context` 原对象保留完整字段，包含六类指定事件、工具成功/失败、完整 agent_end、turn_end 成功/失败/工具回合、完成/取消、UI prompt。没有把夹具手写响应冒充 hook JSON。

**已验证的关键载荷**：

```json
{"type":"session_start","reason":"startup"}
{"type":"agent_start"}
{"type":"tool_call","toolCallId":"call_normapi_1","toolName":"bash","input":{"command":"printf norma-pi-shell-ok"}}
{"type":"agent_settled","aborted":false}
```

`tool_result` 有同一 `toolCallId/toolName`、`input/content/details/isError`；真实缺失文件 `read` 返回 `isError:true`。`turn_end` 有 `turnIndex/message/toolResults/outcome/messageEntryId/toolResultEntryIds` 和完整 boundary context。`agent_end.messages` 包含本次消息，最后 assistant 的 `stopReason/errorMessage` 可区分失败。`agent_settled` 仅有 `aborted`，失败也可能为 false，不能独立据此判断成功。

**已验证（实际触发）事件名**：`session_start`、`session_shutdown`、`input`、`before_agent_start`、`agent_start`、`agent_end`、`agent_settled`、`turn_start`、`turn_end`、`tool_call`、`tool_result`、`ui_prompt_start`、`ui_prompt_end`。

**仅声明已核对，运行语义未验证**：安装包 `ExtensionAPI.on` 列出 41 个事件，完整清单见 [declared-events.json](assets/2026-10-18-pi/declared-events.json)。其余事件没有触发样例，当前方案不依赖它们，也不将类型声明算作运行验证。

**安装/卸载方案（未实施）**：只管理一个独立文件 `~/.pi/agent/extensions/norma-agent-hook.ts`，加 Norma 专有 marker。遇到无 marker 的同名文件拒绝覆盖；升级只原子替换己方文件；卸载只删除带 marker 的己方文件，不改 settings、不清整个目录。尊重可检测到的 `PI_CODING_AGENT_DIR`。自定义目录在 shell 内另行改写、却未传入安装器的情形不支持自动猜测，提示用户检查路径。

**已验证的探针清理**：测试结束按精确文件名删除，并先核对内容与本轮副本一致；空 extensions 目录也已移除。`~/.pi` 内没有遗留测试扩展、模型夹具或确认扩展；模型/确认夹具仅在临时根目录。证据：[install-cleanup.json](assets/2026-10-18-pi/install-cleanup.json)。生产扩展的所有权保护、重复安装与冲突处理尚未实施，必须第二步 TDD 验证。

## 7. 权限、绕过参数与等待用户输入

**已验证**：无额外权限参数的真实 Pi 向本地模型请求暴露 `read/bash/edit/write` 四个默认工具。模型发出的 write、bash 和 read 都直接执行；print 和 TUI 均没有逐次批准确认。生成 `probe.txt` 的内容和 shell 输出验证了执行，不是只看文档。

**已验证**：`--approve/-a` 的 help 意思是信任“项目本地文件”，不是逐个工具免审批模式；`--no-approve/-na` 是忽略项目资源。常规探测与默认启动都不带它们。没有从本机帮助中发现一个通用工具审批绕过参数；不虚构 `--yolo` 或同类开关。

**已验证**：在新的 `$PROBE/trust-work` 添加 `.pi/settings.json` 后，默认 TUI 出现项目信任交互；Esc 后提示 `This project is not trusted. Project .pi resources and packages are ignored.`，随后可正常进入 CLI。证据：[project-trust.txt](assets/2026-10-18-pi/project-trust.txt)。

**已验证**：独立临时确认扩展在 `tool_call` 中调用 `ctx.ui.confirm`。等待时事件为：

```json
{"type":"ui_prompt_start","reason":"ui_prompt","kind":"confirm","title":"Norma probe permission"}
{"type":"ui_prompt_end","reason":"ui_prompt","kind":"confirm","title":"Norma probe permission"}
```

证据：[confirm-wait.txt](assets/2026-10-18-pi/confirm-wait.txt) 和完整 payload samples。取消后 bash 被扩展阻止，Pi 继续处理工具结果。

**未验证、不支持**：为任意第三方扩展、项目启动信任或所有自定义界面可靠判定“权限审批”。`ui_prompt_*` 仅表示扩展等待输入。方案不将其一概映射为 Norma 的 PermissionRequest，也不启用手机远程审批；终端内由用户处理，状态保持当前 Thinking/RunningTool，避免假造审批能力。

## 8. Ready / Thinking / RunningTool / Done / Failed 映射

以下源事件和字段均有本机真实载荷；右侧是**第二步待实施**的 Norma 归一化策略。

| Pi 事件与条件 | Norma 事件 / 状态 | 边界与降级 |
|---|---|---|
| `session_start` | SessionStart / Ready | context 补 session ID/cwd；无模型时也会触发，Ready 只代表终端可交互 |
| `before_agent_start` 的真实 user prompt | UserPromptSubmit / Thinking | 缓存本次 run ID、清上次错误；不要靠 input 事件提前认定已提交 |
| `agent_start` | Thinking 的补充证据 | 不重复生成 UserPromptSubmit，不重复记录用户消息 |
| `tool_call` | PreToolUse / RunningTool | 带真实 toolName/toolCallId；工具可并行，不推断串行顺序 |
| `tool_result isError=false` | PostToolUse / Thinking | 一个工具结束不代表任务结束 |
| `tool_result isError=true` | PostToolUseFailure / Thinking | 实测 read 失败后模型仍正常回复，不直接把整项任务判 Failed |
| `turn_end` | 缓存 outcome/stopReason/errorMessage | 工具回合 stopReason=toolUse 且 outcome=completed，不能 Done |
| `agent_end` | 缓存最后 assistant 文本与错误 | 先不发布 Stop，等待 settled；保留真实结果用于编排回执 |
| `agent_settled` 且末次无错误、非 aborted | Stop / Done | 发送 `last_assistant_message`、本轮 turn_id；只发送文本，不能用工具输出当最终回复 |
| `agent_settled` 且缓存 error 或 aborted=true | StopFailure / Failed | aborted 标明用户取消；不伪造成功结果 |
| `session_shutdown reason=quit` | SessionEnd / Ended | 不作为任务成功；保留已知真实 terminal status |（仅 quit 发送；源码 SessionShutdownEvent.reason 另有 reload/new/resume/fork，进程仍存活，扩展忽略）

**已验证**：本地夹具返回 HTTP 400 时 Pi 退出 1，`turn_end.outcome=error`、assistant `stopReason=error`，`errorMessage` 包含 `norma-pi-fixture-error`；`agent_settled.aborted=false`。因此“settled 且非 aborted ⇒ Done”会误报，必须合并缓存错误。

**已验证**：真实 bash `sleep 20` 在 Escape 后被取消，后续 settled `aborted=true`。无凭据在真正开始之前失败时没有终结 hook；该情况当前不支持可靠 Failed 灯，降级为 Ready + 保留 CLI 错误文本，不从 ANSI 猜错误。

## 9. 退出、清理与进程残留

**已验证**：PTY 操作记录同时保存发键时间、退出码和是否超时强杀，见各 `.meta.json`；以下退出均为自然退出 0，没有记录器强杀：

| 操作 | 实测 |
|---|---|
| `/quit` | 退出；有持久化会话时打印 `To resume this session: pi --session <ID>` |
| `/exit` | 没有退出；无凭据时报 No API key，有本地模型时成为模型输入；所以不支持作为退出命令 |
| 空输入 `Ctrl+D` | 退出 0 |
| 空输入快速两次 `Ctrl+C`（间隔约 0.16 秒） | 退出 0；单次不保证退出 |
| bash 运行中快速两次 `Ctrl+C` | 约 5.26 秒测试结束，退出 0 |
| bash 运行中 Escape，再 `Ctrl+D` | 显示 `Command aborted` 和 `This operation was aborted`，退出 0 |

证据：[ctrl-c.meta.json](assets/2026-10-18-pi/ctrl-c.meta.json)、[ctrl-d.meta.json](assets/2026-10-18-pi/ctrl-d.meta.json)、[active-ctrl-c.meta.json](assets/2026-10-18-pi/active-ctrl-c.meta.json)、[sleep-abort.txt](assets/2026-10-18-pi/sleep-abort.txt)。

**已验证的清理结果**：[process-cleanup.json](assets/2026-10-18-pi/process-cleanup.json) 进一步使用 `sleep 120`：在测试第 2.24 秒确认真实子进程 PID 33273（父 PID 33115）已运行，双 Ctrl+C 退出后第 8.37 秒立即检查已无该子进程，明显早于其自然结束；断言全部通过。早先按固定时间输入的加强用例没有实际启动工具，被“退出前必须看到子进程”的正断言拦截；改用 CLI 初始 prompt 后才得到上述证据，未把空进程表误算为清理成功。所有本轮创建的 Pi PTY 进程已 wait，临时本地 HTTP 服务已关闭。**未验证** 任意扩展长期进程或所有第三方工具的退出清理，Norma 仍复用现有 PTY kill/wait 生命周期，不能仅依赖 Pi 的 `/quit`。

## 10. 未安装 CLI 的提示与图标来源

**已验证的安装命令**：

```bash
npm install --global --prefix "$HOME/.local" --ignore-scripts @earendil-works/pi-coding-agent
```

要求 Node 22.19+；需要确保登录 shell 的 PATH 可找到 `~/.local/bin`。官方主页：<https://pi.dev/>。官方 npm 包：<https://www.npmjs.com/package/@earendil-works/pi-coding-agent>。官方另提供 `curl -fsSL https://pi.dev/install.sh | sh`，该方式本轮未执行。

**未实施**：卡片在 CLI 不可用时持续显示以上命令、Node 要求和官方链接；不能只留下闪过的 `zsh: command not found`。等待骨架后的 Pi 分支中接入缺失检查与说明，按实际 App 结果验收。

**已核对官方图标来源，像素转换未实施**：官方仓库 `packages/coding-agent/src/modes/interactive/components/pi-logo.ts`：
<https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/modes/interactive/components/pi-logo.ts>。本地官方 CLI 首屏也实际渲染此 4×4 像素标识。其官方坐标与颜色代码已留档 [pi-logo.ts](assets/2026-10-18-pi/pi-logo.ts)，不是第三方重绘。

```text
coral coral coral .
blue  .     coral .
blue  blue  .     yellow
blue  .     .     yellow
```

使用条款来源：官方仓库根目录 <https://github.com/earendil-works/pi/blob/main/LICENSE>，本次下载的 [official-LICENSE](assets/2026-10-18-pi/official-LICENSE) 是 MIT，版权 `Copyright (c) 2025 Mario Zechner`，允许修改与分发并要求保留版权和许可文本。该核对仅说明本次下载的官方代码条款；不存在对未取得资产的授权推断。

官网下载的 HTML 引用 `/logo-auto.svg`，但本次抓该 SVG 返回 403，未使用。计划从已取得并带 MIT 的官方 TypeScript 图标像素坐标生成 `toolbar-pi.svg`，用整数 `rect` 和 `currentColor` 适配现有工具栏。脚本 `scripts/generate-pi-icon.py` 的输入和输出路径都由参数传入，并保留源 URL/许可。若后续发现独立限制覆盖该来源，则用 P 字母占位并标注。16/24 px 深浅色真实 App 验收留到第二步。

## 11. 本 CLI 接入方案（基线骨架已合入）

### 启动、恢复与安装

- 新建：`/bin/zsh -lic "exec pi"`。不加 `--approve`、工具白名单变更、模型强制覆盖或跳过审批参数。
- 最近会话手动续接：`pi -c`（已验证）；Norma 自动恢复不使用它，避免串卡。
- 按缓存 ID 自动恢复：`/bin/zsh -lic "exec pi --session-id '<完整 UUID>'"`，复用 `shell_quote`，cwd 使用 checkpoint 保存值。该参数恢复已有记录、空 ID 文件缺失则同 ID 创建（均已验证）；不扫描会话目录。
- 扩展：管理 `~/.pi/agent/extensions/norma-agent-hook.ts`，支持可检测的 `PI_CODING_AGENT_DIR`；只写自身 marker 文件，冲突拒绝覆盖，卸载不碰其他扩展。
- 传输：参照 OpenCode 插件，以继承的 `MULTISOUL_NODE_ID/HOOK_ENDPOINT/HOOK_TOKEN` 转发到本机 Norma ingress，profile 精确为 Pi；缺少环境变量就不发，网络失败 fail-open，短超时并保持事件顺序。生产转发尚未验证，不能声称已接通。
- 状态：采用第 8 节映射；缓存完整 session ID、本轮 ID、末次 assistant 和错误，不从会话存储读取当前 ID/回复。
- 编排：复用现有 provider authorization、Workspace route 和 ask-norma 技能。**已验证** 把最小技能放 `$PI_CODING_AGENT_DIR/skills/norma-probe/SKILL.md`，Pi 首屏实际列出 `norma-probe`（[skill-discovery.txt](assets/2026-10-18-pi/skill-discovery.txt)）。据此采用 Pi 专属 `~/.pi/agent/skills/<managed-skill>/SKILL.md`；**未验证** 真正向其他 Agent 委派与可信结果回收，未通过前保持不支持该能力并展示集成未就绪。

### 文件范围（对照 Grok / OpenCode 匹配点）

本轮运行 `rg -l 'AgentProfile::(Grok|OpenCode)' crates`，完整路径列表存于 [profile-matches.txt](assets/2026-10-18-pi/profile-matches.txt)。第二步先 merge `feat/new-cli-cards-base`，只填骨架提供的 Pi 分支，不改枚举定义、不修改其他 CLI 行。

| 范围 | 文件/职责 | Pi 验收证据计划 |
|---|---|---|
| runtime 命令 | `crates/runtime/src/nodes/agent.rs`、`session_snapshot.rs`、`agent_hooks.rs`、必要的 `command.rs` Pi 占位 | TDD：exec pi、--session-id、shell quoting、cwd、profile env、无绕过参数；会话发现始终 None |
| hooks | `crates/norma/src/agent_hook/{mod.rs,event.rs,install.rs}` 的 Pi 分支；新增 `pi.rs` 与模板文件 | TDD：真实 payload 解析、成功/失败/取消、工具失败不等于任务失败；marker 安装/升级/卸载/冲突；不串 session |
| 对话证据 | `agent_hook/conversation_observe.rs` Pi 分支、`crates/agent/src/conversation_tail.rs` / app `conversation_tails.rs` Pi 占位 | TDD：真实 prompt、最终 assistant 文本、无回复时 unavailable、toolUse 回合不提前 Done |
| 编排 | `crates/norma/src/orchestration/{model.rs,installer.rs,installer/skill_component.rs}` Pi 分支 | TDD：Pi 专属 skill 路径、hook 就绪判断、授权隔离、Workspace route；真实 App 由 Pi 发任务并查 exact task result |
| app 映射/提示 | `app/agent_runtime/{identity.rs,coordination.rs,command_protocol.rs,command_support.rs}`、`settings/{hooks.rs,agent_hooks.rs}`、`settings/agents/*`、`norma/server/*` Pi 占位 | 对照骨架确认已覆盖，缺失安装提示持续可见；不改其他 profile |
| 图标 | `crates/app/assets/icons/toolbar-pi.svg`、Pi 的 toolbar/initialization 分支、`scripts/generate-pi-icon.py` | 整数 rect；来源/许可；真实 App 16/24 px 深浅色 |
| 骨架所有 | canvas 枚举/metadata、storage 编码、protocol/CLI 枚举、agent schema、菜单/Dock/快捷键、iOS 解码 | 只核对骨架的 Pi 行和测试，不重复扩展枚举；若缺失向基线负责人报证据 |
| 测试 | `crates/norma/tests/agent_hook_pi_tests.rs`；runtime/agent 相应 `tests/unit/*pi*`；必要 desktop-e2e Pi 场景 | 注释写场景/数据/过程/预期；正负断言带说明；先红后绿 |

页面/图标改动实施前加载 apple-design 和相关 gpui skill，沿用现有组件、键盘与 hover/focus/pressed/disabled 反馈；菜单遵循仓库 Material 规范。真实 App 用 desktop-e2e 验收，覆盖不到再用 Computer Use，单测不能代替。

实施后所有 cargo 命令使用：`CARGO_TARGET_DIR=$HOME/.cache/norma-new-cli-target-pi`。先运行受影响 crate 的范围测试，再 `cargo check -p app`，以及 `check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh`。没有必要不跑全 workspace；提交不跳过 hooks。基线已推送通知前不推送，不创建伪 PR 链接。

### 已验证 / 未验证汇总

| 能力 | 状态 | 命令 / 证据 / 降级 |
|---|---|---|
| 官方 npm 安装、版本、路径、Node 22 | 已验证 | §1；Pi 1.3.0，Node 22.23.3 |
| curl installer 实际安装 | 未验证 / 不支持承诺副作用 | 仅下载未执行；使用已验证 npm 路径 |
| 全部命令帮助 | 已验证 | cli-help.md，顶层及嵌套帮助均退出 0 |
| zsh PTY 首屏、未登录 /login | 已验证 | unauth 记录；无模型保留终端错误 |
| 真正托管模型登录、任务 | 未验证 / 不支持已完成声明 | 缺可用凭据；本地夹具不冒充外部服务 |
| -c、--session ID/path、--session-id | 已验证 | 五次相同 hook ID；保留历史 |
| -r 直接按 ID 恢复 | 不支持（已实测） | 始终先选择器；使用 --session-id |
| 空会话同 ID 重启 | 已验证 | empty-session-verification.json |
| 当前 ID 无扫描取得 | 已验证 | ctx.sessionManager + cwd；无 hook 则新开，不猜最近 |
| JSONL 会话格式与默认路径 | 已验证 | v3 header；default context |
| 六类扩展事件完整载荷 | 已验证（真实 CLI + 本地夹具） | payload-samples.json；sessionId/cwd 来自 context |
| 41 个声明事件全部运行语义 | 未验证 / 不支持依赖未触发事件 | declared-events.json 仅声明核对 |
| 生产 hook 安装/卸载、HTTP 转发 | 未实施 / 不支持宣称接通 | 第二步 TDD；测试扩展已从 ~/.pi 删除 |
| 默认无逐次工具审批 | 已验证 | write/bash 真正执行；没有添加绕过参数 |
| 扩展 UI 等待事件 | 已验证 | ui_prompt_start/end，kind=confirm |
| 任意审批识别 / 手机远程审批 | 未验证 / 不支持 | 留在终端处理，不伪造 PermissionRequest |
| 五状态源数据 | 已验证 | error+settled、toolUse、tool failure、abort 均有样例 |
| 五状态 App 映射 | 未实施 / 不支持宣称通过 | 等骨架并补范围测试及真实 App |
| /quit、Ctrl+D、快速双 Ctrl+C | 已验证 | PTY exit=0；抽样未见 sleep 子进程 |
| /exit 退出 | 不支持（已实测） | 当普通模型输入；使用 /quit |
| 官方像素来源 / MIT | 已核对；转换与 App 视觉未验证 | pi-logo.ts + official-LICENSE；未取到的官网 SVG 不使用 |
| Pi 专属 skills 发现 | 已验证 | skill-discovery 首屏列出 norma-probe |
| 跨 Agent 编排 | 未验证 / 不支持已接通声明 | 待 Pi provider 骨架、授权和真实 App 验证 |
| PR | 未创建 | 等骨架与基线已推送通知，目标 feat/new-cli-cards-base |

## 复核记录与阶段状态（2026-10-18 第二次复核）

**已复核（本机重跑）**：`pi --version` 为 1.3.0，路径 `/Users/openclawd/.local/bin/pi`；`/bin/zsh -lic 'pi --help'` 仍列出 `--continue/-c`、`--resume/-r`、`--session <path|id>`、`--session-id <id>`（“Use exact project session ID, creating it if missing”）、`--approve/-a`、`--no-approve/-na`；`~/.pi/agent/` 下只有 `auth.json(2 字节)/bin/models-store.json/sessions/settings.json`，没有 `extensions/` 目录，测试扩展已清理。

**与初稿的差异（已验证）**：此次执行 `pi auth check --provider anthropic --json --no-refresh` 返回 `{"status":"ready","provider":"anthropic","authType":"api_key"}`，与初稿的 `not_ready` 不同。原因：本会话进程环境继承了 `ANTHROPIC_AUTH_TOKEN/ANTHROPIC_BASE_URL`（仅核对变量名，未读取值），而 `auth.json` 只有 2 字节。即“是否已登录”取决于继承的环境变量，不是 Pi 自己存储的凭据。初稿“未登录界面”结论是在无这些变量的环境得到的，不变；本次未用该环境发起真实模型请求，真实模型任务仍为未验证。

**基线骨架现状**（`rg 'AgentProfile::Pi\b' crates`）：枚举与菜单/Dock/设置/图标占位/storage/protocol 均已有 Pi 行；占位点为 `runtime/nodes/agent.rs:50`（`exec pi`）、`runtime/session_snapshot.rs:144`（会话发现 None）与 `:293`（恢复退化为普通启动）、`runtime/agent_hooks.rs:75`、`norma/orchestration/model.rs:65`（`Pi` 未进入 `ALL`）、`norma/agent_hook/` 下尚无 Pi 的 install/event 分支。第二步直接在这些位置替换，不再改枚举。

**阶段状态**：第一步（探测与方案）完成并提交；不做 Rust 改动，不推送，不创建 PR。
