//! Canvas 错误反馈：创建或清理失败必须在当前工作区可见且可关闭。

use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-CANVAS-ERROR-01：长路径错误应收在可见横幅内，用户关闭后状态清空。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace error = Chrome bridge 指引 + 148 字符绝对路径，用来覆盖真实打包路径
///   banner max width = 720px（生产横幅的桌面宽度上限）
///   banner count     = 1 个（同一时刻只展示当前错误）
///   dismiss actions  = 1 次点击
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI app 并进入空 Canvas。
///   2. 通过 e2e 公共 seam 注入包含 Chrome 扩展绝对路径的失败文本并触发重绘。
///   3. 从真实渲染树读取 Canvas、横幅、正文与关闭按钮 bounds。
///   4. 模拟点击关闭按钮，再读取公开错误状态。
///
/// 预期结果：
///   - 正断言：错误横幅、正文和关闭按钮都拥有非空 bounds。
///   - 正断言：横幅宽度不超过 720px，且右边缘没有越出 Canvas。
///   - 正断言：关闭按钮完整位于横幅内部，长路径不能把按钮挤出容器。
///   - 正断言：点击关闭后错误状态为 None。
///   - 负断言：错误不得只存在于状态中却缺席渲染树。
///   - 负断言：长路径不得把横幅撑到 Canvas 之外。
///   - 负断言：关闭后状态不得继续保留旧错误文本。
#[test]
fn canvas_workspace_error_is_visible_and_dismissible() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Canvas 错误反馈 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let error = "Chrome session bridge did not connect. Open chrome://extensions, enable Developer mode, then Load unpacked from: /Users/alan/Documents/mybook/multisoul-desktop/.worktrees/feat-browser-chrome-auth/target/debug/Norma OS Dev.app/Contents/Resources/browser-auth-extension";
    app.update(cx, |app, cx| {
        app.show_workspace_error_for_e2e(error, cx);
    });
    cx.run_until_parked();

    let canvas = cx
        .debug_bounds("canvas")
        .expect("Canvas 错误反馈必须在真实 Canvas 渲染树中验证");
    let banner = cx
        .debug_bounds("workspace-error-banner")
        .expect("Canvas 错误状态必须渲染 workspace-error-banner");
    let message = cx
        .debug_bounds("workspace-error-message")
        .expect("Canvas 错误横幅必须渲染非空正文区域");
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
        f32::from(message.size.width) > 0.0 && f32::from(message.size.height) > 0.0,
        "错误正文必须拥有非零宽高，不能只显示空底板"
    );
    assert!(
        f32::from(banner.size.width) <= 720.0,
        "长 Chrome 扩展路径不得把错误横幅撑过 720px 宽度上限"
    );
    assert!(
        f32::from(banner.origin.x) + f32::from(banner.size.width)
            <= f32::from(canvas.origin.x) + f32::from(canvas.size.width),
        "长 Chrome 扩展路径不得让错误横幅右边缘越出 Canvas"
    );
    assert!(
        f32::from(dismiss.origin.x) >= f32::from(banner.origin.x)
            && f32::from(dismiss.origin.x) + f32::from(dismiss.size.width)
                <= f32::from(banner.origin.x) + f32::from(banner.size.width),
        "关闭按钮必须完整保留在错误横幅内部，不能被长路径挤出容器"
    );
    assert!(
        f32::from(message.origin.x) >= f32::from(banner.origin.x)
            && f32::from(message.origin.x) + f32::from(message.size.width)
                <= f32::from(dismiss.origin.x),
        "错误正文必须收在横幅内并止于关闭按钮左侧，不能覆盖或穿出操作区"
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
