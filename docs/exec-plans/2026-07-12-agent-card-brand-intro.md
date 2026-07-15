# Agent 卡片品牌初始化图标

## 目标

将 Agent 卡片初始化覆盖层从统一的 Norma ASCII 画面改为按运行时 profile 显示品牌 ASCII 图标：Codex 使用用户提供的蓝紫终端图标转化后的 ASCII，Claude Code 使用用户提供的橙色机器人转化后的 ASCII。

## 实施步骤

1. 扩展初始化资产生成器，将两张用户提供图片转成紧凑的 100×41×32 双色 ASCII `.bin` 帧。
2. 将初始化动画种类细分为 Terminal、Codex、Claude、OpenCode；四种类型全部走同一 ASCII renderer。
3. 在所有 Agent 新建入口（工具栏、菜单、Norma socket）按 profile 启动正确的初始化种类。
4. Workspace 恢复 canvas snapshot 时，为已恢复的 Terminal 和 Agent 卡片登记相同的初始化覆盖层；不得等待或阻塞 PTY session 的延迟恢复。
5. 为 Codex 新建卡片更新 L1 e2e 验收，并为恢复的 Terminal、Codex、Claude Code 卡片增加 workspace flow 单元测试，验证资源名、种类和淡出前的不透明覆盖层。
6. 运行 app/e2e 测试与仓库机械化约束。
