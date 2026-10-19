# Pi 1.3.0 探测证据

这些文件来自 2026-10-18 本机运行。对应说明见 [调研与方案](../../2026-10-18-pi-coding-agent.md)。

- `cli-help.md`：全部 CLI / 子命令完整帮助输出。
- `command-results.json`：本地 OpenAI SSE 夹具驱动真实 Pi 的完整 argv、stdout、stderr、退出码。
- `payload-samples.json`：完整扩展事件对象和 context，选取成功、工具失败、Provider 失败、取消及确认框样例。模型响应来自本地夹具，不能作为外部模型或 OAuth 验证。
- `*.meta.json`：PTY argv、发键时间、退出码、是否被记录器强杀；`*.txt` 移除了 ANSI 与行尾空白，保留终端文本。选择器有多次重绘，因此可能包含重复内容。
- `session-file.jsonl`：真实创建并恢复的临时会话，含 v3 header；不是 Norma 运行时读取方案。
- `verification.json` / `empty-session-verification.json`：基于真实 hook、落盘文件与恢复结果的断言记录。
- `probe-extension.ts.txt`：测试扩展的非执行副本；原扩展已从 `~/.pi` 删除。
- `install-cleanup.json`：shell 前后哈希、精确扩展清理与 Pi 新建配置文件名，不含凭据内容。
- `pi-logo.ts` / `official-LICENSE`：官方仓库图标源代码与 MIT 条款，源 URL 在调研文档中。

未提交临时 models.json、用户 auth.json、API key 或 HTTP Authorization 头。`norma-probe/fixture` 是明确命名的本地夹具模型。
