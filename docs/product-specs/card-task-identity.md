# Card Task Identity SPEC

## 1. 背景与目标

**背景：** 当画布上并行运行多个 Agent/Terminal 卡片时，用户无法一眼区分「哪张卡片在执行哪个任务」。
现有卡片标题固定为 "Terminal" 或 agent profile 名称，不携带任何任务语义信息。
在任务运行中途和被打断回来后尤为突出。

**目标：**
- 扫一眼卡片标题栏即可识别任务身份，零额外操作
- 状态变化（运行中 / 完成 / 失败）有明确视觉反馈
- 运行时长始终可见
- 摘要行实时反映当前正在做什么

---

## 2. 范围

### 2.1 In Scope
- Agent 卡片（Claude / Codex）的任务标题、摘要行、状态指示器、运行时长
- Terminal 卡片的当前命令显示
- 卡片状态变化的视觉/动态反馈（颜色 + 脉冲动画）

### 2.2 Out of Scope
- 全局任务总览面板（未来 Phase）
- 卡片间依赖关系可视化
- Terminal 卡片的手动任务命名（仅用命令行）
- 颜色主题自定义配置

---

## 3. 用户与使用场景

**典型用户：** 并行运行 2–5 个 Agent 卡片的开发者

**核心场景：**
1. 启动多个 agent 任务后切换到其他工作，回来时需要快速对号入座
2. 某张卡片状态变化（完成/报错），需要立刻识别是哪个任务

---

## 4. 卡片信息结构

### 4.1 视觉结构（方案 A：顶部横向状态条）

```
┌──────────────────────────────────────────┐
│ ● THINKING  │  帮我重构 user_service.rs  │ 01:23 │  ← 状态条（第1行）
├──────────────────────────────────────────┤
│ ▶ Running: Bash                          │          ← 摘要行（第2行）
│                                          │
│  [terminal output area]                  │
└──────────────────────────────────────────┘
```

状态条第一行分三段：`[状态点+标签] [任务标题] [运行时长]`
摘要行第二行：当前正在执行的操作，实时滚动更新。

### 4.2 Agent 卡片字段定义

| 字段 | 内容 | 更新时机 |
|------|------|----------|
| 状态点颜色 | 蓝=Thinking / 黄=RunningTool / 橙=WaitingApproval / 绿=Done / 红=Failed / 灰=Ready/Ended | `AgentTaskState` 变化时 |
| 状态标签 | Thinking / Running Bash / Waiting / Done / Failed | `AgentTaskState` 变化时 |
| 任务标题 | 第一条 `UserPromptSubmit` 的 prompt，截取前 60 字，后续锁定不变 | 首次 `UserPromptSubmit` 时写入，之后只读 |
| 运行时长 | 从卡片注册到现在（`NodeRegistered` 事件为起点） | 每秒更新（app 层 GPUI 定时器） |
| 摘要行 | 当前操作的人类可读描述（见 §4.4） | `AgentTaskState` 变化时实时更新 |

### 4.3 Terminal 卡片字段定义

| 字段 | 内容 | 更新时机 |
|------|------|----------|
| 状态标签 | Running / Finished / Error / Stopped | `NodeRunState` 变化时 |
| 任务标题 | 当前正在执行的命令行，截取前 60 字 | 实时更新（已有数据，复用现有机制） |
| 运行时长 | 进程存活时长 | 每秒更新 |

### 4.4 摘要行内容映射（Agent）

| `AgentTaskState` | 摘要行显示 |
|-----------------|-----------|
| `Ready` | `Waiting for input` |
| `Thinking` | `Thinking…` |
| `RunningTool { tool_name: Some(name) }` | `Running: {name}` |
| `RunningTool { tool_name: None }` | `Running tool…` |
| `WaitingApproval { tool_name: Some(name) }` | `Waiting approval: {name}` |
| `WaitingApproval { tool_name: None }` | `Waiting approval…` |
| `Done` | `Completed` |
| `Failed { message }` | `Failed: {message 前 60 字}` |
| `Ended` | `Session ended` |

---

## 5. 状态颜色与动效

| `AgentTaskState` | 状态点颜色 | 附加效果 |
|----------------|----------|---------|
| `Ready` | 灰色 `#888` | 静止 |
| `Thinking` | 蓝色 `#4A9EFF` | 脉冲动画（1s 周期，opacity 0.4→1.0） |
| `RunningTool` | 黄色 `#F5C842` | 旋转图标（0.8s/圈） |
| `WaitingApproval` | 橙色 `#FF8C42` | 慢速闪烁（2s 周期） |
| `Done` | 绿色 `#4CAF50` | 静止，运行时长定格 |
| `Failed` | 红色 `#E53935` | 静止，卡片顶部边框变红 |
| `Ended` | 灰色 `#888` | 轻微淡出 |

---

## 6. 技术实现概览

### 6.1 数据来源与提取

**Agent 任务标题来源：`UserPromptSubmit` hook 事件**
- Claude、Codex、Cursor 或 OpenCode hook 归一化为 `UserPromptSubmit` 时，从 provider `raw_json` 尝试提取用户 prompt 内容
- 字段名需在实现前验证（候选：`"prompt"`、`"message"`、`"user_message"`）
- 首次收到时提取并写入 runtime 侧的 `task_title` 字段，后续不覆盖
- 若卡片创建后从未收到 `UserPromptSubmit`，标题回退显示 `alias`（若有）或 profile 名

**摘要行：** 纯由当前 `AgentTaskState` 派生，无需额外存储。

**运行时长：** app 层用 GPUI `cx.spawn` 定时器驱动，每秒触发重渲，不入 runtime。

### 6.2 架构边界

```
runtime::AgentTaskState  →  增加 task_title: Option<String>
runtime::EventQueue      →  UserPromptSubmit 时提取 prompt 写入 task_title
app::agent_card          →  读取 runtime 状态渲染状态条 + 摘要行
app::agent_card          →  GPUI 定时器驱动运行时长
```

- `runtime` crate 不引入新的 GPUI 依赖
- SQLite 不参与运行时状态读写（纯内存）

---

## 7. 边界情况处理

| 场景 | 处理方式 |
|------|---------|
| 卡片已开启，尚未收到第一条 prompt | 标题显示 `alias` 或 profile 名（如 "Claude"），首次 prompt 到达后锁定替换 |
| Prompt 超过 60 字 | 截断并追加 `…` |
| `UserPromptSubmit` `raw_json` 中无 prompt 字段 | 保持当前标题，不报错，记录 debug 日志 |
| 网络中断后卡片恢复 | 展示上次锁定的标题，不报错 |

---

## 8. 非功能性需求

- 状态条渲染不引入新的 SQLite 查询（纯内存状态）
- 脉冲/旋转动画帧开销不超过 1ms/frame
- 运行时长定时器间隔 1s，完成/失败后停止

---

## 9. 风险与权衡

| 风险 | 应对 |
|------|------|
| `UserPromptSubmit` 的 raw_json prompt 字段名因 provider 不同而异 | 实现前先用 debug 日志打印 raw_json，验证字段名 |
| 若 provider hook 不传 prompt 内容 | 回退到 alias/profile 名策略，功能可降级 |
| 运行时长精度（GPUI 定时器精度） | 1s 精度足够，不需要毫秒级 |

---

## 10. 验收标准

- [ ] 并行 3 张 Agent 卡片，扫一眼能区分每张在做什么任务（任务标题不同）
- [ ] 卡片状态从 Thinking → RunningTool → Done 变化时，状态点颜色和摘要行实时更新
- [ ] 运行时长始终显示，完成/失败后定格
- [ ] Prompt 超过 60 字时正确截断并显示 `…`
- [ ] 卡片未收到第一条 prompt 时，标题合理回退为 profile 名
- [ ] Terminal 卡片标题栏展示当前命令行（截取前 60 字）
