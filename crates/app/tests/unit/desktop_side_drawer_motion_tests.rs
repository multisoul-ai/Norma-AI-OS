use super::*;
use std::time::Duration;

/// 场景：左右侧抽屉分别从自己的窗口边缘滑入，收起期间保留面板，并可中途反向。
/// 数据构造：显式展开左侧，右侧保持收起；两侧均经过 80ms 的部分动画。
/// 执行过程：左侧收起、右侧展开，推进一帧，再反向两侧并推进至结束。
/// 预期结果：两侧独立运动；反向首帧位置连续；终点准确且无需继续请求动画帧。
#[test]
fn side_drawers_slide_independently_and_reverse_from_displayed_frame() {
    let mut motion = DesktopAssistantDrawerMotion::default();
    assert_eq!(motion.left.fraction(false), 0.0, "左侧默认应完整收起");
    assert_eq!(motion.right.fraction(false), 0.0, "右侧初始完整收起");
    motion.settle_left(true);
    assert_eq!(motion.left.fraction(false), 1.0, "显式展开后左侧应完整呈现");
    assert_eq!(
        motion.left.edge_inset(336.0, 12.0, false),
        12.0,
        "左栏展开时距离左边缘 12px"
    );
    assert_eq!(
        motion.right.edge_inset(404.0, 12.0, false),
        -406.0,
        "右栏收起时应完全退到右边缘外"
    );

    motion.retarget(false, true);
    assert!(motion.is_active(), "开合后应请求动画帧");
    motion.tick(Duration::from_millis(80), false);
    let left_mid = motion.left.fraction(false);
    let right_mid = motion.right.fraction(false);
    assert!(
        left_mid > 0.0 && left_mid < 1.0,
        "左栏应在窗口边缘和终点之间"
    );
    assert!(
        right_mid > 0.0 && right_mid < 1.0,
        "右栏应在窗口边缘和终点之间"
    );
    assert!(motion.left.should_render(), "左栏退出期间必须保留面板内容");

    motion.retarget(true, false);
    assert_eq!(
        motion.left.fraction(false),
        left_mid,
        "左栏反向首帧不能跳跃"
    );
    assert_eq!(
        motion.right.fraction(false),
        right_mid,
        "右栏反向首帧不能跳跃"
    );
    motion.tick(Duration::from_millis(300), false);
    assert_eq!(motion.left.fraction(false), 1.0, "左栏应回到展开终点");
    assert_eq!(motion.right.fraction(false), 0.0, "右栏应回到收起终点");
    assert!(!motion.right.should_render(), "右栏退到边缘后应停止渲染");
    assert!(!motion.is_active(), "抽屉停稳后不应持续请求动画帧");
}

/// 场景：用户启用减少动态效果后，抽屉仍给出状态反馈。
/// 数据构造：右栏初始收起，展开与关闭各推进 80ms。
/// 执行过程：打开右栏并推进半程，然后关闭并推进半程。
/// 预期结果：几何位置不滑动，透明度在两个方向连续变化。
#[test]
fn reduced_motion_fades_at_open_position_without_sliding() {
    let mut motion = DesktopAssistantDrawerMotion::default();
    motion.retarget(true, true);
    motion.tick(Duration::from_millis(80), true);
    assert_eq!(
        motion.right.fraction(true),
        1.0,
        "减少动态效果应在完整展开位置淡入"
    );
    assert!(
        motion.right.opacity(true) > 0.0 && motion.right.opacity(true) < 1.0,
        "右栏淡入半程应可见且未完全不透明"
    );
    motion.tick(Duration::from_millis(80), true);
    motion.retarget(true, false);
    motion.tick(Duration::from_millis(80), true);
    assert_eq!(motion.right.fraction(true), 1.0, "右栏淡出时位置不能移动");
    assert!(
        motion.right.opacity(true) > 0.0 && motion.right.opacity(true) < 1.0,
        "右栏淡出半程应保持连续反馈"
    );
}

/// 场景：原型的整高抽屉在宽桌面占据两侧，收起时画布扩展到窗口边缘。
/// 数据构造：1280px 窗口，左宽为 1280×31%−150=246.8px，右宽为 1280×37%=473.6px。
/// 执行过程：依次测量左开、双开和双关的终态布局。
/// 预期结果：中间宽度分别为 1033.2px、559.6px、1280px，面板不压住画布。
#[test]
fn drawer_layout_reserves_canvas_width_like_prototype() {
    let mut motion = DesktopAssistantDrawerMotion::default();
    motion.settle_left(true);
    let left = motion.layout(1280.0, true, false, false);
    assert!(
        (left.left_width - 246.8).abs() < 0.01,
        "左栏在原 31vw 默认宽度基础上减少 150px"
    );
    assert!(
        (left.right_width - 473.6).abs() < 0.01,
        "右栏按原型的 37vw 计算"
    );
    assert!(
        (left.left_space - 246.8).abs() < 0.01,
        "展开的左栏占据布局宽度"
    );
    assert_eq!(left.right_space, 0.0, "收起的右栏不占布局宽度");
    assert!(
        (left.canvas_width - 1033.2).abs() < 0.01,
        "单侧展开时画布使用余下宽度"
    );

    motion.retarget(true, true);
    motion.tick(Duration::from_millis(300), false);
    let both = motion.layout(1280.0, true, true, false);
    assert!((both.left_space - 246.8).abs() < 0.01, "左栏保持原位");
    assert!(
        (both.right_space - 473.6).abs() < 0.01,
        "右栏占据右侧布局宽度"
    );
    assert!(
        (both.canvas_width - 559.6).abs() < 0.01,
        "两栏展开后画布占据中间剩余宽度"
    );

    motion.retarget(false, false);
    motion.tick(Duration::from_millis(300), false);
    let closed = motion.layout(1280.0, false, false, false);
    assert_eq!(closed.left_space, 0.0, "左栏收起后释放空间");
    assert_eq!(closed.right_space, 0.0, "右栏收起后释放空间");
    assert_eq!(closed.canvas_width, 1280.0, "双栏收起时画布占满窗口");

    // 760px 以下改为覆盖式；900px 桌面保留三栏，画布宽 900−216−333=351px。
    let compact_desktop = DesktopDrawerLayout::new(900.0, 1.0, 1.0);
    assert!(
        compact_desktop.left_space >= 150.0,
        "900px 桌面左栏仍应占位"
    );
    assert!(
        compact_desktop.right_space >= 185.0,
        "900px 桌面右栏仍应占位"
    );
    assert!(
        (compact_desktop.canvas_width - 351.0).abs() < 0.01,
        "默认左栏缩窄后，900px 窗口的画布应保留 351px"
    );

    let mobile = DesktopDrawerLayout::new(759.0, 1.0, 1.0);
    assert_eq!(mobile.left_space, 0.0, "760px 以下左栏应覆盖画布");
    assert_eq!(mobile.right_space, 0.0, "760px 以下右栏应覆盖画布");
}

/// 场景：用户扩大左右抽屉后，画布和底部 Dock 共用新的剩余宽度。
/// 数据构造：1440px 窗口，左栏 420px、右栏 520px，双栏展开。
/// 执行过程：生成带自定义宽度的布局，再关闭右栏。
/// 预期结果：中间画布分别为 500px 和 1020px，收起不丢失右栏宽度。
#[test]
fn resized_drawer_layout_uses_custom_widths_without_losing_canvas() {
    let both = DesktopDrawerLayout::with_widths(1440.0, 1.0, 1.0, Some(420.0), Some(520.0));
    assert_eq!(both.left_width, 420.0, "左栏使用拖拽后的宽度");
    assert_eq!(both.right_width, 520.0, "右栏使用拖拽后的宽度");
    assert_eq!(both.canvas_width, 500.0, "中央画布使用两栏剩余宽度");

    let right_closed = DesktopDrawerLayout::with_widths(1440.0, 1.0, 0.0, Some(420.0), Some(520.0));
    assert_eq!(right_closed.right_width, 520.0, "右栏收起后保留恢复宽度");
    assert_eq!(right_closed.right_space, 0.0, "右栏收起后不占画布空间");
    assert_eq!(right_closed.canvas_width, 1020.0, "中央画布回收右栏空间");
}

/// 场景：大窗口拖宽两栏后，用户缩窄窗口。
/// 数据构造：记忆左 420px、右 520px，窗口由 1440px 缩至 1000px 和 900px。
/// 执行过程：用同一组记忆宽度重新计算双栏布局。
/// 预期结果：1000px 时中央至少 320px；900px 时两栏保持最小宽度，中央仍有 230px。
#[test]
fn remembered_drawer_widths_reflow_when_window_shrinks() {
    let medium = DesktopDrawerLayout::with_widths(1000.0, 1.0, 1.0, Some(420.0), Some(520.0));
    assert!(medium.left_width >= 150.0, "缩窗后左栏不能低于可读最小宽度");
    assert!(
        medium.right_width >= 185.0,
        "缩窗后右栏不能低于可读最小宽度"
    );
    assert!(
        medium.canvas_width >= 320.0,
        "1000px 窗口应保留 320px 中央画布"
    );
    assert!(
        medium.left_width + medium.right_width <= 680.0,
        "双栏合计不能挤走中央画布"
    );

    let compact = DesktopDrawerLayout::with_widths(900.0, 1.0, 1.0, Some(420.0), Some(520.0));
    assert!(compact.left_width >= 150.0, "900px 窗口左栏不低于最小宽度");
    assert!(compact.right_width >= 185.0, "900px 窗口右栏不低于最小宽度");
    assert!(
        (compact.canvas_width - 320.0).abs() < 0.01,
        "900px 三栏布局保留 320px 中央画布"
    );
}
