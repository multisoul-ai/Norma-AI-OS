# Launchpad 高清媒体性能实验报告

日期：2026-07-16
状态：实验完成，推荐方案已在隔离 worktree 实施并通过验证；实验脚本仍标记为 `PROTOTYPE — do not ship`
基线：`dev` / `10d7c4147a18e18385d99cf828c821b95cc20851`
隔离 worktree：`.worktrees/experiment-launchpad-hd-benchmark`
实验分支：`experiment/launchpad-hd-benchmark`

## 0. 实施结果

本报告的推荐方案已在上述隔离 worktree 落地，`dev` 工作区仍未被修改：

- Launchpad 默认素材由 1280×720 的 `wallpapers/home.mp4` 切换为真实 3840×2160、29.93fps H.264 的 `wallpapers/1406443.mp4`。
- 全屏渐变固定为实验验证过的 `balanced` 参数，左右合成不透明度约 23.6% / 26.5%；生产路径不依赖实验环境变量。
- `VideoDecoder` 保留 sample presentation timestamp，后台 worker 按 PTS 差值减去解码耗时后调度。首帧立即展示、循环回绕不阻塞、异常大间隔上限 250ms、缺失 PTS 才回退 40ms。
- 视频帧仍保持 420f、双平面、IOSurface-backed，并继续通过 GPUI `surface()` 上屏；没有引入每帧 CPU RGBA 或 `RenderImage/img()` 回退。
- 无实验环境变量启动真实应用后，连续 10 次采样平均 CPU 为 **4.06%**，平均 RSS 为 **137,352 KiB（约 134.13 MiB）**。相隔 1 秒的两张窗口截图 SHA-256 不同，确认动画持续推进。
- 串行 `cargo test -p app`：476 通过、0 失败、7 忽略；`cargo test -p e2e` 的三个测试目标共 59 通过、0 失败、3 忽略。编译检查、视频低拷贝、性能、存储边界、文件行数、禁用 `allow` 和指南行数护栏全部通过。

本次改造已在隔离 worktree 提交为 `77bf6da`，并随本报告合入 `dev`。

## 1. 结论

### 视频

在本机 M4 + Retina/4K 显示环境中，**4K H.264 + 现有 420f/IOSurface 低拷贝路径 + 轻遮罩**是画质最好的方案，同时没有测到相对 720p/1080p/1440p 的显著 CPU 回退。

- 720p、1080p、1440p、4K 的交错实机 CPU 中位分别为 **4.54%、5.09%、4.97%、4.77%**。最大差值 0.56 个百分点，低于本次高负载环境的波动，不能视为分辨率带来的真实差异。
- 同源全片 VMAF 分别为 **77.48、89.38、93.08、100.00**。当前 `home.mp4` 只有 1280×720，在 2784×1824 的实际窗口像素中需放大约 **2.18× / 2.53×**，源分辨率是主要清晰度瓶颈。
- 4K 原始 H.264 为 41.1 MB，仅比 1440p H.264 的 35.2 MB 大约 **16.8%**；在本机上这 5.9 MB 换来了最高画质，性价比高。
- 若需要更保守的跨机器默认值，选择 **1440p H.264、24/25fps、CRF 18–20**。它仍比 1080p 多 3.71 VMAF，单个 420f 表面约 5.27 MiB。
- 若包体/下载量优先，可选择 **1440p HEVC CRF18**：25.8 MB，比 1440p H.264 小 **26.6%**，VMAF 92.34（低 0.74）；实机 CPU 中位 3.51%，未观察到解码惩罚。但它应作为 macOS 硬件解码环境的存储优化项，不应宣称完全等质。
- 不建议本项目用 GIF 承载高清动态背景：调色板、分辨率、压缩效率和逐帧图片路径都不如 H.264/HEVC，且绕开已经验证的 420f `surface()` 路径。

### 静态图片

静态背景推荐 **4K 高质量 JPEG**，不要为了性能降到 720p。

- 同源 4K JPEG（q2、4:4:4）为 1.35 MB、SSIM 0.9928，中位解码 30.89 ms。
- 原始 4K PNG 为 10.61 MB、SSIM 1.0，中位解码 50.89 ms。
- JPEG 文件小 **87.3%**、一次性解码快 **39.3%**；两者解码后的 RGBA 占用都为 33,177,600 bytes（约 31.64 MiB），因此格式优化降低的是包体、读取和首次解码成本，不会降低最终纹理尺寸。

### 遮罩

当前 Launchpad 的两层渐变不是性能问题，但它是明显的观感问题。

- 当前左侧合成不透明度：`1 - (1 - 244/255) × (1 - 148/255) ≈ 98.2%`。
- 当前右侧合成不透明度：`1 - (1 - 200/255) × (1 - 223/255) ≈ 97.3%`。
- 实验 `balanced` 预设约为 23.6% / 26.5%。固定帧截图中，它能恢复背景细节，同时主要标题和卡片仍可读。
- `clear` 继续增加的可见细节有限，却降低左侧正文对比度，不建议直接无遮挡上线。
- ABBA 交错复测中，`balanced` 与 `current` CPU 中位为 5.91% / 6.54%；0.63 个百分点不足以证明 alpha 值有性能差异。遮罩调整应按视觉和可读性决策。

## 2. 当前画质降低的真正原因

1. **源视频过低**：`home.mp4` 是 1280×720；实际 Retina 窗口截图为 2784×1824 像素。播放器不能从 720p 恢复不存在的纹理细节。
2. **Launchpad 遮罩几乎覆盖源画面**：两层主题色渐变合成后达到约 97%–98% 不透明度。即使换成 4K，当前遮罩下细节也会被大幅洗掉。
3. **不是视频上屏路径在主动降采样**：真实 `VideoDecoder` 测试验证首末帧尺寸保持源尺寸，并且始终是 420f、双平面、IOSurface-backed；没有回退到每帧 CPU RGBA / `RenderImage`。
4. **Canvas 静态图没有额外透明度损失**：当前 `WALLPAPER_IMAGE_OPACITY = 1.0`，上下 shade alpha 都为 0。静态图的清晰度主要取决于原图尺寸和压缩质量。
5. **历史视频节奏曾忽略媒体时间戳**：原 worker 每解一帧后固定 sleep 40 ms，30fps 源会慢放。后续实施已改为 PTS 驱动；前台仍约每 40ms 取 mailbox 最新帧，因此可以丢弃过时帧而不拉长媒体时长。

## 3. 实验设计

### 环境

- 硬件：Mac16,1，Apple M4，10 CPU cores。
- 系统：macOS 15.6.1（24G90）。
- 显示：内置 Retina 3024×1964；Dell S2725QC 3840×2160。
- 项目：Rust 2021、GPUI 0.2.2、CoreVideo/AVFoundation。
- 真实窗口截图：2784×1824 像素。
- 并发噪声：实验期间存在一个已运行约 4 天的 `cargo-packager --release`，观察到约 53.6% CPU；系统 load average 曾为 14.26 / 33.97 / 40.65。未擅自终止该用户进程。

因此，顺序执行结果不能直接用于排序。本实验采用以下纠偏：

- 所有分辨率候选来自同一 4K 源 `1406443.mp4`，只改变一个变量。
- 质量使用全片逐帧 VMAF，而不是抽帧主观判断。
- 实机运行先预热 8 秒，再采 10 个 `ps` CPU/RSS 样本；每个候选先做 3 次。
- 发现顺序混杂后，对分辨率做交错复测，对遮罩做 ABBA 复测。
- CPU 采用每次运行 10 个样本的均值，再对运行均值取中位数。
- RSS 因异步初始化出现 110–544 MiB 多峰分布，无法可靠归因给媒体，报告不以 RSS 排名。

### 候选矩阵

| 候选 | 分辨率 | 编码 | fps | 文件大小 | VMAF |
|---|---:|---|---:|---:|---:|
| H.264 720p | 1280×720 | CRF18 | 30 | 12.05 MB | 77.48 |
| H.264 1080p | 1920×1080 | CRF18 | 30 | 22.22 MB | 89.38 |
| H.264 1440p | 2560×1440 | CRF18 | 30 | 35.19 MB | 93.08 |
| H.264 4K | 3840×2160 | 原始源 | 29.93 | 41.12 MB | 100.00 |
| HEVC 1440p | 2560×1440 | CRF20 | 30 | 19.99 MB | 90.94 |
| HEVC 1440p 近等质 | 2560×1440 | CRF18 | 30 | 25.84 MB | 92.34 |
| HEVC 1440p | 2560×1440 | CRF20 | 24 | 19.84 MB | 92.89 |

VMAF 基准为同一 4K 原始源；低分辨率候选使用 Lanczos 放回 4K 后比较。4K 原始源定义为 100。

## 4. 实机结果

### 分辨率交错复测（主证据）

| 候选 | 运行均值 1 | 运行均值 2 | CPU 中位 |
|---|---:|---:|---:|
| 720p H.264 | 4.74% | 4.33% | **4.54%** |
| 1080p H.264 | 5.70% | 4.48% | **5.09%** |
| 1440p H.264 | 5.22% | 4.71% | **4.97%** |
| 4K H.264 | 4.78% | 4.76% | **4.77%** |

结论：在本机的硬件解码与低拷贝路径下，分辨率没有形成可测的 CPU 单调增长。不能据此推断所有旧 Mac 都一样，因此生产上仍应保留 1440p fallback 或按屏幕/设备选档。

### 编码与帧率

| 候选 | CPU 中位 | 解释 |
|---|---:|---|
| 1440p H.264 CRF18 | 4.97% | 交错分辨率复测 |
| 1440p HEVC CRF20 / 30fps | 4.90% | 与 H.264 基本相同，质量较低 |
| 1440p HEVC CRF18 / 30fps | 3.51% | 三次为 3.51/3.60/3.34；非交错，不能宣称优于 H.264 |
| 1440p HEVC CRF20 / 24fps | 8.90% | 三次为 7.93/8.92/8.90；当前编码每帧复杂度更高，没有省 CPU |

24fps HEVC 的文件大小几乎没有下降，因为 CRF 是恒定质量而非目标码率；更少的帧获得了更多单帧比特。它的 VMAF 较高，但实机 CPU 更高，不是当前最佳综合方案。

### 解码吞吐微基准（辅助证据）

项目真实 `VideoDecoder` 连续解码 180 帧的结果波动很大，例如 720p 173–483fps、1080p 70–262fps。所有候选都高于目标 25fps，但该数据受到 VideoToolbox session、文件缓存和系统竞争影响，不能用于候选排序。它只证明：

- 所有候选均能及时供给目标播放节奏；
- 首末帧保持声明尺寸；
- 输出保持 420f、双平面、IOSurface-backed。

### 静态图

| 候选 | 文件大小 | SSIM | RGBA 展开 | 中位解码 |
|---|---:|---:|---:|---:|
| PNG 720p | 1.94 MB | 0.9560 | 3.52 MiB | 8.65 ms |
| PNG 1080p | 4.20 MB | 0.9823 | 7.91 MiB | 18.93 ms |
| PNG 1440p | 7.17 MB | 0.9916 | 14.06 MiB | 32.10 ms |
| PNG 4K | 10.61 MB | 1.0000 | 31.64 MiB | 50.89 ms |
| JPEG q2 1440p | 0.75 MB | 0.9890 | 14.06 MiB | 15.45 ms |
| JPEG q2 4K | 1.35 MB | 0.9928 | 31.64 MiB | 30.89 ms |

解码测试使用项目锁定的 `image 0.25`，先把压缩字节读入内存，再预热 1 次、采样 11 次取中位，模拟内置 `include_bytes!` 的实际路径。

## 5. 被复测推翻的初步结论

第一轮顺序运行曾得到“1440p current 遮罩 9.84% CPU、balanced 4.98% CPU”。两者 DOM 层数相同，这个结果可疑。ABBA 交错复测后：

- balanced 三次运行均值：5.91%、7.70%、5.13%，中位 5.91%；
- current 三次运行均值：6.54%、6.65%、5.05%，中位 6.54%。

差异缩小且随执行时段改变，因此原先的“遮罩让 CPU 翻倍”属于顺序混杂，已否决。保留这段记录是为了防止后续再次引用错误结论。

## 6. 固定帧视觉检查

为消除视频时间点差异，从 4K 源第 4 秒取帧，生成同一张 1440p 冻结视频，然后由真实 App 窗口分别截图：

- `/tmp/norma-launchpad-hd-benchmark/screenshots/frozen-1440-current.png`
- `/tmp/norma-launchpad-hd-benchmark/screenshots/frozen-1440-balanced.png`
- `/tmp/norma-launchpad-hd-benchmark/screenshots/frozen-1440-clear.png`

截图未纳入仓库，因为窗口包含本地 Recent Projects 路径。SHA-256：

- current：`423d34e1a9238031963a68860bdb93679237c70f8276fc5834f9ffa9879b31fd`
- balanced：`c1ace7e3ef99bb2dcf8f6ecc58a8f3412143c5ed51e2326b458495c48d86847d`
- clear：`5335971a6d28e6957fa2c3025b066dfd3873cd36aacaa20079b8150af583a560`

视觉判断：`current` 几乎看不到源画面；`balanced` 恢复主体轮廓和色彩；`clear` 对背景提升有限但文字对比下降。建议以 `balanced` 为起点，再针对左侧文案增加局部 scrim，而不是恢复全屏 97%–98% 遮罩。

## 7. 推荐方案与实施状态

1. **已实施 — 换真高清源**：Launchpad 使用真实 4K H.264。把 720p 放大到 4K 不会创造真实细节。
2. **已实施 — 降低全屏遮罩**：采用约 24%–27% 的 `balanced`；遮罩按视觉和可读性决策，不作为性能优化项。
3. **未纳入本次范围 — 静态图改 4K 高质量 JPEG**：当前格式白名单已支持 JPG/JPEG，无需新增格式路径，可在替换 Canvas 静态素材时单独执行。
4. **已保留 — 低拷贝视频架构**：AVFoundation → 420f CVPixelBuffer → IOSurface → GPUI `surface()`；护栏继续禁止每帧 `RenderImage/img()`。
5. **已实施 — 修正播放时钟**：按 sample presentation timestamp 推进，前台按约 25fps 取最新帧，不再因为 40ms 固定 worker sleep 慢放 29.93fps 素材。
6. **后续项 — 跨设备动态选档**：4K 屏 + Apple Silicon 可走 4K；低分辨率屏、旧机或节能模式可走 1440p。正式合并前建议至少补一台 Intel Mac 或较早 Apple Silicon 的复测。

## 8. 复现与原始数据

生成同源视频矩阵：

```bash
bash scripts/run-launchpad-hd-benchmark.sh --prepare-media
```

生成同源静态图矩阵：

```bash
bash scripts/run-launchpad-hd-benchmark.sh --prepare-images
```

查看冻结的实验计划：

```bash
bash scripts/run-launchpad-hd-benchmark.sh --plan
```

验证脚本契约和真实 H.264 冒烟编码：

```bash
bash scripts/test-launchpad-hd-benchmark.sh
```

运行视频低拷贝吞吐测试（示例）：

```bash
NORMA_HD_BENCHMARK_VIDEO=/tmp/norma-launchpad-hd-benchmark/media/source-2160-h264.mp4 \
NORMA_HD_EXPECTED_WIDTH=3840 \
NORMA_HD_EXPECTED_HEIGHT=2160 \
cargo test -p app benchmark_video_decoder_hd_matrix -- --ignored --nocapture
```

运行静态图解码测试（示例）：

```bash
NORMA_HD_BENCHMARK_IMAGE=/tmp/norma-launchpad-hd-benchmark/image-media/source-2160-q2.jpg \
NORMA_HD_EXPECTED_WIDTH=3840 \
NORMA_HD_EXPECTED_HEIGHT=2160 \
cargo test -p app benchmark_static_wallpaper_decode_matrix --lib -- --ignored --nocapture
```

原始 TSV 位于 `docs/verification/launchpad-hd-benchmark-data/`：

- `quality-summary.tsv`
- `runtime-results.tsv`
- `resolution-interleaved-results.tsv`
- `overlay-abba-results.tsv`
- `decode-results.tsv`
- `image-decode-results.tsv`
- `hevc-crf18-runtime-results.tsv`

## 9. 适用边界

- CPU 结论只对本次 M4/macOS 15.6.1/GPUI 0.2.2 环境直接成立；旧机需复测。
- RSS 被应用异步初始化和高系统负载污染，本报告只保留原始值，不以它做媒体排名。
- VMAF/SSIM 衡量源保真，不替代 UI 文字可读性检查；遮罩建议同时依据固定帧截图。
- 实验阶段没有直接修改 `dev` 工作区；验证完成后通过普通 merge 合入 `dev`。实验脚本的环境变量只驱动 ignored benchmark，生产 Launchpad 已移除对这些开关的依赖。
