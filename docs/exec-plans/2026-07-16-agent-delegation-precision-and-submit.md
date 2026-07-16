# Agent 委派润色与可靠提交执行计划

## 目标

让 Norma 在明确指派 Agent 时只做轻量润色并保留原意；新激活的 Agent 卡片必须等 CLI 可提交后自动发送任务，不再出现“正文已输入但未发送”。

## 已确认行为

- 任务正文只移除呼叫词、口语冗余并调整语序，不扩写为结构化长简报。
- 保留目标、范围、先后顺序、约束、交付要求和明确命令，不新增要求。
- 上下文指代仅在候选唯一时展开；否则澄清。
- 信息充分时不确认，直接派发。
- 新 Agent 未就绪时回执为等待/排队；就绪后自动提交。超时不得误报 Submitted，并继续安全重试。

## TDD seams

1. `AgentSystem` prompt/tool 边界：生产 prompt 明确约束轻量润色、语义保留和唯一指代；真实 Provider 合规语料检查生成的 `intent` 保留关键要求。
2. App 生产委派入口：新激活 Agent 初始不写 PTY、不返回 Submitted；收到 SessionStart/安全就绪兜底后写入正文与 Enter；既有就绪 Agent 保持立即提交。
3. 用户流程 e2e：明确 Agent 指派经 Dialogue → CardTask → `dispatch_agent_tasks` 保留关键任务正文；新 Agent 等待后提交。

## 切片

1. 红测：强化 prompt 合同与真实 Provider 合规语料；最小修改 Dialogue/CardTask prompt 和工具说明。
2. 红测：新激活 Agent 委派进入等待且零 PTY 写入；实现 pending submission 状态。
3. 红测：SessionStart 后自动写正文与真实 Enter；实现 hook ready 触发。
4. 红测：无 hook 时经过安全启动窗口自动重试；实现非阻塞定时兜底和失败状态。
5. 补跨 crate/e2e 证据并运行受影响 crate、App、约束脚本和真 PTY 冒烟。

## 验证

```bash
cargo test -p agent
cargo test -p app agent_dispatch
cargo test -p e2e
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

