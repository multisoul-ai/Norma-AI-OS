# Dialogue 约束任务，CardTask 以证据完成

我们决定只收紧委派分支，不改造普通对话与流式 TTS。Dialogue 在委派时输出简约、逐目标的 `assignments`，代码解析真实 `agent_id`、生成 `TraceId` 与 `assignment_id`，并选择固定 `AllAssignmentsReceipted` 完成契约；普通回复继续沿用现有可见文本流。任务引用其他 Agent 或卡片时只携带稳定 `card_id` 等引用，目标 Agent 后续通过当前项目内的只读工具查询最新结构化工作摘要。委派类 CardTask 收到已验证的不可变 `assignments` 后，由代码状态机直接执行原子派发，不再让第二个模型决定是否调用派发工具。完成只由确定性代码依据真实回执判断；其他成功工具或模型停止输出均不能让委派成功。派发以 `TraceId + assignment_id` 保证幂等，并只在确认尚未写入目标 Agent 时自动重试一次。这样直接修复模型只查询卡片却未派发的问题，同时避免给普通语音回复增加新阶段。
