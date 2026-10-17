# macOS 发布与公证

现行打包、签名和公证命令以 [packaging.md](packaging.md) 为准。本文只保留已经核对过的决策和验收，不重复步骤清单。

<a id="version-bump"></a>

## 版本递增

> 历史：见 git 提交。原文路径：`docs/superpowers/plans/2026-07-01-auto-release-version-bump.md`。


2026-07-01 的计划曾要求在 `before-packaging-command` 里递增 patch。2026-08-05 的发布计划改为：打包不得在已解析配置后改版本；发布成功后的独立命令才递增下一 patch。以后者为准。

<a id="sparkle"></a>

## Sparkle 与 GitHub Releases

> 历史：见 git 提交。原文路径：`docs/exec-plans/2026-08-05-github-sparkle-auto-update-release.md`。


已确认的边界：

- 更新源首期固定为 `https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/appcast.xml`。
- Sparkle 发行版固定 `2.9.5`，归档 SHA-256 为 `015336b601493e05c237964954bff6191370003d94edefe663724c88840d73cc`。
- 应用持有一个 Sparkle 2 updater。Framework 缺失时开发构建应安全降级。独立 Software Update 页面已移除，检查更新留在应用菜单和 Settings 品牌卡片。
- 工作流只从 `main` 手动触发。先上传 Draft Release 并核验，再公开。同 tag 冲突必须失败，禁止静默覆盖。
- 最终公证并 staple 的 DMG 用 Sparkle EdDSA 签名，`appcast.xml` 引用不可变 tag URL。

2026-08-05 当时写明：工作流和凭据已配置，尚未提交或触发那一次真实发布，首次更新验证仍要按 A → B 两个版本执行。这句是当时状态，不改写成此后某一次发布已经完成。

<a id="notarize"></a>

## Developer ID 签名与公证

> 历史：见 git 提交。原文路径：`docs/design-docs/2026-07-05-developer-id-notarize-design.md`、`docs/exec-plans/2026-07-05-developer-id-notarize.md`。


- 分发路线是 Developer ID，再 codesign、notarize、staple。不走 Mac App Store，也不使用 App Sandbox。
- Team ID 为 `2BF8G9AN4L`。公证 API Key 必须与 Developer ID 证书属于同一 team。
- 2026-07-05 的设计状态是「已批准、待实施」，执行计划用 ad-hoc 签名验证流水线，并写明 Developer ID 证书当时尚未签发。
- 当前 `Packager.toml` 与 [packaging.md](packaging.md) 已描述 Developer ID 签名；默认打包会清掉公证环境变量，只有显式 `--notarize` 才提交公证。证书操作步骤不在本次未提交的本地笔记里重复。

<a id="media-v1"></a>

## 可选媒体发布

> 历史：见 git 提交。原文路径：`docs/exec-plans/2026-10-11-cloud-media-publish.md`。


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
