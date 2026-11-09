# 脚本 Grok 岛问答

- `00-grok-appeared.png`：中文岛展开，可见需要你或需要你处理，计数为 1；题目 Spike color 1?，Blue 与 Green 都在，有自由回复。
- `01-grok-choice.png`：Blue 选中，Green 未选，发送回复可用，原题还在。
- `02-grok-custom.png`：自由回复完整包含 Cobalt blue 及其空格。
- `03-grok-terminal-open.png`：终端作答前岛再次展开 Spike color 3?。
- `04-grok-english.png`：英文 Needs you 与 1 pending，没有中文「需要你处理」残留。
- 硬断言：选项与自由回复的 stdout 是顶层 deny，reason 精确，且没有 hookSpecificOutput、updatedInput、answers。
- 硬断言：终端作答与 native_first 的 hook stdout 为空，岛最终收回。
- 脚本 Agent 调用生产 hook，不等于真实 Grok 模型验收。自由回复用控制通道 `session.type` 键入。
