# Rust Agent 框架选型调研

> 调研日期：2026-07-18
> 目标：轻量、低运行时开销、支持流式输出、支持 tools/function calling，并适合接入 Norma OS 现有 `crates/agent`。
> 决策状态：本文件保留初筛过程；最终选择已改为 `rig-core = 0.40.0`，以 Norma 单一 Master 工具循环替换旧 AgentSystem。最终边界见 [`2026-07-18-rig-master-work-architecture.md`](2026-07-18-rig-master-work-architecture.md) 与 [Norma Runtime SPEC](../product-specs/2026-07-18-SPEC-norma-master-agent-runtime.md)。

## 初筛结论（已被最终 Rig 决策取代）

Norma OS 已拥有自己的 Agent 域模型、工具 schema、工具循环、取消、预算与审计边界。最合适的路线不是整体替换 Agent Runtime，而是保留现有确定性逻辑，只引入一个异步 Provider 层：

1. **首选 `genai 0.6.x`**：最轻、Provider 覆盖广、原生流式与 function calling，接入侵入性最低；它不是完整 Agent 框架，正好不会和 Norma 现有工具循环重叠。
2. **次选 `rig-core 0.40.x`**：完整度、社区和文档最佳；如果后续希望统一引入 typed tools、MCP、RAG 或多 Agent workflow，Rig 的上限更高。
3. **条件选择 Swiftide**：只有明确需要 RAG pipeline、typed task graph、暂停/恢复与 human-in-the-loop 时再选。

不建议为当前需求整体引入 AutoAgents、LangChain Rust 或 Kalosm；不建议新项目采用已长期不活跃的 llm-chain。

## 决策表

评分说明：`5` 最好。轻量/性能是基于默认 feature、架构范围、异步与流式设计的工程判断，不是跨框架吞吐基准；云端 LLM 场景的端到端延迟主要由网络和模型推理决定。

| 候选 | 定位 | 流式输出 | Tools / MCP | Provider | 轻量/性能 | 成熟度 | 接入 Norma 侵入性 | 建议 |
|---|---|---|---|---|---:|---:|---:|---|
| **genai 0.6.x** | 薄 Provider SDK，不含 Agent loop | 原生 chat event stream | Function calling；MCP 和执行循环自管 | 官方称 26+，含 OpenAI、Anthropic、Gemini、Ollama、Bedrock 等 | **5** | 4 | **低** | **首选：保留现有 Runtime，只替换 Provider transport** |
| **Rig / rig-core 0.40.x** | 完整 LLM/Agent 库 | 多轮 agent stream，文本、工具与最终响应事件 | Typed tools；官方 Rust MCP SDK；可自定义工具 | 官方称 20+ | **4** | **5** | 中 | **次选：需要完整生态、MCP/RAG 时选** |
| **Swiftide 0.32.x** | Agent harness + typed task graph + streaming RAG | Agent response stream、stream hooks | Rust function、`Tool`、MCP，支持执行器 | OpenAI、Anthropic、Gemini、Ollama、Groq、Bedrock 等 | 3 | 4 | 高 | 明确需要 RAG/任务图/暂停恢复时选 |
| **AutoAgents 0.4.x** | 完整多 Agent Runtime | 多 Provider streaming | Derive tools、MCP、可选 WASM 沙箱 | 多云端与 Ollama、mistral.rs、llama.cpp | 3 | 3 | **很高** | 多 Agent/WASM 沙箱是硬需求时再评估 |
| **LangChain Rust 4.6.x** | Rust 版 LangChain | 官方首页没有清楚证明 agent token/tool lifecycle streaming | 有 Agent 与 Tools | OpenAI、Azure OpenAI、Ollama、Anthropic | 2 | 3 | 高 | 不作为流式优先项目首选 |
| **Kalosm 0.4.x** | 本地模型/结构化生成生态，不是通用 Agent Runtime | 本地生成可流式 | 强项是约束生成，不是通用 tool loop/MCP | 主要是本地模型 | 2（接云）/4（本地） | 3 | 高 | 只在内嵌本地推理成为核心需求时选 |
| **llm-chain 0.13.x** | 早期 chain/agent 库 | 现有官方资料未证明现代 agent streaming | 有早期 tools | 较有限 | 2 | 1 | 高 | **排除：仓库最后推送停在 2024-10** |

## 为什么首选 genai，而不是直接换成完整框架

Norma 的 `AgentProvider` 已能产生 `TextDelta` 和完整 `ToolCall`，`AgentSystem` 也已承担工具轮次、预算与状态迁移。完整框架会产生两套重叠职责：两套消息模型、工具循环、记忆、停止条件和错误语义。`genai` 只负责 Provider 协议差异、流式解析和 function calling，边界最清楚。

`genai` 官方说明它使用各 Provider 原生协议（适合时使用 OpenAI-compatible），支持 `exec_chat_stream`，并聚焦 chat、vision 和 function calling。建议锁定稳定的 `0.6.x`；`0.7.x` 当前仍为 beta，不适合直接进入核心运行时。

风险：它不替你执行工具，也不提供 MCP host、memory 或 agent orchestration；这些能力需要继续由 Norma 持有。对本项目而言这是优点，不是缺陷。

## 何时改选 Rig

Rig 是当前 Rust Agent 生态里最均衡的完整候选：官方文档明确支持多轮 streaming、typed tools、20+ Provider、MCP、RAG 与 multi-agent workflow，且 `rig-core` 可以单独引入。它还公开列出多个生产使用者，社区规模明显领先其他 Rust Agent 库。

适合改选 Rig 的信号：

- 希望由上游统一 Provider、tool schema、MCP client、RAG 和 memory；
- 接下来会快速扩展多个模型厂商与向量库；
- 愿意为 Rig 设计一个稳定 adapter，隔离其 pre-1.0 API 变化。

风险：官方 README 明确提醒后续仍可能有 breaking changes。无论选择 Rig 还是 genai，都应锁版本并只让本地 adapter 依赖上游类型，不能让第三方消息类型扩散到 `agent` 域模型。

## 性能判断

目前没有可信、统一、可复现的跨框架端到端基准，因此不应根据各项目自报的“高性能”数字决策。对 Norma 更重要的是：

- Provider transport 必须异步，不能在 GPUI 前台阻塞；
- stream 需要背压、取消和完整的 tool-call 参数拼接；
- 不要让第三方框架接管已有的确定性工具状态机；
- 只启用实际需要的 Cargo features，避免把 RAG、向量库、WASM 或本地推理后端带入默认构建。

当前 `AgentProvider` 是同步 trait + callback。接入 Rig/genai 时应在后台 runtime 中消费异步 stream，再映射为现有 `ProviderStreamEvent`，而不是在 GPUI 线程 `block_on`。

## 建议的最小验证 Spike

对最终二选一建议各做一个隔离 adapter，不改 AgentSystem：

1. `genai 0.6.x`：验证 OpenAI-compatible + Ollama，文本 delta、并行 tool calls、取消、错误映射。
2. `rig-core 0.40.x`：验证同一组用例，并记录依赖增量、编译时间、二进制增量与 API 映射复杂度。

验收指标应是首 token 延迟、stream 事件完整性、取消生效时间、工具参数正确率、依赖/二进制增量；不要用模型总生成时间比较框架 CPU 性能。

## 一手来源

- Rig：[官方仓库](https://github.com/0xPlaygrounds/rig)、[官方站点](https://www.rig.rs/)、[Streaming 文档](https://book.rig.rs/playbook/rig-api-call.html)
- Swiftide：[官方仓库](https://github.com/bosun-ai/swiftide)、[官方站点](https://swiftide.rs/)、[Streaming agents 发布说明](https://blog.bosun.ai/swiftide-0-26/)
- AutoAgents：[官方仓库](https://github.com/liquidos-ai/AutoAgents)、[官方文档](https://liquidos-ai.github.io/AutoAgents/)
- LangChain Rust：[官方仓库](https://github.com/Abraxas-365/langchain-rust)
- genai：[官方仓库](https://github.com/jeremychone/rust-genai)、[crates.io](https://crates.io/crates/genai)
- Kalosm：[官方仓库](https://github.com/floneum/kalosm)
- llm-chain：[官方仓库](https://github.com/sobelio/llm-chain)
