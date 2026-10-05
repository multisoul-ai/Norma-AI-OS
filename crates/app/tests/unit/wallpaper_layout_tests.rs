use super::layout::WallpaperLayout;

/// WP-08：壁纸库在宽窗口保持四列，窄窗口减少列数且预览改为纵向。
///
/// 数据构造：宽度 900、320、160 dp；内容上限 856 dp，最小单元宽度 128 dp。
///   宽窗口：4 列 × 205 + 3 间隔 × 12 = 856。
///   窄窗口：2 列 × 154 + 1 间隔 × 12 = 320。
///   极窄窗口：1 列 × 160 = 160，无列间间隔。
/// 执行过程：分别计算三种可用宽度的布局，检查列数、单元宽度和预览排列。
/// 预期结果：宽窗口四列且并排预览；320 dp 两列且纵向预览；160 dp 不挤出第二列。
#[test]
fn wallpaper_layout_adapts_without_horizontal_overflow() {
    let wide = WallpaperLayout::for_width(900.0);
    let narrow = WallpaperLayout::for_width(320.0);
    let tiny = WallpaperLayout::for_width(160.0);
    assert_eq!(wide.columns, 4, "宽窗口应显示四列壁纸");
    assert_eq!(wide.cell_width, 205.0, "宽窗口必须限制内容宽度");
    assert!(!wide.stacked_preview, "宽窗口的预览与信息应并排");
    assert_eq!(narrow.columns, 2, "320 dp 不得强塞四列");
    assert_eq!(narrow.cell_width, 154.0, "窄窗口的单元不得溢出内容区");
    assert!(narrow.stacked_preview, "窄窗口预览应纵向排列");
    assert_eq!(tiny.columns, 1, "160 dp 不应再显示第二列");
    assert_eq!(tiny.cell_width, 160.0, "单列必须适配可用宽度");
}
