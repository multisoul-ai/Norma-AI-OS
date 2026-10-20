# 前置检查

驱动在发送输入之前自己检查。任一项失败时进程退出码为 3，汇报为 ENV。

| 检查 | 失败时 |
| --- | --- |
| 屏幕未锁 | 请用户解锁。不要用其他会话或跳过截图 |
| 测试二进制 | `target/debug/norma-os` 必须存在，是 debug 构建，且不能是 `/Applications/Norma OS.app` |
| 其他 Norma | 只打印 PID 和路径。不结束用户的 App。本次使用独立数据目录和 `NORMA_SOCKET` |
| 上次自己的 PID | 仅当 `owned.json` 的可执行文件就是本次 `target/debug/norma-os` 时结束它 |
| 控制通道 | 启动后 20 秒内连不上即退出码 3。通常是用了 release 或 `production-data` 构建，重新 `cargo build -p app` |

不需要辅助功能或屏幕录制授权：输入和截图都经 debug App 内的测试控制通道完成，见 [`docs/quality/desktop-e2e.md`](../../../../docs/quality/desktop-e2e.md)。看到退出码 3 就停，把原文贴给用户。
