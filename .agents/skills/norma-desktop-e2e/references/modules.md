# desktop-e2e 模块

## island

场景：`island-window`、`island-approvals`、`island-remembered-approval`、`island-native-approval-fallback`、`island-questions`、`island-hook-questions`、`island-hook-providers`、`island-artifacts`、`island-navigation`、`island-migration`、`island-settings`、`island-staff`、`island-staff-real`、`island-hotkey`、`island-redesign`、`island-proposals`。`island-hook-questions` 用脚本 Grok 走生产 hook，验收岛内选项、自由回复、终端收卡、英文 Needs you 和 native_first。`island-hook-providers` 用脚本 Kimi、ZCode、InfCode 验收各家 stdout 与终端收卡。

`island-redesign` 验收灵动岛 2.0 的收起宽度、完成通知、审批队列、运行中筛选和幕僚页。几何看原生窗口，画面对照 `docs/prototypes/island-2/reference/`。

`island-proposals` 验收幕僚提议出现在「需要你处理」：计数为 2、未知 Agent 禁用派发并给出原因、派发进入目标卡片且提议消失、忽略后重启不再出现。中文深色与英文浅色各跑一次，岛本身保持深色。
