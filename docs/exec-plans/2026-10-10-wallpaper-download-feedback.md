# 动态壁纸下载反馈修复与验收

用户反馈：轻量包切换动态壁纸没有生效，素材库未说明哪些壁纸需要下载，也没有直观下载状态。

## 行为与证据

| 场景 | 修复后的行为 | 验收证据 |
| --- | --- | --- |
| 尚未选中的云端壁纸 | 缩略图下显示「需下载」与文件大小 | [深色列表](../research/assets/2026-10-10-wallpaper-download-feedback/01-missing-dark.png) |
| 保存了尚未就绪的选择 | 显示「所选壁纸 · 待应用」「已保存选择 · 下载后生效」，保留保存错误 | [待应用与重试](../research/assets/2026-10-10-wallpaper-download-feedback/03-pending-retry-dark.png)、实际页面回归测试 |
| 后台下载 | 每张壁纸独立显示百分比及进度条；当前选择提供取消入口 | [深色进度](../research/assets/2026-10-10-wallpaper-download-feedback/06-downloading-library-dark.png)、[浅色进度](../research/assets/2026-10-10-wallpaper-download-feedback/08-downloading-unselected-light.png) |
| 下载或资源源不可用 | 缩略图显示「下载失败 · 点按重试」，当前选择提供重试入口 | [深色失败](../research/assets/2026-10-10-wallpaper-download-feedback/02-failed-library-dark.png)、[浅色失败](../research/assets/2026-10-10-wallpaper-download-feedback/09-failed-library-light.png) |
| 取消后重试 | 显示「下载已取消」，鼠标重试可再次下载 | [取消反馈](../research/assets/2026-10-10-wallpaper-download-feedback/07-cancelled-current-dark.png) |
| 收完字节但尚未校验 | 显示「正在校验…」，不提前显示下载成功 | 实际页面回归测试覆盖；真实下载该状态很短，未单独截取 |
| 可信资源下载成功 | 显示「已下载」，自动应用所选动态背景 | [完成列表](../research/assets/2026-10-10-wallpaper-download-feedback/05-ready-library-dark.png)、潮汐缓存大小与 SHA-256 校验 |
| 下载途中改选另一张 | 后台资源完成后更新自己的状态，保持用户的新选择 | [宁静下载中、已改选潮汐](../research/assets/2026-10-10-wallpaper-download-feedback/10-switched-during-download-light.png)、[完成后仍为潮汐](../research/assets/2026-10-10-wallpaper-download-feedback/11-late-download-keeps-selection-light.png) |

## 实现与验证

延续现有后台下载、校验和自动应用机制。本次新增素材库逐项反馈、可信目录大小读取、待应用文案与中英文翻译。状态读取只访问内存，不在 GPUI 前台做文件或网络 IO。采用 Apple 的即时反馈与清晰状态原则，并沿用 Material 选择器的紧凑布局、语义色和选中反馈；没有改变视频低拷贝上屏路径。

- TDD：先运行真实壁纸设置页渲染回归，旧页面缺少云端标记导致断言失败；实现后运行壁纸相关测试，59 通过、0 失败、1 个既有性能基准忽略。[最终运行日志](../research/assets/2026-10-10-wallpaper-download-feedback/app-wallpaper-final-run.log)。
- `cargo check -p app` 通过；`cargo test -p ui` 的 7 个单元测试和 5 个集成测试通过。[UI 测试日志](../research/assets/2026-10-10-wallpaper-download-feedback/ui-tests.log)。
- 格式、diff 空白、单文件行数、禁止 allow、Agent 指南行数、性能护栏、视频低拷贝护栏通过。14 对中英文键及插值占位符一致。
- 独立审查发现待下载选择可能遮蔽保存失败，已修复并补回归；复核无新增 P1/P2。
- Computer Use 操作真实的独立 QA `.app`，分别检查深浅主题、未下载、404 失败、慢速进度、取消、鼠标重试、成功自动应用及迟到下载。没有使用已禁用的旧 e2e 套件，也未修改用户正在运行的 App 或其数据。

真实 QA 使用 `media-download-qa` feature、独立 bundle ID 与 `/private/tmp` 数据目录；本机 HTTP 只替换传输地址，目录中的原始文件大小和哈希不变。源文件未经转码。QA 下载证据不代表 GitHub 资源已经发布。

| 原始资源 | 下载到本机的字节数 | 校验结果 |
| --- | ---: | --- |
| 潮汐 `1403512.mp4` | 8,650,397 | SHA-256 与可信目录一致：[记录](../research/assets/2026-10-10-wallpaper-download-feedback/downloaded-tide.json) |
| 宁静 `1407174.mp4` | 9,453,907 | SHA-256 与可信目录一致：[记录](../research/assets/2026-10-10-wallpaper-download-feedback/downloaded-serenity.json) |

关闭设置后，通过真实 App 的两张截图观察雨景播放帧变化：[帧 A](../research/assets/2026-10-10-wallpaper-download-feedback/12-tide-playing-frame-a.png)、[帧 B](../research/assets/2026-10-10-wallpaper-download-feedback/13-tide-playing-frame-b.png)。QA App 已正常退出，本机资源服务器已停止。

## 云端与交付边界

2026-10-10 本轮检查潮汐的 GitHub Releases 固定地址返回 HTTP 404：[检查记录](../research/assets/2026-10-10-wallpaper-download-feedback/production-source-head.json)。轻量包需要的远端资源仍须发布，界面修复不能让不存在的资源变得可下载。本轮未上传或发布任何资源。

交付包使用正常 `production-data` release 构建，关闭 QA feature，再通过 `release-local.sh --ad-hoc` 和 `light` 资源配置打包；版本保持 0.1.73，不公证。最终包路径、大小、SHA-256 与校验结果随测试说明保存。此前压缩实验与编译矩阵的结果不作为本轮新包的实测结果。

## 最终本地安装包（2026-10-11）

- [轻量 DMG](/Users/cengwei/Documents/code/Norma-AI-OS-worktrees/package-size-reduction/build/dev/wallpaper-feedback-20261011/Norma-OS_0.1.73_wallpaper-feedback_light_aarch64.dmg)：64,867,254 字节（64.87 MB）。
- SHA-256：`d2debeed7f0b3eb60a90fdcf071d5b600722d83c0fee8ac3fc9e5e7134a46b47`。
- [测试说明](/Users/cengwei/Documents/code/Norma-AI-OS-worktrees/package-size-reduction/build/dev/wallpaper-feedback-20261011/测试说明.txt)、[包内核验记录](../research/assets/2026-10-10-wallpaper-download-feedback/contents-validation.json)、[最终打包日志](../research/assets/2026-10-10-wallpaper-download-feedback/package-final.log)。
- 正常生产构建与最终打包均返回 0，主程序 UUID 与 production-data 构建一致。App 的深度/严格签名检查和 DMG 完整性校验通过；只读挂载核对 102 项文件/符号链接一致，校验后已卸载。
- ad-hoc 签名，未公证，Gatekeeper 评估未接受、无 stapler 票据；版本未变化。
- 首次打包缺少 Sparkle 公钥，已沿用上一个测试包的公钥；第二次遇到旧测试卷挂载冲突，正常卸载该测试卷后重试。最终记录不将这些失败尝试称为成功。

## 云端上线更新（2026-10-11）

用户随后授权发布云资源，现已公开发布 [media-v1](https://github.com/multisoul-ai/norma-os-releases/releases/tag/media-v1)：7 个动态壁纸、5 个教程视频，共 135,781,610 原始字节。全部 12 个公开 HTTPS 下载已通过大小与 SHA-256 校验，真实隔离 App 从 GitHub 下载「潮汐」后显示已下载并自动应用。上述 404 是上线前的历史检查结果，当前测试包可直接重试，无需重装；安装包字节、版本与签名状态不变。[本次发布及验收记录](2026-10-11-cloud-media-publish.md)。
