# Agent 卡片自我身份与自派发防循环：三套实施方案

> 状态：供选择的设计方案，尚未进入实现  
> 日期：2026-08-04  
> 代码基线：`1eb7eeeb`；行号按当前工作区文件计算  
> 原始输入：Ruby 的完整调查已转存到 `artifacts/ruby-self-identity-original-report.md`  
> 范围：只设计和细化，不修改生产代码

## 1. 结论先行

建议采用“**方案一作为不可省略的安全底座，再实施兼容性优化后的方案三**”：

1. **P0：方案一——指令性拒绝 + Skill 确定性停止规则。**先阻断换 `operation_id` 重试的循环，并保证两条自派发拒绝路径给出同一身份解释。
2. **P1：方案三——`norma agent self` 主动自查。**优先复用现有、已鉴权的 `ListAgents` 协议，并在 CLI 侧按 `MULTISOUL_NODE_ID == active_node_id` 投影出自身，避免为一个只读查询新增 wire command 和协议升级。
3. **P3：方案二——Hook 身份注入。**先做逐 Provider 能力探针；P2 先补重复拒绝的可观测性。不要直接承诺一次实现覆盖 Claude、Codex、Cursor、OpenCode。Hook 可以成为后续体验增强，但当前不适合作为跨 Provider 主解。

最重要的边界是：**“自己”由 `(workspace_id, definition_id)` 判定，名称和语音别名只用于显示与寻址，不能单独定义身份。**因此，另一个 Workspace 中同名的“浩宇”不是自己。

## 2. 已核实的根因与数据链

当前链路是：

```text
用户对“浩宇”卡片说：“把任务 X 指派给浩宇”
    |
    v
Provider 会话读取共享的静态 norma-orchestration Skill
    |  Skill 不含当前卡片的 name / speech_alias / definition_id / node_id
    v
Provider 调用 norma agent task assign 浩宇 ...
    |
    v
CLI 只把 MULTISOUL_NODE_ID 放进 WireRequest，LLM 本身没有读到该身份
    |
    +--> 全局账本路径：storage::validate_lineage -> SelfDispatch
    |
    +--> Workspace 内路径：validate_delegation_target -> SelfDispatchForbidden
    |
    v
返回“不能派给自己”，但没有说明“浩宇就是你”及“不得换 operation_id 重试”
```

代码证据：

- 共享 Skill 是静态字符串，见 `crates/norma/src/orchestration/skill.rs:38-127`；同 Workspace 派发规则在 `:66-71`，只规定了发现目标和执行 `assign`。
- Skill 只对 `outcome_unknown` 规定复用原 `operation_id`，见 `skill.rs:99-112`；没有 `self_dispatch_forbidden` 的停止规则。
- CLI 从进程环境读取 `MULTISOUL_NODE_ID`，见 `crates/norma/src/client.rs:165-188`。该值用于鉴权/路由，不会自动进入模型上下文。
- 全局账本在 `crates/storage/src/orchestration_task_route_store.rs:324-368` 用 Workspace + definition 判定自派发和祖先环。
- Workspace 内协调器在 `crates/agent/src/coordination/delegation.rs:248-277` 用 definition 判定自派发和当前祖先链环。
- 全局自派发错误目前是泛化文本，见 `crates/app/src/norma/endpoint/task_dispatch.rs:146-161`。
- Workspace 内错误映射只保留 `AgentCoordinationError::to_string()`，见 `crates/app/src/app/agent_runtime/command_protocol.rs:24-47`。
- 当前 `agent list` 已包含 `definition_id`、`name`、持久化 `speech_alias` 和 `active_node_id`，见 `crates/norma/src/protocol.rs:274-284` 与 `crates/app/src/app/agent_runtime/commands.rs:67-82`。

一个容易漏掉的事实：账本在 `reserve()` 中先查 `operation_id`，再验证 lineage，但被拒绝的自派发不会写入任务记录，见 `orchestration_task_route_store.rs:78-105`。所以换新 `operation_id` 仍会再次触发一次拒绝；现有幂等机制不能消除“反复尝试”的观感。

## 3. 设计不变量与验收口径

### 3.1 身份不变量

| 编号 | 不变量 |
|---|---|
| I-1 | 同一 Workspace 内，`source_definition_id == target_definition_id` 才是直接自派发。 |
| I-2 | `node_id` 标识当前活跃卡片实例，并用于验证请求确实来自该 definition 的活跃节点。 |
| I-3 | `name` 与持久化的 `speech_alias` 是寻址键和展示信息，不是跨 Workspace 的全局身份。 |
| I-4 | 不得根据内置名字表重新推导已被迁移清空的 `speech_alias`；必须使用存储中的实际值。 |
| I-5 | `self_dispatch_forbidden`、`delegation_cycle`、`delegation_budget_exceeded` 都是确定性拒绝，模型不得通过换参数或换 `operation_id` 自动重试。 |
| I-6 | Hook 或主动自查失败时不得猜测身份；安全行为是停止派发并报告无法确认。运行时拒绝必须始终保留。 |

### 3.2 用户可观察验收标准

1. 浩宇卡片收到“把 X 指派给浩宇”或其规范化语音称呼时，不产生发给浩宇的新任务。
2. 若 Provider 仍发起第一次错误调用，响应必须包含稳定码 `self_dispatch_forbidden`、当前规范名称、可选语音称呼，以及“不要重试/不要换 operation_id”的明确动作。
3. 直接自派发不得新增全局 task route、不得新增本地 coordinator task、不得向卡片 PTY 写入重复任务。
4. 同一自派发请求使用原 `operation_id` 或新 `operation_id` 都只能被拒绝，不能绕过运行时防线。
5. Workspace A 的“浩宇”可以向 Workspace B 的同名“浩宇”派发；不得仅凭显示名称误判为自己。
6. `A -> B -> A` 仍返回 `delegation_cycle`；不能被降级成自派发，也不能自动重试。
7. 活跃 Agent 改名后，主动自查和运行时错误使用最新持久化名称；旧别名不得继续被凭空生成。
8. 嵌套指令只处理当前层第一个目标：若第一个目标是当前 Agent，当前 Agent 直接处理剩余任务；若目标是祖先 Agent，交由 cycle guard 拒绝。

“没有第二次派发尝试”不是单靠服务端单测就能证明的性质。服务端可以证明零副作用和每次都拒绝；模型是否再次发命令，需要 Skill 契约测试 + 可控 Provider 脚本/L3 真实会话共同验证。

## 4. 公共设计：统一身份快照与错误文案

无论选择哪套方案，都应避免在两条拒绝路径各自拼字符串。建议在 App 层引入一个只读快照：

```rust
struct AgentIdentitySnapshot {
    workspace_id: String,
    definition_id: i64,
    node_id: u64,
    name: String,
    speech_alias: Option<String>,
    profile: AgentProfile,
}
```

推荐查询接口是：

```rust
MultisoulApp::agent_identity_for_source(source: AgentDispatchSource)
    -> Option<AgentIdentitySnapshot>
```

这比 Ruby 初稿里的 `agent_identity_for_definition(id)` 更严格：它同时核对活跃 `node_id`，并把 Workspace 作用域带入身份。`agent_identity_for_definition(id)` 可以作为内部子查询，但不应成为对外的最终身份判定接口。

统一错误构造器应生成同一模板：

```text
你当前的 Norma OS Agent 身份是“浩宇”（语音称呼：“浩宇”）。
目标“浩宇”解析到与你相同的 Workspace 和 definition，因此目标就是你自己。
不要再次调用 agent task assign，不要更换 operation_id；请直接执行任务，
或向用户说明该任务无需/无法再次委派。
```

约束：

- `retryable` 必须保持 `false`。
- 没有 `speech_alias` 时省略整段，不输出 `None`、空字符串或推导值。
- 日志只记录 Workspace、definition、node、错误码和 attempt 序号，不记录任务正文。
- 同一模板同时供全局账本拒绝和 Workspace 内 coordinator 拒绝使用，防止行为漂移。

## 5. 方案一：运行时指令性拒绝 + Skill 停止规则

### 5.1 设计思路

运行时仍是最终权威。第一次错误调用被拒绝时，响应把“禁止”升级为“身份解释 + 下一步动作”；共享 Skill 明确把该错误归类为不可重试。

需要修正 Ruby 初稿中的一句话：静态 Skill 本身不知道当前 Agent 是谁，所以单写“目标名 == 自己时不要 assign”并不足以提前判断。方案一真正可靠的 Skill 规则应是：

1. 如果上下文已经明确给出自身身份，可以在调用前判断。
2. 一旦收到 `self_dispatch_forbidden`，服务端给出的规范目标就是自身；立即停止，不得换 `operation_id`、目标拼写或 Workspace 再试。
3. 对 `delegation_cycle` 和 `delegation_budget_exceeded` 同样停止，避免把自派发循环修好后留下祖先环重试。

### 5.2 具体改动位置

| 文件与当前行号 | 改动 |
|---|---|
| `crates/norma/src/orchestration/skill.rs:66-71,99-116` | 增加确定性拒绝处理表；明确 `self_dispatch_forbidden`、`delegation_cycle`、`delegation_budget_exceeded` 不得自动重试。 |
| `crates/norma/src/orchestration/installer/skill_component.rs:67-106` | 不改比较算法，但确认 Skill 内容变化会被精确字符串校验识别为 `Outdated` 并触发修复。 |
| `crates/app/src/app/agent_runtime/coordination.rs:70-98` | 增加 `agent_identity_for_source`/内部 definition 查询，复用现有 roster 数据源。 |
| `crates/app/src/norma/endpoint/routing.rs:10-29` | 将验证结果收敛成 `ValidatedAgentSource`，携带 dispatch source、provider、身份快照；避免后续仅拿到裸 definition ID。 |
| `crates/app/src/norma/endpoint/task_dispatch.rs:24-38,60-85,146-161` | 把已验证来源身份带入异步 reservation；SelfDispatch 分支调用统一错误构造器。 |
| `crates/app/src/app/agent_runtime/commands.rs:252-266` | 在可变借用 coordinator 前获取来源身份快照，把它传给错误映射。 |
| `crates/app/src/app/agent_runtime/command_protocol.rs:24-47` | 将 `coordination_error` 扩为接受可选身份；SelfDispatchForbidden 使用统一指令性文案，身份缺失时保守回退到通用错误。 |

### 5.3 实施步骤（TDD）

1. 在 `crates/norma/tests/orchestration_skill_contract_tests.rs` 先加失败测试：三个确定性错误均有停止规则，且明确禁止新 `operation_id`。
2. 在 App 单元测试中构造 Alan/艾伦别名和活跃 node，先断言两条错误路径都返回同一规范名称、同一 `retryable=false`。
3. 增加身份快照与统一错误构造器，再接全局账本路径。
4. 接 Workspace 内 coordinator 路径，确保 borrow 顺序是“先取身份快照，再可变借用 coordinator”。
5. 修改 Skill 源并验证安装器把旧内容识别为 `Outdated`。
6. 增加 `crates/e2e/src/e2e_12_agent_self_dispatch.rs`：断言全局 task route、本地 task、目标 PTY 输入都没有新增。
7. 用可控 Provider 脚本记录收到错误后的命令次数；真实 Provider L3 作为补充观察，不用非确定性模型行为替代自动断言。

### 5.4 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 第一次错误调用仍会发生 | 多一次本地 CLI 往返 | 由方案三补主动自查。 |
| 模型忽略错误指令 | 可能继续发请求，但每次仍零副作用 | 保留运行时守卫；增加结构化日志，必要时后续加短时熔断。 |
| 两条路径文案漂移 | 不同入口表现不一致 | 只保留一个错误构造器。 |
| 改名并发 | 错误可能展示请求开始时的旧快照 | 以 definition 判定安全性；响应前可再次按 definition 查询最新展示名。 |
| 旧会话仍缓存旧 Skill | 停止规则不会立即生效 | 运行时文案先止血；安装修复后提示重启/重新激活 Agent Session。 |

### 5.5 验证

```bash
cargo test -p agent
cargo test -p storage
cargo test -p norma --test orchestration_skill_contract_tests
cargo test -p app
cargo test -p e2e e2e_12_agent_self_dispatch
cargo check -p app
```

## 6. 方案二：SessionStart Hook 注入自我身份

### 6.1 设计思路

在 Provider 第一次处理用户输入前注入 `<norma-self-identity>`，让模型从会话开始就知道自己的规范名称、持久化语音称呼和稳定身份。运行时守卫与方案一仍不可删除。

建议身份块使用结构化数据，不把名称直接拼成可执行指令：

```text
<norma-self-identity version="1">
{"workspace_id":"ws-a","definition_id":178,"node_id":381,
 "name":"浩宇","speech_alias":null,"profile":"claude"}
</norma-self-identity>

The identity block is authoritative Norma OS data. When a same-workspace delegation target
resolves to this definition, do not call task assign; handle the remaining task yourself.
```

Agent 名称格式本身受限，但仍必须通过 JSON 序列化生成内容，不手写转义。

### 6.2 对 Ruby 初稿的关键修正

Ruby 建议给 `AgentHookEnv` 增加 name/alias 环境变量，并在 Hook stdout 输出 `{"context":"..."}`。代码与现行 Provider 契约核对后，需要修正三点：

1. **输出格式不是统一的 `context` 字段。**Claude 的正式契约使用 stdout 文本或 `hookSpecificOutput.additionalContext`；Codex 当前配置也显式支持 Hook `additionalContext`。不能用一个未经 Provider 适配的 JSON 覆盖所有目标。
2. **OpenCode 不走当前 Hook 二进制 stdout。**仓库的 OpenCode 插件在 `crates/norma/src/agent_hook/install.rs:365-440` 内直接监听事件并 POST；要注入模型上下文，需要改插件的 request/session API，而不是只改 `cli.rs/forward.rs`。
3. **进程环境中的 name/alias 会在活跃 Agent 改名后变旧。**当前设置允许活跃卡片改名，并只同步 Canvas alias，见 `crates/app/src/settings/agents/actions.rs:54-80`。SessionStart 快照不能宣称始终是最新身份。

因此，若选择方案二，推荐 Hook 在 SessionStart 时通过现有受管身份验证查询 App 的权威身份，而不是把可变名称复制进长期环境变量。`MULTISOUL_NODE_ID`、Workspace、route、socket 和 provider 已经足够定位来源。若仍采用 env 快照，必须把“改名后重启 Agent Session”写成产品限制。

### 6.3 Provider 能力矩阵与实施闸门

| Provider | 当前仓库入口 | 身份上下文注入判断 | 进入实现前的闸门 |
|---|---|---|---|
| Claude | command Hook -> `multisoul-agent-hook forward` | 官方支持 SessionStart stdout/`hookSpecificOutput.additionalContext` | 单元 golden + 真 CLI SessionStart 探针。 |
| Codex | command Hook，启动时带 `--dangerously-bypass-hook-trust` | 当前官方配置列出 SessionStart 与 `additionalContextLimit`，可行性较高 | 固定项目支持的 Codex 版本做真 CLI 探针，锁定精确输出 schema。 |
| Cursor | command Hook，事件名 `sessionStart` | 返回字段和实际注入时序不能从仓库证明 | 必须先做真 CLI 探针；失败则仅使用方案一/三。 |
| OpenCode | 生成的 JS 插件直接 POST 事件 | 当前路径没有 stdout 注入点；V2 request hook 可改 system/messages，但 API 仍标为 beta | 独立插件实现与兼容性测试，不与 command Hook 共用假设。 |
| Grok | command Hook | 当前 App 明确不允许 Grok 编排来源，见 `server/orchestration.rs:121-127` | 不纳入首批身份派发验收。 |

参考：Claude 的 [Hooks reference](https://code.claude.com/docs/en/hooks) 明确 SessionStart 可通过 `hookSpecificOutput.additionalContext` 增加上下文；Codex 的 [Configuration Reference](https://developers.openai.com/codex/config-reference/) 列出 SessionStart 和 `additionalContextLimit`；OpenCode 的 [V2 Plugins](https://opencode.ai/v2/docs/build/plugins) 提供请求前修改 system/messages 的 hook，但同时标注 V2 API 为 beta。

### 6.4 具体改动位置

**权威查询式（推荐）：**

| 文件与当前行号 | 改动 |
|---|---|
| `crates/norma/src/agent_hook/mod.rs:31-35,91-102,162-170` | 保留 node/provider/env；扩展 Hook forward 结果类型以承载可选 Provider 输出，不把身份字段塞入共享事件。 |
| `crates/norma/src/agent_hook/cli.rs:32-63` | `forward_from_stdin` 在 SessionStart 完成事件转发后查询 self，并仅在成功时打印适配后的上下文；失败退出 0 且 stdout 为空。 |
| `crates/norma/src/agent_hook/forward.rs:10-27` | 将“是否 POST 成功”与“是否生成上下文”分开；POST 失败不得阻塞身份输出，身份查询失败也不得阻塞 Provider。 |
| `crates/norma/src/agent_hook/event.rs:5-33,63-123` | 增加 `is_session_start()` 这类纯判定，禁止对其他事件输出身份。 |
| `crates/norma/src/agent_hook/install.rs:120-130` | 为 Claude/Codex/Grok 使用各自 golden schema；不要假设相同字段。 |
| `crates/norma/src/agent_hook/cursor.rs:79-101` | Cursor 单独序列化与实测。 |
| `crates/norma/src/agent_hook/install.rs:365-440` | OpenCode 插件单独增加请求前身份注入；保留现有生命周期 POST。 |
| `crates/norma/src/orchestration/skill.rs:50-80` | 说明 `<norma-self-identity>` 的权威性和同 Workspace 自目标行为。 |

**环境快照式（Ruby 原案，不推荐作为最终形态）：**还需改 `crates/runtime/src/agent_hooks.rs:17-37,49-63`、`runtime.rs:259-277,319-346`、`runtime/definition_identity.rs:6-31`、`app/agent_runtime/command_support.rs:86-116`，并处理 session restore 在 `runtime/session_restore.rs:103-125,217-235` 缺少持久化 speech alias 的问题。不能从内置名字表重建 alias，因为迁移可能有意将冲突 alias 清空。

### 6.5 实施步骤（TDD）

1. 先为 Claude、Codex、Cursor 各写一个最小真 CLI 探针，输入固定 identity sentinel，验证模型实际能读到，而不只是 Hook 日志显示成功。
2. 为 Hook 输出建立 Provider golden tests：SessionStart 有且仅有一个身份块，其他事件 stdout 为空。
3. 为缺 env、App 未运行、socket 超时、身份找不到、JSON 转义失败写 fail-open 测试；所有场景 exit code 为 0。
4. 实现权威 self 查询并注入 Claude/Codex；未通过探针的 Provider 明确降级为方案一/三。
5. OpenCode 作为独立子项目验证插件 request hook，不与 command Hook 同时交付。
6. 增加 resume、改名后新 Session、旧 Session 仍使用旧身份的行为测试和产品提示。
7. 真实 App 中逐 Provider执行“你是谁”“把 X 指派给你自己”“把 X 指派给你的语音称呼”三组验收。

### 6.6 风险与验证

| 风险 | 影响 | 缓解 |
|---|---|---|
| Provider 输出 schema/时序不同 | 部分卡片完全收不到身份 | Provider adapter + 真 CLI 探针 + 按 Provider 降级。 |
| SessionStart 只注入一次 | 活跃改名后上下文陈旧 | 方案一兜底；要求重开 Session，或升级为每轮 UserPromptSubmit 动态注入。 |
| Hook 增加本地查询 | 启动变慢或超时 | 查询只读、短超时；任何失败 fail-open。 |
| OpenCode 插件 API beta | 更新后失效 | 独立版本探针和兼容矩阵，不阻塞公共方案。 |
| 身份上下文被用户提示覆盖 | 模型仍可能发起 assign | 运行时守卫永远保留。 |

## 7. 方案三：CLI 自我查询 + Skill 自查 + 运行时兜底

### 7.1 设计思路

新增用户可见命令：

```bash
norma agent self --format json
```

成功输出：

```json
{
  "schema_version": 1,
  "ok": true,
  "request_id": "...",
  "result": {
    "self": {
      "workspace_id": "workspace-a",
      "definition_id": 178,
      "node_id": 381,
      "name": "浩宇",
      "speech_alias": null,
      "profile": "claude"
    }
  }
}
```

Skill 在每次同 Workspace `assign` 前查询一次最新身份；若第一个目标等于规范名称或持久化语音称呼，则不生成 `operation_id`、不调用 assign，直接处理剩余任务。显式跨 Workspace 派发不按名称与 source self 比较。

### 7.2 兼容性优化：优先不新增 wire command

Ruby 初稿建议新增 protocol command。独立审视后，更推荐以下实现：

1. `agent self` 在 CLI 命令树中仍映射为现有 `CardCommand::ListAgents`。
2. App 继续执行现有来源鉴权并返回 roster。
3. CLI 用进程中的 `MULTISOUL_NODE_ID` 精确匹配唯一的 `AgentSummary.active_node_id`，把该项投影成 `result.self`。
4. 匹配数不是恰好 1 时返回非重试的 `source_agent_unavailable`，绝不猜测名称。

优点：不改 `CURRENT_PROTOCOL_VERSION=3`，不增加旧 App 无法反序列化的新 enum variant，也不复制 App 中已有的 roster 查询。当前 `ListAgents` 已带所有必需字段，唯独 `workspace_id` 可从经过服务端验证的 `NORMA_WORKSPACE_ID` 加入输出。

只有当未来外部客户端也必须直接调用 self API 时，才选择“新增 `CardCommand::GetSelfAgent` + `CardCommandResult::SelfAgent`”的 wire 版本；那时必须显式升级协议并处理旧 App 在解码未知 enum 前无法返回 `client_upgrade_required` 的兼容性问题。

### 7.3 具体改动位置

| 文件与当前行号 | 改动 |
|---|---|
| `crates/norma/src/cli.rs:23-29,85-98,307-358,428-434` | 增加 `agent self` 与 `CliSuccessView::AgentSelf`；命令仍发 `ListAgents`，不接受 `--workspace`。 |
| `crates/norma/src/cli_runtime.rs:43-85,155-173` | 响应成功后做纯投影；从已验证的进程上下文读取 node/workspace，并处理 0/多项匹配。 |
| `crates/norma/src/cli/output.rs:9-65,150-174,238-268` | 为 self 投影输出稳定 JSON/text，不修改普通 `agent list` 格式。 |
| `crates/norma/src/client.rs:165-188,298-314` | 复用现有来源上下文；建议抽出可测试的 `source_identity_context_from_values`，避免测试直接修改全局环境变量。 |
| `crates/norma/src/orchestration/skill.rs:66-80` | 同 Workspace assign 前先 `agent self`，再 `agent list`；明确名称/alias/Workspace 比较规则和查询失败时停止。 |
| 方案一所列的 `task_dispatch.rs`、`command_protocol.rs` | 保留指令性 `self_dispatch_forbidden` 作为漏查兜底。 |

该兼容实现不需要修改 Ruby 初稿列出的 `crates/norma/src/protocol.rs` 与 `crates/app/src/app/agent_runtime/commands.rs`。这是刻意减少协议表面积，不是遗漏。

### 7.4 Skill 的精确决策规则

```text
1. 若用户说“你/你自己/当前 Agent”，直接视为 self，不调用 assign。
2. 同 Workspace 具名派发前运行 agent self；每次副作用前重新查，不跨改名缓存。
3. 将目标与 self.name 做英文大小写无关匹配，并与实际 self.speech_alias 匹配；
   容忍系统已支持的首尾空格和 ,，:：。!！。
4. 若命中 self：不生成 operation_id，不调用 assign，把嵌套包装之后的剩余正文作为当前任务。
5. 若显式目标 Workspace 与 self.workspace_id 不同：同名不代表 self，按跨 Workspace 流程解析。
6. self 查询失败：停止派发并报告无法确认来源身份；只对 orchestration_checking 做有界只读重试。
7. 若兜底收到 self_dispatch_forbidden / delegation_cycle / delegation_budget_exceeded：立即停止，
   不换名字、不换 Workspace、不换 operation_id。
```

### 7.5 实施步骤（TDD）

1. CLI 解析失败测试：`agent self` 映射 `ListAgents + AgentSelf view`，默认 JSON，且拒绝 `--workspace`。
2. 纯投影测试：roster 中 1 个 node 命中返回该 identity；0 个返回 `source_agent_unavailable`；2 个命中返回内部一致性错误；不得按 name 猜。
3. 输出契约测试：`result.self` 字段完整，不泄露 token，不把整个 roster 一并输出。
4. Skill 契约测试：每个同 Workspace assign 前有 self 查询；跨 Workspace 不用同名做 self 判定；self 查询失败和三个确定性错误均停止。
5. 接入方案一的统一错误兜底。
6. E2E 覆盖规范名、语音 alias、活跃改名、跨 Workspace 同名、嵌套 self、祖先 cycle。
7. 逐 Provider L3 抽查模型是否遵循“先 self、后 list、再 assign”的顺序；服务端零副作用断言仍由自动 E2E 负责。

### 7.6 风险与验证

| 风险 | 影响 | 缓解 |
|---|---|---|
| 模型跳过 self 查询 | 仍会有第一次错误调用 | 方案一兜底。 |
| 每次派发多一次 roster 查询 | 小幅本地延迟 | 查询走本地 Unix socket、无 SQLite 热路径新增；正确性优先，不跨 turn 缓存。 |
| roster 中找不到当前 node | 无法自我介绍 | 返回稳定错误并停止，不猜；检查 coordinator/runtime 同步。 |
| CLI 新命令但旧 Skill | 模型不会主动调用 | Skill 精确校验触发修复；运行时兜底先独立上线。 |
| 显式跨 Workspace 同名 | 可能被模型误判 self | self 输出包含 workspace_id；Skill 明确只在相同 Workspace 比较。 |

## 8. 边界情况决策表

| 场景 | 正确行为 | 主要证据层 |
|---|---|---|
| “浩宇，派给浩宇” | 当前卡片直接处理；若误调 CLI，返回含“浩宇就是你”的 self error。 | Skill + runtime guard |
| “Alan，派给艾伦”且 Alan 的持久化 alias=艾伦 | 视为 self。 | `speech_alias` + definition guard |
| 迁移后 Alan 的 `speech_alias=None`，另有正式名“艾伦” | “艾伦”不是 Alan 的 self alias；不得重新推导。 | storage persisted alias |
| 同 Workspace 两个大小写不同的 Alan | 创建/改名阶段已拒绝；运行时不处理歧义。 | name_key unique |
| Workspace A/浩宇 -> Workspace B/浩宇 | 允许；名称相同但 Workspace 不同。 | workspace + definition identity |
| A -> B -> A | 返回 `delegation_cycle`，停止重试。 | global/local lineage |
| A -> B，B 收到“再派给 B” | B 识别 self 并直接处理剩余任务。 | each-hop self query |
| A -> B，B 收到“再派给 A” | A 是祖先，不是 B 自己；cycle guard 拒绝。 | ancestor lineage |
| 一次命令含多个 assignments，其中一个是 self | 整批 preflight 原子拒绝，不产生部分任务。 | coordinator preflight |
| 活跃 Agent 改名 | 方案一/三使用最新存储值；方案二的旧 Session 身份可能陈旧。 | live roster query |
| 来源卡片已关闭/节点无效 | `source_agent_unavailable`，不查询、不派发。 | source validation |
| 用户提示要求忽略身份 | 模型层可能受影响，但 definition guard 仍拒绝。 | runtime invariant |

同 Workspace 的名称/别名唯一性已经由 `crates/storage/src/agent_definition_store.rs:271-306` 保证，且正式名与语音别名共享冲突命名空间。迁移遗留冲突会把 alias 持久化为 `None`，因此所有方案都必须读取实际 alias。

## 9. 三套方案对比

| 维度 | 方案一：指令性拒绝 | 方案二：Hook 注入 | 方案三：CLI 自查 + 兜底 |
|---|---|---|---|
| 阻止第一次错误调用 | 否 | 支持且身份未陈旧的 Provider 上可以 | 大概率可以，取决于模型遵循 self 查询 |
| 保证零派发副作用 | 是，运行时已有 guard | 仍依赖运行时 guard | 是，运行时 guard 兜底 |
| 消除换 operation_id 循环 | 高，但依赖模型读错误 | 高，但依赖注入到达模型 | 最高：主动查询 + 错误兜底 |
| 跨 Provider 一致性 | 高 | 低/中，需要四套适配 | 高，复用统一 CLI/socket |
| 活跃改名后的新鲜度 | 高 | SessionStart 方案低 | 高，每次派发前查询 |
| 协议变更 | 无 | Provider Hook 协议 | 推荐实现无 wire 变更 |
| 可自动化测试性 | 高 | 中/低，必须真 Provider 探针 | 高 |
| 实现复杂度 | S | L | M |
| 长期复用价值 | 中：统一确定性错误 | 中：身份上下文基础设施 | 高：自我介绍、诊断、审计都可复用 |

没有一套纯提示方案能从数学上保证模型“永远不再发第二个请求”。系统能严格保证的是：每次自派发均被拒绝且零副作用。若产品还要求 UI 上完全看不到重复尝试，应另立 P2 加固：按 `(workspace_id, source_node_id, target_definition_id, error_code)` 记录短时拒绝次数并熔断/聚合展示。该加固不应替代三套方案中的身份认知。

## 10. 推荐与落地优先级

### 推荐：方案一 + 兼容性优化后的方案三

理由：

1. **安全边界最深。**方案一继续由 definition/Workspace 做权威拒绝，方案三只改善模型决策，不把安全性交给提示词。
2. **跨 Provider 最稳定。**四类 Provider 都能调用相同的 `norma` CLI；不用等待各家 Hook 注入契约收敛。
3. **对改名和 alias 正确。**每次派发前读取最新 roster，且尊重持久化 alias=None 的迁移语义。
4. **可以不改 wire protocol。**`agent self` 复用现有 `ListAgents`，避免未知 enum 的旧 App 解码失败问题。
5. **验证闭环清楚。**CLI 投影、Skill 规则、两条运行时错误和零副作用都能做确定性测试；真实 Provider 只负责验证模型遵循率。

### 优先级

| 优先级 | 交付 | 完成定义 |
|---|---|---|
| P0 | 方案一公共层 | 两条错误路径同文案；Skill 停止规则；self/alias/cycle 零副作用 E2E。 |
| P1 | 方案三 | `agent self` 稳定输出；每次同 Workspace assign 前自查；改名与跨 Workspace 用例通过。 |
| P2 | 重复拒绝可观测性 | 结构化 attempt 日志/计数；不记录任务正文；确认是否还需短时熔断。 |
| P3 | 方案二体验增强 | Claude/Codex/Cursor/OpenCode 各自探针通过后按 Provider 启用，不作为公共能力前置。 |

## 11. 验证矩阵

| 层级 | 必测内容 | 建议位置 |
|---|---|---|
| agent unit | self、整批原子拒绝、A->B->A cycle | `crates/agent/tests/agent_delegation_tree_tests.rs` |
| storage unit | 同 Workspace self、跨 Workspace 同 definition 数字不算 self、祖先环 | `crates/storage/tests/unit/orchestration_task_route_store_tests.rs` |
| norma unit | self CLI 投影 0/1/2 命中、JSON/text、无 token | `crates/norma/tests/orchestration_cli_tests.rs` |
| Skill contract | self 查询顺序、alias/Workspace、三个确定性停止码 | `crates/norma/tests/orchestration_skill_contract_tests.rs` |
| Hook unit | 每 Provider golden、非 SessionStart 静默、fail-open | `crates/norma/tests/agent_hook*_tests.rs` |
| app unit | 全局/本地错误身份一致、最新改名、retryable=false | `crates/app/tests/unit/` 新聚焦文件 |
| e2e L2 | task route/local queue/PTY 零副作用；跨 Workspace 同名可派发 | `crates/e2e/src/e2e_12_agent_self_dispatch.rs` |
| e2e L3 | 真 App + 各 Provider：知道自己、无二次 assign | 独立 ignored runner；明确标注 Provider/版本 |

所有新增 Rust 测试必须遵守项目的完整场景注释、数值推导、正负断言和逐条错误说明规范。

## 12. 供用户选择

- **A：只实施 P0 方案一。**最快止血，但第一次错误调用仍会发生。
- **B：实施 P0 + P1（推荐）。**获得主动自查、跨 Provider 一致性和运行时兜底。
- **C：实施 P0 + 方案二。**仅建议在明确限定 Provider 且先完成真 CLI 探针时选择。
- **D：P0 + P1 后，再把方案二作为按 Provider 开启的增强。**最终体验上限最高，但不应把 Hook 适配放到主链路首批。
