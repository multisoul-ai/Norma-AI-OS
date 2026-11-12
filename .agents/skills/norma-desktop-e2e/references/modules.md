# desktop-e2e 模块

## island

场景：`island-window`、`island-approvals`、`island-remembered-approval`、`island-native-approval-fallback`、`island-questions`、`island-hook-questions`、`island-hook-providers`、`island-artifacts`、`island-navigation`、`island-migration`、`island-settings`、`island-staff`、`island-staff-real`、`island-hotkey`、`island-redesign`、`island-proposals`。`island-hook-questions` 用脚本 Grok 走生产 hook，验收岛内选项、自由回复、终端收卡、英文 Needs you 和 native_first。`island-hook-providers` 用脚本 Kimi、ZCode、InfCode 验收各家 stdout 与终端收卡。

`island-redesign` 验收灵动岛 2.0 的收起宽度、完成通知、审批队列、运行中筛选和幕僚页。几何看原生窗口，画面对照 `docs/prototypes/island-2/reference/`。

`island-proposals` 验收幕僚提议出现在「需要你处理」：计数为 2、未知 Agent 禁用派发并给出原因、派发进入目标卡片且提议消失、忽略后重启不再出现。中文深色与英文浅色各跑一次，岛本身保持深色。

## voice

场景：`voice-live-call`、`voice-live-acoustic`、`voice-doubao-direct`、`voice-command-budget`、`voice-settings-modes`。

`voice-settings-modes` 用 `--visual` 看语音指令、豆包 2.0、GPT-Live 在蓝宝石之夜和墨黑下的设置页。现行目录没有浅色主题。凭据行不回显 Key。通话场景不走模型看图。

## staff

场景：`staff-workflow`、`staff-workflow-en`，均通过 `--visual`。真实 App 中新建任务、ACP 提议、用户采用、CLI 新建独立 Worker、生产 hook 结果落盘、自动阶段、手动保护、完成重开、右侧文件、全屏/Escape 与画布切换。脚本只替代外部模型与 Agent；不得预写 TaskStore 或注入 App 内部任务状态。
