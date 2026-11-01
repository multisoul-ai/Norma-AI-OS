use super::CommandPreview;
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

/// 场景：在空画布输入指令，预览只显示规则结论；编辑原话后必须重新预览。
/// 构造：真实组件Input、Button和1280×850隔离窗口；不配置任何模型或派发适配器。
/// 执行：经Input公共set_value输入，点击真实预览按钮，检查渲染结果；再次编辑。
/// 预期：输入前/编辑后显示等待；分析后显示需要主模型，不能保留旧结果或假装已派发。
#[test]
fn voice_command_preview_edit_invalidates_visible_result() {
    let mut test = TestAppContext::single();
    let mut preview = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        ui::init_components(cx);
        let entity = cx.new(|cx| CommandPreview::new(ui::Theme::default(), window, cx));
        preview = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(size(px(1280.0), px(850.0)));
    let preview = preview.expect("预览实体");
    let input = preview.read_with(cx, |view, _| view.input.clone());
    input.update_in(cx, |input, window, cx| {
        input.set_value("测试员，请继续刚才的任务", window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let analyze = cx
        .debug_bounds("voice-command-preview-analyze")
        .expect("本地预览按钮");
    cx.simulate_click(analyze.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-command-preview-result-voice.command_preview_model")
            .is_some(),
        "空画布指令不能伪造完整语义，应显示需要主模型"
    );
    assert!(
        cx.debug_bounds("voice-command-preview-result-voice.command_preview_pending")
            .is_none(),
        "主动预览后不能仍停留在等待状态"
    );
    input.update_in(cx, |input, window, cx| {
        input.set_value("新的指令，先别执行", window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-command-preview-result-voice.command_preview_pending")
            .is_some(),
        "编辑原话必须让旧结果失效"
    );
    assert!(
        cx.debug_bounds("voice-command-preview-result-voice.command_preview_model")
            .is_none(),
        "修改后不得继续展示旧句的判断"
    );
}

/// 场景：真实App设置遮罩有自己的焦点处理，点击预览编辑器后文字仍须进入该编辑器。
/// 构造：1280×850隔离App；不配置模型、不创建PTY，ASCII probe共5字符。
/// 执行：从设置入口打开语音页，点击输入框中心，再通过平台键盘事件输入probe。
/// 预期：Input实际内容等于probe，设置遮罩不能把输入焦点抢回画布。
#[test]
fn voice_command_preview_keeps_keyboard_focus_inside_settings() {
    use crate::app::MultisoulApp;
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
    let host = host.expect("App宿主");
    host.update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let field = cx
        .debug_bounds("voice-command-preview-input")
        .expect("预览编辑器");
    cx.simulate_click(field.center(), Modifiers::none());
    cx.simulate_keystrokes("p r o b e");
    cx.run_until_parked();
    let preview = host.read_with(cx, |app, _| {
        app.voice_settings_inputs
            .command_preview
            .clone()
            .expect("预览实体")
    });
    let value = preview.read_with(cx, |view, cx| view.input.read(cx).value().to_string());
    assert_eq!(
        value, "probe",
        "点击后的按键必须进入真实预览Input，不能被设置遮罩抢走"
    );
    assert!(
        !host.read_with(cx, |app, _| app.interaction.space_held),
        "设置输入不得启动画布平移"
    );
}
