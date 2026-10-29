# Feature Guide 终版演示

更新日期：2026-10-29。来源为用户提供的 `demo/终版/` 九段真实产品录屏。
功能引导与 `/help` 共用这一组资源；中英文说明对应同一录屏，保留原始画面、字幕和声音。

MP4 保留完整时长与原速，压缩为宽 1280 像素的 H.264 / yuv420p（CRF 23、slow），音频 AAC 96k，并启用 faststart。
WebP 取同段录屏的中间帧（宽 1280，quality 85），用于加载回退和“减少动态效果”；不是动图。
App 沿用后台解码和低拷贝 `surface()` 播放，支持暂停、隐藏停止与静态回退。

| 资源前缀（MP4 / WebP） | 原始文件 | 内容 |
| --- | --- | --- |
| `demo-voice-command` | `语音交互.mp4` | 语音交互 |
| `demo-agent-collaboration` | `05-multi-agent-collaboration.mp4` | 多 Agent 协作 |
| `demo-live-node-shortcuts` | `01-quick-agent-cards.mp4` | 快速创建卡片 |
| `demo-layout-command-m` | `02-cmd-m-expand.mp4` | 卡片展开 |
| `demo-chief-of-staff` | `幕僚.mp4` | 幕僚 |
| `demo-right-drawer` | `03-right-drawer.mp4` | 右侧抽屉 |
| `demo-switch-workspaces` | `04-switch-projects-workspaces.mp4` | 项目与工作空间 |
| `demo-workspace-home` | `07-home-create-search-workspace.mp4` | 工作空间首页 |
| `demo-slash-commands` | `08-slash-commands.mp4` | 斜杠指令 |

完整源文件与输出 SHA-256、字节数及 GIF 片段范围见 `docs/assets/readme/final-demos.json`。
发布白名单与哈希在 `packaging/media/resources.json`；full 与 wallpapers-on-demand 包含全部视频，light 保留全部静态海报。

生成命令（`SOURCE` 为单段原片，`OUTPUT` 为资源前缀）：

```bash
ffmpeg -i "$SOURCE" -map 0:v:0 -map '0:a?' -vf scale=1280:-2 \
  -c:v libx264 -preset slow -crf 23 -pix_fmt yuv420p \
  -c:a aac -b:a 96k -movflags +faststart "$OUTPUT.mp4"
# 中间帧先由 FFmpeg 导出 PNG，再用 Pillow 保存 WebP（quality=85）。
```
