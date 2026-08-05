//! Canvas 卡片圆角材质的 headless GPUI 几何验收。

use gpui::TestAppContext;

use crate::harness::{create_terminal, echo_factory, enter_canvas, make_app};

/// E2E-14：选中 Terminal 的标题材质必须连续覆盖两个顶角，不能叠加近不透明深色角块。
///
/// 数据构造（含关键数值的推导过程）：
///   默认 viewport zoom = 1.0。
///   card radius = clamp(22 × 1.0, 12, 28) = 22px。
///   title height = 32 × 1.0 = 32px。
///   create_terminal 会聚焦并选中新卡片，因此外框为 2px 强调态。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过标准 harness 进入真实 headless Canvas，创建并选中一张 Terminal 卡片。
///   2. 从生产 GPUI 渲染树读取标题容器与自带圆角的标题材质层。
///   3. 检查标题材质层完整覆盖 32px 标题区，并检查旧左右实色角块不再渲染。
///
/// 预期结果：
///   - 正断言：标题材质层存在，与标题容器的位置、宽度及 32px 高度逐项一致。
///   - 负断言：旧 `card-title-corner-left/right` 近不透明角块均不得存在。
#[test]
fn e2e_14_selected_terminal_title_material_has_no_opaque_corner_caps() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("卡片圆角 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let _terminal = create_terminal(&app, cx);

    let title = cx
        .debug_bounds("card-title-layer")
        .expect("Terminal 卡片必须渲染共享标题材质层");
    let material = cx
        .debug_bounds("card-titlebar")
        .expect("标题渐变材质必须以单一自带圆角的层进入渲染树");

    assert_eq!(
        (f32::from(material.origin.x), f32::from(material.origin.y)),
        (f32::from(title.origin.x), f32::from(title.origin.y)),
        "标题材质必须从标题容器左上角开始，否则顶角会露底"
    );
    assert_eq!(
        f32::from(material.size.width),
        f32::from(title.size.width),
        "标题材质必须覆盖标题容器全宽，不能在左右顶角留缺口"
    );
    assert_eq!(
        f32::from(material.size.height),
        32.0,
        "默认 1× zoom 下标题材质高度必须为 32px"
    );
    assert_eq!(
        f32::from(material.size.height),
        f32::from(title.size.height),
        "标题材质必须覆盖标题容器全高，不能留下透明横带"
    );
    assert!(
        cx.debug_bounds("card-title-corner-left").is_none(),
        "左上角不得再渲染近不透明深色衬块"
    );
    assert!(
        cx.debug_bounds("card-title-corner-right").is_none(),
        "右上角不得再渲染近不透明深色衬块"
    );
}
