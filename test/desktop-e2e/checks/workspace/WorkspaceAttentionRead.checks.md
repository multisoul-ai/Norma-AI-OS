# 待处理提醒的已读清除

参考 apple-design：打开后即时反馈，不增加动效，沿用列表层级与键盘操作。

- `01-unread.png`：中文 Sapphire Night、英文 Ink Black 均显示 5 条提醒，新版本同路径仍提醒。
- `02-file-read.png`：alpha 冻结正文加载成功；计数 4，beta/gamma 未读保留。
- `03-turn-read.png`：beta 正文加载成功；同回合提醒消失，计数 2。
- `04-cleared.png`：键盘打开最后成果，工作台展示，计数 0。
- `05-restarted.png`：重启后计数 0；程序硬断言冻结成果和源文件仍保留。

英文场景沿用旧偏好标识 `paper-blue`，由生产 `ThemeId::from_saved_id` 迁移为 Ink Black；相应截图前缀不代表浅色主题。当前产品已退休浅色主题。
