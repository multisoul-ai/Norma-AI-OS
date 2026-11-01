# agent-background-completion

中文界面（`NORMA_E2E_LANGUAGE=zh-Hans`），一个项目。HOME 重定向到带 Claude hook 配置的假目录。假 `claude` 画出 `BG HOLD` 后上报后台 Bash，并在 `settle` 文件出现前停住。

- `01-background-held.png`：卡片标题栏是「运行中」，终端里能看到 `BG HOLD`。整屏没有 `Done`。没有完成弹窗，徽标文字不被截断。
- `02-background-done.png`：结清通知和空 `Stop` 经过确认窗口后，卡片标题栏是 `Done`，终端里能看到 `BG SETTLED`。标题栏不再是「运行中」。没有弹窗遮住卡片。
