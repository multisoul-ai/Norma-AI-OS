use crate::app::MultisoulApp;
use gpui::{px, size, AppContext, TestAppContext};

/// 场景：未配置语音时也呈现用户选定的四组列表和本地预览，而不是另一套安装向导。
/// 构造：隔离空数据库；1280×850 为原型的桌面验收尺寸，640×700 为窄窗口。
/// 执行：从公开的设置入口绘制，比较四组边界和预览；再缩窄窗口并重新绘制。
/// 预期：四组依次同宽；宽屏预览在右侧，窄屏在列表下方；均不越过内容视口。
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
    host.expect("宿主")
        .update_in(cx, |app, window, cx| app.open_voice_settings(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let model = cx
        .debug_bounds("voice-command-value-voice.command_current_model")
        .expect("当前模型状态");
    assert!(
        model.size.width > px(0.0) && model.size.height > px(0.0),
        "未配置模型也必须显示明确状态，不能留空白行"
    );
    let input = cx
        .debug_bounds("voice-command-group-input")
        .expect("01语音输入分组应存在");
    let understanding = cx
        .debug_bounds("voice-command-group-understanding")
        .expect("02理解与目标分组应存在");
    let dispatch = cx
        .debug_bounds("voice-command-group-dispatch")
        .expect("03任务派发分组应存在");
    let reply = cx
        .debug_bounds("voice-command-group-reply")
        .expect("04语音回复分组应存在");
    let preview = cx
        .debug_bounds("voice-command-preview")
        .expect("真实本地规则预览应存在");
    assert!(
        input.bottom() <= understanding.top()
            && understanding.bottom() <= dispatch.top()
            && dispatch.bottom() <= reply.top(),
        "四组必须按原型顺序排列，不能重叠"
    );
    assert_eq!(input.size.width, reply.size.width, "输入与回复分组应同宽");
    assert!(
        preview.left() >= input.right(),
        "桌面宽屏预览应在设置列表右侧"
    );
    cx.simulate_resize(size(px(640.0), px(700.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let input = cx
        .debug_bounds("voice-command-group-input")
        .expect("窄屏输入分组");
    let reply = cx
        .debug_bounds("voice-command-group-reply")
        .expect("窄屏回复分组");
    let preview = cx.debug_bounds("voice-command-preview").expect("窄屏预览");
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("内容视口");
    assert!(preview.top() >= reply.bottom(), "窄屏预览应移到列表下方");
    assert!(
        input.left() >= viewport.left()
            && input.right() <= viewport.right()
            && preview.right() <= viewport.right(),
        "窄屏内容不能横向溢出"
    );
}
