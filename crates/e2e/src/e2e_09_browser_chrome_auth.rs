//! E2E-09b：Browser 卡片显式导入 Chrome session 的用户可见闭环。

use app::NewBrowser;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime, runtime_drain_step};

/// E2E-09b：点击 Chrome session 按钮后，卡片应从 Idle 进入 Ready 且保持当前 URL。
///
/// 数据构造（含关键数值的推导过程）：
///   Browser cards = 1，因此新建 node_id 唯一；
///   fake export = 1 条可导入 Cookie + 1 条 skipped Cookie；
///   expected imported = 1，expected skipped = 1；
///   initial URL = `https://www.google.com`，由 NewBrowser 默认值确定。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless app、进入 Canvas、新建 Browser 卡片。
///   2. 在 Chrome 外部系统边界注入确定性的成功响应，不替换 app 内部状态机。
///   3. 点击真实 `browser-chrome-auth` toolbar hitbox。
///   4. 按生产 runtime_drain_step 推进一帧，排空 Browser event，应用导入结果并重绘 toolbar 状态。
///
/// 预期结果：
///   - 正断言：点击前状态精确为 `idle`，按钮 hitbox 真实存在。
///   - 正断言：Chrome 文字按钮宽度至少 56px，不能沿用 22px 图标按钮而裁字。
///   - 正断言：点击后状态精确为 `ready`。
///   - 正断言：Browser URL 仍为默认 Google URL，导入只应 reload 不应导航。
///   - 负断言：URL 不得变为空或被扩展响应覆盖。
#[test]
fn e2e_09_browser_chrome_auth_button_imports_session_without_changing_url() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let browser_id = create_browser(&app, cx);

    let idle = app.update(cx, |app, _cx| app.browser_auth_state_for_e2e(browser_id));
    assert_eq!(
        idle, "idle",
        "新建 Browser 在用户请求前必须处于 idle，不能静默导出 Chrome Cookie"
    );
    let auth_button = cx
        .debug_bounds("browser-chrome-auth")
        .expect("Chrome session 按钮必须存在可点击 bounds");
    assert!(
        auth_button.size.width >= gpui::px(56.0),
        "Chrome session 文字按钮宽度必须至少 56px，不能裁掉 Chrome 或状态标签"
    );
    app.update(cx, |app, _cx| {
        app.set_browser_auth_fake_success_for_e2e(1, 1);
    });
    click_debug(cx, "browser-chrome-auth");
    cx.cx
        .background_executor
        .advance_clock(runtime_drain_step());
    cx.run_until_parked();

    let ready = app.update(cx, |app, _cx| app.browser_auth_state_for_e2e(browser_id));
    assert_eq!(
        ready, "ready",
        "点击 Chrome session 按钮并收到成功导出后，卡片状态必须变为 ready"
    );
    let saved_url = read_runtime(&app, cx, |runtime| browser_url(runtime, browser_id));
    assert_eq!(
        saved_url, "https://www.google.com",
        "Chrome Cookie 导入只应 reload 当前页面，不得改变 Browser URL"
    );
    assert!(
        !saved_url.is_empty(),
        "Chrome auth 响应不得把当前 Browser URL 清空"
    );
}

/// E2E-09c：Chrome connector 失败后 Browser 地址编辑与导航必须继续可用。
///
/// 数据构造（含关键数值的推导过程）：
///   Browser cards = 1；fake connector result = error；
///   runtime drain tick = runtime_drain_step()，从生产帧周期读取，不复制毫秒常量；
///   recovery URL = `https://recovery.example.test/path`，与默认 URL 不同。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新建 Browser，注入 Chrome 外部边界失败并点击 session 按钮。
///   2. 推进生产帧周期，让 Browser event queue 把状态置为 Failed；再等待最多3帧完成重绘节流。
///   3. 点击真实地址栏，Cmd+A 后输入 recovery URL 并 Enter 提交。
///   4. 读取 Runtime 中同一 Browser node 的 URL 与 auth 状态。
///
/// 预期结果：
///   - 正断言：connector 错误产生 `failed` 状态，而不是卡死在 waiting。
///   - 正断言：workspace error banner 显示失败原因，用户知道可以修复并重试。
///   - 正断言：地址栏仍能保存 recovery URL，Browser 业务链路未被阻断。
///   - 正断言：成功导航后 auth 状态重置为 `idle`，可对新 URL 重试。
///   - 负断言：失败后 URL 不得仍锁死为默认 Google URL。
#[test]
fn e2e_09_browser_chrome_auth_failure_keeps_address_navigation_usable() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let browser_id = create_browser(&app, cx);
    app.update(cx, |app, _cx| {
        app.set_browser_auth_fake_failure_for_e2e("connector unavailable");
    });

    click_debug(cx, "browser-chrome-auth");
    cx.cx
        .background_executor
        .advance_clock(runtime_drain_step());
    cx.run_until_parked();
    let failed = app.update(cx, |app, _cx| app.browser_auth_state_for_e2e(browser_id));
    assert_eq!(
        failed, "failed",
        "Chrome connector 错误必须进入 failed，不能永久停在 waiting"
    );
    let visible_error = app.update(cx, |app, _cx| app.workspace_error_for_e2e());
    assert!(
        visible_error
            .as_deref()
            .is_some_and(|message| message.contains("connector unavailable")),
        "Chrome connector 失败原因必须进入可见 workspace error banner"
    );
    // State application and painting use separate production timers: 3×16ms=48ms covers
    // the unfocused33ms repaint budget. Do not force a draw or assume one drain also paints.
    for _ in 0..3 {
        if cx.debug_bounds("workspace-error-banner").is_some() {
            break;
        }
        cx.cx
            .background_executor
            .advance_clock(runtime_drain_step());
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("workspace-error-banner").is_some(),
        "Chrome connector 失败后必须渲染可关闭的 workspace error banner"
    );

    click_debug(cx, "browser-address");
    let recovery_url = "https://recovery.example.test/path";
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input(recovery_url);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let saved_url = read_runtime(&app, cx, |runtime| browser_url(runtime, browser_id));
    assert_eq!(
        saved_url, recovery_url,
        "Chrome connector 失败后地址栏仍必须能提交新 URL"
    );
    assert_ne!(
        saved_url, "https://www.google.com",
        "失败状态不得锁死默认 Browser URL"
    );
    let reset = app.update(cx, |app, _cx| app.browser_auth_state_for_e2e(browser_id));
    assert_eq!(
        reset, "idle",
        "导航到新 URL 后旧页面的 Chrome auth 失败状态必须重置"
    );
}

fn create_browser(view: &gpui::Entity<app::MultisoulApp>, cx: &mut VisualTestContext) -> NodeId {
    let max_before = read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .map(|node| node.id)
            .max()
            .unwrap_or(0)
    });
    cx.dispatch_action(NewBrowser);
    cx.run_until_parked();
    read_runtime(view, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Browser(_)))
            .map(|node| node.id)
            .max()
            .filter(|node_id| *node_id > max_before)
            .expect("NewBrowser 应创建唯一的新 Browser node")
    })
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 必须存在可点击 bounds"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn browser_url(runtime: &runtime::Runtime, node_id: NodeId) -> String {
    let node = runtime
        .document
        .node(node_id)
        .expect("Browser node 必须仍存在于 document");
    let CanvasNodeKind::Browser(spec) = &node.kind else {
        panic!("id {node_id} 必须是 Browser node");
    };
    spec.url.clone()
}
