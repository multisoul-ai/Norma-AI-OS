# 持久化并限期保留应用诊断日志

## 决策

Norma OS 的 GPUI app 壳层负责安装全局 `tracing` subscriber。日志继续输出到
stderr，同时通过 `tracing-appender` 的 non-blocking writer 写入：

`~/.norma/logs/norma-os.YYYY-MM-DD.log`

其中 `~` 由运行时解析为当前用户主目录；Windows 对应
`C:\Users\<用户名>\.norma\logs\`。

文件按天滚动。独立维护线程在应用启动后立即清理一次，之后每 24 小时清理一次；
只删除名称严格匹配 Norma OS daily logger、修改时间超过 14 天的普通文件。日志目录
不可用时应用继续启动并降级为 stderr。

`WorkerGuard` 与 app 生命周期一致，退出时先停止维护线程，再刷新文件 writer。
默认 non-blocking writer 在队列过载时允许丢弃日志，而不是阻塞 GPUI、PTY output
或其他交互热路径。

## 边界

- 日志是诊断副本，不是任务、会话或业务状态的事实源。
- 清理器只扫描固定 `logs` 目录，不跟随符号链接，不删除外部文件或同名目录。
- `RUST_LOG` 继续控制 stderr 与文件的共同过滤级别，未设置时默认 `info`。
- 日志内容继续遵守 ADR-0019：默认不记录完整用户内容、模型内容、Terminal 内容、
  API Key、Authorization Header 或内部思考文本。

## 取舍

按天滚动加固定时间保留比按文件大小配额更容易解释和测试，但无法提供严格的磁盘
占用上限。14 天窗口为故障追踪提供跨多次启动的上下文，同时避免日志无限增长；
若未来观察到单日日志量过大，再增加总容量上限，不改变当前内容隐私边界。
