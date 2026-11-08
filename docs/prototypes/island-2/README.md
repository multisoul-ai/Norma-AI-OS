# Norma 灵动岛 2.0 高保真原型

权威视觉与交互依据，生产实现必须一比一还原。浏览器直接打开 `island-2.html`。

- `island-2.html`：可交互主舞台，含五种形态切换、2/5/8 个 Agent 数量切换，以及收起形态对比、完成通知、审批提醒的静态展示。
- `reference/*.png`：由该原型生成的参考图，2x，1280×640。用于真实 App E2E 看图对照。
- `reference-multi-agent-roster.html`：多 Agent 网格展开的早期探索，仅作参考，不是还原对象。

固定状态参数（用于截图，可复现）：

```text
island-2.html?solo=1&still=1&mode=<compact|activity|alert|expanded|speaking>&n=<2|5|8>&tab=<pending|running|staff>
```

`solo=1` 只显示舞台，`still=1` 关闭全部动画。重新生成参考图：

```bash
C="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
"$C" --headless=new --hide-scrollbars --force-device-scale-factor=2 --window-size=1280,640 \
  --virtual-time-budget=4000 --screenshot=reference/alert.png \
  "file://$PWD/island-2.html?solo=1&still=1&mode=alert"
```
