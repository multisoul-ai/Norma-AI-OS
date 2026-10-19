# 文件工作台视觉验收

- 01-file-tree.png：项目选择紧凑；文件筛选、Git 变更和完整目录层级清晰；窄抽屉列表不横向溢出。
- 02-text-preview.png：正文可读；目录可收起；标题、编辑、系统打开入口不重叠。
- 03-file-saved.png：保存反馈清楚，编辑区和画布互不抢焦点；检查磁盘断言。
- 03b-git-committed.png：Git 完成反馈可见；索引与提交必须通过夹具中的真实 Git 断言。
- 04-markdown-preview.png：标题、表格以富文本渲染；WebView 不覆盖抽屉工具栏。
- 05-html-preview.png：HTML 正文显示，原生视图贴合内容区，未遮挡顶部标签。

补充人工验收：鼠标/键盘状态；视频拖动进度；图片平移；外部站点登录状态与失败回执。

- 06-image-preview.png：测试图像完整可见，系统按钮清晰。
- 07-image-zoom.png：放大后可平移，不遮挡工作台标题。
- 08-video-preview.png：系统中央播放入口位于抽屉内，后续截图验证实际播放。

- 08a-video-playing.png、08b-video-progress.png：动态画面与进度发生变化，原生控件完整位于内容区。
- 08c-video-returned.png：离开期间暂停；返回时进度约 2 秒，不能在后台继续推进至结束。
- 09-browser-first.png：本机 HTTP 页面加载，地址栏和正文互不遮挡，表单内容已输入。
- 10-browser-retained.png：切换文件标签后返回，表单仍为 RETAINED E2E。
- 11-browser-second.png：链接导航成功；脚本继续验证后退、前进与刷新，并检查 SQLite 最终地址。
- 12-light-document.png、13-light-file-tree.png：Paper Blue 下正文、目录、选择器及工具按钮清楚可辨。
