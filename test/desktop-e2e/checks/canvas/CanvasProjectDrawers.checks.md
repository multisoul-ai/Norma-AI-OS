# CanvasProjectDrawers 视觉检查

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `00-project-transition.png` | 项目进入时左栏已收起，画布与预览卡片从同一背景过渡 | 过渡中左栏突然出现或覆盖项目预览 |
| `01-project-left-closed.png` | 进入具体项目后左侧 Norma 对话栏默认收起，真实项目画布使用释放出的宽度；左上保留可重新展开的入口，默认背景图连续 | 左栏在项目加载中突然出现、抽屉覆盖项目卡片或画布宽度跳变 |
| `02-project-terminal.png` | 中央真实画布出现 Terminal 卡片，保留默认背景图 | 输入命令后卡片仍不存在 |
| `03-project-agent.png` | 中央真实画布同时有 Agent 与 Terminal 卡片，新增卡片不与已有卡片重叠 | 命令后只有建议列表、没有新卡片或卡片互相覆盖 |
| `04-project-both-open.png` | 左右抽屉各自贴边、透出但压暗默认背景；中间标题靠左，Agent/Terminal 卡片占满项目场景宽度并纵排；中央底部保留原输入及语音入口；右侧空态在抽屉剩余高度居中 | 右栏压在卡片上、卡片两边出现二次留白、左栏消失、底部输入被裁掉或空态上移 |
| `05-project-chat-input.png` | 左侧输入框显示“项目对话输入验收”，Agent/Terminal 卡片和右栏仍可见 | 左侧输入失焦或文字只出现在中央 Dock |
| `06-project-right-closed.png` | 右抽屉收起后中央画布拓宽，Agent 与 Terminal 卡片恢复并排、完整可见 | 卡片仍被窄画布布局约束或内容浏览器残留 |
