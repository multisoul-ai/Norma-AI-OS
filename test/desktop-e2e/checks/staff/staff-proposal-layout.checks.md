# 建议卡与上方说明文字

运行 `bash scripts/desktop-e2e.sh staff-proposal-layout --visual`，英文为 `staff-proposal-layout-en`。

- `layout-*.png`：1280px、900px、恢复1280px，末段 LAYOUT END 与卡片保持24px空隙。
- 标题、理由、指令、Claude目标与模型选择器、操作按钮完整，没有穿模或裁切。
- 第一步夹具只提供 ACP 回复和生产 CLI 提议；采用前未调用归档或派发操作。
- `dispatched-*.png`：点采用后在1280px、900px、600px，LAYOUT END 末行完整且与已派发单行卡留约24px，卡片左上不露出说明文字或引号；卡内标题、「智能体名 · 引擎名」、「✓ 已派发」互不遮挡，窄宽时右侧整组换行。
