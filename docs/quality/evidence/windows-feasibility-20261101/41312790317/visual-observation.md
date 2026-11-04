# GPUI 示例原图核验

- Run：41312790317；attempt：1。
- 源码：e29f676e6fce66894b87a8ab7fb1e23e3538d8b0。
- 原图：`docs/quality/evidence/windows-feasibility-20261101/41312790317/graphics/gpui-window.png`。
- SHA-256：be835bf81ca389d4bc2be113c023046cba657bdabddafbbb44095ffb2e078f95。

当前 Agent 已用图像工具直接读取 640×560 原图。
图中有 Windows 窗框、灰色内容区、Hello, World! 文本，以及红、绿、蓝、黄、黑、白六个色块。
文本和色块清晰可见，不是全黑窗口或仅有 PNG 文件。

自动报告另验证该窗口属于示例进程、已取得前台；红/绿/蓝采样为 100/100/267。
OS Alt+F4 后窗口销毁由自动报告证明，单张原图不能证明键盘关闭行为。

这里只核验钉死 GPUI 0.2.2 的独立示例。
真实 Norma 的画布、终端、输入、主题、持久恢复以及 Windows 11 设备仍未验收。
