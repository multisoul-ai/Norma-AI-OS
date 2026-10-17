# WorkspaceLifecycle 视觉检查

脚本退出码只代表 OCR、SQLite 和文件系统断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-welcome.png` | 首次启动的创建入口，主按钮可读 | 已有工作空间卡片 |
| `02-create-dialog.png` | 创建对话框，含名称字段和取消/创建 | 对话框外的内容盖住按钮 |
| `03-name-entered.png` | 名称字段里出现 `AlphaKeep` | 空字段或只剩占位符 |
| `04-created.png` | 已进入新建的工作空间 | 创建对话框仍开着 |
| `05-directory-both.png` | 目录同时列出 `AlphaKeep` 与 `BetaKeep`，有搜索框 | 空目录欢迎页 |
| `06-search-filtered.png` | 只剩 `AlphaKeep`；搜索框里有 `Alpha` | `BetaKeep` 卡片 |
| `07-search-cleared.png` | 两张卡片都回到目录 | 搜索框仍留着过滤词，或只剩一张 |
| `08-directory-with-project.png` | 目录仍列出两个名称 | 删除确认框 |
| `09-actions-menu.png` | 精选卡片旁的操作菜单，含重命名和删除 | 菜单跑到窗口外 |
| `10-rename-dialog.png` | 重命名对话框，原名可见 | 菜单和对话框叠成无法点击 |
| `11-renamed.png` | 卡片名称变为 `GammaKeep`，`BetaKeep` 仍在 | 重命名对话框未关，或仍显示 `AlphaKeep` |
| `12-delete-confirmation.png` | 删除确认框盖住目录，取消和删除都可读，文案写明项目文件夹保留 | 确认框被当成已关闭 |
| `13-delete-canceled.png` | 确认框消失，`GammaKeep` 与 `BetaKeep` 仍在 | 确认框残留 |
| `14-deleted.png` | 目录不再有 `GammaKeep`，`BetaKeep` 还在 | 删除确认框仍开着 |
| `15-after-restart.png` | 重启后的目录只有 `BetaKeep` | `GammaKeep` 或 `AlphaKeep` 被恢复 |

不确定时看同一次运行的 `app.log` 和 `report.json`，不要用相邻运行的截图补判。
