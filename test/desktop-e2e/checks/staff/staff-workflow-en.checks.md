# 幕僚 M1 关键图

- 01-empty：按原型核对 280px 任务栏、单一顶部工作空间/模式导航、无重复标题或统计页脚、左栏项目筛选/搜索、48px 新任务入口、圆角多行输入与圆形发送键；三栏可读，不叠画布，项目别名与语言正确。
- multiline：Shift+Enter 后两行文字仍在新任务草稿；Enter 后 Task.messages 完整保存两行，未提前创建任务。
- 02-user：首条用户消息右对齐圆角气泡，完整保留两行原话，不与固定标题或 Worker 栏重叠。
- 02-proposal：Norma 头像和开放正文、圆角提议及紧凑动作；任务标题、状态、目标/理由/完整提议与采用按钮无裁切。
- edit-draft：输入框已有 EDIT-DRAFT 时，编辑提议追加 STAFF-M1-WORK，原草稿保留且不自动发送。
- 03-hover / 03-pressed：明确悬停与按下反馈，文字可读、尺寸不变，松开后才派发。
- 03-dispatched：（1）已派发提议为单行精简卡——宠物头像、任务名「实现验收文件」、「智能体名 · 引擎名」、绿色「✓ Dispatched」，不出现原话/理由/任务内容面板/智能体与模型下拉/采用、忽略、修改后发送按钮；（2）Worker 卡条胶囊最左侧是带眼睛与表情的宠物头像，不是字母缩写圆或纯色圆点。
- 04-result / worker-result-pet：紧凑 Worker 行含真实名称/引擎/状态，角色可悬停查看；Worker 结果卡头部同样以宠物头像开头，其后才是 Agent 名称与绿色 Result received；可信结果归回原任务并自动待验收。
- activity-waiting：本轮尚无工具步骤时，Norma 活动区为两行（第一行 Norma 圆形头像 + 名字 + Super-Agent；第二行只有状态图标与状态文字），不得出现「0 steps」「0.0s」后缀。
- spacing：Norma 长回复（粗体标题 + 两条列表）最后一行与紧邻其下的组件之间保留约 24px 空白；末行完整可见，不下沉、不贴住、不压叠下一条消息或提议卡。
- 04-search：输入框先存两行 DRAFT，再验证无匹配、Agent/标题搜索、Running/All 筛选与恢复；当前任务和草稿始终保留。
- wide-drawer：右栏拖至上限后中央至少约 320px，标题、Worker 卡、输入和发送无重叠，两行草稿保留。
- 04-narrow / breakpoint / compact：900/760px 隐藏右栏、保留约 280px 左栏和可用对话；600px 隐藏左右栏，任务图标可开浮层，浮层→全屏→单次Escape恢复；恢复 1280px 后抽屉状态保留，并恢复到 380px 宽。
- 05-artifact：原文件抽屉打开 Worker 真实产物 STAFF-RESULT.md，中左栏保留。
- 06-reopened：自动完成、新消息重开后存储 phase 为 Active；无运行中 Worker 时左栏显示等待中/Waiting，重开提示与 STAFF REOPEN 回复可见；标题没有阶段下拉。
- 07-fullscreen / escape：保留单一顶栏与退出入口，任务列表收起，任务与文件保留；标题避开原生窗口按钮；Escape 恢复。
- 08-canvas：两张独立卡，未多派发，文件标签保留。

无固定文件名的条目按 visual 请求中的 criterion id 查找原 PNG 与匹配 SHA 的 response。不得仅凭 exit 0 或 fixture 日志判视觉通过；逐张读取原图。

## 消息卡片与录像

- markdown：粗体 STAFF PLAN、两条列表文字和行内代码均可读，不能只有项目符号或直接显示 Markdown 标记。
- pending / choices / cards-01-answers：两张并发卡及左栏待答数量；顶部视图里 Dark 单选与 Keyboard shortcuts 打勾，下移后 Accessibility 与该多选同时打勾、Private build label 为掩码圆点；跳过一张后提交可用。存储不得含密码明文，原工具仅收到 decline。
- ignored / restore / cards-02-restored：忽略后虚线单行，展开可恢复，恢复后采用按钮重新可用，未自动派发。
- activity / cards-03-tools：完成摘要含步数和耗时，无残留 spinner；拒绝调用为橙色，展开入参与拒绝输出，长参数截断。
- worker-question：真实 Worker Hook 镜像到幕僚，提交后原 Hook 恰好消费一次，镜像进入已回答。
- worker-needs-you-pet：Worker 提问时卡条显示橙色 Needs you，胶囊最左侧为带表情的宠物头像（不是字母圆）。
- staff-conversation.mp4：连续真实窗口录像覆盖提问、提议、忽略/恢复、工具活动与 Worker 回答；检查统计 JSON、视频时长与抽帧方向。
