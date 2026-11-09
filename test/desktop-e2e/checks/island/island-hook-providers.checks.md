# Kimi / ZCode / InfCode 岛问答

- `10-kimi-appeared.png`：Kimi 题出现在岛上，Spike color 1?，Blue 与 Green 可见，有需要你处理的计数。
- `10-zcode-appeared.png`：ZCode 同样展开原题与两个选项。
- `10-infcode-appeared.png`：InfCode 同样展开原题与两个选项。
- 硬断言：Kimi stdout 只有 hookSpecificOutput.permissionDecision=deny 与 permissionDecisionReason，没有顶层 decision，也没有 updatedInput。
- 硬断言：ZCode stdout 是 allow，updatedInput 同时有原 questions 和 answers Spike color 1?=Blue。
- 硬断言：InfCode stdout 顶层 decision=deny 与 hookSpecificOutput reason 都在，没有桥接 answers。
- 硬断言：三家终端作答后 hook stdout 为空，岛收回。
- 脚本 Agent 调用生产 hook，不等于真实 CLI 或真实模型验收。
