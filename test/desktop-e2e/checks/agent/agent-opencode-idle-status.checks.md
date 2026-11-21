# OpenCode 原生卡片状态

从 `OpenCode Status Check` 工作空间打开 `E2E Project`，按 Cmd+5 创建卡片。使用 App 为该卡分配的默认名称，Grace 不参与此场景。真实 OpenCode 1.19.3 与生产生命周期插件使用隔离配置，本机模型只控制回复内容及两段流的结束时机。

检查同一张卡片的标题栏和原生终端正文。所有截图都应清晰可读，没有遮挡、重叠或裁切。

- `00-idle-build.png`：出现 Build、Question fixture 与空输入框。卡片显示「待开始」，无运行、问答或完成标记。可通过 `NORMA_E2E_OPENCODE_IDLE_FRAMES=1` 额外记录十帧 `00-idle-sample-*.png`，用于查找短暂的错误状态。
- `01-generating.png`：首段 `I will check the fixture color.` 可见，标题为「运行中」。问答表单和最终回复尚未出现。
- `02-waiting-question.png`：标题为「等待回答」，原生表单完整显示 `Which fixture color should I use?` 和 Green、Blue 选项，第一项 Green 高亮。以 Return 作答后，`opencode-fixture/consumed.json` 应记录一次 Green 工具答案。
- `03-generating-after-answer.png`：活动表单已关闭，标题回到「运行中」。可见 `Color confirmed.`，最终标记尚未出现。
- `04-completed.png`：标题为 Done，原生终端可见 `Color confirmed. NATIVE_FIXTURE_DONE`。没有活动问题、运行态或错误。
