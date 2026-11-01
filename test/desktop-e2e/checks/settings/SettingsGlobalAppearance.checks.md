# 全局外观兼容与多窗口

沿用 apple-design 的直接反馈原则；只检查本次全局化差异。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-migrated-wallpaper.png` | 紫色星云当前预览、全局生效说明 | 原全局金色星环、空间蓝色地平线、范围/跟随开关 |
| `02-migrated-effects.png` | Fireworks 当前选择、全局说明 | 空间默认 Meteor、范围/跟随开关 |
| `03-second-window.png` | 第二个真实首页窗口 | 第一窗口复用冒充新窗口（另由 windowID 断言） |
| `04-second-window-saved.png` | Voice ribbon 当前选择、全局说明 | Fireworks 当前选择 |
| `05-first-window-synced-effects.png` | 原窗口当前声纹 | 旧烟花 |
| `06-first-window-synced-wallpaper.png` | 原窗口当前蓝色地平线 | 旧紫色星云 |
| `07-restored-global-wallpaper.png` | 重启后蓝色地平线、全局说明 | 旧项目紫色星云、范围/跟随开关 |
| `08-restored-global-effects.png` | 重启后声纹、全局说明 | 旧项目烟花、范围/跟随开关 |
