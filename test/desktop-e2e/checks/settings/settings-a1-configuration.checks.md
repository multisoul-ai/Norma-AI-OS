# 语音设置：分组列表与统一来源

运行 `bash scripts/desktop-e2e.sh settings-a1-configuration --visual`。

- 中文 sapphire-night、英文 ink-black：状态概览与分组表单层级、对比、边界清楚。
- 产品来源只有本地识别与云端识别；A1 只在本地高级诊断显示。
- 麦克风、权限、来源和表单无重叠，按钮有明确状态。
- 浏览 Azure 与 A1 不改运行 provider；不录音、不准备模型、不发云请求。
- `report.json` 必须为 model-assisted 且全部关键判据有真实读图结论。

基线已退役全部可选浅色主题；paper-blue 会迁移到 ink-black。
浅色真实窗口未验收，不为本任务恢复主题目录。
