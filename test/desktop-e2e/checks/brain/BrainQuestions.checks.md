# 原问题回答 UI 验收

场景：`brain-questions-ui`。真实 Norma App + 生产 Hook 桥，使用确定性脚本 Agent/幕僚；此场景不替代四家真实 CLI 验收。

- `01-question-suggestion.png`：原题、选项、建议、理由相邻；采用、编辑、原卡片及明确委托入口可辨认。概览计数没有重复状态行。
- `02-question-consumed.png`：收到原 tool_use_id 消费证据后，第一题不再显示等待回答。
- `03-question-edited.png`：编辑态选中的 Blue 清晰可辨，与原建议 Green 分开；提交按钮在题目附近，文字及控件不裁切。
- `04-delegated-consumed.png`：明确委托第三题后正常结束，原题不残留。
- `05-custom-question-edited.png`：自定义输入含空格的 Cobalt blue，选项已撤销、提交启用，输入内容完整可见。
- `06-short-overview-bottom.png`：窗口缩到640px且原题仍在编辑时，外层可滚到想法栏，标签和输入框完整可见；恢复尺寸和滚动后仍能提交原题。
- `consumed.jsonl`：四条且仅四条原题消费，依次 Green、Blue、Green、Cobalt blue；题目身份准确。
- `question-commands.jsonl`：普通事件只建议，明确委托后才 answer；不出现 assign/propose 新任务。

按 apple-design 落地：原题邻近操作、编辑即时反馈、提交防重复、真实消费后才移除待答。键盘使用组件原生语义，并隔离概览导航；键盘/并发失败边界另有 GPUI 与 Runtime 聚焦回归。浅深色与短面板的逐图结论记录到当次 artifacts 报告；未实际看图的项不得标记通过。
