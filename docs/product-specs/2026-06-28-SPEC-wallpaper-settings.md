# Wallpaper Settings SPEC

> 日期：2026-06-28 · 状态：已确认，待实施
> 来源：采访式需求梳理（interview-spec）+ GPUI 0.2.2 源码可行性验证

## 1. 背景与目标

Multisoul Desktop 当前壁纸写死为内置单张 PNG（`crates/app/assets/wallpapers/1331865.png`），唯一切换途径是环境变量 `MULTISOUL_WALLPAPER`（见 `crates/app/src/app/wallpaper.rs`）。用户无法在应用内更换壁纸。

**目标**：在 Settings 面板内提供壁纸切换，支持三种来源——静态图（PNG/JPG）、动图（GIF）、循环视频（mp4/mov），并提供 **10 个开箱即用的内置素材**，选择重启后持久化。

## 2. 范围

### 2.1 In Scope
- Settings 侧边栏新增独立 **Wallpaper** 项：缩略图网格 + "Import…" 按钮。
- **内置库 10 个素材**（2 PNG + 2 GIF + 5 mp4 + 现有默认 PNG），全部随 App bundle 分发。
- **混合打包**（实现期修订，见 §9）：图片内置项（PNG/GIF）走 `include_bytes!` 编进二进制（GPUI `AssetSource` 需 `ImageSource::Resource` 服务缩略图/背景）；视频内置项（mp4）作为 bundle 独立资源文件（`Contents/Resources/wallpapers/`），不编进二进制（体积过大）。开发期视频路径回退 `CARGO_MANIFEST_DIR/assets/wallpapers/`。
- 自定义导入：拷贝到 App 数据目录，可选中、可删除（内置不可删）。
- 三类渲染：静态图 `img()`、GIF `img()`+原生多帧、视频 AVFoundation + `surface()` 零拷贝。
- 持久化（复用 `settings_store`）。
- 加载失败静默回退默认 + `tracing::warn!`。

### 2.2 Out of Scope
- 多壁纸目录批量扫描；每个 Workspace 独立壁纸；联网下载。
- 非 macOS 视频壁纸（`surface()` 仅 macOS）。
- 壁纸缩放/对齐/虚化调整；可见性暂停/省电开关；视频音轨。
- 内置素材的转码/压缩（用原文件，压缩列为实现期优化）。

## 3. 用户与使用场景

1. Settings → Wallpaper → 选内置视频 `1406489.mp4` → 循环播放 → 重启仍播放。
2. 选内置 GIF `220240.gif` → 循环动画 → 重启仍循环。
3. Import 本地 mp4 → 循环播放 → 重启仍播放。
4. 删当前在用自定义项 → 回退默认。
5. 导入损坏/非媒体 → 不 panic，回默认 + warn。

## 4. 业务流程与信息架构

- **切换**：点选 → 写偏好 → 更新 `wallpaper` → `cx.notify()` → 重绘。
- **导入**：文件选择器 → 校验+类型识别 → 拷入数据目录 → 刷新列表 → 自动选中。
- **删除**：删数据目录文件 → 若是当前选中回默认 → 刷新列表。
- **壁纸类型路由**（`app` 层）：按扩展名+文件头判 `WallpaperKind::{Static, Gif, Video}` → 决定走 `img()` 还是 `surface()`。

## 5. 数据模型与接口

```rust
pub(crate) enum WorkspaceWallpaper {
    Embedded(&'static str),   // 内置：bundle 资源相对路径 "wallpapers/xxx.mp4"
    File(PathBuf),            // 自定义：数据目录路径
}
```
新增 `WallpaperKind` 与"资源解析"：内置项从 bundle resources 路径解析，自定义项从数据目录解析。

**内置库清单**（静态配置，固定 10 项，源自 `/Users/alan/Documents/壁纸/mut/` + 现有默认）：

| 文件 | 类型 | 尺寸/大小 |
|---|---|---|
| `1331865.png` | PNG | 现有默认壁纸，保留 |
| `1350954.png` | PNG | 5824×3264, 19MB |
| `1358524.png` | PNG | 2912×1632, 7MB |
| `215824.gif` | GIF | 640×400, 0.4MB |
| `220240.gif` | GIF | 444×250, 14MB |
| `1403512.mp4` | Video | 8.6MB |
| `1404942.mp4` | Video | 30MB |
| `1406443.mp4` | Video | 41MB |
| `1406489.mp4` | Video | 52MB |
| `1407174.mp4` | Video | 9.5MB |

**持久化**（复用 `settings_store` kv，`persist_string` 套路）：
- `wallpaper.kind` ∈ {`embedded`,`file`}
- `wallpaper.value` = 资源相对路径(内置) 或 数据目录文件名(自定义)
- 自定义项列表真源 = 数据目录现有文件，启动扫描重建。

**数据目录**：`~/Library/Application Support/Multisoul/wallpapers/`（实现前核实 app data dir 约定）。

**新增依赖**（`crates/app/Cargo.toml`）：
- `image = "0.25"`（导入校验/GIF 读取，与 gpui 同小版本复用同一编译产物）
- `core-video = "=0.4.3"` + `features=["metal"]`（**版本精确锁 `=0.4.3`**，与 gpui 一致，否则 `CVPixelBuffer` 类型不匹配，`Into<SurfaceSource>` 编译失败；gpui 不 re-export）
- `objc2` + `block2` + AVFoundation 绑定（`AVAssetReader` 解码；实现期核实 objc2 生态具体 crate 名与覆盖度）

## 6. 技术实现概览

**架构边界**：渲染与解码全留 `app` 层（`wallpaper.rs` + `chrome.rs` + 新增 `video_decoder` 模块 + 内置库配置），不下沉 domain crate。偏好持久化借道 `settings_store`。

**打包迁移**：
- `crates/app/assets/wallpapers/` 下放全部 10 个素材文件（复制自 `/Users/alan/Documents/壁纸/mut/`，保留现有 `1331865.png`）。
- PNG/GIF 走 GPUI asset source（`include_bytes!` 嵌入，`ImageSource::Resource` 解析），**视频不走 asset source**——视频由资源**文件路径**直接喂 `AVAssetReader`。内置视频项 `Embedded(&str)` 语义 = "资源相对路径"，运行时解析：打包态 `Contents/Resources/wallpapers/<file>`，开发态回退 `CARGO_MANIFEST_DIR/assets/wallpapers/<file>`。
- 打包脚本（`scripts/copy-wallpaper-resources.sh`）：release 时把 `assets/wallpapers/*` 拷进 `.app/Contents/Resources/wallpapers/`（供视频文件路径解析）。图片已通过 `include_bytes!` 在二进制内，不需额外拷贝。
- 现有默认壁纸 `1331865.png` 保留 `include_bytes!` 嵌入（与其它图片内置项一致）。

**三条渲染路径**：

1. **静态图（PNG/JPG）**：`img(image_source)`，`ObjectFit::Cover`。
2. **GIF**：GPUI 原生多帧。`ImageSource::Resource` 经 `GifDecoder::into_frames()` 装入单个 `RenderImage`，按 `delay()` 自驱动 `request_animation_frame()` 循环，sprite atlas 缓存纹理。**硬要求：`chrome.rs:34` 壁纸 `img()` 必须加 `.id("wallpaper")`**——GPUI 多帧自循环条件是 `global_id.is_some() && frame_count > 1`，缺 id 卡在第 0 帧。
3. **视频**：AVFoundation + GPUI `surface()` 零拷贝。

**视频路径硬约束**：
- 像素格式 = `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`（420f 双平面 YUV）。`metal_renderer.rs:1085`/`blade_renderer.rs:832` 均 `assert_eq!`，传 BGRA/420v/其它直接 panic。AVAssetReader `outputSettings` 必须显式设 420f（GPU 直吃 Y+CbCr 双平面，零 CPU 颜色转换）。
- 无自驱动：`surface()` 的 `request_layout/paint` 无 `request_animation_frame` 循环，`Surface::id()` 返回 `None`。帧推进 100% 靠 `cx.spawn` + `background_executor().timer(Duration::from_millis(40))`（25fps）换 entity 持有的 `CVPixelBuffer` + `cx.notify()`。每次重绘 `paint` 重新读当前 buffer。**非 vsync 对齐**。
- 不需 `.id()`；样式 `surface(buf).object_fit(ObjectFit::Cover).absolute().size_full()`（默认 Contain 会留黑边）；无圆角靠外层 div 裁切；buffer 单帧持有自然释放。
- macOS-only：`SurfaceSource` 非 mac 为空枚举，`surface()` 不可构造。视频壁纸整体 `#[cfg(target_os="macos")]`。

**约束**：`gpui=0.2.2` pinned；500 行单文件上限（视频解码独立模块，避免 `chrome.rs`/`wallpaper.rs` 超限）；无 `#[allow(...)]`；TDD 先行。

## 7. UI/UX 需求

- 侧边栏新增 `SettingsSection::Wallpaper`（icon/index 同现有枚举）。
- 缩略图网格：10 内置 + 自定义统一展示，当前选中高亮。
- 缩略图首帧静态展示（GIF/视频不播放动画，避免多动图同播性能问题）；视频缩略图用 `AVAssetImageGenerator` 取首帧。
- "Import…" 按钮 → 原生文件选择器（png/jpg/gif/mp4/mov）。
- 自定义项删除入口（hover 按钮），内置不可删。
- 切换即时生效，无确认弹窗。

## 8. 状态、错误与边界情况

| 场景 | 行为 |
|---|---|
| 导入非媒体/损坏文件 | 静默回退默认 + `tracing::warn!` |
| 视频解码失败/不支持的编码 | 回默认 + warn，不 panic |
| bundle 资源缺失（如开发期未拷贝） | 回默认 + warn，提示资源未找到 |
| 数据目录文件被外部删（启动时） | 偏好指向 file 不存在 → 回默认 + warn |
| 删当前在用自定义项 | 回默认 |
| 拷贝到数据目录失败 | 回默认 + warn，导入视为失败 |
| 首次启动/偏好未设 | 用 `1331865.png` 默认 |
| 视频 timer 在窗口关闭后 | `cx.update` 返回 Err → `break`，避免 task leak |
| CVPixelBuffer 格式非 420f | 解码层断言拦截，绝不传给 surface |

## 9. 非功能性需求

- **性能**：GIF 每帧首次上传后复用 atlas；视频 420f 零拷贝（CVMetalTextureCache 直采 Y/CbCr 双平面），无 CPU→GPU RGBA 拷贝、无 atlas 膨胀，25fps timer。始终循环（已接受电池权衡；未来可加可见性暂停，列为未来项）。
- **包体积**：图片内置项经 `include_bytes!` 编进二进制（增 ~40MB，3 PNG + 2 GIF）；视频内置项作为 bundle 资源文件（5 mp4 ~141MB），不进二进制。App bundle 总增 ~168MB（实现期评估压缩/降分辨率，列为优化，可考虑精选 2-3 个 mp4）。dev 模式下 `cargo run` 二进制即含 ~40MB 图片；release bundle 另含视频资源文件。
- **安全/权限**：仅读用户主动选择文件；仅写应用自身数据目录；视频静音。
- **可维护性**：内置库静态配置；视频解码模块独立。

## 10. 风险、权衡与未决问题

- **包体积 ~168MB**：5 个 mp4 是主因。已接受"原文件内置"，压缩/转码/降分辨率列为实现期优化。
- **trade-off**：GIF/视频始终循环 → 笔记本不插电持续占 CPU/GPU。接受，未来可加可见性暂停。
- **风险**：视频 timer 非 vsync 对齐可能轻微抖动——实现期实测，必要时改用 `request_animation_frame` 跨帧节流。
- **风险**：objc2 对 AVAssetReader 覆盖度未实测——实现前需 spike 验证最小闭环（`copyNextSampleBuffer → CMSampleBufferGetImageBuffer → 420f CVPixelBuffer`）。
- **风险**：视频资源路径解析依赖 bundle 或 `CARGO_MANIFEST_DIR/assets/wallpapers/` 回退；开发期 `cargo run` 无 `.app` bundle 时走回退路径，若素材文件缺失则视频项回默认 + warn（图片项不受影响，已嵌入二进制）。
- **未决**：数据目录确切路径；视频缩略图首帧提取实现；删除入口交互（hover 按钮 vs 右键）。

## 11. 验收标准与示例

**必须达成**：
- [ ] 内置 10 个素材可在 Settings 切换，画布立即更新，重启保留。
- [ ] 4 类切换都成立：内置 PNG / 内置 GIF / 内置 mp4 / 自定义导入。
- [ ] 自定义导入图片：导入→列表→选中→更新→删除→回默认闭环。
- [ ] GIF 循环播放，重启仍循环（验证 `img().id("wallpaper")`）。
- [ ] **视频循环播放，重启仍循环**（验证 420f + surface() + timer）。
- [ ] 加载失败（损坏/非媒体/不可读/不支持编码）：不 panic，回默认 + `tracing::warn!`。

**代表性用例**：
1. 首启 → 默认 `1331865.png` → 选内置 `1406489.mp4` → 循环 → 重启仍播放。（核心路径）
2. 导入损坏 mp4 → 无崩溃 → 回默认 + warn。
3. 删当前在用自定义视频 → 自动回默认。

**测试范围**（TDD，遵守项目测试注释规范 `docs/quality/test-requirements.md`）：
- 单元层：壁纸偏好编解码、类型判定(扩展名/文件头)、损坏文件回退、bundle 资源路径解析（含开发期回退）、视频格式 420f 契约——纯函数化便于测试。
- GPUI 层：`chrome.rs` 壁纸 `img().id("wallpaper")` 断言（现有 `app_chrome_tests.rs` 风格）。
- 视频解码层：420f 格式断言、AVAssetReader 产出物格式契约测试。
- 集成：导入→拷贝→选→删 闭环。
