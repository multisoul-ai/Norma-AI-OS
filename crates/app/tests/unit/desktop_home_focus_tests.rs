use super::input_tests::with_home;
use gpui::{Focusable, VisualTestContext};

fn settle_focus(cx: &mut VisualTestContext) {
    // 第一次绘制派发Focus/Blur，第二次绘制呈现订阅者处理后的最终状态。
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear());
        cx.run_until_parked();
    }
}

/// 场景：首页上的命令输入框必须独占自己的文本输入。
/// 数据：四个工作空间、首页搜索关闭、Root承载输入与键盘事件。
/// 执行：通过⌘K聚焦命令输入框，逐键输入help，完成后续绘制与事件处理。
/// 预期：完整文本保留在命令输入框，焦点不变；首页查询为空且面板保持关闭。
#[test]
fn desktop_home_command_input_keeps_text_and_focus() {
    with_home(|host, cx| {
        cx.simulate_keystrokes("cmd-k");
        cx.update(|window, cx| window.draw(cx).clear());
        host.update_in(cx, |app, window, cx| {
            assert!(app.command_input_focused(window, cx), "⌘K先聚焦命令输入");
            assert!(
                !app.desktop_workspace.home.search_open,
                "输入前首页搜索关闭"
            );
        });
        cx.simulate_keystrokes("h e l p");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        host.update_in(cx, |app, window, cx| {
            assert!(
                app.command_input_focused(window, cx),
                "首页不能抢走命令输入焦点"
            );
            assert_eq!(
                app.command_dock.as_ref().expect("命令输入").draft,
                "help",
                "每个字符必须进入命令输入"
            );
            assert!(
                app.desktop_workspace.search_query.is_empty(),
                "首页查询不受命令文本影响"
            );
            assert!(
                !app.desktop_workspace.home.search_open,
                "命令输入不能展开首页搜索"
            );
        });
    });
}

/// 场景：克隆表单与命令输入一样，不能被首页的直接输入搜索劫持。
/// 数据：四空间首页，打开已有克隆表单但不提交、不发网络请求。
/// 执行：记录表单焦点，输入repo并完成后续绘制。
/// 预期：表单保持打开和原焦点，首页搜索及查询都不改变。
#[test]
fn desktop_home_clone_form_keeps_keyboard_focus() {
    with_home(|host, cx| {
        host.update_in(cx, |app, window, cx| {
            app.open_clone_project_dialog(window, cx);
        });
        settle_focus(cx);
        // 克隆表单通过window.defer回焦，必须在它完成后记录实际输入焦点。
        let focus = host.update_in(cx, |app, window, cx| {
            assert!(
                !app.focus_handle.is_focused(window),
                "克隆表单打开后焦点已离开首页根节点"
            );
            window.focused(cx).expect("克隆表单必须聚焦输入框")
        });
        cx.simulate_keystrokes("r e p o");
        settle_focus(cx);
        host.update_in(cx, |app, window, _| {
            assert!(app.clone_project.is_open(), "输入不能关闭克隆表单");
            assert!(focus.is_focused(window), "字符输入后仍聚焦克隆表单");
            assert!(
                !app.desktop_workspace.home.search_open,
                "克隆输入不能展开首页搜索"
            );
            assert!(
                app.desktop_workspace.search_query.is_empty(),
                "克隆输入不写首页查询"
            );
        });
    });
}

/// 场景：鼠标点击搜索关闭按钮，不应同时触发重叠搜索入口的点击。
/// 数据：四空间首页、已展开面板和活动窗口；关闭按钮位于面板右上。
/// 执行：按40px按钮宽、右距8px和顶距7px推导中心，点击后处理后续帧。
/// 预期：搜索保持关闭、焦点回入口；随后输入b仍能重新展开。
#[test]
fn desktop_home_mouse_close_stays_closed() {
    with_home(|host, cx| {
        let input = host.update_in(cx, |app, window, cx| {
            let input = app
                .desktop_workspace
                .home
                .search_input
                .clone()
                .expect("首页搜索");
            app.open_home_search(&input, window, cx);
            input
        });
        settle_focus(cx);
        let panel = cx
            .debug_bounds("desktop-workspace-search")
            .expect("搜索面板");
        let close = gpui::point(panel.right() - gpui::px(28.0), panel.top() + gpui::px(27.0));
        cx.simulate_click(close, gpui::Modifiers::none());
        settle_focus(cx);
        host.update_in(cx, |app, window, cx| {
            assert!(
                !app.desktop_workspace.home.search_open,
                "鼠标关闭不能冒泡到搜索入口再次展开"
            );
            assert!(
                input.focus_handle(cx).is_focused(window),
                "关闭后入口保持焦点"
            );
        });
        assert!(
            cx.debug_bounds("desktop-home-search-results").is_none(),
            "最终帧没有搜索结果面板"
        );
        cx.simulate_keystrokes("b");
        settle_focus(cx);
        host.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.home.search_open,
                "鼠标关闭后输入可重新展开"
            );
            assert_eq!(app.desktop_workspace.search_query, "b", "再次输入完整保留");
        });
    });
}

/// 场景：焦点在搜索面板操作按钮时，关闭操作必须跨后续Focus事件保持生效。
/// 数据：四条8天前记录，搜索含关闭与显示全部按钮；开启减动效。
/// 执行：Tab到关闭/显示全部后Esc，或Tab到关闭后完整Enter；绘制两帧；随后输入b。
/// 预期：面板稳定关闭、查询清空且焦点回搜索入口；随后输入可重新展开筛选。
#[test]
fn desktop_home_close_from_button_stays_closed_after_focus_events() {
    for (navigation, key) in [("tab", "escape"), ("tab tab", "escape"), ("tab", "enter")] {
        with_home(|host, cx| {
            let input = host.update_in(cx, |app, window, cx| {
                let input = app
                    .desktop_workspace
                    .home
                    .search_input
                    .clone()
                    .expect("首页搜索框");
                app.open_home_search(&input, window, cx);
                input
            });
            settle_focus(cx);
            cx.simulate_keystrokes(navigation);
            settle_focus(cx);
            host.update_in(cx, |_, window, cx| {
                assert!(window.is_window_active(), "活动窗口才能派发真实Focus事件");
                assert!(
                    !input.focus_handle(cx).is_focused(window),
                    "{navigation}确实将焦点移到按钮"
                );
            });
            cx.simulate_keystrokes(key);
            cx.simulate_event(gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse(key).expect("合法关闭按键"),
            });
            settle_focus(cx);
            host.update_in(cx, |app, window, cx| {
                assert!(
                    !app.desktop_workspace.home.search_open,
                    "{navigation}后{key}必须跨Focus事件保持关闭"
                );
                assert!(
                    app.desktop_workspace.search_query.is_empty(),
                    "关闭搜索清空查询"
                );
                assert!(
                    input.focus_handle(cx).is_focused(window),
                    "关闭后焦点回到同一搜索入口"
                );
            });
            assert!(
                cx.debug_bounds("desktop-home-search-results").is_none(),
                "最终帧不能绘制搜索结果面板"
            );
            cx.simulate_keystrokes("b");
            settle_focus(cx);
            host.read_with(cx, |app, _| {
                assert!(
                    app.desktop_workspace.home.search_open,
                    "关闭后输入仍可重新展开"
                );
                assert_eq!(
                    app.desktop_workspace.search_query, "b",
                    "重新输入不能丢失字符"
                );
            });
            cx.simulate_keystrokes("escape");
            settle_focus(cx);
            host.update_in(cx, |app, window, _| app.focus_handle.focus(window));
            settle_focus(cx);
            host.update_in(cx, |_, window, cx| input.focus_handle(cx).focus(window));
            settle_focus(cx);
            host.read_with(cx, |app, _| {
                assert!(
                    app.desktop_workspace.home.search_open,
                    "离开入口后重新聚焦仍应展开"
                );
                assert!(
                    app.desktop_workspace.search_query.is_empty(),
                    "重新聚焦不恢复旧查询"
                );
            });
        });
    }
}
