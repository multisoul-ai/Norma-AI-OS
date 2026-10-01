use crate::harness::{echo_factory, enter_canvas, make_app};
use gpui::TestAppContext;

/// 手机连接在 Settings 可发现，默认不监听且不展示假二维码或预选授权。
///
/// 数据构造：隔离 App 数据库、一个空项目窗口；不启用网络，不请求生产凭据。
/// 执行过程：打开 Settings → 手机连接 → 通知配置 → 拒绝HTTP → 保存主域名 /norma/ HTTPS入口 → 关闭重开 → 清除 → 关闭 Settings。
/// 预期结果：合法配置可恢复、清除；非法地址显示错误且不保存；整个流程不启用网络、二维码或Agent。
#[test]
fn e2e_mobile_settings_are_opt_in_and_have_no_fake_pairing() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("隔离空项目");
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let section = cx
        .debug_bounds("settings-section-mobile")
        .expect("Settings 必须有手机连接入口");
    cx.simulate_mouse_move(section.center(), None, Default::default());
    cx.simulate_click(section.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-enable").is_some(),
        "用户应看到显式启用操作"
    );
    assert!(
        cx.debug_bounds("mobile-qr").is_none(),
        "未启用不能生成或展示二维码"
    );
    assert!(
        cx.debug_bounds("mobile-code").is_none(),
        "未启用服务不能显示验证码"
    );
    assert!(
        cx.debug_bounds("mobile-copy-code").is_none(),
        "未启用服务时不能复制可用验证码"
    );
    assert!(
        cx.debug_bounds("mobile-approve").is_none(),
        "没有手机申请不能显示批准操作"
    );
    assert!(
        cx.debug_bounds("mobile-disable").is_none(),
        "默认不能处于监听状态"
    );
    let push = cx
        .debug_bounds("mobile-push-settings")
        .expect("任务提醒服务配置入口");
    cx.simulate_click(push.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-push-url").is_some(),
        "配置必须提供服务地址输入框"
    );
    assert!(
        cx.debug_bounds("mobile-push-key").is_some(),
        "配置必须提供遮蔽的服务凭据输入框"
    );
    assert!(
        cx.debug_bounds("mobile-qr").is_none(),
        "进入通知配置不能自动开启配对"
    );
    let url = cx.debug_bounds("mobile-push-url").expect("地址输入框");
    cx.simulate_click(url.center(), Default::default());
    cx.simulate_input("http://soulvoiceai.com/norma/v1/notifications");
    let key = cx.debug_bounds("mobile-push-key").expect("凭据输入框");
    cx.simulate_click(key.center(), Default::default());
    cx.simulate_input(&"c".repeat(64)); // 32-byte test credential = 64 hex digits; never a production secret.
    cx.run_until_parked();
    let save = cx.debug_bounds("mobile-push-save").expect("保存");
    cx.simulate_click(save.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-push-error").is_some(),
        "HTTP地址必须反馈验证错误"
    );
    assert!(
        cx.debug_bounds("mobile-push-configured").is_none(),
        "错误配置不能显示保存成功"
    );
    let url = cx.debug_bounds("mobile-push-url").expect("地址");
    cx.simulate_click(url.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("https://soulvoiceai.com/norma/v1/notifications");
    let key = cx.debug_bounds("mobile-push-key").expect("凭据");
    cx.simulate_click(key.center(), Default::default());
    cx.simulate_input(&"c".repeat(64));
    cx.run_until_parked();
    let save = cx.debug_bounds("mobile-push-save").expect("再次保存");
    cx.simulate_click(save.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-push-configured").is_some(),
        "合法配置保存成功必须反馈"
    );
    // GPUI 0.2.2 keeps old debug_bounds across frames; verify removed state through the live controller.
    let saved = app.update(cx, |app, cx| app.mobile_push_settings_for_e2e(cx));
    assert_eq!(
        saved.1.as_deref(),
        Some("https://soulvoiceai.com/norma/v1/notifications"),
        "必须保存用户输入的准确地址"
    );
    assert!(saved.2.is_none(), "成功后不能残留失败提示");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let push = cx
        .debug_bounds("mobile-push-settings")
        .expect("重新配置入口");
    cx.simulate_click(push.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-push-configured").is_some(),
        "重开必须读取已保存配置"
    );
    let clear = cx.debug_bounds("mobile-push-clear").expect("清除并停用");
    cx.simulate_click(clear.center(), Default::default());
    cx.run_until_parked();
    let cleared = app.update(cx, |app, cx| app.mobile_push_settings_for_e2e(cx));
    assert!(cleared.1.is_none(), "停用后不得继续显示已配置");
    assert!(!cleared.0, "配置服务不得自动启用网络连接");
    assert!(
        cx.debug_bounds("mobile-disable").is_none(),
        "通知配置不得启用手机网络服务"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let close = cx.debug_bounds("settings-close").expect("面板有退出入口");
    cx.simulate_mouse_move(close.center(), None, Default::default());
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !closed.settings_open,
        "关闭操作必须关闭整个 Settings 面板，不能继续显示移动设置"
    );
}

/// 欢迎页移除独立配对入口，项目内的 Settings 仍可打开手机连接。
///
/// 数据构造：隔离数据库的空 Launchpad，以及一个没有终端的临时项目。
/// 执行过程：检查欢迎页 → 打开项目 → Cmd+, → Mobile Connection → 关闭 Settings。
/// 预期结果：欢迎页没有配对按钮；设置中有启用按钮、没有二维码；关闭后设置面板不再打开。
#[test]
fn e2e_launchpad_leaves_mobile_pairing_in_project_settings() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("launchpad-mobile").is_none(),
        "欢迎页不得保留独立的手机连接按钮"
    );
    let workspace = tempfile::tempdir().expect("隔离空项目");
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let button = cx
        .debug_bounds("settings-section-mobile")
        .expect("项目内必须能从 Settings 进入手机连接");
    cx.simulate_mouse_move(button.center(), None, Default::default());
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mobile-enable").is_some(),
        "Settings 必须提供真实手机连接设置"
    );
    assert!(
        cx.debug_bounds("mobile-qr").is_none(),
        "未启用前不得伪造二维码"
    );
    let close = cx.debug_bounds("settings-close").expect("设置关闭入口");
    cx.simulate_click(close.center(), Default::default());
    cx.run_until_parked();
    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(!closed.settings_open, "关闭后不能继续显示手机连接设置");
}

/// 小窗口内的手机连接设置可滚动到最后一台设备，不把撤销按钮裁到窗口外。
///
/// 数据构造：960×480px 窗口；8 台手机 × 约 64px 行高 > 480px，另有标题、状态与帮助文本。
/// 执行过程：写入隔离设备 → 打开手机连接设置 → 缩小窗口 → 向下滚动 1000px → 检查末行。
/// 预期结果：第八台手机的撤销按钮进入可见区域；滚动前后位置必须变化，不能仅渲染在屏幕外。
#[test]
fn e2e_mobile_settings_scroll_inside_small_window() {
    use gpui::{point, px, size, ScrollDelta, ScrollWheelEvent};
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("隔离空项目");
    enter_canvas(&app, cx, workspace.path());
    let path = app.update(cx, |app, _| app.app_database_path_for_e2e());
    let store = storage::AppStore::new(path);
    for i in 0..8 {
        store
            .mobile_access_store()
            .register(
                &format!("phone-{i}"),
                &format!("iPhone {i}"),
                &"a".repeat(64),
                &[],
            )
            .expect("隔离手机");
    }
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let entry = cx
        .debug_bounds("settings-section-mobile")
        .expect("手机连接设置入口");
    cx.simulate_click(entry.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_resize(size(px(960.0), px(480.0)));
    cx.run_until_parked();
    let scroll = cx
        .debug_bounds("settings-content-scroll")
        .expect("手机连接设置必须使用原生设置滚动区");
    let before = cx.debug_bounds("mobile-revoke-7");
    cx.simulate_mouse_move(scroll.center(), None, Default::default());
    cx.simulate_event(ScrollWheelEvent {
        position: scroll.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-1000.0))),
        ..Default::default()
    });
    cx.run_until_parked();
    let last = cx
        .debug_bounds("mobile-revoke-7")
        .expect("滚动后的末台手机");
    assert!(
        last.top() >= px(0.0) && last.bottom() <= px(480.0),
        "最后一个撤销按钮必须在窗口内，actual={last:?}"
    );
    assert!(
        before.is_none_or(|bounds| bounds.top() > last.top()),
        "必须发生实际滚动，不能只把内容画到窗口之外"
    );
}
