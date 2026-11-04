# GPT-Live 设置真实窗口看图清单

按 `docs/prototypes/gpt-live-settings/index.html` 重构：保留三张模式卡片；GPT-Live 为单栏纯配置页，通话由 ⌘E 开启，不放连接/挂断按钮；Key 已保存后收起，编辑在卡片内展开且不回显；偏好为一张带分隔线的卡片（分段控件、音色单选、系统麦克风行）；诊断默认收起；页脚显示保存状态。Material 语义色，Apple 平台习惯，无装饰动效。

模型看图版：`bash scripts/desktop-e2e.sh settings-live-configuration --visual`（`test/desktop-e2e/Scenarios/settings/SettingsLiveVisual.swift`）。测试 Key 为本地无效夹具，不连接云端；麦克风状态只代表系统枚举。前半中文，重启后英文 Sapphire Night 覆盖双语固定文案与保存状态；系统设备名等用户内容不翻译。产品当前无浅色主题。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| 01-live-dark.png | GPT-Live 卡片选中；图标+标题+说明；Key 卡片琥珀色「需要 API Key」与空输入框；对话偏好+更改会自动保存 | 连接/挂断/静音/停播按钮、连接状态卡、标题逐字竖排 |
| 02-prefs-dark.png | 分段控件「按住说话」选中带勾；Cedar 实心点；麦克风行状态点、声音设置外链、刷新图标 | 手动设备选择器、后台供应商配置 |
| 03-diagnostics-dark.png | 诊断整行展开：服务/输入/连接方式/缓冲区事实、小号原始报错块、刷新核对、隐私说明；随后箭头收起/展开各一次均有后置看图断言 | 超大字号报错、居中挤在一起的标题行、箭头点击重复切回原态 |
| 03d-key-saved-dark.png | 本地无效Key首字符保存后能继续输入完整值；点完成后已安全保存、更换 | 常驻密钥输入框、完成、移除、密钥明文 |
| 03e-key-blur-dark.png | 点更换后不再点击输入框即可直接输入完整新Key；按Tab失焦后收起，只保留已安全保存、更换 | 失焦后仍展开的编辑框、完成、移除、密钥明文 |
| 03f-key-removed-dark.png | 移除后空输入框显示未配置、需要API Key；再按Enter仍无凭据引用 | 复活的旧Key、已安全保存、旧Key掩码 |
| 04-live-sapphire.png | 英文 Sapphire Night 重启后 GPT-Live、Push to talk、Cedar 仍选中，Interaction mode、Reply voice、Microphone、Sound settings 为英文，描边与选中态可辨 | 选择重置、错位、中英固定文案混排 |
| 04b-footer-sapphire-en.png | 英文诊断默认收起：Diagnostics & connection details、For troubleshooting only；页脚 All changes saved、Uses the default macOS microphone | 中文固定文案、与保存成功同时出现的 Saving... 或失败提示 |

模型看图版通过真实输入与隔离SQLite精确值覆盖 Key 保存/更换自动焦点/完成/Tab失焦折叠/移除后Enter不复活，并逐项检查诊断整行与箭头切换。OCR 版另覆盖 Doubao、语音指令回归截图。此场景不覆盖云端通话、通话中静音/停播条、失败提醒（由 `gpt_live_material_failed_call_alert_opens_diagnostics` 单测覆盖）、全部键盘遍历与 hover。
