# GPUI 示例原图核验

- 运行：[41297206819 / attempt 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819)。
- 源码：`58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4`。
- 原始截图：`graphics/gpui-window.png`，640×560；未编辑图片。
- 核验方式：Agent 多模态读取下载后的原图，与 CI 像素报告交叉检查。
- 可见内容：Windows 窗口、Hello, World! 文本、红绿蓝黄黑白六个色块。
- CI 报告：前台窗口归属正确；RGB 采样分别为 100、100、267。
- CI 的 `gpui_os_keyboard_close` 为 PASS，OS Alt+F4 后目标窗口销毁。

这只证明 vendored GPUI 示例的基础渲染、窗口捕获和 OS 键盘路径。
没有操作真实 Norma 画布、终端、输入框或 Agent。
不能将此记录作为 AC04 或 AC09 的产品验收通过证据。
