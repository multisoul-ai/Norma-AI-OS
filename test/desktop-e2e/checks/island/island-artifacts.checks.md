# 岛成果与跨空间定位

入口：`bash scripts/desktop-e2e.sh island-artifacts --visual`。

- `01-unread.png`：当前主窗口为 Other workspace，岛仍显示来源 Island artifacts / Review Project 的成果。
- `02-read.png`：标记 alpha/beta 所在回合已读后，该回合与两文件提醒消失，gamma 回合保留；两个冻结成果记录未删除。
- `03-opened.png`：岛打开 gamma 后切换到来源工作空间/项目，真实冻结正文为 Frozen gamma.txt island。
- `04-cleared.png`：全部提醒已消费，不残留已看成果。
- `05-restarted-en.png`：英文页签及空态无中文残留，重启未复活已读提醒；三个源文件内容保持不变。

采用 Apple 直接操作与即时反馈；不把 CLI 成功或截图存在当作内容打开证据。
