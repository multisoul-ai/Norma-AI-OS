//! Canvas 错误反馈：创建或清理失败必须在当前工作区可见且可关闭。

use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-CANVAS-ERROR-01：Canvas 错误应渲染可见横幅，用户关闭后状态清空。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace error = 1 条固定文本 `Could not create temporary Agent`
///   banner count    = 1 个（同一时刻只展示当前错误）
///   dismiss actions = 1 次点击
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI app 并进入空 Canvas。
///   2. 通过 e2e 公共 seam 注入创建失败文本并触发重绘。
///   3. 从真实渲染树读取 `workspace-error-banner` 与关闭按钮 bounds。
///   4. 模拟点击关闭按钮，再读取公开错误状态。
///
/// 预期结果：
///   - 正断言：错误横幅和关闭按钮都拥有非空可点击 bounds。
///   - 正断言：点击关闭后错误状态为 None。
///   - 负断言：错误不得只存在于状态中却缺席渲染树。
///   - 负断言：关闭后状态不得继续保留旧错误文本。
#[test]
fn canvas_workspace_error_is_visible_and_dismissible() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Canvas 错误反馈 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_workspace_error_for_e2e("Could not create temporary Agent", cx);
    });
    cx.run_until_parked();

    let banner = cx
        .debug_bounds("workspace-error-banner")
        .expect("Canvas 错误状态必须渲染 workspace-error-banner");
    let dismiss = cx
        .debug_bounds("workspace-error-dismiss")
        .expect("Canvas 错误横幅必须提供可点击关闭按钮");

    assert!(
        f32::from(banner.size.width) > 0.0,
        "错误横幅宽度必须大于 0，不能只写状态不渲染"
    );
    assert!(
        f32::from(banner.size.height) > 0.0,
        "错误横幅高度必须大于 0，不能成为不可见元素"
    );
    assert!(
        app.update(cx, |app, _cx| app.workspace_error_for_e2e())
            .is_some(),
        "点击关闭前错误状态应仍存在，证明渲染来自真实状态"
    );

    cx.simulate_click(dismiss.center(), Modifiers::none());
    cx.run_until_parked();

    let error_after = app.update(cx, |app, _cx| app.workspace_error_for_e2e());
    assert_eq!(
        error_after, None,
        "点击关闭后 Canvas 不得继续保留旧错误文本"
    );
}
