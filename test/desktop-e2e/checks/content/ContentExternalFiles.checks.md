# 外部文件预览

运行 `bash scripts/desktop-e2e.sh content-external-files --visual`。

- 中文 Sapphire Night / 英文 Ink Black：从空内容标签开始，在真实 PTY 输出并点击项目外路径；路径目录同时包含中文与空格。
- TXT 正文、Markdown 标题、HTML 页面、PNG 图像显示正确；没有项目外错误。检查各语言的 `note.txt`、`note.md`、`page.html`、`PatternE2E.png` 截图。
- MP4 播放按钮出现后，经 AppKit 原始鼠标输入点击。相同纯画面区域播放前变化率小于 1%，播放后相隔 850ms 的变化率大于 5%（首次解码最多等待 6 组采样）；核对 `video-paused`、`video-playing-<组号>-a`、`video-playing-<组号>-b` 和播放后模型看图请求。不能仅凭播放器出现判定通过。
- 每次点击核对真实数据库：标签数量恰好增加一个、kind 为 file、selected 与完整外部绝对路径匹配；内置格式不得触发系统打开。
- PDF / DOCX：点击终端路径后，观察器记录准确路径与真实 `/usr/bin/open` 的成功退出，默认应用实际运行，内容标签不变；保存 `system-open.jsonl`。此项验证系统交接，不宣称 Norma 内置预览或系统阅读器正文验收。

设计参考 apple-design：保留现有标签导航与播放器的层级和平台习惯；打开文件立即进入既有加载状态，不增加弹窗或装饰动画。外部文件只读展示，未授权的相邻本地资源由协议测试验证不可读取。
