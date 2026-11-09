# M3 真实 App 视觉证据

这些 PNG 直接复制自 desktop-e2e 原始窗口截图，未裁剪或重绘。交互与样式依据上级目录的 prototype.html；设计沿用 apple-design / emil-design-eng 的信息层级、直接反馈和克制动效。

| 图片 | 来源 | 已验证内容 |
| --- | --- | --- |
| approval.png | 2026-11-06T09-33-26Z / 02-approval.png | 黑底卡片、身份与项目层次、拒绝/允许/精确记忆授权；原请求消费与前台PID另外由E2E硬断言 |
| question.png | 2026-11-06T09-26-12Z / 00-direct-question.png | 提醒内直接选项、自由输入和提交；答案消费及操作反馈自动收回另外由E2E硬断言 |
| running.png | 2026-11-06T10-16-09Z / 02-cross-workspace.png | 跨空间摘要、头像/Provider、紧凑任务行与定位；重开原窗口宿主及Agent身份另外由E2E硬断言 |
| artifacts.png | 2026-11-06T11-17-52Z / 01-unread.png | 成果卡的空间/项目身份、文件标签与两类操作；冻结正文、已读跨重启及源文件保留另外由E2E验证 |

完整请求、SHA256、模型判据响应、回执与报告保存在对应 worktree 的 artifacts/desktop-workspace-e2e/ 时间目录。该记录只证明已运行范围；单张静态截图不能证明动画过程、系统快捷键、全屏Space或刘海真机。

M3不包含全局幕僚实现或旧左抽屉删除，分别属于M4与M5。输入框采用点击后激活App的既定退路；审批动作不激活App。最新完整进展与失败轮见 docs/exec-plans/2026-11-05-norma-dynamic-island-progress.md。
