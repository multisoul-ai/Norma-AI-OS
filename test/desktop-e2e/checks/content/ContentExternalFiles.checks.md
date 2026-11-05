# 外部文件预览

运行 `bash scripts/desktop-e2e.sh content-external-files --visual`。

- 中文 Sapphire Night / 英文 Paper Blue：普通文件标签恢复后，外部文本直接显示正文。
- Markdown 标题、HTML 页面、PNG 图像显示正确；没有项目外错误。
- MP4 能播放，前后截图时间推进或正常播完 12 秒。
- 每次文件切换后核对真实数据库的 selected 标签和原始资源路径。

设计参考 apple-design：保留现有标签导航与播放器的层级和平台习惯；打开文件立即进入既有加载状态，不增加弹窗或装饰动画。外部文件只读展示，未授权的相邻本地资源由协议测试验证不可读取。
