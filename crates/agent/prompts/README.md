# Agent prompts

`agent` 是 Norma OS 唯一的生产 Agent runtime；所有 system prompt 都在本目录维护。

| 入口 | 模板 | 运行时组合 |
| --- | --- | --- |
| Dialogue | `dialogue/norma-persona.txt` | 固定产品运行规则 + 原始 persona |
| CardTask | `card_task/system.md` | 独立执行型 system prompt |

## Dialogue persona

`dialogue/norma-persona.txt` 是用户指定来源的逐字快照，不得在原文件内编辑、格式化或转码。来源：

<https://github.com/dongshuyan/Awesome-Prompts/blob/master/%E8%A7%92%E8%89%B2%E6%89%AE%E6%BC%94/%E5%A4%9A%E8%A7%92%E8%89%B2%E4%BA%BA%E8%AE%BE/%E8%AF%BA%E7%8E%9B-19%E5%B2%81%E8%AE%BE%E8%AE%A1%E7%B3%BB%E5%A4%A7%E4%B8%80-%E6%B8%A9%E6%9F%94%E4%BE%9D%E8%B5%96%E5%9E%8B%20NSFW%20%E6%8F%90%E7%A4%BA%E8%AF%8D.txt>

当前批准的 SHA-256：

`56ff8153df1cfa1d103b67b314391f8aadd5c85ac38ba2c533617ce323e19e5d`

`src/prompts.rs` 的 `dialogue_system_prompt()` 在原文外层加入不可违背的产品规则；原文自身不变。修改来源快照时，必须显式更新校验值和 `tests/prompt_templates_tests.rs`，并在变更说明中记录原因。

## CardTask

`card_task/system.md` 专用于短生命周期的执行代理。它不包含 Dialogue persona，并规定既有卡片先查证、新建卡片使用 `Created` 回执、默认自动提交和基于真实回执的结束报告。

## 上下文策略

Dialogue 每次请求都重新注入完整 system prompt。运行时最多保留最近四个完整对话轮次；每轮的用户消息、assistant tool call 和 tool receipt 一起裁剪，避免生成断裂的工具上下文。
