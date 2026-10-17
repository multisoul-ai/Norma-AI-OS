# 前置检查

驱动在发键鼠之前自己检查。任一项失败时进程退出码为 3，汇报为 ENV。

| 检查 | 失败时 |
| --- | --- |
| 屏幕未锁 | 请用户解锁。不要用其他会话或跳过截图 |
| 辅助功能 | 系统设置 → 隐私与安全性 → 辅助功能，允许 `target/desktop-workspace-e2e/NormaE2E.app`（`ai.norma.desktop-e2e`）。重新签名后系统可能要求重新勾选 |
| 屏幕录制 | 同一设置里的屏幕录制，允许 Norma Desktop E2E |
| 测试二进制 | `target/debug/norma-os` 必须存在，且不能是 `/Applications/Norma OS.app` |
| 其他 Norma | 只打印 PID 和路径。不结束用户的 App。本次使用独立数据目录和 `NORMA_SOCKET` |
| 上次自己的 PID | 仅当 `owned.json` 的可执行文件就是本次 `target/debug/norma-os` 时结束它 |
| 键鼠互斥 | 用户临时目录的 `norma-desktop-e2e.lock`。其他 worktree 正在跑时排队；超时退出码 3，不并行点击 |

授权对话框需要人点允许。驱动不会自己点系统设置。看到退出码 3 就停，把原文贴给用户。
