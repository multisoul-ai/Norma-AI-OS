//! Preview 图片查看器：在真实 GPUI 渲染树中验证多图、缩放与拖动漫游闭环。

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};
use canvas::CanvasRect;
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};

/// E2E-PREVIEW-IMAGE-01：三张图片应可缩放拖动，并在切换时恢复适合窗口状态。
///
/// 数据构造（含关键数值的推导过程）：
///   image_count       = 3 张（cover.png + detail.jpg + ending.webp）
///   initial_zoom      = 1.00x（适合窗口）
///   zoom_step         = 0.25x，因此点击一次放大后为 1.00 + 0.25 = 1.25x
///   pointer_delta     = (+40px, +30px)，用于验证二维拖动漫游而非单轴滚动
///   navigation_steps  = 1 次 Previous 边界点击 + 1 次 Next 有效点击
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI App，进入临时 Canvas，并通过公开 e2e seam 创建三图 Preview。
///   2. 从真实渲染树读取 viewer、viewport、toolbar、前后切换和缩放控件 bounds。
///   3. 在第一张图点击禁用的 Previous，验证索引仍停在 0。
///   4. 点击 Zoom In，把 1.00x 放大到 1.25x；在 viewport 内按下并拖动 40×30px。
///   5. 释放鼠标后点击 Next，读取图片索引、路径、缩放、滚动偏移和卡片位置。
///
/// 预期结果：
///   - 正断言：三图计数、首图路径和所有核心控件都出现在真实渲染树中。
///   - 正断言：放大后缩放值为 1.25，按下时进入 pan，拖动后两个偏移都改变。
///   - 正断言：Next 切到 detail.jpg，并把缩放与偏移恢复为 1.00 和 (0, 0)。
///   - 负断言：首图 Previous 不得越过索引 0；图片拖动不得移动 Preview 卡片。
///   - 负断言：切换一次不得跳到 ending.webp，释放鼠标后不得继续保持 pan。
#[test]
fn preview_gallery_zooms_pans_and_resets_when_navigating() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Preview 图片 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let cover = workspace.path().join("cover.png");
    let detail = workspace.path().join("detail.jpg");
    let ending = workspace.path().join("ending.webp");
    std::fs::write(&cover, b"preview-cover").expect("Preview 图片 e2e 应创建首图占位文件");
    std::fs::write(&detail, b"preview-detail").expect("Preview 图片 e2e 应创建第二图占位文件");
    std::fs::write(&ending, b"preview-ending").expect("Preview 图片 e2e 应创建末图占位文件");

    let node_id = app
        .update(cx, |app, cx| {
            app.spawn_preview_gallery_for_e2e(
                vec![cover.clone(), detail.clone(), ending.clone()],
                cx,
            )
        })
        .expect("三张合法图片应创建一个 Preview gallery 节点");
    cx.run_until_parked();

    let initial = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("Preview gallery 异步加载完成后应暴露图片查看器状态");
    let initial_bounds = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .node(node_id)
            .map(|node| node.bounds)
            .expect("Preview 图片节点应存在于 runtime document")
    });

    assert_eq!(
        initial.image_count, 3,
        "三条路径应合并为一个含 3 张图的 Preview"
    );
    assert_eq!(initial.index, 0, "Preview gallery 初始应显示第 1 张图片");
    assert_eq!(
        initial.zoom, 1.0,
        "Preview gallery 初始缩放应为适合窗口的 1.00x"
    );
    assert_eq!(
        initial.current_path,
        Some(cover.to_string_lossy().into_owned()),
        "Preview gallery 初始路径应指向 cover.png"
    );
    assert!(!initial.pan_active, "未按下图片时不应提前进入拖动漫游状态");

    let viewer = cx
        .debug_bounds("preview-image-viewer")
        .expect("图片查看器必须出现在真实 GPUI 渲染树中");
    let viewport = cx
        .debug_bounds("preview-image-viewport")
        .expect("图片可拖动 viewport 必须出现在真实 GPUI 渲染树中");
    let toolbar = cx
        .debug_bounds("preview-image-toolbar")
        .expect("图片查看器控制条必须出现在真实 GPUI 渲染树中");
    let previous = cx
        .debug_bounds("preview-image-previous")
        .expect("多图查看器必须渲染 Previous 控件");
    let next = cx
        .debug_bounds("preview-image-next")
        .expect("多图查看器必须渲染 Next 控件");
    let zoom_in = cx
        .debug_bounds("preview-image-zoom-in")
        .expect("图片查看器必须渲染 Zoom In 控件");
    assert!(
        f32::from(viewer.size.width) > 0.0 && f32::from(viewer.size.height) > 0.0,
        "图片查看器应拥有非零宽高，不能只有状态没有可见内容"
    );
    assert!(
        f32::from(viewport.size.width) > 0.0 && f32::from(viewport.size.height) > 0.0,
        "图片 viewport 应拥有非零宽高，才能承载缩放和拖动"
    );
    assert!(
        f32::from(toolbar.origin.y) >= f32::from(viewport.origin.y + viewport.size.height),
        "控制条应位于图片 viewport 下方，不能覆盖图片操作区域"
    );

    cx.simulate_click(previous.center(), Modifiers::none());
    cx.run_until_parked();
    let after_previous = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("点击首图 Previous 后图片查看器状态仍应存在");
    assert_eq!(
        after_previous.index, 0,
        "首图的禁用 Previous 不得把图片索引减到边界之外"
    );

    cx.simulate_click(zoom_in.center(), Modifiers::none());
    cx.run_until_parked();
    let zoomed = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("点击 Zoom In 后图片查看器状态仍应存在");
    assert_eq!(
        zoomed.zoom, 1.25,
        "一次 Zoom In 应把缩放从 1.00x 增加到 1.25x"
    );
    assert_ne!(
        (zoomed.offset_x, zoomed.offset_y),
        (0.0, 0.0),
        "以 viewport 中心放大后滚动偏移应同步居中，不能固定在左上角"
    );

    let drag_start = viewport.center();
    let drag_end = point(drag_start.x + px(40.0), drag_start.y + px(30.0));
    cx.simulate_mouse_down(drag_start, MouseButton::Left, Modifiers::none());
    let pressed = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("按下已放大的图片后查看器状态仍应存在");
    assert!(pressed.pan_active, "按下已放大的图片应进入拖动漫游状态");

    cx.simulate_mouse_move(drag_end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(drag_end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let panned = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("拖动并释放图片后查看器状态仍应存在");
    let bounds_after_pan: CanvasRect = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .node(node_id)
            .map(|node| node.bounds)
            .expect("拖动图片后 Preview 节点应继续存在")
    });
    assert_ne!(
        panned.offset_x, zoomed.offset_x,
        "水平拖动 40px 后图片 X 偏移应发生变化"
    );
    assert_ne!(
        panned.offset_y, zoomed.offset_y,
        "垂直拖动 30px 后图片 Y 偏移应发生变化"
    );
    assert!(!panned.pan_active, "鼠标释放后不得继续保持图片拖动漫游状态");
    assert_eq!(
        bounds_after_pan, initial_bounds,
        "拖动图片内容不得误触 Preview 卡片移动"
    );

    cx.simulate_click(next.center(), Modifiers::none());
    cx.run_until_parked();
    let navigated = app
        .update(cx, |app, _cx| app.preview_image_e2e_snapshot(node_id))
        .expect("切换图片后查看器状态仍应存在");
    assert_eq!(navigated.index, 1, "点击一次 Next 应从第 1 张切到第 2 张");
    assert_eq!(
        navigated.current_path,
        Some(detail.to_string_lossy().into_owned()),
        "点击一次 Next 后当前路径应指向 detail.jpg"
    );
    assert_ne!(
        navigated.current_path,
        Some(ending.to_string_lossy().into_owned()),
        "点击一次 Next 不得跨过第二张直接跳到 ending.webp"
    );
    assert_eq!(navigated.zoom, 1.0, "切换图片后应恢复适合窗口的 1.00x 缩放");
    assert_eq!(
        (navigated.offset_x, navigated.offset_y),
        (0.0, 0.0),
        "切换图片后应清空上一张图片的拖动偏移"
    );
    assert!(
        !navigated.pan_active,
        "切换图片后不得继承上一张图片的 pan 状态"
    );
}
