# WorkspaceTitlebarBrand 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-initial.png` | 交通灯右侧依次是侧栏开关、logo；面包屑与 logo 的关系和左栏状态一致 | 孤立的「Norma」文字、logo 缺失、元素重叠 |
| `02-toggled.png` | 开关与 logo 的 x 与上一张完全相同；展开时 logo 右侧有「Norma」字样，收起时只有 logo | 开关或 logo 左右跳动，收起态仍有「Norma」字样 |
| `03-back.png` | 与 `01-initial.png` 一致 | 与初始状态不一致 |

浅色与深色主题各看一遍；不确定时看同一次运行的 `app.log` 和 `report.json`。
