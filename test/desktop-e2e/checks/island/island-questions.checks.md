# 岛问答

- `00-direct-question.png`：首次提醒直接显示Blue/Green与自由输入，输入和发送同行，无“回答”中间步骤及无关运行空态。
- `01-choice.png`：Choose color 1原题、Blue单选选中、Green未选、发送按钮可用。
- `02-custom-reply.png`：Choose color 2的自由回复完整包含Cobalt blue及其空格。
- `01-submitted.png`、`02-submitted.png`：实际发送后成功反馈与原Hook一致，不能误报未送达或保留可重复提交的表单。
- 硬断言：原Hook分别仅消费一次Blue/Cobalt blue，tool_use_id精确匹配；不能出现普通PTY任务输入。
- 脚本Agent通过生产Hook运行，不等于真实Claude模型验收；Input点击激活是M0确认后的退路。
