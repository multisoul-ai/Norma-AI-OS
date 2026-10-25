# 设置 · 特效 · 卡片运行特效

参考的 skill 与文档：`apple-design`（内容优先、直接反馈、可访问性）、`emil-design-eng`（动效服务于状态理解、可中断）、`review-animations`。结构与派发特效同构：当前大预览 → 状态芯片 → 风格网格；全局偏好，不随工作空间/项目范围变化。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-canvas-classic.png` | 画布上六张卡片 Run 1–6；Run 2/3 有光蛇，Run 4 边框暖色，Run 5 冷色柔光 | 卡片重叠、Run 6 出现边框特效（经典失败无特效） |
| `02-run-section-dark.png` | “Agent card run effects” 标题与说明；当前卡里预览 + Classic 名称与说明；Replay 按钮 | 未替换的 i18n 键（`effects.run_*`）、预览为空白 |
| `03-aura-picked.png` | 当前名为 Aura；状态行 `Run effect changed to "Aura"`；Undo 按钮；网格第三格（Aura）蓝色描边 + 勾标 | 两格同时有勾、状态行停在 Saving… |
| `04-thinking-pinned.png` | Thinking 芯片为选中态；预览卡片外缘磁流体膜有尖刺 | Auto 与 Thinking 同时选中 |
| `05-failed-pinned.png` | Failed 芯片选中；预览卡片底部三处下垂滴落、外缘红色 | 预览仍在抖动（自动断言 ≤12px） |
| `06-canvas-ferrofluid.png` | 关闭设置后画布六张卡片换成磁流体：Run 6 红色下垂、Run 2/3 尖刺 | 仍为经典光蛇（未立即生效） |
| `07-run-section-light.png` | Paper Blue 下分区文字、芯片、预览对比度正常 | 白底上看不清的芯片色点或文字 |
| `08-after-restart.png` | 重启后当前名称仍为 Ferrofluid | 回到 Classic |
