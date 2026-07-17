# Agent 委派润色与可靠提交执行计划

## 目标

让 Norma 在明确指派 Agent 时只做轻量润色并保留原意；新激活的 Agent 卡片必须等 CLI 可提交后自动发送任务，不再出现“正文已输入但未发送”。

## 已确认行为

- 任务正文只移除呼叫词、口语冗余并调整语序，不扩写为结构化长简报。
- 保留目标、范围、先后顺序、约束、交付要求和明确命令，不新增要求。
- 委派层只做文字润色和中转，不代替目标 Agent 分析需求或补全规格。
- 目标 Agent 和动作可识别时直接派发，不因任务简短或缺少实现细节、背景、偏好、验收标准而确认。
- 只有目标、动作或多候选指代导致核心意图无法识别时才问一个最小问题；不得反复确认。用户只回答一个 Agent 名称、动作或类型时，结合上一轮原任务恢复完整意图并直接派发，不要求重述。
- 新 Agent 未就绪时回执为等待/排队；收到 Hook Ready 或经过 30 秒保守启动窗口后，先写正文，再于 100ms 稳定窗口后独立发送 Enter。超时不得误报 Submitted，并继续安全重试。
- Codex 启动与恢复命令关闭 paste burst，避免快速输入被 TUI 识别为只粘贴不提交；Claude、Cursor、OpenCode 复用同一两阶段提交路径。
- 输入区状态不确定时不得盲目重写；明确引导连续关闭两次卡片以清空会话，再重新激活后派发。

## TDD seams

1. `AgentSystem` prompt/tool 边界：生产 prompt 明确约束轻量润色、语义保留和唯一指代；真实 Provider 合规语料检查生成的 `intent` 保留关键要求。
2. App 生产委派入口：新激活 Agent 初始不写 PTY、不返回 Submitted；收到 SessionStart/安全就绪兜底后分两阶段写入正文与 Enter；既有就绪 Agent 同样经过短暂稳定窗口自动提交。
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
NORMA_E2E_AGENT_CLI=codex cargo test -p e2e real_agent_cli_receives_and_submits_prompt -- --ignored --nocapture
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```
