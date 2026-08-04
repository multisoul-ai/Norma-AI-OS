use super::*;

fn rgb_distance_after_overlay(background: u32, overlay: u32) -> f32 {
    let overlay_alpha = (overlay & 0xff) as f32 / 255.0;
    let channel = |shift: u32| {
        let background = ((background >> shift) & 0xff) as f32;
        let foreground = ((overlay >> shift) & 0xff) as f32;
        background + (foreground - background) * overlay_alpha
    };
    let red_delta = channel(24) - ((background >> 24) & 0xff) as f32;
    let green_delta = channel(16) - ((background >> 16) & 0xff) as f32;
    let blue_delta = channel(8) - ((background >> 8) & 0xff) as f32;
    (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt()
}

/// New 菜单 hover：默认浅色主题的菜单项必须产生清晰且非白色叠加的选中反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   菜单静止背景 = Paper Blue `panel` RGB（alpha 由菜单容器统一处理）。
///   hover 叠加色 = `new_menu_item_hover_fill(Theme::paper_blue())`。
///   最小 RGB 距离 = 24；三通道平均至少约 24 / sqrt(3) ≈ 13.9 级变化，
///                   足以避免白色 panel 上叠加近白色后肉眼无变化。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造应用默认 Paper Blue 主题并取得菜单项 hover token。
///   2. 按 alpha 将 hover token 合成到菜单 panel RGB 上。
///   3. 计算合成前后 RGB 欧氏距离，并检查 hover 色相不等于 edge highlight。
///
/// 预期结果：
///   - 正断言：hover 合成后的 RGB 距离至少为 24，鼠标移入时有明确背景变化。
///   - 正断言：hover alpha 不为 0，反馈实际参与绘制。
///   - 负断言：hover 不得继续使用与浅色 panel 同为近白色的 edge highlight。
#[test]
fn test_new_menu_hover_is_visibly_distinct_on_default_light_theme() {
    let theme = Theme::paper_blue();
    let hover_fill = new_menu_item_hover_fill(theme);
    let distance = rgb_distance_after_overlay(theme.panel, hover_fill);

    assert!(
        distance >= 24.0,
        "New 菜单 hover 与默认浅色 panel 的 RGB 距离必须至少为 24，实际为 {distance:.2}；更低会让选中反馈难以辨认"
    );
    assert_ne!(
        hover_fill & 0xff,
        0,
        "New 菜单 hover alpha 必须非零，否则鼠标移入不会产生背景反馈"
    );
    assert_ne!(
        hover_fill & 0xffffff00,
        theme.edge_highlight & 0xffffff00,
        "New 菜单 hover 不得沿用与默认浅色 panel 同为近白色的 edge_highlight RGB"
    );
}

/// New 菜单 hover：深色主题中也必须保持与菜单面板可辨识的强调色反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   菜单静止背景 = Sapphire Night `panel` RGB。
///   hover 叠加色 = `new_menu_item_hover_fill(Theme::sapphire_night())`。
///   最小 RGB 距离 = 24；与浅色主题使用同一验收下限，防止修浅色时回退深色。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Sapphire Night 主题并取得生产 hover token。
///   2. 按 hover alpha 合成到深色 panel，计算合成前后 RGB 距离。
///   3. 对照 panel 原色，确认 hover 不是透明或同色覆盖。
///
/// 预期结果：
///   - 正断言：深色主题 hover 合成后的 RGB 距离至少为 24。
///   - 正断言：hover alpha 非零，背景反馈参与绘制。
///   - 负断言：hover RGB 不得与深色 panel RGB 相同。
#[test]
fn test_new_menu_hover_is_visibly_distinct_on_dark_theme() {
    let theme = Theme::sapphire_night();
    let hover_fill = new_menu_item_hover_fill(theme);
    let distance = rgb_distance_after_overlay(theme.panel, hover_fill);

    assert!(
        distance >= 24.0,
        "New 菜单 hover 与 Sapphire Night panel 的 RGB 距离必须至少为 24，实际为 {distance:.2}"
    );
    assert_ne!(
        hover_fill & 0xff,
        0,
        "Sapphire Night 的 New 菜单 hover alpha 必须非零"
    );
    assert_ne!(
        hover_fill & 0xffffff00,
        theme.panel & 0xffffff00,
        "Sapphire Night 的 New 菜单 hover RGB 不得与 panel RGB 相同"
    );
}

/// New 菜单遮挡几何：菜单展开时必须用包含触发器水平容错的屏幕矩形。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport height = 800px。
///   activation_x = 603px（鼠标在 38px New 按钮内的释放位置）。
///   horizontal guard = trigger_width(38) / 2 = 19px。
///   left = 603 - menu_width(180) / 2 - 19 = 494px。
///   width = menu_width(180) + trigger_width(38) = 218px。
///   bottom = dock_bottom(22) + dock_padding((52 - 38) / 2 = 7) + trigger(38) + gap(12) = 79px。
///   top = viewport_height(800) - bottom(79) - menu_height(362) = 359px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open=false 且有 activation_x → 返回 None，菜单关闭不遮挡 Browser。
///   2. open=true 但没有 activation_x → 返回 None，不凭空猜测锚点。
///   3. open=true 且 activation_x=603 → 生成 (494, 359, 218, 362) 的遮挡矩形。
///
/// 预期结果：
///   - 正断言：开启且有锚点时返回精确容错矩形，覆盖菜单的所有可能 x 位置。
///   - 负断言：关闭时不得返回矩形；缺少锚点时不得误遮挡整屏 Browser。
#[test]
fn test_new_menu_screen_rect_guards_trigger_width_without_occluding_when_closed() {
    assert!(
        new_menu_screen_rect(false, Some(603.0), 800.0).is_none(),
        "New 菜单关闭时必须返回 None，不得遮挡 Browser"
    );
    assert!(
        new_menu_screen_rect(true, None, 800.0).is_none(),
        "New 菜单缺少触发锚点时必须返回 None，不得猜测全屏范围"
    );
    assert_eq!(
        new_menu_screen_rect(true, Some(603.0), 800.0),
        Some(CanvasRect::new(494.0, 359.0, 218.0, 362.0)),
        "New 菜单遮挡矩形必须同时覆盖 180px 菜单和触发器两侧各 19px 容错"
    );
}
