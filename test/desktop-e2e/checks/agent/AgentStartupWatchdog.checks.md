# agent-startup-watchdog

中文界面（`NORMA_E2E_LANGUAGE=zh-Hans`），一个项目；HOME 重定向到带「已安装 hook」配置的假目录。假 `codex` 启动即画出首屏 `CODEX BANNER`，但在收到 `release` 文件前不发任何 hook；假 `claude` 画出 `CLAUDE BANNER` 后立即发 SessionStart：

- `01-codex-before-threshold.png`：Codex 卡片创建后约 6 秒，标题栏没有警告徽标。
- `02-codex-stalled-badge.png`：约 15 秒后，Codex 卡片标题栏出现琥珀色警告徽标「迟迟未收到会话启动信号，可能是网络问题」；终端里仍能看到 `CODEX BANNER`（证明判据是 hook 而不是 PTY 输出）；没有弹窗、没有遮挡；徽标文字完整不被截断，与标题不重叠。
- `03-codex-recovered.png`：假 CLI 发出 SessionStart 后徽标消失，终端内容完整显示。
- `04-claude-healthy.png`：启动即发 SessionStart 的 Claude 卡片等过 15 秒仍无徽标；此时 Codex 的徽标也不在（它已恢复），整屏没有「迟迟未收到会话启动信号」。
