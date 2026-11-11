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
    assert!(
        cx.debug_bounds("settings-tts-change-voice").is_some(),
        "语音回复应是可点开声音面板的一行"
    );
    assert!(
        cx.debug_bounds("settings-tts-current-voice").is_none(),
        "主界面不应再展示当前声音大卡片"
    );
    assert!(
        cx.debug_bounds("voice-a1-status-row").is_none(),
        "A1 状态行已并入更多设置，主界面不再单列"
    );
    assert!(
        cx.debug_bounds("voice-status-next").is_some(),
        "缺项由顶部横幅给出授权入口"
    );
    assert!(
        cx.debug_bounds("voice-a1-download-install").is_none(),
        "A1 完整安装面板默认应收进更多设置"
    );
    assert!(
        cx.debug_bounds("voice-setup-asr-azure-detail").is_none(),
        "Azure 凭据默认不应铺在语音输入主界面"
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
    open_group(
        &host.clone().expect("宿主"),
        cx,
        crate::settings::voice::AdvancedGroup::Model,
    );
    let model = cx
        .debug_bounds("voice-advanced-status-Model")
        .expect("模型分组标题应显示状态标签");
    assert!(
        model.size.width > px(0.0) && model.size.height > px(0.0),
        "未配置模型也必须显示明确状态标签，不能留空白"
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

/// 场景：A1 状态行与安装面板都收进更多设置；展开更多设置后才出现完整安装和 Jev 配置。
/// 构造：隔离 App 与默认语音模式，1280×850 采用原有桌面验收基准。
/// 执行：从公开语音设置入口绘制，确认安装按钮未展开；再展开更多设置并查找安装与 Jev 操作。
/// 预期：折叠时没有安装按钮和 Python 输入；展开后安装与 Jev 入口存在，不能虚构已连接或生产已启用。
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
    let host = host.expect("宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-a1-status-row").is_none(),
        "A1 状态行收进更多设置，折叠时不应出现"
    );
    assert!(
        cx.debug_bounds("voice-a1-download-install").is_none(),
        "未展开更多设置时不应绘制完整安装面板"
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
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("展开后内容视口");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), -px(4000.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    // 只显示当前来源的设置：先切到 A1 来源再找安装入口。
    host.update(cx, |app, cx| {
        app.voice_settings
            .open_setup_destination(crate::settings::voice::VoiceSetupDestination::AsrA1);
        cx.notify();
    });
    open_group(
        &host,
        cx,
        crate::settings::voice::AdvancedGroup::Recognition,
    );
    assert!(
        cx.debug_bounds("voice-a1-download-install").is_some(),
        "A1 来源下应能找到安装入口"
    );
    open_group(&host, cx, crate::settings::voice::AdvancedGroup::Jev);
    for selector in [
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

/// 手风琴同一时刻只展开一组：测试需要哪组内容就打开哪组。
fn open_group(
    host: &gpui::Entity<MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    group: crate::settings::voice::AdvancedGroup,
) {
    host.update(cx, |app, cx| {
        app.voice_settings.open_advanced_group(group);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
}

/// 场景：豆包模式下顶部分段条必须与内容区同宽，不能按文字内容收缩。
/// 构造：隔离 App，切到豆包模式，957 宽窗口（接近用户截图）。
/// 执行：绘制后读取 GPT-Live 分段与内容视口的右边界。
/// 预期：最右一格贴近内容视口右侧（差距小于 48px 内边距）。
#[test]
fn doubao_mode_tabs_fill_content_width() {
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
    cx.simulate_resize(size(px(957.0), px(600.0)));
    let host = host.expect("宿主");
    host.update_in(cx, |app, window, cx| {
        app.open_voice_settings(window, cx);
        app.select_desktop_voice_mode(crate::settings::voice::DesktopVoiceMode::Doubao, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let viewport = cx.debug_bounds("settings-content-scroll").expect("视口");
    let tab = cx
        .debug_bounds("settings-voice-desktop-mode-gpt-live")
        .expect("分段");
    assert!(
        viewport.right() - tab.right() < px(48.0),
        "分段条应撑满内容宽度：{tab:?} vs {viewport:?}"
    );
}
