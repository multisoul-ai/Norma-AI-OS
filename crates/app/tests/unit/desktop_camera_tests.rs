use super::{desktop_scene_rect, DesktopCamera};
use canvas::CanvasRect;
use std::time::Duration;

/// 项目和工作空间返回总览时，外壳随镜头恢复，不能整个转场都隐藏。
///
/// 数据构造：展开区域(0,0,1200,800)，总览区域(100,100,400,300)；
///   中点为两端各取一半=(50,50,800,550)，应恢复一半外壳透明度。
/// 执行过程：分别查询展开端、中途和总览端的外壳可见度，再检查只有高度变化的单项目布局。
/// 预期结果：展开端不显示外围装饰；中点已经显示；总览端完整显示，不能在终点才突然出现。
#[test]
fn overview_chrome_returns_with_the_camera() {
    use super::super::scene::overview_chrome_opacity;
    let full = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let overview = CanvasRect::new(100.0, 100.0, 400.0, 300.0);
    let midway = CanvasRect::new(50.0, 50.0, 800.0, 550.0);
    assert_eq!(
        overview_chrome_opacity(full, overview, full),
        0.0,
        "展开后必须保持无外围边框和标题"
    );
    assert_eq!(
        overview_chrome_opacity(midway, overview, full),
        0.5,
        "返回中途应同步恢复外壳，不能只剩裸卡片"
    );
    assert_eq!(
        overview_chrome_opacity(overview, overview, full),
        1.0,
        "落回总览时边框、背景和标题应完整恢复"
    );
    let one_project = CanvasRect::new(0.0, 40.0, 1200.0, 720.0);
    assert_eq!(
        overview_chrome_opacity(one_project, one_project, full),
        1.0,
        "同宽单项目也必须恢复外壳，不能只用宽度判断"
    );
}

/// 项目镜头的终点是窗口安全边距与底部命令栏之间的完整场景区域。
///
/// 数据构造（含关键数值的推导过程）：
///   窗口 1600×1000；左右各 14、顶部标题栏 52、标题下间距 12、底部 91。
///   场景顶部 = 52 + 12 = 64；宽 = 1600 - 2×14 = 1572；高 = 1000 - 64 - 91 = 845。
/// 执行过程（逐步说明系统如何处理）：
///   1. 以窗口尺寸计算项目镜头目标矩形。
///   2. 检查镜头顶部与标题栏分离，底部仍为命令栏保留空间。
///
/// 预期结果：
///   - 正断言：项目从 y=64 开始，标题栏下留出 12px。
///   - 负断言：项目不覆盖标题栏，也不沿用会重叠的 y=36 旧边界。
#[test]
fn desktop_project_camera_fits_between_fixed_chrome() {
    let scene = desktop_scene_rect(1600.0, 1000.0);
    assert_eq!(
        scene,
        CanvasRect::new(14.0, 64.0, 1572.0, 845.0),
        "项目镜头应完整占用窗口安全边距和命令栏之间的场景"
    );
    assert!(scene.origin.y >= 52.0, "项目画布不得进入顶部标题栏");
    assert_ne!(scene.origin.y, 36.0, "不能沿用小于标题栏高度的旧边界");
    assert_ne!(
        scene,
        CanvasRect::new(0.0, 0.0, 1600.0, 1000.0),
        "项目画布不得再盖住窗口控件与命令栏"
    );
}

/// 桌面镜头在工作空间与项目之间改变目标时从当前可见位置继续运动。
///
/// 数据构造（含关键数值的推导过程）：
///   缩略区域 300×200，起点 (100,100)；全画布 1200×800，起点 (0,0)。
///   首次推进 80ms = 5 帧 × 16ms；中途改为另一块 300×200 区域。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从缩略区域朝全画布推进 80ms → 镜头进入两者之间。
///   2. 中途改目标 → 当前画面与速度保持连续，不回到最初缩略区域。
///   3. 再推进 16ms → 镜头继续向新目标运动。
///
/// 预期结果：
///   - 正断言：中途重定向保留当前可见矩形，后续仍可运动。
///   - 负断言：切换时不得跳回起点或直接跳到新终点。
#[test]
fn desktop_camera_retargets_from_current_presentation() {
    let source = CanvasRect::new(100.0, 100.0, 300.0, 200.0);
    let full = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let next = CanvasRect::new(500.0, 200.0, 300.0, 200.0);
    let mut camera = DesktopCamera::new(source);
    camera.retarget(full, false);
    for _ in 0..5 {
        camera.tick(Duration::from_millis(16));
    }
    let midway = camera.rect();
    assert!(
        midway.size.x > source.size.x,
        "镜头应已从源缩略区域开始放大"
    );
    assert!(
        midway.size.x < full.size.x,
        "镜头在 80ms 时不应突然到达全画布"
    );
    camera.retarget(next, false);
    assert_eq!(camera.rect(), midway, "中途重定向必须保留当前画面的矩形");
    assert_ne!(camera.rect(), source, "中途重定向不得跳回最初的缩略区域");
    assert_ne!(camera.rect(), next, "中途重定向不得直接跳到新目标");
    camera.tick(Duration::from_millis(16));
    assert_ne!(camera.rect(), midway, "改目标后下一帧必须继续运动");
}

/// 减少动态效果时镜头立即定位，避免全窗口移动。
///
/// 数据构造：起点 300×200，目标 1200×800；两者宽度差 = 900px。
/// 执行过程：创建镜头后以减少动态效果模式重定向到全画布。
/// 预期结果：正断言为立即到达目标；负断言为不保留 900px 宽度差的移动动画。
#[test]
fn desktop_camera_reduced_motion_snaps_to_target() {
    let source = CanvasRect::new(100.0, 100.0, 300.0, 200.0);
    let full = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
    let mut camera = DesktopCamera::new(source);
    camera.retarget(full, true);
    assert_eq!(camera.rect(), full, "减少动态效果时必须立即呈现目标画布");
    assert!(
        !camera.is_active(),
        "减少动态效果时不得继续请求大范围移动帧"
    );
    assert_ne!(camera.rect(), source, "减少动态效果时不得停在旧缩略区域");
}

/// 缩略与展开共用完整画布尺寸，镜头起止端保持相同比例。
///
/// 数据构造：原画布1200×600，外壳420×260；标题28、下边距8，舞台404×224；
///   缩放=min(404/1200,224/600)，内容404×202；顶部=(224-202)/2+28=39。
/// 执行过程：计算缩略内容 → 弹簧从内容推进至全屏 → 比较宽高比与精确终点。
/// 预期结果：整个画布缩小、无节点适配重排；中间保持2:1，终点不留缩略偏移或比例。
#[test]
fn desktop_project_camera_scales_the_complete_canvas() {
    use super::super::preview_canvas::project_canvas_rect;
    let full = CanvasRect::new(14.0, 36.0, 1200.0, 600.0);
    let card = CanvasRect::new(100.0, 150.0, 420.0, 260.0);
    let preview = project_canvas_rect(card, full.size, 1.0);
    assert_eq!(
        preview.size,
        canvas::Vec2::new(404.0, 202.0),
        "缩略必须缩小完整视区"
    );
    assert!(
        (preview.origin.y - 189.0).abs() < 0.001,
        "画布应在标题下方等比居中"
    );
    assert_ne!(preview, card, "画布不能覆盖项目标题栏");
    let mut camera = DesktopCamera::new(preview);
    camera.retarget(full, false);
    for _ in 0..100 {
        // 100×16ms=1600ms，超过弹簧收敛时间。
        camera.tick(Duration::from_millis(16));
        let frame = camera.rect();
        assert!(
            (frame.size.x / frame.size.y - 2.0).abs() < 0.001,
            "动画期间不得拉伸字体或改变原卡片布局"
        );
    }
    assert_eq!(
        camera.rect(),
        full,
        "落定时完整复用原画布，无 viewport 交接跳变"
    );
    assert!(!camera.is_active(), "落定后不应继续动画");
}
