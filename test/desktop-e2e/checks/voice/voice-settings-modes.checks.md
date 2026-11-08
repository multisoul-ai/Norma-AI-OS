# 语音设置三种模式

运行 `bash scripts/desktop-e2e.sh voice-settings-modes --visual`。

蓝宝石之夜（sapphire-night）和墨黑（ink-black）各看一遍语音指令、豆包 2.0、GPT-Live。现行主题目录没有浅色；保存的 paper-blue 会迁移成墨黑。主界面应有「语音输入 / 语音输出」，凭据行只显示保存和状态，不回显 Key。不打开麦克风，不发起通话。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| 00-command-top-sapphire-night.png | 深色；语音指令选中；语音输入 | 明文 API Key |
| 00-command-sapphire-night.png | 深色；语音输出、语音回复、收起的更多设置 | 明文 API Key、展开的长列表 |
| 00-doubao-sapphire-night.png | 深色；豆包选中；火山方舟凭据行与保存 | 回显的 Key |
| 00-live-sapphire-night.png | 深色；GPT-Live 选中；OpenAI 凭据行与保存 | 回显的 Key、常驻挂断 |
| 01-command-top-ink-black.png | 墨黑；语音指令选中；语音输入，文字可读 | 明文 API Key |
| 01-command-ink-black.png | 墨黑；语音输出、语音回复、收起的更多设置 | 明文 API Key |
| 01-doubao-ink-black.png | 墨黑；豆包凭据行 | 回显的 Key |
| 01-live-ink-black.png | 墨黑；GPT-Live 凭据行 | 回显的 Key |
