# 画布命令联想原型

问题：底部输入框如何在保留画布上下文的同时，提供类似 Spotlight 的指令联想？

直接打开 `browser-preview.html` 体验；`command-dock.html` 是对话内展示源。
仅模拟创建和主题切换，不连接 Norma、启动 Agent 或保存应用设置。

## 已实现的交互

- 输入「创建」、`/ne`、`/new`，选择 Agent 后继续选择位置。
- Tab 补全；上下键选择；Enter 执行；Esc 收起但保留草稿。
- 相对位置在画布上显示落点；取消选择参照卡片后，相对位置不能执行。
- `/theme` 切换三个示例主题；「在右边创建一个 Codex」演示自然语言映射。
- 中文输入法组词期间 Enter 不执行；未知指令和参数不产生操作。

## 设计与接入边界

参考 apple-design：建议面板锚定输入框，保留画布可见性，明确焦点、选择与禁用状态。
参考 emil-design-eng：高频键盘联想不加动画。
生产版可用同一份操作定义提供别名、参数候选和校验；精确命令本地解析，自由语言经 Norma 解析后调用相同动作。
原型的中文匹配只覆盖少量示例；位置保留策略仍待产品讨论，不代表已修改自动布局。
GPUI 接入时复用组件库输入控件、现有 Agent 创建和 set_theme；此原型未修改 Rust 代码。

## 检查

```sh
node docs/prototypes/norma-command-dock-2026-10-01/check.mjs
python3 -m http.server 8766 --bind 127.0.0.1 --directory docs/prototypes/norma-command-dock-2026-10-01
node docs/prototypes/norma-command-dock-2026-10-01/browser-check.cjs /absolute/path/to/playwright
```

纯联想检查及浏览器自动化已通过，覆盖补全、方向键、输入法、定位创建、主题与 320px 布局。
Computer Use 两次连接超时，未完成真实浏览器 Computer Use 验收；自动化结果不替代该验收。
