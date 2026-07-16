# Launchpad 专用视频背景决策

**状态：已实施**（2026-07-13，提交 `79bb2f4`）

## 决策

Launchpad 使用打包资源 `crates/app/assets/wallpapers/home.mp4` 作为固定专用视频背景。它不读取 Canvas 壁纸设置，也不读取 `MULTISOUL_WALLPAPER`。

Canvas 仍保留自己的壁纸配置；两个界面不再承诺视觉资产一致。

## 原因与边界

- Launchpad 是应用入口，拥有独立的沉浸式视觉身份；Canvas 壁纸是进入工作台后的用户配置。
- 专用视频的解码在后台推进，并通过低拷贝 `surface()` 路径上屏；不得回退为逐帧 CPU RGBA、`RenderImage` 或 `img()` 渲染。
- `home.mp4` 必须作为打包资源存在；缺失时 Launchpad 应保留可用的非视频降级显示。

## 自动化证据

- `crates/app/tests/unit/launchpad_video_tests.rs` 验证固定资源路径与 Launchpad 专用渲染层。
- `crates/e2e/src/e2e_11_launchpad_video.rs` 验证启动入口使用专用 `home.mp4`。
- `scripts/check-video-wallpaper-performance.py` 防止低拷贝路径回退。

旧的“Launchpad 与 Canvas 共用 wallpaper”验收标准已由本决策替代，见 [`2026-06-28-SPEC-launchpad-static-visual.md`](../product-specs/2026-06-28-SPEC-launchpad-static-visual.md)。
