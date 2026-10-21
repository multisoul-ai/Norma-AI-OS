# Agent 文件路径输出约定：各 Agent 的注入通道调研

目标：让 Agent 回复里的文件路径统一写成「反引号包裹、仅含路径（可带 `:行[:列]`）」，由 Norma 稳定识别并在右抽屉打开。约定文案见 `crates/norma/src/agent_hook/path_convention.rs` 的 `PATH_CONVENTION_PROMPT`。

方法：用唯一暗号（或约定文案）通过各 Agent 自己的扩展点注入，再让真实模型复述；对照组不注入。探针均在临时目录/临时 HOME 中进行，不改用户配置。

| Agent | 通道 | 结论 | 证据 |
| --- | --- | --- | --- |
| Claude Code 2.x | `SessionStart` hook stdout 的 `hookSpecificOutput.additionalContext` | 已验证：模型能复述暗号；对照组答 NONE。真实 `multisoul-agent-hook` 二进制在托管会话输出，非托管输出 0 字节 | `claude -p --settings` |
| Codex 0.162.1 | 同上（`hooks.json` 的 `SessionStart`） | 已验证：复述暗号。hook 需信任哈希（探针用 `--dangerously-bypass-hook-trust`，仅限临时 CODEX_HOME）；Norma 已安装的 hook 沿用用户已有信任，注入内容来自同一命令，无需新信任 | `codex exec` |
| OpenCode 1.19.3 | 插件 `experimental.chat.system.transform`，向 `output.system` push（修改而非返回） | 已验证：用真实安装器生成的插件，托管会话复述约定首句，非托管答 NONE。免费模型偶发超时，重试通过 | `opencode run -m opencode/big-pickle` |
| Kimi Code 2.2.0 | **`SessionStart` stdout 不入上下文**（官方文档称其仅观察、探针确认）；`UserPromptSubmit` stdout 会追加到上下文 | 已验证（本地 mock OpenAI 服务记录完整请求）：仅首条提示词注入一次，历史里保留，第二轮不重复。用 `$TMPDIR/norma-path-convention/<session_id>` 标记保证只注入一次 | `kimi -p` + `mock_openai.py` |
| Cursor Agent | `sessionStart` 输出顶层 `additional_context` | **未验证**：本机连不上 Cursor 云端（`Connection lost`）。论坛有多篇报告称 `sessionStart` 的 `additional_context` 存在竞态/不到达模型的问题。已按文档实现，输出无副作用；是否生效待可联网环境验证 | 论坛报告（二手） |
| Pi | 扩展 `before_agent_start` 返回 `{systemPrompt}`（类型定义确认） | **未真机验证**：本机 Pi 无可用模型。已在扩展里实现（仅托管会话、fail-open），仅按 `.d.ts` 类型对照 | `types.d.ts` |
| ZCode | Claude 风格 hook（`SessionStart` 等 7 个事件） | **未实现**：本机未安装 ZCode，调研文档与公开资料都没有说明 hook stdout 是否进入上下文 | 无 |
| Hermes / Grok | 未评估 | 未实现 | — |

## 取舍

- Claude 在对照组里本来就用反引号写路径（含空格路径），约定对它是加固；收益主要在 Codex / OpenCode / Kimi 等更随意的模型。
- 约定只对 Norma 托管会话生效（有 `MULTISOUL_NODE_ID` 等环境变量）。
- 解析端同步支持：反引号路径含空格、被终端软折行拆成多行（`terminal/src/grid/path_backtick.rs`），点击时优先于折行启发式候选。
- 开关在 Settings → 编排 顶部（键 `orchestration.path_convention.v1`，默认开）。关闭后 Norma 以 `NORMA_PATH_CONVENTION=0` 启动之后的 Agent，hook / OpenCode 插件 / Pi 扩展据此不注入；已运行的 Agent 不受影响。

## 已知边界

- **Claude Code 交互式 TUI 渲染行内代码时会去掉反引号**（真机 tmux 抓屏：回复与回显里都只剩 `src/main.rs:1`，反引号被样式取代）。因此在 Claude（以及渲染方式类似的 TUI）里，约定的反引号识别用不上：无空格、带目录和扩展名的路径仍靠原有 token 识别点击；**含空格路径无法识别**。反引号识别只对原样输出文本的场景有效（脚本输出、`-p` 输出、不剥反引号的 TUI）。
- 约定的实际收益主要是让模型稳定写出「目录 + 扩展名 + 可选 :行号、不加粗、不加链接」的路径，使原有 token 识别命中率更高。
- 带空格的反引号内容必须“像路径”才识别（首段含目录或以 `~`/`.` 开头，且无 `-` 参数），避免把 `` `git diff src/a.rs` `` 这类命令整串当成路径。
- 逻辑行向上/向下最多连接 8 个可视行；无反引号的行在分配前直接返回，保证每行渲染的开销有上限。
- Kimi 的“只注入一次”标记在 `$TMPDIR/norma-path-convention/`（0700，文件名含 session_id 哈希，7 天后清理）；写 stdout 失败时撤销标记。
