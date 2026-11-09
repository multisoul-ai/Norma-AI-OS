# desktop-e2e 模块

## island

场景：`island-window`、`island-approvals`、`island-remembered-approval`、`island-native-approval-fallback`、`island-questions`、`island-hook-questions`、`island-hook-providers`、`island-artifacts`、`island-navigation`、`island-migration`、`island-settings`、`island-staff`、`island-staff-real`、`island-hotkey`、`island-redesign`。`island-hook-questions` 用脚本 Grok 走生产 hook，验收岛内选项、自由回复、终端收卡、英文 Needs you 和 native_first。`island-hook-providers` 用脚本 Kimi、ZCode、InfCode 验收各家 stdout 与终端收卡。

`island-redesign` 验收灵动岛 2.0 的收起宽度、完成通知、审批队列、运行中筛选和幕僚页。几何看原生窗口，画面对照 `docs/prototypes/island-2/reference/`。
