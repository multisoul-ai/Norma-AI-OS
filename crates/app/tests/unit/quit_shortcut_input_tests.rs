use super::*;
use crate::shortcuts::{EffectiveShortcutEntry, ShortcutCommandId};
use gpui::{AppContext, KeyDownEvent, Keystroke, TestAppContext};
use std::{cell::Cell, rc::Rc};

/// 场景：可配置的App退出键必须先于输入框的SelectAll等局部键图执行。
/// 构造：生产Root→GateRoot→MultisoulApp、已聚焦命令输入，Quit映射为原生全选键。
/// 执行：通过GPUI输入派发发送该键，全局Quit回调读取原App实体以检查借用已释放。
/// 预期：正常按键收到Quit；长按和录制不触发；结束录制后可再次退出，不重入读取实体。
#[test]
fn quit_shortcut_routes_before_focused_input_keymap() {
    assert_quit_shortcut_routes(true);
}

/// 场景：直接项目窗没有GateRoot和DesktopHosts登记，也必须遵守自定义退出键。
/// 构造：Root直接挂载隔离工作台，Quit映射为Win Ctrl+A/Mac Cmd+A并聚焦真实输入框。
/// 执行：正常按键、held、录制、再次请求退出均走正式GPUI派发。
/// 预期：局部SelectAll不能吞退出，held/录制不能退出，旧默认键不能重新生效。
#[test]
fn quit_shortcut_routes_for_direct_project_input() {
    assert_quit_shortcut_routes(false);
}

fn assert_quit_shortcut_routes(gated: bool) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        ui::init_components(cx);
        if gated {
            let gate = cx.new(|cx| {
                let mut gate = GateRoot::new(BootPhase::Activation, window, cx);
                gate.mount_workbench_from_context(paths, store, window, cx);
                app = gate.workbench.clone();
                gate
            });
            ui::component::Root::new(gate, window, cx)
        } else {
            let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
            app = Some(entity.clone());
            ui::component::Root::new(entity, window, cx)
        }
    });
    let app = app.unwrap();
    let quits = Rc::new(Cell::new(0));
    let observed = quits.clone();
    let target = app.clone();
    cx.update(|_, cx| {
        crate::quit_shortcut::install(cx);
        cx.on_action(move |_: &crate::app::QuitNormaOs, cx| {
            let _ = target.read(cx).screen;
            observed.set(observed.get() + 1);
        });
    });
    let key = if cfg!(windows) { "ctrl-a" } else { "cmd-a" };
    app.update_in(cx, |app, window, cx| {
        // Inject only the routing policy on macOS; its production Cmd+Q menu binding is unchanged.
        app.shortcuts
            .effective
            .retain(|entry| entry.command != ShortcutCommandId::QuitNormaOs);
        app.shortcuts.effective.push(EffectiveShortcutEntry {
            command: ShortcutCommandId::QuitNormaOs,
            category: "common.workspace",
            title: "menu.quit_norma_os",
            hint: "shortcuts.quit_norma_os_hint",
            default_keystroke: None,
            current_keystroke: Some(key.into()),
            editable: true,
            modified: true,
        });
        app.focus_command_input(window, cx);
        assert!(
            app.command_input_focused(window, cx),
            "反例必须实际聚焦InputState"
        );
    });
    cx.run_until_parked();
    cx.simulate_input("selection");
    app.update_in(cx, |app, window, cx| {
        assert!(
            app.command_input_focused(window, cx),
            "派发前的实际焦点必须仍在输入框，防止root路径假阳性"
        );
    });
    #[cfg(windows)]
    {
        cx.simulate_keystrokes("ctrl-shift-q");
        cx.run_until_parked();
        assert_eq!(quits.get(), 0, "自定义退出键后，旧默认键不能重新生效");
    }
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    assert_eq!(
        quits.get(),
        1,
        "全局退出必须先于输入框全选，并在实体归还后只分派一次"
    );
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse(key).unwrap(),
        is_held: true,
    });
    cx.run_until_parked();
    assert_eq!(quits.get(), 1, "长按重复消息不能重复请求退出");
    app.update(cx, |app, _| {
        app.shortcuts.recording = Some(ShortcutCommandId::QuitNormaOs)
    });
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    assert_eq!(quits.get(), 1, "录制期间的合法退出键只能供录制使用");
    app.update(cx, |app, _| app.shortcuts.recording = None);
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    assert_eq!(
        quits.get(),
        2,
        "被脏文档等拦截后，后续正常按键仍可再次请求退出"
    );
    #[cfg(windows)]
    {
        app.update(cx, |app, _| {
            app.shortcuts.recording = Some(ShortcutCommandId::QuitNormaOs)
        });
        cx.simulate_keystrokes("ctrl-shift-q");
        cx.run_until_parked();
        assert_eq!(quits.get(), 2, "录制期间旧默认退出键也只能供录制使用");
    }
}
