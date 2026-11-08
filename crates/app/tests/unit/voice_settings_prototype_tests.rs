use crate::app::MultisoulApp;
use gpui::{
    point, px, size, AppContext, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase,
};

/// 场景：未配置语音时呈现「听 / 说 / 高级设置」三组；高级设置默认折叠，展开后才出现模型与诊断。
/// 构造：隔离空数据库；1280×850 为桌面验收尺寸，640×700 为窄窗口。
/// 执行：从公开的设置入口绘制，检查三组顺序与宽度；展开高级设置；再缩窄窗口并重新绘制。
/// 预期：三组按顺序同宽；高级设置默认不含模型行；展开后模型状态可见；窄屏不横向溢出。
#[test]
fn voice_settings_match_grouped_prototype_before_setup() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离设置");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(850.0)));
    host.clone()
        .expect("宿主")
        .update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let listen = cx
        .debug_bounds("voice-command-group-listen")
        .expect("听分组应存在");
    let speak = cx
        .debug_bounds("voice-command-group-speak")
        .expect("说分组应存在");
    let advanced = cx
        .debug_bounds("voice-command-group-advanced")
        .expect("高级设置分组应存在");
    assert!(
        listen.bottom() <= speak.top() && speak.bottom() <= advanced.top(),
        "三组必须按顺序排列，不能重叠"
    );
    assert_eq!(listen.size.width, speak.size.width, "听与说分组应同宽");
    assert!(
        cx.debug_bounds("voice-command-value-voice.command_current_model")
            .is_none(),
        "高级设置默认折叠，不应显示模型行"
    );
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -px(4000.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let toggle = cx
        .debug_bounds("voice-command-advanced-toggle")
        .expect("高级设置开关应存在");
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -px(4000.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let model = cx
        .debug_bounds("voice-command-value-voice.command_current_model")
        .expect("展开后应显示当前模型状态");
    assert!(
        model.size.width > px(0.0) && model.size.height > px(0.0),
        "未配置模型也必须显示明确状态，不能留空白行"
    );
    cx.simulate_resize(size(px(640.0), px(700.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let listen = cx
        .debug_bounds("voice-command-group-listen")
        .expect("窄屏听分组");
    let speak = cx
        .debug_bounds("voice-command-group-speak")
        .expect("窄屏说分组");
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    assert!(
        listen.left() >= viewport.left()
            && listen.right() <= viewport.right()
            && speak.right() <= viewport.right(),
        "窄屏内容不能横向溢出"
    );
}

/// 场景：分组列表必须提供A1完整安装和Jev配置入口，不能只显示诊断或未接入提示。
/// 构造：隔离App与默认语音模式，1280×850采用原有桌面验收基准。
/// 执行：从公开语音设置入口绘制，查找安装操作及Jev密钥/保存/验证操作。
/// 预期：安装无需Python输入，真实配置入口存在，不能虚构已连接或Jev生产已启用。
#[test]
fn voice_settings_expose_a1_install_and_jev_configuration() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离设置");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(850.0)));
    host.expect("宿主")
        .update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-a1-download-install").is_some(),
        "A1 安装入口应在听分组中可见"
    );
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -px(4000.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -px(4000.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let toggle = cx
        .debug_bounds("voice-command-advanced-toggle")
        .expect("高级设置开关应存在");
    cx.simulate_click(toggle.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    for selector in [
        "voice-a1-download-install",
        "voice-jev-api-key",
        "voice-jev-save",
        "voice-jev-verify",
        "voice-jev-production-disabled",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "用户应能找到真实配置入口：{selector}"
        );
    }
    assert!(
        cx.debug_bounds("voice-a1-python").is_none(),
        "应用托管引擎不能再要求普通用户配置Python"
    );
    assert!(
        cx.debug_bounds("voice-jev-connected").is_none(),
        "未经真实校验不能声称连接成功"
    );
    assert!(
        cx.debug_bounds("voice-jev-production-enabled").is_none(),
        "配置页不能开启生产入口"
    );
}
