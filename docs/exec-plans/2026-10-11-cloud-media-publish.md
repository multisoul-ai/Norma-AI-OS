# 云端可选媒体发布与验收

2026-10-11，按用户授权将轻量包需要的原始媒体发布到现有 GitHub Releases：[media-v1](https://github.com/multisoul-ai/norma-os-releases/releases/tag/media-v1)。

| 内容 | 数量 | 原始字节数 | 发布方式 |
| --- | ---: | ---: | --- |
| 动态壁纸（MP4 / GIF） | 7 | 113,723,884 | 每个文件按 SHA-256 命名，按需下载 |
| 教程视频（MP4） | 5 | 22,057,726 | 每个文件按 SHA-256 命名，按需下载 |
| 媒体合计 | 12 | 135,781,610（135.78 MB） | 原始字节，不转码 |
| 清单与校验附件 | 4 | 不计入上述媒体体积 | resources.json、media-catalog.json、media-assets.json、SHA256SUMS |

先创建草稿、上传，再核对附件后公开发布。Release ID 为 `409282524`，非预发布，不设为最新 App 版本。最新 App Release 仍为 `v0.1.73`（ID `406760540`）。本轮没有发布 App 安装包、修改 appcast 或推送源码。

## 验收证据

- 本地原始资源通过现有严格 manifest 校验；与已交付轻量 App 中的可信目录逐项一致。[发布输入](../research/assets/2026-10-11-cloud-media-publish/publish-inputs.json)。
- GitHub 上 16 个附件的文件名、大小及 SHA-256 digest 全部匹配。[附件核验](../research/assets/2026-10-11-cloud-media-publish/attachment-check.json)、[公开发布核验](../research/assets/2026-10-11-cloud-media-publish/published-check.json)。
- `python3 scripts/verify-release-media.py --profile light` 返回 0，匿名通过 App 固定 HTTPS URL 完整下载全部 12 个媒体，验证原始大小及 SHA-256。[下载日志](../research/assets/2026-10-11-cloud-media-publish/public-download-verification.log)、[验收记录](../research/assets/2026-10-11-cloud-media-publish/validation-summary.json)。
- Computer Use 操作独立真实 QA App，使用新的空缓存和原始 GitHub URL，没有本机传输地址覆盖。点击「潮汐」后观察到准备下载、已下载及动态背景自动应用；缓存为 8,650,397 字节，SHA-256 与可信目录一致。[运行环境](../research/assets/2026-10-11-cloud-media-publish/real-app-test-environment.json)、[缓存核验](../research/assets/2026-10-11-cloud-media-publish/real-app-download-check.json)。
- 验收结束后退出隔离 QA App；未修改用户的正式 App 数据。

![潮汐从 GitHub 下载完成并自动应用](../research/assets/2026-10-11-cloud-media-publish/cloud-wallpaper-ready.jpg)

![关闭设置后的实际动态背景](../research/assets/2026-10-11-cloud-media-publish/cloud-wallpaper-applied.jpg)

## 用户复测

已经安装壁纸反馈修复版轻量测试包的用户，可在「设置 → 壁纸」点击需下载资源或失败项重试。固定资源地址已上线，无需重新打包或重新安装。原测试 DMG 保持 64,867,254 字节，SHA-256 仍为 `d2debeed7f0b3eb60a90fdcf071d5b600722d83c0fee8ac3fc9e5e7134a46b47`；其 ad-hoc 签名、未公证状态不变。
