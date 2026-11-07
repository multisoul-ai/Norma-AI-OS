use super::*;
use std::time::Duration;

/// 场景：右侧内容抽屉从窗口右边缘滑入，收起期间保留面板，并可中途反向。
/// 数据构造：抽屉默认收起；展开后经过 80ms 的部分动画再反向。
/// 执行过程：展开推进一帧，反向并推进至结束。
/// 预期结果：反向首帧位置连续；终点准确且无需继续请求动画帧。
#[test]
fn right_drawer_slides_and_reverses_from_displayed_frame() {
    let mut motion = DesktopAssistantDrawerMotion::default();
    assert_eq!(motion.right.fraction(false), 0.0, "右侧初始完整收起");
    assert_eq!(
        motion.right.edge_inset(404.0, 12.0, false),
        -406.0,
        "右栏收起时应完全退到右边缘外"
    );

    motion.retarget(true);
    assert!(motion.is_active(), "开合后应请求动画帧");
    motion.tick(Duration::from_millis(80), false);
    let mid = motion.right.fraction(false);
    assert!(mid > 0.0 && mid < 1.0, "右栏应在窗口边缘和终点之间");

    motion.retarget(false);
    assert_eq!(motion.right.fraction(false), mid, "右栏反向首帧不能跳跃");
    assert!(motion.right.should_render(), "右栏退出期间必须保留面板内容");
    motion.tick(Duration::from_millis(300), false);
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
    motion.retarget(true);
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
    motion.retarget(false);
    motion.tick(Duration::from_millis(80), true);
    assert_eq!(motion.right.fraction(true), 1.0, "右栏淡出时位置不能移动");
    assert!(
        motion.right.opacity(true) > 0.0 && motion.right.opacity(true) < 1.0,
        "右栏淡出半程应保持连续反馈"
    );
}

/// 场景：宽桌面的右抽屉占据布局宽度，收起时画布扩展到窗口边缘。
/// 数据构造：1280px 窗口，右宽为 1280×37%=473.6px；另测 900px 与 759px 窗口。
/// 执行过程：依次测量展开、收起、900px 桌面和 759px 覆盖式布局。
/// 预期结果：展开画布 1280−473.6=806.4px；收起 1280px；900px 为 900−333=567px；759px 不占位。
#[test]
fn drawer_layout_reserves_canvas_width_like_prototype() {
    let mut motion = DesktopAssistantDrawerMotion::default();
    motion.retarget(true);
    motion.tick(Duration::from_millis(300), false);
    let open = motion.layout(1280.0, true, false);
    assert!(
        (open.right_width - 473.6).abs() < 0.01,
        "右栏按原型的 37vw 计算"
    );
    assert!(
        (open.right_space - 473.6).abs() < 0.01,
        "展开的右栏占据布局宽度"
    );
    assert!(
        (open.canvas_width - 806.4).abs() < 0.01,
        "展开后画布使用余下宽度"
    );

    motion.retarget(false);
    motion.tick(Duration::from_millis(300), false);
    let closed = motion.layout(1280.0, false, false);
    assert_eq!(closed.right_space, 0.0, "右栏收起后释放空间");
    assert_eq!(closed.canvas_width, 1280.0, "收起时画布占满窗口");

    let compact_desktop = DesktopDrawerLayout::new(900.0, 1.0);
    assert!(
        compact_desktop.right_space >= 185.0,
        "900px 桌面右栏仍应占位"
    );
    assert!(
        (compact_desktop.canvas_width - 567.0).abs() < 0.01,
        "900px 窗口右栏 333px，画布应保留 567px"
    );

    let mobile = DesktopDrawerLayout::new(759.0, 1.0);
    assert_eq!(mobile.right_space, 0.0, "760px 以下右栏应覆盖画布");
}

/// 场景：用户扩大右抽屉后，画布和底部 Dock 共用新的剩余宽度；缩窄窗口时仍保留最小画布。
/// 数据构造：1440px 窗口右栏 520px；同一记忆宽度在 900px 窗口重算。
/// 执行过程：生成带自定义宽度的布局，再关闭右栏，再缩窗。
/// 预期结果：画布 920px / 1440px；900px 时右栏被夹到 580px 以内且画布至少 320px。
#[test]
fn resized_drawer_layout_uses_custom_width_without_losing_canvas() {
    let open = DesktopDrawerLayout::with_width(1440.0, 1.0, Some(520.0));
    assert_eq!(open.right_width, 520.0, "右栏使用拖拽后的宽度");
    assert_eq!(open.canvas_width, 920.0, "中央画布使用右栏剩余宽度");

    let closed = DesktopDrawerLayout::with_width(1440.0, 0.0, Some(520.0));
    assert_eq!(closed.right_width, 520.0, "右栏收起后保留恢复宽度");
    assert_eq!(closed.right_space, 0.0, "右栏收起后不占画布空间");
    assert_eq!(closed.canvas_width, 1440.0, "中央画布回收右栏空间");

    let compact = DesktopDrawerLayout::with_width(900.0, 1.0, Some(520.0));
    assert!(compact.right_width >= 185.0, "900px 窗口右栏不低于最小宽度");
    assert!(
        compact.canvas_width >= 320.0,
        "900px 窗口应保留 320px 中央画布"
    );
}
