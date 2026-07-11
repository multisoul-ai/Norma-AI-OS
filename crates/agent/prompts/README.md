# Agent prompts

`agent` 是 Norma OS 唯一的生产 Agent runtime；所有 system prompt 都在本目录维护。

| 入口 | 模板 | 运行时组合 |
| --- | --- | --- |
| Dialogue | `dialogue/norma-persona.txt` | 固定产品运行规则 + 原始 persona |
| CardTask | `card_task/system.md` | 独立执行型 system prompt |

## Dialogue persona

`dialogue/norma-persona.txt` 是用户指定的 Dialogue 角色原文，不得在未授权时编辑、格式化或转码。

当前人设：**Norma 多人工智能助理**（贾维斯式：严谨、干练、可调度多能力的统一对外接口）。旧版「QQ 私聊女友」快照保留在同目录的 `.bk` / `copy` 备份中，仅作历史参考。

当前批准的 SHA-256：

`d031af097e201ac84fe834227ea01ae7074d47069359e11a169d56b9bfcf34e1`

`src/prompts.rs` 的 `dialogue_system_prompt()` 在原文外层加入不可违背的产品规则；原文自身不变。修改来源快照时，必须显式更新校验值和 `tests/prompt_templates_tests.rs`，并在变更说明中记录原因。

## CardTask

`card_task/system.md` 专用于短生命周期的执行代理。它不包含 Dialogue persona，并规定既有卡片先查证、新建卡片使用 `Created` 回执、默认自动提交和基于真实回执的结束报告。

## 上下文策略

Dialogue 每次请求都重新注入完整 system prompt。运行时最多保留最近四个完整对话轮次；每轮的用户消息、assistant tool call 和 tool receipt 一起裁剪，避免生成断裂的工具上下文。
