use crate::app::MultisoulApp;
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

/// 已配置的 GPT-Live 只展示日常偏好，密钥编辑与诊断按需展开。
/// 数据构造：隔离 SQLite 预置 GPT 专属引用与 cedar，无真实 Key；窗口 1280×900。
/// 执行过程：打开真实渲染入口，检查默认内容，依次点击更换 Key、收起和诊断。
/// 预期结果：Key 不回填、不常驻编辑；离线无静音/停播按钮；诊断展开后才能核对。
#[test]
fn gpt_live_material_settings_progressively_disclose_key_and_diagnostics() {
    let temp = tempfile::tempdir().expect("隔离数据目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let settings = store.settings_store();
    for (key, value) in [
        ("voice.desktop.mode", "gpt_live"),
        (
            "voice.gpt_live.credential_ref",
            "gpt-live-desktop-fixture-ref",
        ),
        ("voice.gpt_live.voice", "cedar"),
        ("voice.doubao.credential_ref", "doubao-fixture-ref"),
    ] {
        settings.set_string(key, value).expect("写入非机密夹具");
    }
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(900.0)));
    let host = host.expect("创建宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-live-reconcile").is_none(),
        "诊断操作默认收起"
    );
    assert!(
        cx.debug_bounds("settings-live-call-mute").is_none(),
        "未通话时不展示无效静音按钮"
    );
    assert!(
        cx.debug_bounds("settings-live-call-output").is_none(),
        "未通话时不展示无效停播按钮"
    );
    assert!(
        cx.debug_bounds("settings-voice-field-gpt-live-api-key")
            .is_none(),
        "已保存 Key 默认不展示编辑框"
    );
    let edit = cx
        .debug_bounds("settings-live-key-edit")
        .expect("保留可发现的更换 Key 入口");
    cx.simulate_click(edit.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-voice-field-gpt-live-api-key")
            .is_some(),
        "更换时展开 Key 编辑框"
    );
    assert!(
        !host
            .read_with(cx, |app, cx| app
                .voice_settings_inputs
                .rendered_text(
                    crate::settings::voice::VoiceSettingsField::GptLiveApiKey,
                    cx
                )
                .unwrap_or_default())
            .contains("fixture-ref"),
        "界面不能显示凭据引用或真实 Key"
    );
    let collapse = cx
        .debug_bounds("settings-live-key-edit")
        .expect("可收起 Key 编辑器");
    cx.simulate_click(collapse.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-voice-input-device-picker")
            .is_none(),
        "不暴露采集层未使用的手动设备选择器"
    );
    assert!(
        cx.debug_bounds("settings-live-system-microphone").is_some(),
        "保留系统输入状态与恢复入口"
    );
    host.update(cx, |app, cx| {
        let height = app.settings_viewport.content_scroll.max_offset().height;
        app.settings_viewport
            .content_scroll
            .set_offset(gpui::point(px(0.0), -height));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let diagnostics = cx
        .debug_bounds("settings-live-diagnostics-toggle")
        .expect("保留低频诊断入口");
    cx.simulate_click(diagnostics.center(), Modifiers::none());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("settings-live-reconcile").is_some(),
        "显式展开后保留真实任务核对能力"
    );
    assert_eq!(
        settings
            .get_string("voice.doubao.credential_ref")
            .expect("读取豆包配置")
            .as_deref(),
        Some("doubao-fixture-ref"),
        "页面操作不改豆包凭据"
    );
    assert_eq!(
        settings
            .get_string("voice.gpt_live.voice")
            .expect("读取音色")
            .as_deref(),
        Some("cedar"),
        "浏览设置不重置音色"
    );
}

/// 首次配置 Key 时，分两次原生文本输入不能被自动保存引起的重绘打断。
/// 数据构造：隔离空 Key 配置；先输入一个字符，再补齐本地无效夹具，不连接云端；分别用 Enter、鼠标隐藏和移除结束。
/// 执行过程：点击 Key 输入框→输入首字符→等待后台保存→继续输入→Enter 或 MouseDown/重绘/MouseUp→等待保存。
/// 预期结果：首次输入期间字段持续存在；提交后收起，凭据存储中保存的是完整值。
#[test]
fn gpt_live_material_first_key_stays_visible_until_editing_finishes() {
    use gpui::{EntityInputHandler, Focusable};
    for finish in ["enter", "hide", "remove"] {
        let temp = tempfile::tempdir().expect("隔离目录");
        let paths = storage::DatabasePaths::isolated_test(temp.path());
        let store = storage::AppStore::initialize(&paths).expect("隔离存储");
        let settings = store.settings_store();
        settings
            .set_string("voice.desktop.mode", "gpt_live")
            .expect("GPT模式");
        let mut test = TestAppContext::single();
        let mut host = None;
        let (_, cx) = test.add_window_view(|window, cx| {
            let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
            host = Some(entity.clone());
            ui::component::Root::new(entity, window, cx)
        });
        cx.simulate_resize(size(px(1280.0), px(900.0)));
        let host = host.expect("宿主");
        host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let field = cx
            .debug_bounds("settings-voice-field-gpt-live-api-key")
            .expect("首次直接展示 Key 编辑器");
        cx.simulate_click(field.center(), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        cx.run_until_parked();
        let input = host.read_with(cx, |app, _| {
            app.voice_settings_inputs
                .input(crate::settings::voice::VoiceSettingsField::GptLiveApiKey)
        });
        // This test starts at the native editing seam; pointer focus is covered by desktop-e2e.
        host.update_in(cx, |app, window, cx| {
            app.begin_voice_text_input(
                crate::settings::voice::VoiceSettingsField::GptLiveApiKey,
                String::new(),
                cx,
            );
            input.read(cx).focus_handle(cx).focus(window);
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.run_until_parked();
        let focused = host.read_with(cx, |app, _| app.voice_settings.editing_field());
        assert_eq!(
            focused,
            Some(crate::settings::voice::VoiceSettingsField::GptLiveApiKey),
            "原生输入前必须已经聚焦 Key，finish={finish}, field={field:?}, viewport={:?}",
            cx.debug_bounds("settings-content-scroll")
        );
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "f", window, cx)
            })
        });
        host.update(cx, |app, cx| app.persist_voice_settings_in_background(cx));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear());
            if !host.read_with(cx, |app, _| {
                app.voice_settings.credential_persistence_pending()
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "必须等待首字符后台保存后再续写"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        cx.update(|window, cx| window.draw(cx).clear());
        assert!(
            cx.debug_bounds("settings-voice-field-gpt-live-api-key")
                .is_some(),
            "首字符自动保存后不能移除仍在编辑的输入框"
        );
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "ixture-only-live-layout-key", window, cx)
            })
        });
        cx.update(|window, cx| window.draw(cx).clear());
        assert!(
            cx.debug_bounds("settings-voice-field-gpt-live-api-key")
                .is_some(),
            "补齐 Key 后提交前仍能编辑"
        );
        if finish != "enter" {
            host.update(cx, |app, cx| app.persist_voice_settings_in_background(cx));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                cx.run_until_parked();
                cx.update(|window, cx| window.draw(cx).clear());
                if !host.read_with(cx, |app, _| {
                    app.voice_settings.credential_persistence_pending()
                }) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "结束编辑前必须等待后台凭据保存完成"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let hide = cx
                .debug_bounds(if finish == "remove" {
                    "settings-live-key-clear"
                } else {
                    "settings-live-key-edit"
                })
                .unwrap_or_else(|| {
                    let snapshot = host.read_with(cx, |app, _| (
                        app.voice_settings.values().gpt_live_profile().credential_ref().is_some(),
                        app.voice_settings.editing_field(),
                        app.voice_settings.credential_persistence_pending(),
                    ));
                    panic!("首次编辑期间有结束入口，finish={finish}, configured/editing/pending={snapshot:?}")
                });
            let viewport = cx
                .debug_bounds("settings-content-scroll")
                .expect("设置视口");
            assert!(viewport.contains(&hide.center()), "结束编辑按钮必须在可点击视口内，finish={finish}, button={hide:?}, viewport={viewport:?}");
            cx.simulate_mouse_move(hide.center(), None, Modifiers::none());
            cx.update(|window, cx| window.draw(cx).clear());
            cx.simulate_mouse_down(hide.center(), gpui::MouseButton::Left, Modifiers::none());
            cx.update(|window, cx| window.draw(cx).clear());
            cx.simulate_mouse_up(hide.center(), gpui::MouseButton::Left, Modifiers::none());
        } else {
            cx.simulate_keystrokes("enter");
        }
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        if finish == "remove" {
            let empty = cx
                .debug_bounds("settings-voice-field-gpt-live-api-key")
                .expect("移除后显示空编辑器");
            cx.simulate_click(empty.center(), Modifiers::none());
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
            assert_eq!(
                settings
                    .get_string("voice.gpt_live.credential_ref")
                    .expect("查询移除结果"),
                None,
                "移除后 Enter 不得重新保存编辑器中残留的旧 Key"
            );
            continue;
        }
        assert!(
            cx.debug_bounds("settings-voice-field-gpt-live-api-key")
                .is_none(),
            "提交或隐藏后收起首次 Key 编辑器，finish={finish}, state={:?}",
            host.read_with(cx, |app, _| (
                app.voice_settings.editing_field(),
                app.voice_settings.gpt_live_key_expanded(),
                app.voice_settings
                    .values()
                    .gpt_live_profile()
                    .credential_ref()
                    .is_some()
            ))
        );
        let account = settings
            .get_string("voice.gpt_live.credential_ref")
            .expect("读取引用")
            .expect("已持久化引用");
        let complete =
            crate::settings::voice::VoiceSettingsState::load_credential(&settings, &account)
                .expect("读取本地夹具凭据");
        assert!(
            complete.as_deref() == Some("fixture-only-live-layout-key"),
            "保存值必须完整，不能只保存首字符；断言不输出值"
        );
    }
}

/// GPT 设置按钮的 Enter/Space 必须只在一次完整按下/松开中执行一次。
/// 数据构造：通过生产 live_action 渲染单按钮、计数从 0 开始；2 次键盘 + 1 次鼠标 = 3。
/// 执行过程：Tab 聚焦；分别派发 KeyDown、重绘、KeyUp，再执行鼠标点击。
/// 预期结果：每组事件只增长一次，不因共享旧 KeyDown 适配器与 GPUI KeyUp 重复切换。
#[test]
fn gpt_live_material_actions_activate_once_per_complete_key_press() {
    use gpui::{prelude::*, Context, KeyDownEvent, KeyUpEvent, Keystroke, Render, Window};
    use std::{cell::Cell, rc::Rc};
    struct Fixture(gpui::Entity<MultisoulApp>, Rc<Cell<usize>>);
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let count = self.1.clone();
            self.0.update(cx, |_, cx| {
                super::live_action(
                    ui::Theme::sapphire_night(),
                    "live-keyboard-fixture",
                    "Action",
                    false,
                    false,
                    cx,
                    move |_, _, _| count.set(count.get() + 1),
                )
            })
        }
    }
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let count = Rc::new(Cell::new(0));
    let mut test = TestAppContext::single();
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let fixture = cx.new(|_| Fixture(app, count.clone()));
        ui::component::Root::new(fixture, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.update(|window, _| window.focus_next());
    cx.simulate_keystrokes("tab escape");
    for (key, expected) in [("enter", 1), ("space", 2)] {
        let stroke = Keystroke::parse(key).expect("合法按键");
        cx.simulate_event(KeyDownEvent {
            keystroke: stroke.clone(),
            is_held: false,
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_event(KeyUpEvent { keystroke: stroke });
        assert_eq!(count.get(), expected, "每次完整键盘操作必须且只能激活一次");
    }
    let button = cx.debug_bounds("live-keyboard-fixture").expect("按钮可见");
    cx.simulate_click(button.center(), Modifiers::none());
    assert_eq!(count.get(), 3, "鼠标仍使用同一生产回调且不重复执行");
}
