use super::{execute_command, protocol::Command, Outcome};
use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, Modifiers, TestAppContext};
use ui::component::WindowExt;

/// 场景：控制通道向真实首页搜索控件投递文字、全选和删除，不能在 KeyUp 重绘时丢字。
/// 数据构造：隔离存储含 AlphaKeep、BetaKeep；搜索词 Alpha 只匹配前者。
/// 执行过程：渲染实际首页、点击控件边界中心，经服务循环使用的命令入口输入，
/// 再全选删除；每次命令完成后检查搜索状态与过滤结果。
/// 预期结果：完整保留 Alpha；匹配项出现、不匹配项消失；删除后空字符串与两项恢复。
#[test]
fn control_text_updates_workspace_search_before_key_up_redraw() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    catalog.create("AlphaKeep").expect("应创建匹配夹具");
    catalog.create("BetaKeep").expect("应创建不匹配夹具");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建真实首页宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.workspaces = catalog.list().expect("应加载两项夹具");
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let search = cx
        .debug_bounds("desktop-workspace-search")
        .expect("应渲染搜索控件");
    cx.simulate_click(search.center(), Modifiers::none());
    cx.run_until_parked();
    let input = cx.update(|window, cx| {
        window
            .focused_input(cx)
            .expect("点击必须确实建立输入焦点，不能只验证命令应答")
    });
    input.read_with(cx, |input, _| {
        assert_eq!(input.value().as_ref(), "", "输入前控件应为空");
    });
    let mut async_cx = cx.to_async();
    let send = |command, cx: &mut gpui::AsyncApp| {
        let Outcome::Done(response) = execute_command(command, cx) else {
            panic!("输入命令不能返回截图结果");
        };
        assert_eq!(response.to_line(), r#"{"ok":true}"#, "控制通道应完成输入");
    };
    send(
        Command::Text {
            text: "Alpha".into(),
        },
        &mut async_cx,
    );
    input.read_with(cx, |input, _| {
        assert_eq!(
            input.value().as_ref(),
            "Alpha",
            "控件本身必须保留完整文本，不能仅模型改变"
        );
    });
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.search_query, "Alpha",
            "命令完成时搜索词必须完整保留，不能被旧值覆盖"
        );
        let names: Vec<_> = app
            .desktop_workspace
            .filtered()
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, ["AlphaKeep"], "输入必须同时更新过滤结果");
        assert!(!names.contains(&"BetaKeep"), "不匹配的空间必须消失");
    });
    send(
        Command::Key {
            keystroke: "cmd-a".into(),
        },
        &mut async_cx,
    );
    send(
        Command::Key {
            keystroke: "backspace".into(),
        },
        &mut async_cx,
    );
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.search_query, "",
            "全选删除必须清空搜索，不能由 KeyUp 恢复旧值"
        );
        assert_eq!(
            app.desktop_workspace.filtered().len(),
            2,
            "清空后两项必须恢复"
        );
    });
    input.read_with(cx, |input, _| {
        assert_eq!(input.value().as_ref(), "", "删除必须实际清空控件文本");
    });
}
