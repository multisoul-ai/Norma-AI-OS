# Grok Build 与 Norma OS 对接调研

> 调研日期：2026-07-19
> 调研目标：确认“马斯克的代码智能体 GROUP”所指产品，并找出接入 Norma OS 的可行方法。
> 结论性质：产品/API 事实来自 xAI 官方文档与官方仓库；Norma 接入建议是基于当前仓库代码的工程判断。

## 结论

用户所说的 `GROUP` 应是 **Grok**。如果指“完整代码智能体”，准确产品名是 **Grok Build**；如果只指其底层代码模型，则当前首选是 `grok-4.5`，也可选择代码专用的 `grok-build-0.1`。

Norma OS 有三种不同深度的对接方式：

1. **把 Grok 作为 Norma Master 的模型 Provider**：现有代码已经支持 OpenAI-compatible Chat Completions，配置 `https://api.x.ai/v1`、xAI API Key 和 `grok-4.5` 即可。这是最小改动，但接入的是模型，不是完整 Grok Build 智能体。
2. **把 Grok Build 作为第五种 Agent 卡片**：启动官方 `grok` TUI，复用 Norma 现有 Terminal-backed Agent、PTY、任务输入、会话恢复和 hooks。这条路线与当前 Codex/Claude/OpenCode/Cursor 架构最一致，建议作为一期正式方案。
3. **通过 ACP 原生嵌入 Grok Build**：启动 `grok agent stdio`，以 stdin/stdout 上的 JSON-RPC 驱动 `initialize`、`authenticate`、`session/new`、`session/prompt`，并消费 `session/update`。它最适合未来不依赖终端屏幕解析的深度集成，但改造范围更大。

## 产品名称与当前模型

- **Grok Build** 是 xAI 的完整编码智能体和 CLI，支持交互 TUI、headless 脚本模式和 ACP 嵌入。
- **`grok-4.5`** 是当前 Grok Build 的默认模型，可用于 coding、agentic tasks 和 knowledge work；官方同时支持 Responses API 与 Chat Completions。
- **`grok-build-0.1`** 是代码/工程工作流模型，支持 function calling、structured outputs 和 reasoning。
- 旧的 **`grok-code-fast-1` 已于 2026-05-15 退役**，官方建议迁移到 `grok-build-0.1`；继续使用旧 slug 会发生重定向，因此新接入不要再写死旧模型名。

## 路线 A：直接接入 xAI 模型 API

### 官方接口

```text
Base URL: https://api.x.ai/v1
Auth:     Authorization: Bearer <XAI_API_KEY>
Model:    grok-4.5
API:      /v1/chat/completions 或 /v1/responses
```

xAI 官方说明 Chat Completions 是 legacy 接口，新的 agentic tools、服务端状态和后续能力优先进入 Responses API。但 Norma 当前 Rig Provider 使用 OpenAI-compatible Chat Completions，因此可以先直连；若未来需要 xAI 服务端 web/X search、code execution、MCP 或原生状态会话，应新增 Responses API transport。

最小请求：

```bash
curl https://api.x.ai/v1/chat/completions \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "grok-4.5",
    "messages": [{"role": "user", "content": "检查这个 Rust 函数"}]
  }'
```

### 在当前 Norma OS 中配置

当前 `crates/agent/src/master.rs` 已用 Rig 的 OpenAI-compatible client，`ProviderConfig` 支持自定义 endpoint、model 和 API key；Settings 也已经有 OpenAI-compatible Provider 表单。因此可直接填写：

```text
Provider: OpenAI-compatible
Base URL: https://api.x.ai/v1
API Key:  xAI Console 创建的 key
Model:    grok-4.5
```

当前 Norma 的 `discover_provider_models` 会访问 `<base_url>/models`；若目录发现不可用，现有错误模型允许回退到手动填写 model。连接后仍应运行现有 structured tool-call validation，证明 Grok 能实际调用 `create_terminal`，而不是只生成文字。

局限：这条路线让 Grok 驱动 Norma 自己的工具循环，并不会获得 Grok Build 完整的文件编辑器、shell harness、skills、plugins、hooks、MCP、subagents、session UI 等能力。

## 路线 B：新增 Grok Build Agent 卡片（推荐）

### 安装与认证

官方安装命令：

```bash
curl -fsSL https://x.ai/cli/install.sh | bash
grok login
```

无浏览器或自动化环境可使用：

```bash
export XAI_API_KEY="xai-..."
grok
```

正式接入前应检查 `grok --help` 是否包含当前 Grok Build 的 `login`、`sessions`、`agent stdio` 等命令。本机现有 npm 包 `@xai-official/grok@0.1.4` 只显示旧的 `acp` wrapper 接口，与当前官方 Grok Build 文档不一致，不能直接据此宣称接入成功；应先单独解决 CLI 版本/安装来源冲突，但本次调研没有执行升级或覆盖安装。

### 为什么适合 Norma

Norma 当前四种 Agent 都是 `AgentProfile -> PTY command -> AgentCard`：

```text
Claude   -> claude
Codex    -> codex
Cursor   -> cursor-agent
OpenCode -> opencode
Grok     -> grok
```

Grok 官方 hooks 事件包括：

```text
SessionStart / SessionEnd
UserPromptSubmit
PreToolUse
PostToolUse / PostToolUseFailure
Stop / StopFailure
Notification
SubagentStart / SubagentStop
PreCompact / PostCompact
```

这些事件与 Norma 现有 Claude/Codex/Cursor/OpenCode hook 归一化模型高度一致。Grok hook JSON 可放在 `~/.grok/hooks/*.json`，事件 JSON 通过 stdin 传给命令，包含 `hookEventName`、`sessionId`、`cwd`、`workspaceRoot`、`toolName` 和 `toolInput`。因此可以复用现有 `norma agent-hook forward` 思路，把 Grok 的生命周期转成 Norma 的 Ready、Thinking、RunningTool、Done、Failed。

### 需要修改的仓库边界

- `canvas`：为 `AgentProfile` 增加 `Grok`，补标题、serde 与节点测试。
- `runtime`：`agent_command(Grok)` 启动 `grok`；恢复命令使用官方 `grok --resume <ID>`；会话目录按官方 `~/.grok/sessions` 做受控发现。
- `norma`：CLI/profile 协议和 hook source 增加 `grok`；安装/卸载只管理 Norma 自己的 `~/.grok/hooks/*.json`，不得覆盖用户其他 hooks。
- `storage`：profile 编解码增加稳定值 `grok`，兼容已有数据库。
- `app/ui`：New menu、Settings Agent profile、图标/初始化状态、protocol 映射增加 Grok。
- `agent`：`create_agent.profile` schema 与 prompt 增加 `grok`。
- `e2e`：覆盖创建、PTY 启动、任务提交、hook Ready/Stop、session restore 和“不误标为其他 Agent”。

安全建议：不要默认使用 `--always-approve`/`--yolo`。Norma 当前某些 Agent 以跳过审批方式启动，但 Grok 应先保持官方默认 `ask` 权限模式；若产品以后要自动审批，应把允许范围落实为显式 permission/sandbox rules，并由用户可见设置控制。

## 路线 C：ACP 原生嵌入

官方建议 IDE 或工具集成使用 ACP：

```bash
grok agent stdio
```

它在 stdin/stdout 上运行 JSON-RPC。最小生命周期是：

```text
spawn grok agent stdio
  -> initialize(protocolVersion=1, clientCapabilities)
  -> authenticate(methodId=xai.api_key | cached_token)
  -> session/new(cwd, mcpServers=[])
  -> session/prompt(sessionId, prompt=[...])
  <- session/update(agent_message_chunk / tool events / ...)
  <- session/prompt result(stopReason)
```

对 Norma 的价值：输出与工具状态成为结构化事件，不再依赖 ANSI grid 或 hook 的旁路状态；还可以在 Norma UI 中实现权限确认、流式消息、session、tool call 和取消。

代价：需要新增 ACP client、子进程监管、JSON-RPC 请求关联、认证、能力协商、文件/终端 capability、审批回调、session 生命周期和错误映射。当前 AgentCard 本质是 Terminal-backed，直接上 ACP 会形成第二类 LiveNode runtime，因此建议在 PTY+hooks 方案稳定后再做。

## 建议实施顺序

1. 用路线 A 做无代码/小配置 Spike：验证 `grok-4.5` 能通过 Norma 的 tool-call validation。
2. 用路线 B 做正式 Grok Agent 卡片：先实现 `AgentProfile::Grok + grok PTY + hooks + session restore`，保持现有架构一致。
3. 若需要原生结构化权限、无 TUI 嵌入或更可靠的工具事件，再以路线 C 替换 Grok 卡片内部 transport；不要同时重写 Norma Master Provider。

## 一手来源

- [Grok Build 官方概览](https://docs.x.ai/build/overview)
- [Grok Build Headless 与 ACP](https://docs.x.ai/build/cli/headless-scripting)
- [Grok Build CLI Reference](https://docs.x.ai/build/cli/reference)
- [Grok Build Hooks](https://docs.x.ai/build/features/hooks)
- [Grok Build Settings](https://docs.x.ai/build/settings)
- [Grok Build 官方开源仓库](https://github.com/xai-org/grok-build)
- [Grok 4.5](https://docs.x.ai/developers/grok-4-5)
- [Chat Completions](https://docs.x.ai/developers/model-capabilities/legacy/chat-completions)
- [Responses API 与 Chat Completions 对比](https://docs.x.ai/developers/model-capabilities/text/comparison)
- [Grok Build 0.1 模型页](https://docs.x.ai/developers/models/grok-build-0.1)
- [2026-05-15 模型退役与迁移](https://docs.x.ai/developers/migration/may-15-retirement)
