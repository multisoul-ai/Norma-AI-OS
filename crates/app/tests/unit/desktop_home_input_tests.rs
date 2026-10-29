use super::*;
use crate::app::AppScreen;
use gpui::{size, AppContext, EntityInputHandler, Focusable, TestAppContext, VisualTestContext};

pub(super) fn with_home(run: impl FnOnce(Entity<MultisoulApp>, &mut VisualTestContext)) {
    let temp = tempfile::tempdir().expect("隔离数据目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化数据库");
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        store
            .desktop_workspace_store()
            .create(name)
            .expect("创建空间");
    }
    // 启动和返回首页都会异步刷新目录；时间夹具必须落到隔离数据库，不能只改内存。
    // 8天严格早于最近7天边界，4条记录触发“更早”默认折叠。
    let changed = rusqlite::Connection::open(paths.app_db_path())
        .expect("打开隔离数据库")
        .execute(
            "UPDATE desktop_workspaces SET last_opened_at_unix_ms=?1",
            [search_input::now_ms() - 8 * 86_400_000],
        )
        .expect("持久化更早组夹具");
    assert_eq!(changed, 4, "全部4个空间均需属于更早组");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("宿主");
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        window.activate_window();
        app.motion.reduce_motion = true;
        app.show_desktop_launchpad(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    run(host, cx);
}

/// 场景：返回首页后直接输入启动搜索。
/// 数据：4个空间，初始聚焦App根节点，未点击搜索。
/// 执行：发送a按键，绘制搜索面板。
/// 预期：查询为a、搜索展开且输入框聚焦；不能打开空间。
#[test]
fn desktop_home_typing_from_root_opens_search() {
    with_home(|host, cx| {
        cx.simulate_keystrokes("a");
        cx.update(|window, cx| window.draw(cx).clear());
        host.update_in(cx, |app, window, cx| {
            assert_eq!(
                app.desktop_workspace.search_query, "a",
                "首字符不能丢失或重复"
            );
            assert!(app.desktop_workspace.home.search_open, "直接输入展开搜索");
            assert!(
                app.desktop_workspace
                    .home
                    .search_input
                    .as_ref()
                    .expect("搜索框")
                    .focus_handle(cx)
                    .is_focused(window),
                "焦点转到输入框"
            );
            assert_eq!(app.screen, AppScreen::Launchpad, "输入不能打开空间");
        });
    });
}

/// 场景：点击搜索面板中的“显示全部”，不能穿透到关闭面板的遮罩。
/// 数据：4个8天前空间，默认折叠为2个；通过Root绘制真实输入、按钮和遮罩。
/// 执行：打开搜索，鼠标点击展开按钮，再点击面板外空白。
/// 预期：首次点击展开全部且保留面板，不打开空间；外部点击才关闭搜索。
#[test]
fn desktop_home_search_mouse_does_not_click_through_panel() {
    with_home(|host, cx| {
        host.update_in(cx, |app, window, cx| {
            let input = app
                .desktop_workspace
                .home
                .search_input
                .clone()
                .expect("搜索框");
            app.open_home_search(&input, window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let button = cx.debug_bounds("desktop-home-older").expect("更早展开按钮");
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear());
        host.read_with(cx, |app, cx| {
            assert!(
                app.desktop_workspace.home.search_open,
                "面板内按钮不能触发遮罩关闭"
            );
            assert!(
                app.desktop_workspace.home.older_expanded,
                "鼠标点击必须展开更早组"
            );
            assert_eq!(
                app.home_search_groups(cx)[0].entries.len(),
                4,
                "展开后应显示全部4个空间"
            );
            assert!(
                app.desktop_workspace.active_id.is_none(),
                "展开按钮不能打开空间"
            );
        });
        cx.simulate_click(gpui::point(px(40.0), px(120.0)), gpui::Modifiers::none());
        host.read_with(cx, |app, _| {
            assert!(
                !app.desktop_workspace.home.search_open,
                "面板外遮罩仍必须支持关闭"
            );
        });
    });
}

/// 场景：IME候选确认不触发工作空间导航。
/// 数据：4个空间，组合串Alpha匹配现有空间。
/// 执行：标记组合文本，再向首页捕获器发送Enter和Esc。
/// 预期：搜索保持打开，空间未激活；组合文本仍存在。
#[test]
fn desktop_home_composition_keeps_navigation_keys_for_ime() {
    with_home(|host, cx| {
        host.update_in(cx, |app, window, cx| {
            let input = app
                .desktop_workspace
                .home
                .search_input
                .clone()
                .expect("搜索框");
            app.open_home_search(&input, window, cx);
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "Alpha", Some(5..5), window, cx)
            });
            app.desktop_workspace.search_query = "Alpha".into();
            for key in ["down", "up", "enter", "escape"] {
                app.capture_home_search_key(
                    &input,
                    &gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse(key).expect("合法按键"),
                        is_held: false,
                    },
                    window,
                    cx,
                );
            }
            assert!(
                app.desktop_workspace.home.search_open,
                "IME按键不能关闭搜索"
            );
            assert!(
                app.desktop_workspace.active_id.is_none(),
                "候选确认不能打开空间"
            );
            assert_eq!(app.desktop_workspace.home.cursor, 0, "IME不能移动结果选择");
            assert!(
                input.update(cx, |input, cx| input
                    .marked_text_range(window, cx)
                    .is_some()),
                "组合范围不能被清除"
            );
        });
    });
}

/// 场景：输入组件取消IME组合时，Esc不能同时关闭首页搜索。
/// 数据：4个空间，搜索框有尚未提交的中文组合文本“工作”。
/// 执行：从Root派发Esc，经过Input Escape action和首页捕获器。
/// 预期：第一次Esc仅取消组合，面板和输入焦点保留；第二次Esc关闭面板。
#[test]
fn desktop_home_ime_escape_through_root_keeps_search_open() {
    with_home(|host, cx| {
        let input = host.update_in(cx, |app, window, cx| {
            let input = app
                .desktop_workspace
                .home
                .search_input
                .clone()
                .expect("搜索框");
            app.open_home_search(&input, window, cx);
            input.update(cx, |state, cx| {
                state.replace_and_mark_text_in_range(None, "工作", Some(2..2), window, cx)
            });
            input
        });
        cx.update(|window, cx| window.draw(cx).clear());
        host.update_in(cx, |_, window, cx| {
            assert_eq!(
                input.read(cx).value().as_ref(),
                "工作",
                "重绘不能清除尚未提交的组合文本"
            );
            assert!(
                input.update(cx, |state, cx| state
                    .marked_text_range(window, cx)
                    .is_some()),
                "派发Esc前组合范围必须仍存在"
            );
        });
        cx.simulate_keystrokes("escape");
        host.update_in(cx, |app, window, cx| {
            assert!(
                app.desktop_workspace.home.search_open,
                "首次Esc只能取消IME组合"
            );
            assert!(
                input.focus_handle(cx).is_focused(window),
                "取消组合保留输入焦点"
            );
            assert!(
                input.update(cx, |state, cx| state
                    .marked_text_range(window, cx)
                    .is_none()),
                "Input应结束组合"
            );
            assert!(
                app.desktop_workspace.active_id.is_none(),
                "取消组合不能导航"
            );
        });
        cx.simulate_keystrokes("escape");
        host.read_with(cx, |app, _| {
            assert!(
                !app.desktop_workspace.home.search_open,
                "无组合文本时Esc关闭面板"
            );
        });
    });
}

/// 场景：Tab可达面板内操作，Esc关闭面板。
/// 数据：4个8天前空间，更早默认显示2个；焦点在搜索框。
/// 执行：两次Tab经过关闭按钮到显示全部，Enter展开；验证正反Tab环绕，再Esc。
/// 预期：键盘确实可展开更早组；Esc关闭并返回同一搜索入口。
#[test]
fn desktop_home_tab_keeps_panel_controls_accessible() {
    with_home(|host, cx| {
        host.update_in(cx, |app, window, cx| {
            let input = app
                .desktop_workspace
                .home
                .search_input
                .clone()
                .expect("搜索框");
            app.open_home_search(&input, window, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_keystrokes("tab");
        host.read_with(cx, |app, cx| {
            assert!(
                app.desktop_workspace.home.search_open,
                "Tab不能卸载面板按钮"
            );
            let groups = app.home_search_groups(cx);
            assert_eq!(groups.len(), 1, "目录刷新后仍应只有更早组");
            assert_eq!(
                groups[0].kind,
                catalog::GroupKind::Older,
                "时间夹具不能被刷新覆盖"
            );
            assert_eq!(groups[0].total, 4, "4条更早记录必须保留展开入口");
            assert_eq!(groups[0].entries.len(), 2, "展开前只能显示2条");
        });
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
        });
        host.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.home.older_expanded,
                "Enter必须触发展开按钮，不能被搜索导航抢走"
            )
        });
        cx.simulate_keystrokes("tab");
        host.update_in(cx, |app, window, cx| {
            assert!(
                app.desktop_workspace
                    .home
                    .search_input
                    .as_ref()
                    .expect("搜索框")
                    .focus_handle(cx)
                    .is_focused(window),
                "面板末尾Tab必须回到输入框，不能进入遮罩后方控件"
            );
        });
        cx.simulate_keystrokes("shift-tab");
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
        });
        host.read_with(cx, |app, _| {
            assert!(
                !app.desktop_workspace.home.older_expanded,
                "反向Tab应回到更早组按钮并可收起"
            );
            assert!(
                app.desktop_workspace.home.search_open,
                "焦点环绕不能关闭面板"
            );
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| window.draw(cx).clear());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        host.read_with(cx, |app, _| {
            assert!(!app.desktop_workspace.home.search_open, "Esc关闭面板")
        });
    });
}

/// 场景：菜单Tab选择删除后，Enter必须执行聚焦项。
/// 数据：四个工作空间，菜单初始键盘游标停在重命名。
/// 执行：分别Tab到删除、从删除用Up回重命名、Down后Tab回重命名，再发送完整Enter。
/// 预期：执行实际聚焦项或箭头选择项；另一动作不发生，也不能打开工作空间。
#[test]
fn desktop_home_menu_tab_enter_activates_focused_item() {
    for (keys, delete, on_release) in [
        ("tab tab", true, true),
        ("tab tab up", false, false),
        ("down tab", false, true),
    ] {
        with_home(|host, cx| {
            let trigger = cx
                .debug_bounds("desktop-workspace-actions-0")
                .expect("更多按钮");
            cx.simulate_click(trigger.center(), gpui::Modifiers::none());
            for key in keys.split(' ') {
                cx.update(|window, cx| window.draw(cx).clear());
                cx.simulate_keystrokes(key);
            }
            cx.update(|window, cx| window.draw(cx).clear());
            cx.simulate_keystrokes("enter");
            if on_release {
                host.read_with(cx, |app, _| {
                    assert!(
                        app.desktop_workspace.rename_target.is_none()
                            && app.desktop_workspace.pending_delete.is_none(),
                        "按钮必须等待松开触发当前聚焦项：{keys}"
                    );
                });
            }
            cx.update(|window, cx| window.draw(cx).clear());
            cx.simulate_event(gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse("enter").expect("合法按键"),
            });
            host.read_with(cx, |app, _| {
                assert_eq!(
                    app.desktop_workspace.pending_delete.is_some(),
                    delete,
                    "删除状态必须符合实际聚焦项：{keys}"
                );
                assert_eq!(
                    app.desktop_workspace.rename_target.is_some(),
                    !delete,
                    "重命名不能使用旧游标：{keys}"
                );
                assert!(
                    app.desktop_workspace.active_id.is_none(),
                    "菜单不得打开工作空间：{keys}"
                );
            });
        });
    }
}

/// 场景：窄窗换行后，卡片菜单仍完整位于窗口内。
/// 数据：4个空间，600×1000窗口，两列两行，菜单184px宽。
/// 执行：打开第一张及末张卡片菜单，读取最终绘制边界，再Esc关闭。
/// 预期：菜单不越过8px窗口安全距，且菜单点击不进入工作空间。
#[test]
fn desktop_home_menu_stays_inside_narrow_viewport() {
    with_home(|host, cx| {
        cx.simulate_resize(size(px(600.0), px(1000.0)));
        for index in [0, 3] {
            cx.update(|window, cx| window.draw(cx).clear());
            let trigger = cx
                .debug_bounds(if index == 0 {
                    "desktop-workspace-actions-0"
                } else {
                    "desktop-workspace-actions-3"
                })
                .expect("菜单触发器");
            cx.simulate_click(trigger.center(), gpui::Modifiers::none());
            cx.update(|window, cx| window.draw(cx).clear());
            let menu = cx
                .debug_bounds("desktop-home-actions-menu")
                .expect("菜单应打开");
            assert!(
                menu.top() >= px(8.0) && menu.bottom() <= px(992.0),
                "菜单必须落在窗口内：{menu:?}"
            );
            assert!(
                menu.left() >= px(8.0) && menu.right() <= px(592.0),
                "菜单横向不能越界"
            );
            host.read_with(cx, |app, _| {
                assert_eq!(app.screen, AppScreen::Launchpad, "点击菜单不得打开空间")
            });
            let rename = cx
                .debug_bounds(if index == 0 {
                    "desktop-workspace-rename-0"
                } else {
                    "desktop-workspace-rename-3"
                })
                .expect("重命名项");
            cx.simulate_click(rename.center(), gpui::Modifiers::none());
            host.read_with(cx, |app, _| {
                assert!(
                    app.desktop_workspace.rename_target.is_some(),
                    "菜单必须收到点击，不能被后绘制卡片遮住"
                );
                assert_eq!(app.screen, AppScreen::Launchpad, "菜单不能打开后方卡片");
            });
            cx.update(|window, cx| window.draw(cx).clear());
            let cancel = cx
                .debug_bounds("desktop-workspace-rename-cancel")
                .expect("取消重命名");
            cx.simulate_click(cancel.center(), gpui::Modifiers::none());
            cx.simulate_keystrokes("escape");
        }
    });
}

/// 场景：滚动首页后打开第四卡，异步访问时间更新不能改变放大起点。
/// 数据：600×840、四卡两行，滚动50px；预览比外卡内缩7px（边框1+留白6）。
/// 执行：冻结第四卡锚点→更新其时间为最新→推进相机3×80ms→绘制。
/// 预期：原第四卡预览与相机矩形一致，允许1px取整，不能改为第一槽或漏缩放滚动。
#[test]
fn desktop_home_motion_freezes_scrolled_card_after_touch() {
    with_home(|host, cx| {
        cx.simulate_resize(size(px(600.0), px(840.0)));
        cx.update(|window, cx| window.draw(cx).clear());
        host.update(cx, |app, cx| {
            app.desktop_directory_scroll
                .set_offset(gpui::point(px(0.0), px(-50.0)));
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let before = cx
            .debug_bounds("desktop-home-preview-panel-3")
            .expect("第四卡预览");
        host.update_in(cx, |app, window, cx| {
            let target = catalog::recent(&app.home_entries(cx))[3].id.clone();
            app.motion.reduce_motion = false;
            app.open_desktop_workspace(target.clone(), window, cx);
            let transition = app
                .desktop_workspace
                .transition
                .as_mut()
                .expect("最近卡有真实放大来源");
            assert!(
                (transition.rect().origin.y - f32::from(before.top())).abs() <= 1.0,
                "起点必须含实际滚动偏移"
            );
            for _ in 0..3 {
                transition.tick(std::time::Duration::from_millis(80));
            }
            app.desktop_workspace
                .workspaces
                .iter_mut()
                .find(|w| w.id == target)
                .expect("目标存在")
                .last_opened_at_unix_ms = search_input::now_ms();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let actual = cx
            .debug_bounds("desktop-home-preview-panel-3")
            .expect("放大中仍保留第四槽");
        host.read_with(cx, |app, _| {
            let expected = app
                .desktop_workspace
                .transition
                .as_ref()
                .expect("仍在转场")
                .rect();
            assert!(
                (f32::from(actual.left()) - expected.origin.x).abs() <= 1.0
                    && (f32::from(actual.top()) - expected.origin.y).abs() <= 1.0,
                "外壳必须跟随相机：actual={actual:?} expected={expected:?}"
            );
            assert!(
                (f32::from(actual.size.width) - expected.size.x).abs() <= 1.0,
                "预览与相机同步缩放"
            );
        });
    });
}
