# desktop-e2e 模块

## island

场景：`island-window`、`island-approvals`、`island-remembered-approval`、`island-native-approval-fallback`、`island-questions`、`island-hook-questions`、`island-hook-providers`、`island-artifacts`、`island-navigation`、`island-migration`、`island-settings`、`island-staff`、`island-staff-real`、`island-hotkey`、`island-redesign`、`island-proposals`。`island-hook-questions` 用脚本 Grok 走生产 hook，验收岛内选项、自由回复、终端收卡、英文 Needs you 和 native_first。`island-hook-providers` 用脚本 Kimi、ZCode、InfCode 验收各家 stdout 与终端收卡。

`island-redesign` 验收灵动岛 2.0 的收起宽度、完成通知、审批队列、运行中筛选和幕僚页。几何看原生窗口，画面对照 `docs/prototypes/island-2/reference/`。

`island-proposals` 验收幕僚提议出现在「需要你处理」：计数为 2、未知 Agent 禁用派发并给出原因、派发进入目标卡片且提议消失、忽略后重启不再出现。中文深色与英文浅色各跑一次，岛本身保持深色。

## voice

场景：`voice-live-call`、`voice-live-acoustic`、`voice-doubao-direct`、`voice-command-budget`、`voice-settings-modes`。

`voice-settings-modes` 用 `--visual` 看语音指令、豆包 2.0、GPT-Live 在蓝宝石之夜和墨黑下的设置页。现行目录没有浅色主题。凭据行不回显 Key。通话场景不走模型看图。

## staff

`staff-composer-images` / `staff-composer-images-en` 使用 `--visual` 定向验收输入框左侧加号、真实剪贴板图片缩略图及移除后的图标，保留发送箭头和快捷键提示；分别使用中文墨黑与英文蓝宝石之夜。

场景：`staff-workflow`、`staff-workflow-en`、`staff-opencode-dispatch`、`staff-archive`、`staff-archive-en`，均通过 `--visual`。`staff-archive` 在墨黑中文下覆盖悬停归档图标、头部归档、仍在进行的撤销提示、已归档入口与返回、新消息取消归档，以及全部/需要你/执行中不含已归档任务。`staff-archive-en` 用蓝宝石之夜覆盖同一流程的英文文案。现行目录没有浅色主题，paper-blue 会迁移成 ink-black。`staff-workflow` 覆盖新建任务、ACP 提议、用户采用、CLI 新建独立 Worker、生产 hook 结果落盘、自动阶段、手动保护、完成重开、右侧文件、全屏/Escape 与画布切换，并核对精简已派发卡、Worker 宠物头像（卡条/结果卡/需要你）、无工具步骤时的两行活动状态与长回复后的 24px 间距。`staff-opencode-dispatch` 覆盖无 SessionStart 的 OpenCode：第一次粘贴不确认，15 秒后需要你，重新投递仍是同一张卡。脚本只替代外部模型与 Agent；不得预写 TaskStore 或注入 App 内部任务状态。

`staff-protocol-recovery` / `staff-protocol-recovery-en` 定向验证 OpenCode 原生 q0 问答、命令组合拒绝后的同会话纠正、双语格式错误反馈与提议唯一性，并连续录像。

`staff-proposal-layout` / `staff-proposal-layout-en` 用长中英混排说明与待采用建议，验收1280px、900px及恢复宽窗后的24px间距，以及标题、理由、指令、目标与按钮完整性；不执行归档或派发。
