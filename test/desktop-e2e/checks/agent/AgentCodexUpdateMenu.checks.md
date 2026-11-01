# agent-codex-update-menu

中文界面，一个项目；HOME 重定向到带 hook 配置的假目录。假 `codex` 报告 0.162.1，使应用走原生握手；`--remote` 画出菜单（`UPDATE MENU`、`> n. …`、`SELECTED n`），不连接中继：

- `01-menu-shown.png`：Codex 卡片显示菜单，`SELECTED 1`；卡片标题栏无「启动失败」，没有遮挡。
- `02-menu-down.png`：按 ↓ 后 `SELECTED 2`，`>` 标记落在第 2 项。
- `03-menu-chosen.png`：回车后出现 `CHOSEN 2`，没有「Codex 启动失败」，卡片仍在运行。
- `04-stuck-timed-out.png`：卡死模式（持续刷屏 `STUCK TICK` 但不连接）约 60 秒后出现「Codex 启动失败：pty spawn failed: native Codex startup timed out」（终端内会折行）与「进程已退出 code=1」；不再接受输入。
