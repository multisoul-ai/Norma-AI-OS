# Norma iOS G1 · Provider 接入验证

2026-09-07 · 状态：G1 接入验证完成（v12）；能力边界见末节。G2–G5 仍需实施和验收。

## 早期独立协议证据（正式 v12 结果见末节）

| 检查 | 结果 | 实际边界 |
|---|---|---|
| 本机 CLI | Codex 0.152.1；Claude Code 2.1.202 | 未升级 CLI 或修改用户全局配置 |
| iOS 构建工具 | Xcode 26.2；本机有 iOS 26.3 Simulator | 不等于有已连接真机、签名或 APNs 配置 |
| Codex 流式聊天 | 通过 | 独立 app-server stdio 会话，收到 7 个文本增量，完整历史包含用户与 Agent 两条消息 |
| Claude 流式聊天 | 通过 | SDK stream-json stdio 会话，收到 2 个文本增量和完整回复 |
| Claude 拒绝审批 | 通过 | 收到具体 `can_use_tool` 请求并返回 deny，目标临时文件未创建 |
| Claude 批准审批 | 通过 | 收到具体 `can_use_tool` 请求并返回 allow，目标临时文件内容与预期完全一致 |
| Codex 客户端审批 | 隔离测试通过 | 用户允许测试进程暂不加载外部 Hook；真实批准写出精确内容，拒绝不写文件，均收到准确 `serverRequest/resolved` |
| Claude 交互式终端 Hook | 批准 / 拒绝通过 | 同一个真实 PTY 会话中的两轮 Write；Hook session_id 相同，拒绝未写，批准精确写入，无需改成 SDK 界面 |
| Claude 历史来源 | 单会话读取通过 | 只读取自建测试会话 Hook 提供的 transcript_path，包含 9 条用户 / Agent 记录；分页与增量读取器待实现 |
| Codex 原生终端附着 | 通过 | `codex --remote … resume <测试 threadId>` 展示初始历史、API 发起的后续回合和拒绝结果；不是既有 Norma 卡片迁移验证 |
| Codex 两客户端竞争 | 通过 | 两个独立 WebSocket 连接附着同一测试线程；A 拒绝后两端收到同一请求的 resolved，B 迟到的 allow 未执行命令 |
| Codex 历史分页 | 通过 | 两轮真实历史按每页 1 轮读取，游标定位正确且无重复；`itemsView: full` |
| Codex / Claude 取消后迟到批准 | 通过 | Codex interrupt / resolved、Claude interrupt / control_cancel_request 后再发 allow，临时文件均未创建 |
| Claude 同连接冲突重复响应 | 通过 | 同一 request_id 先 deny 后 allow，未执行；不等同于桌面与手机竞争 |
| 既有 PTY 会话与手机同时控制 | 待验证 | 上述独立协议会话不证明可附着现有终端会话 |
| Claude 桌面先拒绝 | 通过 | 真实终端选择 No，延迟 Hook 等待随后释放，文件仍不存在；未观察到 Hook 返回输出，不声称迟到 allow 已被 Provider 接收 |
| Claude Hook 超时 | 通过 | Hook 睡眠 3 秒、CLI 上限 1 秒，Hook 被停止，原生审批仍待用户处理，文件未创建；这不等于已实现 Norma 请求 TTL |

实际模型：Codex 测试服务器返回 `gpt-5.6-sol`，原生 TUI 显示 xhigh；Claude Code TUI 显示 `ark-code-latest`（用户已有配置）。Claude 结果验证的是 Claude Code 协议运行时，不能表述为已验证 Anthropic Claude 模型。

可重跑脚本：[probe-norma-mobile-providers.py](../../scripts/probe-norma-mobile-providers.py)。保存结果见 [结构化证据](2026-09-07-norma-ios-provider-evidence.json)。

```sh
python3 scripts/probe-norma-mobile-providers.py --provider codex --case chat --output /tmp/norma-ios-provider-codex-chat.json
python3 scripts/probe-norma-mobile-providers.py --provider claude --case chat --output /tmp/norma-ios-provider-claude-chat.json
python3 scripts/probe-norma-mobile-providers.py --provider claude --case approval-deny --output /tmp/norma-ios-provider-claude-deny.json
python3 scripts/probe-norma-mobile-providers.py --provider claude --case approval-accept --output /tmp/norma-ios-provider-claude-accept.json
python3 scripts/probe-norma-mobile-providers.py --provider codex --case approval-deny --isolate-codex-hooks --output /tmp/norma-ios-provider-codex-isolated-deny.json
python3 scripts/probe-norma-mobile-providers.py --provider codex --case approval-accept --isolate-codex-hooks --output /tmp/norma-ios-provider-codex-isolated-accept.json
python3 scripts/probe-norma-mobile-providers.py --provider codex --case approval-cancel --isolate-codex-hooks --output /tmp/norma-ios-provider-codex-isolated-cancel.json
python3 scripts/probe-norma-mobile-providers.py --provider claude --case approval-duplicate --output /tmp/norma-ios-provider-claude-duplicate.json
python3 scripts/probe-norma-mobile-providers.py --provider claude --case approval-cancel --output /tmp/norma-ios-provider-claude-cancel.json
node scripts/probe-norma-codex-multiclient.cjs --isolate-codex-hooks --output /tmp/norma-ios-provider-codex-multiclient.json
python3 scripts/check-norma-provider-probe.py
```

脚本使用现有 CLI 登录，每次创建独立临时目录与测试会话；只批准精确匹配测试文件和内容的请求，不返回会话级或持久权限。重复 / 取消实验只对同一已决请求发送冲突响应，验证无副作用。超时、未知请求和不匹配目标均失败。子进程退出时 terminate / wait，必要时结束该验证进程组。Codex 测试线程在检查后归档；Claude SDK 验证禁用会话持久化，真实 PTY 验证保留其自建测试历史。

离线校验覆盖 shell 包装中的命令等价性；额外命令、不同路径、未知 shell、额外参数和错误引号全部拒绝。Node 多连接实验可加 `--wait-for-tui`，在输出测试地址与 threadId 后，另开终端按上述 remote resume 命令连接，再向检查进程按回车继续。这个 PTY 操作仅为协议证据，不替代正式页面的 Computer Use 验收。

## 发现的接入差异

1. 当前 [新建 Agent](../../crates/runtime/src/nodes/agent.rs) 和 [恢复会话](../../crates/runtime/src/session_snapshot.rs) 都用跳过权限检查的 CLI 参数。要产生真实待审批事项，启用手机审批的会话需要使用会产生权限请求的模式；不能只添加手机按钮。
2. 初始审计时 [Hook forward](../../crates/norma/src/agent_hook/forward.rs) 只判断 HTTP 2xx，[接收端](../../crates/app/src/settings/agent_hook_ingress.rs) 只返回无正文状态码。官方 Codex / Claude Hook 均支持 `PermissionRequest` 返回 `hookSpecificOutput.decision.behavior = allow / deny`；需要实测并接通双向响应。
3. 现有 [会话摘要](../../crates/agent/src/conversation_tail.rs) 只保留最后消息且截断到 1200 字符。完整聊天需要额外的可靠消息来源，不能直接扩大摘要字段后视为完成。
4. 本机 Codex 的全局 Vibe Island PermissionRequest Hook 配置等待上限为 7200 秒，排在客户端审批前。用户已明确允许隔离验证，脚本仅对测试子进程使用 `--disable hooks`，保留 read-only 沙箱和 untrusted 审批策略，不改全局配置。该授权不表示可以在正式产品中关闭外部 Hook。
5. Codex 空线程尚未写入 rollout 时，第二客户端 `thread/resume` 返回 `no rollout found`；第一轮完成后附着成功。空闲卡片应保留为 Norma 的卡片元数据，安排任务时才创建 Provider 会话，避免为卡片初始化发送假任务。
6. Codex 两客户端 / TUI 实验中，被拒绝的 commandExecution 出现在实时事件，却未出现在完整历史。Norma 必须记录决策请求、处理端与结果，不能靠回读 Provider 历史重建审批审计。
7. Claude SDK 的 PermissionRequest Hook 实验落回 can_use_tool；真实交互式 PTY Hook 则能批准与拒绝。这是两种运行模式的差异。PTY Hook 输入本次含 session_id、prompt_id、工具与输入，未含稳定 tool_use_id；请求必须绑定一次 Hook 调用及其生命周期，不能按命令文本去重。
8. Claude 取消时发出准确的 control_cancel_request 和成功的 interrupt 回执，但最终 result 为 `error_during_execution`。适配层应结合主动取消状态显示“已中断”，不能仅凭 result.is_error 把它算成执行失败。

## 已确认的正式接入方案（v4 记录）

2026-09-07 用户明确选择“按此方案实施”：只让新建且启用手机控制的卡片使用受控启动方式，已有运行会话保持原状，任务结束后由用户显式重开。卡片创建本身不启动任务、不调用模型。

| 接缝 | 当前行为 | 建议变化 |
|---|---|---|
| Codex 启动 | PTY 直接执行 CLI，跳过沙箱与审批 | Norma 窗口持有本地 app-server，原生 Codex TUI 附着同一会话；手机命令由 Norma 校验后调用协议 |
| Claude 启动 | PTY CLI 跳过审批 | 保留 PTY 和原生终端，使用会产生权限请求的模式；现有 Hook 升级为有请求生命周期的双向回传 |
| 审批策略 | 不产生正常权限询问 | 按 Provider 权限策略处理需要确认的操作；具体请求只允许一次，不扩为全会话授权 |
| 旧卡片恢复 | 直接带 bypass 参数 resume | 保存卡片运行模式；不把旧卡片静默迁移为新模式，也不同时启动同一个会话的第二个执行器 |
| 聊天 / 结果 | 1200 字符摘要 + PTY | Codex 读取协议分页历史与事件；Claude 从精确绑定的 transcript 读取完整消息，先验证分段更新和取消 |
| 窗口关闭 | kill / wait 现有 PTY | 一并结束该窗口持有的 app-server / Hook 等待者，旧运行实例立即失效；不使用系统常驻 daemon |

正式接入保留现有外部 Hook 和管理策略。若外部 Hook 正在处理请求，手机应显示真实处理来源和受限能力，不能声称该请求已由 Norma 接管。当前机器的 Vibe Island 冲突仍需单独兼容验证；隔离测试通过不代表用户配置下的手机审批已经通过。

后续需要接入 `crates/runtime/src/nodes/agent.rs`、`crates/runtime/src/session_snapshot.rs`、`crates/norma/src/agent_hook/forward.rs`、`crates/app/src/settings/agent_hook_ingress.rs` 及窗口生命周期。其中 Claude 受控启动与双向 Hook 已在 v5 接通，详见下节；Codex host 与 Claude 正式审批执行已在 v6 接通，完整消息同步及 Codex 正式审批仍未完成。

第一段代码已落地：`AgentSpec.mobile_control` 持久化且兼容旧快照；`Runtime::create_mobile_agent_card` 创建空闲卡片；旧恢复入口不启动受控卡片；共享 `bind_discovered_agent_session` 拒绝将桌面历史发现绑定到手机卡片。聚焦测试先验证失败再实现，另有真实 SQLite 创建 / 重开 / 删除集成检查。用户尚不能通过此段代码从手机派发任务。

## 正式代码进展 · 2026-09-07 v5

已新增 `norma::agent_hook::HookApproval` 与独立 PermissionRequest 传输，保留原观察 Hook；Claude 单次输出只含 allow / deny，不允许更新全局规则或工具输入。入口限 loopback、请求/响应大小有界，安全随机身份生成失败时关闭入口能力，没有时间随机数回退。

App 后台准备 Claude 原生 PTY，使用本进程 `--settings` 添加 Hook 并保留原有配置。运行实例、session 和 `prompt_id` 共同限定审批；过期、已取消、重复决定和上一回合请求被拒绝。等待线程只管理有界非阻塞 socket，5 个等待请求不会耗尽 4 个普通 HTTP workers。接收端观察到 EOF 后，尚未提交发送的 allow 会被撤销。HTTP workers 显式恢复阻塞读取并保留 2 秒超时，避免 macOS 继承非阻塞标志后将分包误判为坏请求；该问题已有失败→修复→通过的聚焦证据。

Runtime 复用现有 `TermLive` / `PtyHandle` 生命周期；关窗清空控制实例，删除卡片或工作区换代后后台结果不能复活节点。空闲会话不保存无法恢复的历史身份，首条真实消息到达后才持久化绑定。

证据范围：

- `cargo test -p norma -p runtime`：域层与真实 Hook bridge 进程测试通过；包含分包响应、单次决定、会话/回合隔离、窗口关闭及晚到启动。
- `cargo test -p app --lib agent_hook`：9 项真实 loopback 接收端 / 并发 / 注销 / EOF / 分包检查通过；另有 10 轮共 30 次传输复验通过。
- `env PATH="$PWD/target/debug:$PATH" cargo test -p e2e e2e_mobile_controlled_claude_native_launch -- --ignored --nocapture`：**headless GPUI + 真实 Claude CLI** 启动通过。仅启动，不发送模型提示；不是实际 `.app` 或手机验收。
- 完整 E2E 仍有 5 项 Chrome auth / 语音失败，未解决。`cargo test --workspace` 另在 App 单测因 Packager 缺少 `production-data` 的断言失败（788 通过、1 失败、7 忽略）；没有 HEAD 基线复验，不声明全仓通过。

Claude 受控路径依赖 `prompt_id`（官方说明 v2.1.196+）；缺少准确回合身份时不接受移动端审批，保留原生确认回退。当前用户 CLI 为 2.1.202。v5 当时尚未完成正式任务与审批执行；此项在 v6 已由正式 App / Runtime 链路取得执行证据，见下节。

## 正式代码进展 · 2026-09-07 v6

Claude 正式 `App → Runtime → CLI → Hook → 决策 → 执行` 检查已通过，先拒绝后批准两条独立临时文件写入，按磁盘效果验证。命令只允许精确 printf，不相等立即拒绝；重复决定失败，等待真正完成后再关窗。使用 Claude Code 2.1.202 / ark-code-latest；未修改或关闭全局 Hook。

Codex 使用当前 CLI 支持的 `unix://PATH` WebSocket，在 0700 临时目录中创建专用 socket。Norma 通过已有 PTY 进程树生命周期管理 host，后台持续消费输出；窗口拥有取消身份，关闭、删除、准备失败时会回收。正式会话使用 read-only / untrusted / user。用户真实首条任务生成 rollout 后，第二客户端成功 resume 才附着原生 TUI；不为空闲卡片发送假任务。

`CodexConnection` 的真实 Unix WebSocket 检查覆盖初始化、同时在途的逆序回复、4000 字符完整正文、服务端请求原始 ID、断开拒绝新写入、事件队列溢出与在途错误。正式 Runtime 和 App 入口已验证真实任务、TUI、Done 与分页历史；历史断言要求 `agentMessage` 包含精确回复文本，不会把 userMessage 的提示文本误认作模型回复。生命周期修复后，强化断言的真实检查分别在 8.53 / 7.65 秒内通过。审批事件目前只推进状态，尚未实现 Runtime 请求记录、正式响应或持久化审计。传输的 `respond` 通过不等于正式审批闭环通过。

发现并修复两处问题：原生 CLI 不接受协议接口支持的 `untrusted` 参数值，TUI 因此提前退出；现在审批策略只在服务端会话配置，TUI 只附着。受控进程关闭时若没有最终 Hook，状态可能残留 Thinking；共享关闭入口现将未结束任务置为 Ended，并保留 Done / Failed 结果。

最后一轮检查曾发现停止态孤儿 host：只有后台取消标志不足以保证应用退出前执行完整进程树回收。现通过 `LaunchControl` 跟踪 host / 原生终端的已有 `PtyHandle`，让创建和取消共享进程所有权；取消返回前发出完整终止信号，输出和临时目录清理留在后台。无论后台准备未附着、用户关闭窗口，还是所属进程随后立即退出，均不依赖后台线程抢到调度。独立子进程 `kill_all → exit` 检查通过；本轮已清理明确识别的测试残留进程。

生命周期修复后的 Claude 审批复验曾两次失败。终端证据为模型在调用工具前拒绝：它把 `NORMA_MOBILE_APPROVED` / `denied.txt` 误判为权限标记，协议状态是 Done，因此没有 PermissionRequest。测试改成普通 `hello-from-norma` 文本和两条独立临时文件路径，随后真实拒绝 / 批准和磁盘效果在 24.11 秒通过；没有更改审批策略、跳过 Hook 或扩大权限。测试现在遇到本轮已结束却未产生审批时会立即失败。两种 CLI、共享进程退出测试、域层、App check、护栏和 Release 构建均已通过；真实 App / iPhone 仍未验收。

可重跑命令：

```sh
cargo test -p norma --test codex_connection
cargo test -p runtime --test mobile_codex_session
cargo test -p runtime controlled_host_is_killed_before_owner_process_exits
cargo test -p runtime --test mobile_codex_session controlled_codex_owns -- --ignored --nocapture
cargo test -p e2e e2e_mobile_controlled_codex_dispatch -- --ignored --nocapture
env PATH="$PWD/target/debug:$PATH" cargo test -p e2e e2e_mobile_claude_permission_executes_only -- --ignored --nocapture
```

这仍是 headless GPUI / 域层 + 真实 CLI，没有真实 `.app`、iPhone 或跨机连接证据。完整 E2E 当前为 197 通过、5 失败、7 忽略；全仓测试在 App 的 Packager `production-data` 断言停止（789 通过、1 失败、7 忽略）。尚无未修改 HEAD 基线，不能认定失败与本改动无关。

## 正式代码进展 · 2026-09-07 v7

Codex Runtime 已接入具体请求审批和项目数据库审计。手机选择先变为 Chosen，后台 SQLite 提交成功后，Runtime 再核对请求仍然有效才发送原始 RPC ID 的响应。Submitted 只表示本地入队；Resolved 只表示 Provider 清理请求，不能推断胜方、最终选择或工具结果。每次选择和提交都保存准确的 Provider 响应。

真实 Unix WebSocket + Runtime 检查覆盖错误运行、桌面先处理、重复决定、TTL、旧回合及 ID 复用；SQLite 检查覆盖重开、项目隔离、幂等、冲突回滚、占锁与写失败。占锁期间不会提前发响应，写失败会停止受控通道；桌面在落盘期间先处理后也不会补发。选择不在重启时自动重放；异常退出缺失的最终结果仍需回查 Provider。

本机 CLI 本轮已升为 **Codex 0.153.4**。真实审批提供 accept / execpolicy amendment / cancel，未提供 decline。现在拒绝优先选择 decline，只有 cancel 时选择拒绝并结束回合；单次批准绝不改成 session 或 execpolicy 授权。由此复现了共享状态入口只允许 UserPromptSubmit 恢复 Ended 的问题，已让准确的 turn/started 也恢复新任务，保留迟到工具事件防护。

真实 Runtime 检查在 **36.57 秒**通过：first.txt 拒绝后不存在，second.txt 批准后内容为精确 `hello-from-norma`，third.txt 在 Provider 中断后不存在，旧请求无法再次批准，数据库保留独立阶段。测试仅按此前授权在临时 host 关闭外部 Hook；正式代码不禁用 Hook，用户全局配置不变。取消由测试直接调用 Provider RPC 后验证 Runtime 失效，公开手机中断指令仍待接入。App 正式启动入口的 headless GPUI + 真实 CLI 无工具消息检查通过（8.75 秒）。这些不是实际 `.app` / iPhone 验收。

```sh
cargo test -p norma --test codex_approvals
cargo test -p storage --test approval_audit
cargo test -p runtime --lib runtime::mobile_cards::codex::tests
cargo test -p runtime --lib real_codex_runtime_approval_audit_and_cancel -- --ignored --nocapture
```

v7 回归：领域测试合计 291 通过、2 忽略；末次无回合 ID 通知不得覆盖 Done 的回归及完整 Runtime、App check 通过。E2E 为 197 通过、5 失败、7 忽略；全仓测试仍停在 Packager `production-data` 断言（App 789 通过、1 失败、7 忽略），未做 HEAD 基线，不宣称全绿。最终 Release 增量构建通过（2m 05s），护栏通过。

## 正式代码进展 · 2026-09-07 v8

`Runtime::interrupt_mobile_codex` 现在提供正式中断入口和最终回执。它核验运行与回合，撤销待落盘选择及迟到审批；RPC ack 后仍保持请求中，收到真实终态才返回 interrupted / completed / failed。拒绝中断的错误会终止受控连接，不把失败伪装成已停止。Unix WS + Runtime 检查覆盖锁库、桌面事件、重复请求、自然完成和失败；真实 Codex 三回合检查已改用该公开入口，在 33.04 秒通过。

Claude 完整历史使用 `norma::claude::read_transcript`，从已验证 Hook 的 transcript_path 读取，并核对受控配置根、session 与项目 cwd。保留主会话的完整用户 / assistant 内容与工具块，跳过 sidechain 与非消息元数据。文件游标支持跨回合增量和半行追加；文件替换、截断、非法位置返回重新读取错误。单行超过 8 MiB 明确失败，不冒充完整内容；每页扫描上限为 16 MiB。消息之间保留父 ID，具体聊天呈现及分支选择属于后续 G3。

App 在后台读文件，Runtime 的读取对象与活跃运行绑定；窗口关闭后不能继续使用旧请求。域层检查覆盖 5000 字中文、完整工具输入、半行补全、路径 / 项目 / 会话拒绝。Claude Code 2.1.202 的真实 headless GPUI + CLI 先拒绝后批准检查新增完整历史验收通过（23.62 秒）；进一步在第一回合读取游标、第二回合追加读取并确认不重复旧消息，在 32.36 秒通过。未关闭 Claude 外部 Hook，未改全局配置。这是消息块 / JSONL 级增量，不能声称逐 token 推流，也不能替代实际 .app / iPhone 验收。

```sh
cargo test -p runtime --lib runtime::mobile_cards::codex::interrupt_tests
cargo test -p norma --test claude_transcript
cargo test -p runtime --lib claude_history_binds_verified_hook_and_revokes_inflight_read
cargo test -p runtime --lib real_codex_runtime_approval_audit_and_cancel -- --ignored --nocapture
cargo build -p norma --bin multisoul-agent-hook
env PATH="$PWD/target/debug:$PATH" cargo test -p e2e e2e_mobile_claude_permission_executes_only -- --ignored --nocapture
```

v8 最终回归：Norma / Runtime 合计 **227 通过、2 忽略、0 失败**；App check（5.53 秒）和 Release（2m 44s）通过。行数 / no-allow / 存储 / 性能 / diff 护栏通过，域层依赖无 GPUI，没有遗留本次 Codex host。完整 E2E 为 **197 通过、5 失败、7 忽略**：两项 Chrome auth、MiniMax manual voice、large voice catalog 及 legacy bilingual install retry（后两项锁 poisoned）；全仓在 App Packager `production-data` 断言失败，App 为 789 通过、1 失败、7 忽略。未建立未修改 HEAD 基线，不能归因这些失败。

## 正式代码进展 · 2026-09-07 v9

第六段正式代码（v9）：

- Claude 复用项目 `BackgroundApprovalAudit`，后台启动时打开真实项目库；缺少审计库则拒绝受控启动。没有新建另一套存储，也不修改全局 Hook。
- `HookApproval` 增加 Chosen 阶段。选择先持久化；Runtime 消费提交成功回执、再核验请求状态，才转为 Decided 交给 Hook。重复、错误运行、超时、桌面断开、Stop 和关窗均不能复活旧选择。提交失败结束受控运行。
- 审计保留 Pending / Chosen / Decided / Responded / Cancelled 与精确会话、回合、调用及 Provider 输出。Responded 仅表示本地 transport 取出响应，不证明网络送达、竞争胜方或工具执行。异常退出时最后几项后台记录可能尚未提交，未知结果仍须回查真实 Provider。
- 三项 Runtime + 真实 SQLite 检查覆盖锁库、迟到提交、桌面取消、TTL、回合结束后新回合、写失败及关闭。现有生命周期与完整历史回归保留。
- 正式 App / Runtime / Claude Code 2.1.202 / Hook 两轮拒绝和批准，验证精确临时文件结果、每轮四阶段审计及完整增量历史，在 **20.47 秒**通过（此前一轮 21.86 秒）。仍是 headless GPUI + 真实 CLI，不是实际 .app / iPhone 验收。

v9 最终回归：`cargo test -p norma -p runtime` 为 **230 通过、2 忽略、0 失败**；App check 通过（2.90 秒），Release 构建通过（1m 57s）。完整 E2E 为 **197 通过、5 失败、7 忽略**（两项 Chrome auth、MiniMax manual voice 及两项锁 poisoned）；全仓在 App 的 Packager `production-data` 断言失败（App 789 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不能归因这些失败。行数 / no-allow / 存储 / 性能 / diff 检查通过，域层依赖无 GPUI。G1 尚未完成，G2–G5 未实现。

## 正式代码进展 · 2026-09-07 v10

第七段正式代码（v10）：

- `Runtime::enable_mobile_agent_control` 提供旧卡片显式切换：核对原 session、Provider 与空闲状态；拒绝运行 / 等待审批 / 未确认状态和重复切换。成功后回收旧 PTY，保留原卡 ID、别名、任务名称与历史绑定，不派新任务。已明确退出的旧 PTY 即使缺少最终 Hook，也不会被过时 Thinking 永久挡住；App 层仍保留队列检查。
- `MultisoulApp::reopen_mobile_agent` 同时检查任务协调器、待发送任务、会话回复队列及尚未消费的 Hook。前置失败不关闭卡片、不吞掉队列；切换后即使启动失败也保留受控模式，避免重试退回绕过审批的旧路径。
- Codex 在已有准确会话时支持无提示恢复；新会话仍要求真实首条任务。恢复只执行 thread/resume 与原生 TUI 附着，不发送 turn/start。后台审计、只读沙箱和逐次审批配置仍保留。
- 两种真实 CLI 的 App 重开验收通过：Codex **10.31 秒**，恢复前后完整回合数据一致；Claude 两轮拒绝 / 批准 / 审计 / 历史加重开 **28.54 秒**，控制 run 更新、session 保留、旧游标续读没有新增消息。测试以刚验证的真实历史构造旧卡元数据，不声称启动了用户已有的非受控会话，也不是真实 .app / iPhone 验收。
- 原生 Claude 中断验证取得新证据：当前 2.1.202 的 PermissionRequest 界面使用 Escape。Ctrl-C 未撤销审批；Escape 撤销了 Hook，终端回到输入状态；额外观察 5 秒仍无 Stop / SessionEnd，Runtime 仍为 WaitingApproval。JSONL 追加了带准确 promptId 的 tool_result 与 `[Request interrupted by user for tool use]`。因此 **正式 Claude 中断入口仍未完成**，下一步需结合发送前的历史位置、精确回合身份及发送后的新增中断记录确认终态，避免以按键写入或旧文本冒充完成。

v10 最终回归：`cargo test -p norma -p runtime` 为 **232 通过、2 忽略、0 失败**；包含后补的“PTY 已退出但无 Stop Hook”失败→修复验证。重开时的排队回复 / 启动保护 App 测试通过，最后 App check（2.47 秒）和 Release 构建（1m 44s）通过。完整 E2E 为 **197 通过、5 失败、8 忽略**，失败仍在两项 Chrome auth 与三项语音测试；全仓停在 Packager `production-data` 断言，App 为 790 通过、1 失败、7 忽略。没有未修改 HEAD 基线，不归因这些失败；最后纯 Runtime 退出分支修正后重跑了完整领域测试、App check 与 Release。行数 / no-allow / 存储 / 性能 / diff 护栏通过，域层依赖无 GPUI。G1 继续进行，G2–G5 未实现。

## 下一步

- 补 Claude 正式中断，保留原生终端与外部 Hook，并验证正式两端竞争。
- 补原生桌面与正式手机决策入口竞争的集成证据；旧卡重开已接通，真实 .app / iPhone 页面仍待后续。
- 完成 G1 剩余验收后进入 G2；双机、iPhone、配对、SwiftUI 与 APNs 均未实现。

## 官方资料

- [OpenAI App Server](https://learn.chatgpt.com/docs/app-server)：消息、历史、服务端审批请求与 `serverRequest/resolved`。
- [OpenAI Hooks](https://learn.chatgpt.com/docs/hooks)：PermissionRequest 的 allow / deny 输出；多个 Hook 决策时 deny 优先。
- [Claude Hooks](https://code.claude.com/docs/en/hooks.md)：PermissionRequest 输入与决策输出；文档包含比本机更新版本的行为，须以 2.1.202 实测为准。
- [Claude Agent SDK 官方控制协议实现](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py)：`initialize`、`can_use_tool`、`control_response`。

上述文档已实际抓取；本机 Codex JSON Schema 由 `codex app-server generate-json-schema` 导出并用于核对参数。

## 正式代码进展 · 2026-09-07 v11

- 新增 `MultisoulApp::interrupt_mobile_claude` 与 Runtime 正式入口。请求绑定卡片 / run / prompt，先撤销尚未发送的审批；后台捕获完整 JSONL 末尾，前台复核身份后只发送一次 Escape，后台读取新增记录，最后再次核对回合才返回 `interrupted` 并更新 Ended。
- 旧记录、其他回合和用户输入的同名字面文本不能作为中断证据；发送前遇到半条记录明确失败并要求重试，发送后的半条记录保留游标等追加。文件替换、截断、读取失败、换回合、超时和关窗不能伪造成功。自然 Stop 返回 `completed`，StopFailure 返回 `failed`；丢弃后台请求不会永久占用中断入口。
- 保留原生终端，文件读取和轮询均在 GPUI 后台。App 在发送与接受结果前检查待同步 Hook，让常规帧循环先处理已到达的新回合事件及其协调副作用。
- 真实 Claude Code 2.1.202 / ark-code-latest：等待审批时的正式中断在 **9.87 秒**通过；生成中与已执行工具的两项中断合计 **30.57 秒**通过。后者先批准精确临时命令、观察 started 文件，然后中断，继续观察 6 秒确认原定 sleep(5s) 后的 output 未写出。三种场景均返回准确 prompt 的 `interrupted` 与 Ended；仍为 headless GPUI + 真实 CLI，不是实际 .app / iPhone。
- 已验证的限制：在真实回复出现前发 Escape（分别在 UserPromptSubmit 后 1s / 5s）会退回输入框，但没有 Stop Hook 或合格的新增中断记录。正式入口在 30s 后返回“结果未知”，保留最后已知回合状态，不伪造 Ended。生成测试因此以实际回复首行作为开始证据；此限制须进入 G3 的命令回执展示，不能把未知状态显示为已停止。

本轮领域回归：`cargo test -p norma -p runtime` **236 通过、2 忽略、0 失败**；新增 transcript 边界检查与三项 Runtime 竞争 / 失败检查均有失败→实现→通过记录。App check 通过（4.30 秒），行数 / no-allow / 存储 / 性能 / diff 护栏通过。最终 Release 构建通过（1m 55s）。完整 E2E 为 **197 通过、5 失败、10 忽略**，失败在两项 Chrome auth 与三项语音用例；全仓停在 Packager `production-data` 断言（App 790 通过、1 失败、7 忽略）。没有未修改 HEAD 基线，不能把这些失败归因为既有或与本次无关。G1 仍需正式多端竞争与外部 Hook 兼容证据；G2–G5 未实现。

可重跑入口：

```sh
cargo test -p norma --test claude_transcript
cargo test -p runtime --lib claude_interrupt_tests
# target/debug 中须有已编译的 multisoul-agent-hook；不修改用户全局配置。
env PATH="$PWD/target/debug:$PATH" cargo test -p e2e --lib e2e_mobile_claude_interrupt -- --ignored --nocapture --test-threads=1
```

初次真实检查未设置测试 Hook 的 PATH，SessionStart 在 30s 内未就绪；按既有测试命令补入 target/debug 后通过。生成前的两次真实失败保留在 `/tmp/norma-ios-claude-phases-v11.log` 与 `/tmp/norma-ios-claude-phases-v11-generating.log`，不当作生成中中断的通过证据。成功记录：`/tmp/norma-ios-claude-native-interrupt-v11-retry.log`、`/tmp/norma-ios-claude-active-phases-v11.log`；领域记录 `/tmp/norma-ios-domain-v11.log`。

## 正式代码进展与接入结论 · v12

第九段正式代码与 G1 结论（v12）：

- Codex 通过 `hook/started` / `hook/completed` 追踪当前线程、当前回合的同步 PermissionRequest Hook，保留来源类型与文件，不暴露 Hook 命令或输出。外部等待不生成可批准请求；只有 Provider 真实 requestApproval 才进入手机决策 API。完成、换回合与关闭清空旧等待，异步和旧回合事件不混入当前状态。
- 保留本机全部外部 Hook 的正式 `prepare → attach → Runtime` 检查在 **16.15 秒**通过：观察到用户 Hook 的真实等待来源，尝试以 Hook ID 批准被拒绝，正式中断得到准确 `interrupted`，目标文件不存在。未关闭或修改外部 Hook。当前配置含 7200 秒的 Vibe Island Hook；本次证据定位到全局 Hook 文件，不把通知 ID 当成插件胜方的证明。
- Codex 正式 Runtime / 项目审计与另一独立协议客户端竞争通过：另一端先拒绝，或手机已选择 Allow 但 SQLite 提交被延迟时另一端先拒绝，均不补发手机批准、不产生目标文件；保留 Chosen 意图与真实 resolved，不能把 resolved 当作已执行。连同已有拒绝 / 批准 / 中断三回合，五回合检查 **53.09 秒**通过，原生 TUI 保留。仅该隔离审批 host 按既有授权关闭外部 Hook。
- Claude 的正式 App / Runtime / Hook / 原生终端竞争在 **7.07 秒**通过：桌面 Escape 撤销具体 Hook 后，手机迟到 Allow 返回 false，没有响应和磁盘写入；用户外部 Hook 保留。锁库、TTL、晚到提交与关闭仍由 v9–v11 的聚焦状态机及传输检查覆盖。
- G1 退出条件已有证据：两种 Provider 的准确会话消息、完整历史来源、增量读取、真实审批与竞争、旧卡显式重开、原生终端和关闭回收。接入方案已由用户确认。**G1 完成验证，进入 G2；不代表 iPhone / 实际 .app / 双机网络已验收。**

已验证的能力边界继续保留：Codex 外部 Hook 有优先处理权，手机仅能处理实际交给客户端的请求；Claude 生成前中断可能返回结果未知，桌面原生中断后的历史同步将在 G3 接齐；Claude 目前为完整 JSONL 消息块增量，非逐 token 推流。上述差异需要在手机能力与回执中明确展示。

协议依据：[OpenAI 官方 App Server 文档](https://learn.chatgpt.com/docs/app-server)，同步 Hook 发送 `hook/started` / `hook/completed`；异步 Hook 不发送此通知。实现字段由本机 Codex 0.153.4 的 `generate-json-schema --experimental` 复核。

可重跑：

```sh
cargo test -p runtime --lib external_hooks_are_visible
cargo test -p runtime --lib real_codex_external_hook_remains_owned_by_provider -- --ignored --nocapture
cargo test -p runtime --lib real_codex_runtime_approval_audit_and_cancel -- --ignored --nocapture
env PATH="$PWD/target/debug:$PATH" cargo test -p e2e --lib e2e_mobile_claude_desktop_cancellation_rejects_late_mobile_approval -- --ignored --nocapture
```

通过日志：`/tmp/norma-ios-codex-external-hook-v12.log`、`/tmp/norma-ios-codex-competition-v12.log`、`/tmp/norma-ios-claude-desktop-competition-v12.log`。SQLite 授权验证与 G2 进度见实时计划，不计入 Provider 接入完成证明。


v12 最终回归补记：Release 通过（1m 57s），行数 / no-allow / 存储 / 性能 / diff 护栏通过。全仓与 E2E 失败保留在实时计划及证据 JSON 中，未核验未修改 HEAD 基线。后续 v13 属 G2 连接实现，见 [连接与配对](2026-09-07-norma-ios-connection.md)，不变更 G1 的 Provider 能力结论。
