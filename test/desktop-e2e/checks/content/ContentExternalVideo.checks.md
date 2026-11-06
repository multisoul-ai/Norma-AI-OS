# 项目外视频定向验收

运行 `bash scripts/desktop-e2e.sh content-external-video --visual`。

复用 `ContentExternalFiles` 的真实终端路径点击与 `ContentExternalPlayback` 的播放断言，仅缩小文件范围到 MP4；仍覆盖中文 Sapphire Night 和英文 Ink Black。

- 从空内容标签开始，点击含中文与空格的项目外视频路径后，仅创建并选中该准确路径的 file 标签。
- 播放前为静止对照，无错误或持续加载。
- 经 AppKit 原始点击，连续纯画面采样变化率大于 5%，不得以控制条变化或计时单独判定播放成功。
- 实际画面为蓝紫背景与黄色块，无黑屏或解码错误；查看每组播放截图和最终模型看图结果。

这是视频子集，不代表 TXT、Markdown、HTML、PNG 或 PDF/DOCX 系统交接通过。
