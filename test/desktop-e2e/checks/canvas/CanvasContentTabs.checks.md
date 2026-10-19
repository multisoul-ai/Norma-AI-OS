# CanvasContentTabs 视觉检查

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-file-restored.png` | 右栏同时显示文件、网页标签；文件正文 `E2E PLAN CONTENT` 清晰可读，来源为 `E2E Project` | 文件卡片出现在中央画布 |
| `02-browser-selected.png` | 网页标签选中，右栏出现地址栏和网页正文 `WEB PAGE E2E`；来源项目仍可见 | 黑色空白网页或地址栏常驻在中央画布 |
| `03-file-reselected.png` | 切回文件后正文恢复，地址栏消失 | 文件内容被网页盖住 |
| `04a-drawer-closed.png` | 右栏和标签完全退场，中间画布伸到窗口右侧 | 右栏仍盖在画布上 |
| `04-drawer-reopened.png` | 收起后重新展开仍显示两个标签和文件正文 | 标签被清空 |
| `05-after-restart.png` | 重启后标签列表、选中项和文件正文恢复 | 重启变成空态 |
| `06-source-removed.png` | 文件正文仍可读，来源标记明确显示“来源项目已移除” | 来源项目误显示为仍可定位 |
| `07-file-closed.png` | 文件标签消失，网页标签保留 | 文件正文仍在右栏或中央画布 |
