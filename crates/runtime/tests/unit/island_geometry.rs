use super::*;
use crate::island::IslandMode;

/// 场景：原型展开高度随内容变化，空态不留整屏黑色空白，长列表才滚动。
/// 数据：无刘海屏顶部900，菜单栏34；展开正文及页签总高160点，头部34点。
/// 执行：分别给出短内容、超长内容和收起模式，比较尺寸与顶部锚点。
/// 预期：短内容总高194点；长内容受原最大高度限制；收起始终34点。
#[test]
fn island_geometry_content_height_keeps_top_anchor_and_caps_long_lists() {
    let screen = IslandScreen {
        frame: IslandFrame {
            x: 0.,
            y: 0.,
            width: 1440.,
            height: 900.,
        },
        visible_top: 866.,
        safe_top: 0.,
        left_max_x: 0.,
        right_min_x: 0.,
    };
    let short = screen.layout_for_content(IslandMode::Expanded, 160.);
    assert_eq!(short.frame.height, 194., "34点头部加160点实际内容");
    assert_eq!(
        short.frame.y + short.frame.height,
        860.,
        "缩短保持菜单栏下6点顶部锚定"
    );
    assert_eq!(
        screen
            .layout_for_content(IslandMode::Expanded, 2000.)
            .frame
            .height,
        544.,
        "长列表只增长到原型最大高度"
    );
    assert_eq!(
        screen
            .layout_for_content(IslandMode::Compact, 2000.)
            .frame
            .height,
        34.,
        "收起不能保留列表高度"
    );
}

/// 场景：副屏位于主屏左侧且没有刘海，菜单栏占顶部 24 点。
/// 数据：屏幕 (-1920, 80, 1920, 1080)，可见顶部 1136 = 80+1080-24。
/// 执行：计算收起/展开窗口，再检查透明圆角和内部点命中。
/// 预期：胶囊在菜单栏下留 6 点，保持同一中心；收起尺寸不保留展开的透明区域。
#[test]
fn island_geometry_external_display_uses_menu_bar_and_exact_hit_region() {
    let screen = IslandScreen {
        frame: IslandFrame {
            x: -1920.,
            y: 80.,
            width: 1920.,
            height: 1080.,
        },
        visible_top: 1136.,
        safe_top: 0.,
        left_max_x: 0.,
        right_min_x: 0.,
    };
    let compact = screen.layout(IslandMode::Compact);
    let expanded = screen.layout(IslandMode::Expanded);
    assert_eq!(
        compact.frame,
        IslandFrame {
            x: -1125.,
            y: 1096.,
            width: 330.,
            height: 34.
        },
        "无刘海胶囊应位于菜单栏下方"
    );
    assert_eq!(
        expanded.frame.x + expanded.frame.width / 2.,
        -960.,
        "形变应锚定同一屏幕中心"
    );
    assert!(
        expanded.frame.height > compact.frame.height,
        "收起必须真正缩窗"
    );
    assert!(!compact.contains(-1125., 1096.), "透明圆角不得阻挡下层窗口");
    assert!(compact.contains(-960., 1113.), "胶囊中心必须可点");
    assert!(!compact.contains(-960., 1095.), "窗口外不能命中");
}

/// 场景：刘海屏的辅助区域给出真实缺口，分辨率变化后面板仍不越出屏幕。
/// 数据：屏宽1512、顶部982，左右辅助区端点664/848，缺口184×32。
/// 执行：计算收起、展开，再使用窄屏与不完整的辅助区域。
/// 预期：顶部贴齐屏幕且避开184点硬件缺口；窄屏限宽；缺失辅助区不伪造刘海。
#[test]
fn island_geometry_notch_uses_auxiliary_areas_and_clamps_small_displays() {
    let mut screen = IslandScreen {
        frame: IslandFrame {
            x: 0.,
            y: 0.,
            width: 1512.,
            height: 982.,
        },
        visible_top: 950.,
        safe_top: 32.,
        left_max_x: 664.,
        right_min_x: 848.,
    };
    let layout = screen.layout(IslandMode::Compact);
    assert_eq!(layout.notch_width, 184., "刘海宽度来自辅助区域间隙");
    assert_eq!(layout.frame.y + layout.frame.height, 982., "有刘海应贴顶部");
    assert!(
        layout.contains(layout.frame.x + 1., 981.),
        "刘海屏顶部两角应为直角"
    );
    screen.frame.width = 480.;
    assert!(
        screen.layout(IslandMode::Expanded).frame.width <= 480.,
        "小屏幕不能越界"
    );
    screen.right_min_x = screen.left_max_x;
    assert_eq!(
        screen.layout(IslandMode::Compact).notch_width,
        0.,
        "辅助区无间隙时回退胶囊"
    );
}
