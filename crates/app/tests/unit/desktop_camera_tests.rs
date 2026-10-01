use super::{
    desktop_scene_rect, embedded_viewport, project_preview_viewport, project_transition_viewport,
    DesktopCamera,
};
use canvas::{CanvasNode, CanvasNodeKind, CanvasRect, TermSpec, Vec2, Viewport};
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
/// 数据构造（含关键数值的推导过程）：窗口 1600×1000；左右各 14、上 36、下 91；
///   场景宽 = 1600 - 28 = 1572，高 = 1000 - 36 - 91 = 873。
/// 执行过程：以窗口尺寸计算项目镜头目标矩形。
/// 预期结果：正断言为场景矩形精确落在保留窗口安全边距和命令栏之后；
///   负断言为项目镜头没有继续覆盖全窗口。
#[test]
fn desktop_project_camera_fits_between_fixed_chrome() {
    let scene = desktop_scene_rect(1600.0, 1000.0);
    assert_eq!(
        scene,
        CanvasRect::new(14.0, 36.0, 1572.0, 873.0),
        "项目镜头应完整占用窗口安全边距和命令栏之间的场景"
    );
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

/// 项目画布缩入区域时，卡片位置与尺寸随同一镜头缩放。
///
/// 数据构造（含关键数值的推导过程）：全窗 1920×1080，区域 480×270，比例均为 1/4；
///   原 viewport zoom=1.2、pan=(60,-20)，缩后应为 zoom=0.3、pan=(15,-5)。
/// 执行过程：按区域尺寸构造项目会话的视觉 viewport，再读取缩放后的坐标。
/// 预期结果：正断言为 zoom/pan 均按 1/4 缩放；负断言为未保留全屏 viewport。
#[test]
fn desktop_project_camera_scales_visual_viewport() {
    let viewport = Viewport {
        pan: Vec2::new(60.0, -20.0),
        zoom: 1.2,
    };
    let frame = CanvasRect::new(100.0, 100.0, 480.0, 270.0);
    let actual = embedded_viewport(viewport, 1920.0, 1080.0, frame);
    assert_eq!(actual.zoom, 0.3, "卡片宽高应随镜头缩为四分之一");
    assert_eq!(
        actual.pan,
        Vec2::new(15.0, -5.0),
        "卡片位置也应按同一比例缩放"
    );
    assert_ne!(actual, viewport, "缩略画布不得沿用全屏 viewport");
}

/// 项目镜头的起点应与总览中的真实卡片缩略位置一致。
///
/// 数据构造（含关键数值的推导过程）：
///   区域 400×300；标题高 = 300×0.14 = 42，内容区 = 380×248。
///   卡片 640×400 位于画布 (100,50)；缩放 = min(364/640,232/400,1) = 0.56875。
///   缩略卡片左上角 = (10+8,42+10.25) = (18,52.25)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用区域和项目节点计算镜头起始 viewport。
///   2. 投影原节点左上角，得到总览缩略卡片的位置。
///
/// 预期结果：
///   - 正断言：起点坐标为 (18,52.25)，尺寸缩放为 0.56875。
///   - 负断言：不能沿用全屏 viewport 或简单按区域宽度缩放。
#[test]
fn desktop_project_camera_starts_at_the_visible_preview_card() {
    let node = CanvasNode {
        id: 7,
        kind: CanvasNodeKind::Term(TermSpec),
        bounds: CanvasRect::new(100.0, 50.0, 640.0, 400.0),
        z_index: 0,
        selected: false,
    };
    let area = CanvasRect::new(80.0, 200.0, 400.0, 300.0);
    let preview =
        project_preview_viewport(&[node], area).expect("含卡片的项目区域应能计算缩略 viewport");
    let projected = preview.canvas_to_screen(Vec2::new(100.0, 50.0));
    assert!(
        (projected.x - 18.0).abs() < 0.01,
        "项目卡片起点应与总览缩略卡片的水平位置重合"
    );
    assert!(
        (projected.y - 52.25).abs() < 0.01,
        "项目卡片起点应与总览缩略卡片的垂直位置重合"
    );
    assert!(
        (preview.zoom - 0.56875).abs() < 0.0001,
        "项目卡片应按内容区适配比例缩小"
    );
    assert_ne!(
        preview,
        Viewport::default(),
        "缩略 viewport 不得沿用全屏画布"
    );
    assert!(
        (preview.zoom - 400.0 / 1200.0).abs() > 0.1,
        "缩略 viewport 不能只按项目区域与全屏宽度比例计算"
    );
}

/// 项目镜头最后一帧与动画结束后的真实画布完全重合。
///
/// 数据构造：窗口 1600×1000，实际画布宽 = 1600-14×2 = 1572；
///   源区域宽 600，镜头移动距离 = 1572-600 = 972；viewport pan=(30,-10)、zoom=1.2。
/// 执行过程：使用实际画布终点计算动画 viewport，再与静止 viewport 比较。
/// 预期结果：正断言为 pan/zoom 精确到达原 viewport；负断言为不残留缩略图比例。
#[test]
fn desktop_project_camera_lands_on_the_live_viewport() {
    let viewport = Viewport {
        pan: Vec2::new(30.0, -10.0),
        zoom: 1.2,
    };
    let node = CanvasNode {
        id: 7,
        kind: CanvasNodeKind::Term(TermSpec),
        bounds: CanvasRect::new(100.0, 50.0, 640.0, 400.0),
        z_index: 0,
        selected: false,
    };
    let card = CanvasRect::new(40.0, 92.0, 600.0, 796.0);
    let full = desktop_scene_rect(1600.0, 1000.0);
    let actual = project_transition_viewport(viewport, card, full, 1600.0, &[node.clone()])
        .expect("非空项目应有镜头 viewport");
    assert_eq!(
        actual, viewport,
        "移除动画层时卡片不能再次跳到不同的 pan/zoom"
    );
    assert_ne!(
        actual,
        project_preview_viewport(&[node], card).unwrap(),
        "终点不能仍是缩略 viewport"
    );
}
