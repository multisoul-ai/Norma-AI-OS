use super::material;
use crate::app::wallpaper_resolvers::WallpaperKind;
use crate::settings::wallpaper::state::WallpaperSelection;
use image::{Rgba, RgbaImage};
use std::path::PathBuf;

/// 场景：抽屉蒙层应跟随主题，不能在浅色或紫绿主题下继续使用固定深蓝。
/// 数据构造：六个主题分别传入左右抽屉；目标不透明度 60% 按 8 位取整为 153。
/// 执行过程：读取真正供 render_drawer_frost 使用的 tint。
/// 预期结果：蒙层采用所选主题的表面色，左右与渐变两端均为约 60% 不透明度。
#[test]
fn desktop_drawer_tint_tracks_selected_theme() {
    for id in ui::ThemeId::ALL {
        let theme = ui::Theme::for_id(id);
        for left in [false, true] {
            let (top, bottom) = material::drawer_tint(theme, left);
            assert_eq!(
                top >> 8,
                theme.canvas_from >> 8,
                "{id:?} left={left} 顶部不得固定深蓝"
            );
            assert_eq!(
                bottom >> 8,
                theme.canvas_to >> 8,
                "{id:?} left={left} 底部必须沿用同一主题"
            );
            assert_eq!(
                (top & 0xff, bottom & 0xff),
                (153, 153),
                "{id:?} left={left} 蒙层应按 round(255 × 0.60) 透出约 40% 壁纸"
            );
        }
    }
}

/// 场景：光环动画每帧重绘抽屉，壁纸来源不应每帧重新解析文件。
/// 数据构造：默认内置壁纸连续出现两次，随后切换到一张自定义壁纸。
/// 执行过程：按每次渲染顺序询问毛玻璃状态是否需要刷新。
/// 预期结果：首次与选择变化时刷新，相同选择重复渲染时不刷新。
#[test]
fn drawer_frost_refreshes_only_when_selection_changes() {
    let mut state = material::FrostedWallpaperState::default();
    let default = WallpaperSelection::default();
    let (first_generation, first_selection) = state
        .take_selection_update(&default)
        .expect("首次渲染应解析默认壁纸");
    assert_eq!(first_selection, default, "首次请求应使用默认壁纸");
    assert_eq!(
        state.take_selection_update(&default),
        None,
        "相同壁纸在动画帧重绘时不得重复解析"
    );
    let custom = WallpaperSelection::Custom {
        filename: "scene.png".to_owned(),
    };
    let (second_generation, second_selection) = state
        .take_selection_update(&custom)
        .expect("切换壁纸后应重新解析并模糊");
    assert_eq!(second_selection, custom, "第二次请求应使用自定义壁纸");
    assert_ne!(
        first_generation, second_generation,
        "切换壁纸应使之前的后台解码结果失效"
    );
    assert_eq!(
        state.take_selection_update(&custom),
        None,
        "切换完成后仍应跳过重复帧"
    );
    let (third_generation, _) = state
        .take_selection_update(&default)
        .expect("切回默认壁纸应生成新请求");
    assert_ne!(
        first_generation, third_generation,
        "A→B→A 的旧 A 后台任务不得覆盖新 A"
    );
    assert!(
        state.invalidate_for_builtin(crate::assets::DEFAULT_WALLPAPER_ASSET),
        "当前内置壁纸下载完成后应使毛玻璃来源失效"
    );
    assert!(
        state.take_selection_update(&default).is_some(),
        "同一选择的资源从缺失变为可用时应重新解析"
    );
    assert!(
        !state.invalidate_for_builtin("wallpapers/unrelated.webp"),
        "其他内置壁纸下载不应重建当前毛玻璃"
    );
    assert!(state.invalidate(), "媒体目录初始化后应允许重新解析当前壁纸");
    assert!(
        state.take_selection_update(&default).is_some(),
        "媒体目录刷新后即使壁纸选择不变也应重新加载"
    );
}

/// 场景：抽屉毛玻璃让壁纸的高频纹理变柔和，仍保留底图的色彩信息。
/// 数据构造：64×64 红蓝交错细条，每列交替；相邻列原始红通道差 255。
/// 执行过程：只对壁纸生成一次模糊纹理，读取中央相邻列。
/// 预期结果：尺寸不变，红蓝通道仍同时存在，相邻列差距明显缩小。
#[test]
fn drawer_frost_blurs_wallpaper_without_flattening_it() {
    let source = RgbaImage::from_fn(64, 64, |x, _| {
        if x % 2 == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let frosted = material::frost_wallpaper_pixels(&source);
    assert_eq!(frosted.dimensions(), (64, 64), "模糊不应改变壁纸宽高比");
    let even = frosted.get_pixel(30, 32);
    let odd = frosted.get_pixel(31, 32);
    assert!(
        (i16::from(even[0]) - i16::from(odd[0])).abs() < 40,
        "细条的红色高频差异应被模糊"
    );
    assert!(even[0] > 0 && even[2] > 0, "毛玻璃仍应透出底图的红蓝色彩");
    assert_eq!(
        even[3], 255,
        "壁纸纹理本身应保持不透明，透光度由抽屉蒙层控制"
    );
    assert_ne!(even[0], 255, "毛玻璃不能直接使用未模糊原图");
}

/// 场景：地球壁纸的明暗分界穿过抽屉时，玻璃层需呈现宽而柔和的过渡。
/// 数据构造：128×64 左黑右白的垂直硬边界，边界位于 x=64。
/// 执行过程：对整张纹理模糊，读取边界左侧 20px 的中线亮度。
/// 预期结果：亮度超过 35，说明高光扩散到抽屉表面，避免仅像深色透明遮罩。
#[test]
fn drawer_frost_softens_large_wallpaper_edges() {
    let source = RgbaImage::from_fn(128, 64, |x, _| {
        let value = if x < 64 { 0 } else { 255 };
        Rgba([value, value, value, 255])
    });
    let frosted = material::frost_wallpaper_pixels(&source);
    assert!(
        frosted.get_pixel(44, 32)[0] > 35,
        "毛玻璃应把壁纸硬边界的高光扩散到 20px 外"
    );
    assert!(
        frosted.get_pixel(84, 32)[0] < 220,
        "硬边界另一侧也应柔化，不能保留清晰纹理"
    );
}

/// 场景：用户选中的自定义壁纸文件失效，画布回退默认地球背景。
/// 数据构造：没有可解析的选中壁纸路径，原选择类型视作 GIF。
/// 执行过程：读取抽屉应使用的壁纸来源。
/// 预期结果：抽屉改用默认静态地球图，保持与画布背景一致。
#[test]
fn drawer_frost_falls_back_to_canvas_default_wallpaper() {
    let (source, kind) =
        material::effective_frost_source(None, crate::app::wallpaper_resolvers::WallpaperKind::Gif)
            .expect("应能找到内置默认壁纸");
    assert!(
        source.ends_with(crate::assets::DEFAULT_WALLPAPER_ASSET),
        "玻璃纹理应回退地球壁纸"
    );
    assert_eq!(
        kind,
        crate::app::wallpaper_resolvers::WallpaperKind::Static,
        "默认纹理应按静态图片解码"
    );
}

/// 场景：已选视频文件存在记录但首帧无法解码，画布会回退默认壁纸。
/// 数据构造：一个不存在的视频路径，默认地球图片已随 App 打包。
/// 执行过程：尝试解码选中视频，再检查毛玻璃最终使用的图片来源。
/// 预期结果：返回默认地球纹理，而不是让左右抽屉只剩纯色蒙层。
#[test]
fn drawer_frost_falls_back_when_video_frame_cannot_decode() {
    let invalid_video = PathBuf::from("/__norma_missing_frost__/broken.mp4");
    let (source, _) = material::decode_frosted_source(
        Some(invalid_video.clone()),
        crate::app::wallpaper_resolvers::WallpaperKind::Video,
    )
    .expect("视频解码失败时应生成默认地球毛玻璃纹理");
    assert!(
        source.ends_with(crate::assets::DEFAULT_WALLPAPER_ASSET),
        "毛玻璃应跟随画布的默认地球回退图"
    );
    assert_ne!(source, invalid_video, "损坏视频不得成为最终毛玻璃来源");
}

/// 场景：默认壁纸已内置，抽屉毛玻璃解码不得再读取磁盘上的源码资源。
/// 背景：未打包的 debug 构建会回退到 `CARGO_MANIFEST_DIR/assets`，它位于用户“文稿”目录，
///   对从未授权的宿主会触发文件夹访问授权弹窗。
/// 数据构造：默认内置壁纸选择；测试环境里仓库路径下的真实文件存在，旧实现必然返回绝对路径。
/// 执行过程：用 decode_frosted_selection 解码默认选择。
/// 预期结果：
///   - 正断言：解码成功，来源标识是内置名 home/earth-horizon.png
///   - 负断言：来源标识不是绝对路径，说明没有走文件系统
///   - 正断言：图像最长边不超过 1280，沿用同一套尺寸上限
#[test]
fn drawer_frost_decodes_the_default_wallpaper_from_embedded_bytes() {
    let (source, image) =
        material::decode_frosted_selection(&WallpaperSelection::default(), WallpaperKind::Static)
            .expect("内置默认壁纸应能解码");

    assert_eq!(
        source,
        PathBuf::from(crate::assets::DEFAULT_WALLPAPER_ASSET)
    );
    assert!(!source.is_absolute(), "不得读取文件系统路径：{source:?}");
    let size = image.size(0);
    assert!(
        size.width.0.max(size.height.0) <= 1280,
        "尺寸应受 1280 上限约束：{size:?}"
    );
}
