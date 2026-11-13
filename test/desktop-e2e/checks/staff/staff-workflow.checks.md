# 幕僚 M1 关键图

- 01-empty：按原型核对 280px 任务栏、单一顶部工作空间/模式导航、无重复标题或统计页脚、左栏项目筛选/搜索、48px 新任务入口、圆角多行输入与圆形发送键；三栏可读，不叠画布，项目别名与语言正确。
- multiline：普通 Enter 后两行文字仍在新任务草稿；Cmd+Enter 后 Task.messages 完整保存两行，未提前创建任务。
- 02-user：首条用户消息右对齐圆角气泡，完整保留两行原话，不与固定标题或 Worker 栏重叠。
- 02-proposal：Norma 头像和开放正文、圆角提议及紧凑动作；任务标题、状态、目标/理由/完整提议与采用按钮无裁切。
- edit-draft：输入框已有 EDIT-DRAFT 时，编辑提议追加 STAFF-M1-WORK，原草稿保留且不自动发送。
- 03-hover / 03-pressed：明确悬停与按下反馈，文字可读、尺寸不变，松开后才派发。
- dispatched / 04-result：紧凑 Worker 行含真实名称/引擎/状态，角色可悬停查看；可信结果归回原任务并自动待验收；历史提议的已派发按钮禁用。
- 04-search：输入框先存两行 DRAFT，再验证无匹配、Agent/标题搜索、Running/All 筛选与恢复；当前任务和草稿始终保留。
- wide-drawer：右栏拖至上限后中央至少约 320px，标题、Worker 卡、输入和发送无重叠，两行草稿保留。
- 04-narrow / breakpoint / compact：900/760px 隐藏右栏、保留约 280px 左栏和可用对话；600px 隐藏左右栏，任务图标可开浮层，浮层→全屏→单次Escape恢复；恢复 1280px 后抽屉状态保留，并恢复到 380px 宽。
- 05-artifact：原文件抽屉打开 Worker 真实产物 STAFF-RESULT.md，中左栏保留。
- 06-manual：自动完成、新消息重开后为 Active，手动固定阶段的说明与 STAFF MANUAL 回复可见。
- 07-fullscreen / escape：保留单一顶栏与退出入口，任务列表收起，任务与文件保留；标题避开原生窗口按钮；Escape 恢复。
- 08-canvas：两张独立卡，未多派发，文件标签保留。

无固定文件名的条目按 visual 请求中的 criterion id 查找原 PNG 与匹配 SHA 的 response。不得仅凭 exit 0 或 fixture 日志判视觉通过；逐张读取原图。
