use super::*;
use crate::shortcuts::EffectiveShortcutEntry;
use gpui::{AppContext, TestAppContext};
use std::{cell::Cell, rc::Rc};

/// 场景：可配置的App退出键必须先于输入框的SelectAll等局部键图执行。
/// 构造：真实Root与MultisoulApp、已聚焦命令输入，Quit映射为原生全选键（Win Ctrl+A/Mac Cmd+A）。
/// 执行：通过GPUI输入派发发送该键，全局Quit回调读取原App实体以检查借用已释放。
/// 预期：只收到一次Quit，不能被InputState吞掉或在Entity仍被借用时重入读取。
#[test]
fn quit_shortcut_routes_before_focused_input_keymap() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        ui::init_components(cx);
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.unwrap();
    let quits = Rc::new(Cell::new(0));
    let observed = quits.clone();
    let target = app.clone();
    cx.update(|_, cx| {
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
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    assert_eq!(
        quits.get(),
        1,
        "全局退出必须先于输入框全选，并在实体归还后只分派一次"
    );
}
