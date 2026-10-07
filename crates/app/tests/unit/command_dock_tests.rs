use super::*;
use gpui::{EntityInputHandler, KeyDownEvent, Keystroke, TestAppContext};
use terminal::{PtyBackend, TestPtyBackend};

/// 场景：无项目主页中的文字对话提示也需要显示，不能等待 Canvas 动画调度。
///
/// 数据构造：空数据根、Launchpad、活跃窗口；输入「交流偏好」没有指令候选。
/// 执行过程：聚焦输入 → 输入非空中文 → 同步抽屉 → 查询动画调度。
/// 预期结果：抽屉使用 NextFrame，不能退回 idle timer 使反馈始终透明；没有项目。
#[test]
fn personal_conversation_drawer_advances_without_a_canvas() {
    let temp = tempfile::tempdir().expect("应创建隔离数据根");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.window_active = true;
        app.focus_command_input(window, cx);
        app.set_command_query("交流偏好".into(), true, window, cx);
        app.sync_command_drawer(window);
        assert!(
            matches!(
                app.animation_next_wakeup(),
                super::super::frame_schedule::AnimationWakeup::NextFrame(_)
            ),
            "主页必须推进抽屉动画，显示发送、等待、回复和错误状态"
        );
        assert!(
            app.workspace_state.current_root.is_none(),
            "个人入口不能创建伪项目"
        );
    });
}

/// 场景：App 帧循环驱动底部抽屉，清空输入时保留退出内容并在结束后释放遮挡。
///
/// 数据构造：/terminal 对应 1 条建议，高度 = 40 + 54 + 8 = 102px。
///   展开 220ms 的半程为 110ms；关闭 160ms 的半程为 80ms。
///   底边 = 视口高度 - Dock 底部距离 - Dock 高度（面板紧贴 Dock，无间隔）。
/// 执行过程：设置查询 → 同步展示 → App 推进半程、结束 → 清空 → 推进退出半程、结束。
/// 预期结果：帧调度进入活动状态；底边不移动；退出保留原建议且禁止执行；结束释放面板和帧调度。
#[test]
fn command_dock_drawer_uses_frame_driver_and_releases_overlay() {
    use crate::effects::dispatch_receipt::{COMMAND_DOCK_BOTTOM_PX, COMMAND_DOCK_HEIGHT_PX};
    use std::time::Duration;

    let workspace = tempfile::tempdir().expect("临时 workspace 应可创建");
    let mut test_cx = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test_cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| {
                    Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
                }),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("App entity 应创建");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.motion = crate::motion::MotionStore::new(false);
            app.focus_command_input(window, cx);
            app.set_command_query("/terminal".into(), true, window, cx);
            app.sync_command_drawer(window);
            assert!(
                app.animation_activity().canvas_frame_synced_active,
                "抽屉必须激活现有帧循环，不能等待 idle timer"
            );
            let width = f32::from(window.viewport_size().width);
            let height = f32::from(window.viewport_size().height);
            assert!(
                app.command_panel_rect(width, height).is_none(),
                "展开首帧尚未有可见面板"
            );
            let report = app.advance_animation_frame(Duration::from_millis(110), cx);
            assert!(report.motion_changed, "抽屉变化必须通知界面重绘");
            let opening = app
                .command_panel_rect(width, height)
                .expect("展开中应存在面板");
            let bottom = height - COMMAND_DOCK_BOTTOM_PX - COMMAND_DOCK_HEIGHT_PX;
            assert_eq!(
                opening.origin.y + opening.size.y,
                bottom,
                "展开必须固定底边"
            );
            assert!(
                opening.size.y > 0.0 && opening.size.y < 102.0,
                "展开中不能直接显示最终高度"
            );
            app.advance_animation_frame(Duration::from_millis(110), cx);
            let opened = app
                .command_panel_rect(width, height)
                .expect("展开完成应存在面板");
            assert_eq!(opened.size.y, 102.0, "单条建议最终高度应准确");
            assert!(opened.origin.y < opening.origin.y, "抽屉顶部应向上延伸");
            app.set_command_query(String::new(), false, window, cx);
            app.sync_command_drawer(window);
            let state = app.command_dock.as_ref().unwrap();
            assert_eq!(
                state.drawer.draft, "/terminal",
                "退出不能闪成空查询的默认建议列表"
            );
            assert!(!state.visible, "退出中不能继续执行候选");
            app.advance_animation_frame(Duration::from_millis(80), cx);
            let closing = app
                .command_panel_rect(width, height)
                .expect("退出中应保留面板");
            assert_eq!(
                closing.origin.y + closing.size.y,
                bottom,
                "收回必须固定同一底边"
            );
            assert!(closing.origin.y > opened.origin.y, "收回应朝下退回 Dock");
            app.advance_animation_frame(Duration::from_millis(80), cx);
            assert!(
                app.command_panel_rect(width, height).is_none(),
                "退出结束必须释放浏览器遮挡区域"
            );
            assert!(
                !app.animation_activity().canvas_frame_synced_active,
                "抽屉结束后不能单独维持高频帧调度"
            );
        })
    });
}

/// 场景：聚焦空指令框不展开建议；只有非空白内容才展开，清空后立即停止接收候选操作。
///
/// 数据构造：临时空 workspace 与 echo PTY；空字符串、3 个空格、/terminal 依次作为输入。
/// 执行过程：Cmd-K 聚焦 → Down/Enter → 输入空格 → 输入 /terminal → 全选删除 → Down/Enter。
/// 预期结果：空值与空白不显示建议、不创建卡片；有效内容显示建议；清空后仍不创建卡片。
#[test]
fn command_dock_suggestions_require_nonblank_input() {
    let workspace = tempfile::tempdir().expect("临时 workspace 应可创建");
    let mut test_cx = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test_cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| {
                    Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
                }),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("App entity 应创建");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let baseline = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    cx.update(|window, cx| app.update(cx, |app, cx| app.focus_command_input(window, cx)));
    cx.simulate_keystrokes("cmd-k");
    app.read_with(cx, |app, _| {
        assert!(
            !app.command_dock.as_ref().unwrap().visible,
            "仅聚焦空输入不能展开建议"
        );
    });
    cx.simulate_keystrokes("down enter");
    app.read_with(cx, |app, _| {
        let state = app.command_dock.as_ref().unwrap();
        assert!(!state.visible, "空输入的方向键不能打开建议");
        assert!(state.draft.is_empty(), "空输入回车不能补全指令");
    });
    cx.simulate_input("   ");
    app.read_with(cx, |app, _| {
        assert!(
            !app.command_dock.as_ref().unwrap().visible,
            "仅空格不能展开建议"
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("/terminal");
    app.read_with(cx, |app, _| {
        assert!(
            app.command_dock.as_ref().unwrap().visible,
            "输入内容应立即启用建议，不等待动画结束"
        );
    });
    cx.simulate_keystrokes("cmd-a backspace down enter");
    app.read_with(cx, |app, _| {
        let state = app.command_dock.as_ref().unwrap();
        assert!(!state.visible, "删除所有内容后必须收起建议");
        assert!(state.draft.is_empty(), "清空后不能被旧候选重新填入");
        assert_eq!(
            app.runtime.document.nodes().len(),
            baseline,
            "空输入不能执行旧候选或创建卡片"
        );
    });
}

/// 场景：未结束的预览选择不能锁住其他指令，也不能在返回后覆盖新草稿。
///
/// 数据构造：空 workspace、echo PTY；oneshot 保留原生文件窗口尚未返回的状态。
///   Codex 1 张 + 迟到的 Markdown 预览 1 张 + 终端 1 张 = baseline + 3 张。
/// 执行过程：预览等待 → 再次预览得到提示 → 实际键路由输入 Codex 并回车 → 输入终端指令 →
///   预览迟到完成 → 终端回车；记录每步草稿和卡片数。
/// 预期结果：重复预览被提示拦截；Codex 只创建一次；迟到结果保留终端草稿且终端仍能执行。
#[test]
fn command_dock_pending_preview_does_not_block_new_commands() {
    let workspace = tempfile::tempdir().expect("临时 workspace 应可创建");
    let preview_path = workspace.path().join("hello.md");
    std::fs::write(&preview_path, "# Preview").expect("预览测试文件应可写入");
    let (sender, receiver) = futures::channel::oneshot::channel();
    let mut test_cx = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test_cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| {
                    Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
                }),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("App entity 应创建");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let baseline = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.focus_command_input(window, cx);
            app.set_command_query("/preview".into(), false, window, cx);
            app.command_dock.as_mut().unwrap().pending_preview = Some("/preview".into());
            app.handle_preview_picker_receiver(receiver, true, window, cx);
        });
    });
    cx.simulate_keystrokes("cmd-k enter");
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.command_dock.as_ref().unwrap().error.as_deref(),
            Some("Finish or cancel the open file picker first"),
            "重复预览必须解释等待原因"
        );
        assert_eq!(
            app.runtime.document.nodes().len(),
            baseline,
            "重复预览不得创建卡片或打开第二个文件选择窗口"
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("/new codex");
    cx.simulate_keystrokes("enter");
    app.read_with(cx, |app, _| {
        assert_eq!(app.runtime.document.nodes().len(), baseline + 1,
            "等待预览期间回车必须创建 Codex，不能被全局 pending 拦截");
        assert!(app.runtime.document.nodes().iter().any(|node| matches!(
            &node.kind, canvas::CanvasNodeKind::Agent(spec) if spec.profile == canvas::AgentProfile::Codex
        )), "必须创建 Codex Agent，不能以普通终端代替");
        assert!(app.command_dock.as_ref().unwrap().draft.is_empty(),
            "Codex 创建成功应清空草稿");
        assert!(!app.command_dock.as_ref().unwrap().visible,
            "执行成功不能留下可重复触发的候选");
    });
    cx.simulate_keystrokes("cmd-k");
    cx.simulate_input("/terminal");
    sender
        .send(Ok(Some(vec![preview_path])))
        .expect("等待中的预览回调应仍存在");
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let state = app.command_dock.as_ref().unwrap();
        assert_eq!(state.draft, "/terminal", "预览迟到结果不得清掉新草稿");
        assert!(state.pending_preview.is_none(), "预览结束必须释放等待状态");
    });
    cx.simulate_keystrokes("enter");
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.nodes().len(),
            baseline + 3,
            "预览返回后新终端指令仍应能执行且不能重复创建"
        );
    });
}

/// 场景：Dock 输入和 IME 组合不会创建卡片，确认后仅创建一次并交还键盘焦点。
///
/// 数据构造：临时空 workspace；echo PTY 不启动外部进程；指令 /terminal 共 9 个 ASCII 字符。
/// 执行过程：输入指令 → Esc 保留草稿 → 重新聚焦 → 标记完整指令为 IME 组合 → Enter；
///   结束组合后 Enter，再模拟组件 PressEnter 回调；最后模拟预览选择取消，再经实际键绑定发送 Down/Tab 补全 Claude。
/// 预期结果：组合期间零新增；结束组合只新增 1 张终端且聚焦；成功清空；取消预览保留草稿且无错误。
#[test]
fn command_dock_input_ime_execution_and_cancel() {
    let workspace = tempfile::tempdir().expect("临时 workspace 应可创建");
    let mut test_cx = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test_cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| {
                    Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
                }),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("App entity 应创建");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let baseline = app.read_with(cx, |app, _| app.runtime.document.nodes().len());
    let key = |name: &str| KeyDownEvent {
        keystroke: Keystroke::parse(name).expect("测试键应可解析"),
        is_held: false,
    };
    let input = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.focus_command_input(window, cx);
            app.command_dock.as_ref().unwrap().input.clone()
        })
    });
    cx.update(|window, cx| input.update(cx, |input, cx| input.set_value("/terminal", window, cx)));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.nodes().len(),
            baseline,
            "输入只过滤，不能创建终端"
        );
        assert_eq!(
            app.command_dock.as_ref().unwrap().draft,
            "/terminal",
            "输入变更应同步草稿"
        );
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.capture_command_dock_key(&key("escape"), window, cx);
            let state = app.command_dock.as_ref().unwrap();
            assert!(!state.visible, "Esc 应收起列表");
            assert_eq!(state.draft, "/terminal", "Esc 不能删除草稿");
            app.focus_command_input(window, cx);
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(
                    Some(0..9),
                    "/terminal",
                    Some(9..9),
                    window,
                    cx,
                )
            });
            app.capture_command_dock_key(&key("enter"), window, cx);
            app.execute_dock_suggestion(window, cx);
            assert_eq!(
                app.runtime.document.nodes().len(),
                baseline,
                "IME 组合 Enter 不能创建卡片"
            );
            input.update(cx, |input, cx| input.unmark_text(window, cx));
            app.capture_command_dock_key(&key("enter"), window, cx);
            app.execute_dock_suggestion(window, cx);
            assert_eq!(
                app.runtime.document.nodes().len(),
                baseline + 1,
                "一次确认只能创建一张终端"
            );
            assert!(
                matches!(app.runtime.focus.mode(), runtime::InputMode::NodeFocused(_)),
                "新终端应获得运行时焦点"
            );
            assert!(
                !app.command_input_focused(window, cx),
                "键盘焦点应离开指令框"
            );
            assert!(
                app.command_dock.as_ref().unwrap().draft.is_empty(),
                "成功应清空草稿"
            );
            app.focus_command_input(window, cx);
            app.set_command_query("/preview".into(), true, window, cx);
            app.workspace_state.error = Some("旧的无关错误".into());
            app.command_dock.as_mut().unwrap().pending_preview = Some("/preview".into());
            app.finish_command_preview(false, None, window, cx);
            let state = app.command_dock.as_ref().unwrap();
            assert_eq!(state.draft, "/preview", "取消文件选择应保留草稿");
            assert!(state.pending_preview.is_none(), "取消后应允许重试");
            assert!(state.error.is_none(), "取消不能显示旧 workspace 错误");
        })
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.set_command_query("/new".into(), true, window, cx);
        })
    });
    cx.simulate_keystrokes("down tab");
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.command_dock.as_ref().unwrap().draft,
            "/new claude",
            "实际 Tab action 必须补全当前候选"
        );
        assert_eq!(
            app.runtime.document.nodes().len(),
            baseline + 1,
            "Tab 补全不得执行创建"
        );
    });
}

/// 场景：快捷指令面板从底部 Dock 顶边直接向上展开，与 Dock 融合为同一块表面。
///
/// 数据构造：/terminal 对应 1 条建议；展开 220ms，推进 220ms 后完全展开（高度 102px）。
/// 执行过程：聚焦输入 → 输入 /terminal → 推进到完全展开 → 绘制 → 读取 Dock 与面板边界。
///
/// 预期结果：
///   - 面板左右边界与 Dock 外边界相同：融合后不能比 Dock 窄或偏移。
///   - 面板底边等于 Dock 顶边：两者之间不能留出缝隙。
///   - 面板位于 Dock 之上，且高度大于 0。
#[test]
fn command_panel_grows_flush_from_the_dock_top() {
    use std::time::Duration;

    let workspace = tempfile::tempdir().expect("临时 workspace 应可创建");
    let mut test_cx = TestAppContext::single();
    let mut app = None;
    let (_, cx) = test_cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| {
                    Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
                }),
            )
        });
        app = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let app = app.expect("App entity 应创建");
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.motion = crate::motion::MotionStore::new(false);
            app.focus_command_input(window, cx);
            app.set_command_query("/terminal".into(), true, window, cx);
            app.sync_command_drawer(window);
            app.advance_animation_frame(Duration::from_millis(220), cx);
        })
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let dock = cx
        .debug_bounds("workspace-command-dock")
        .expect("应显示底栏");
    let panel = cx
        .debug_bounds("command-dock-suggestions")
        .expect("展开后应显示快捷指令面板");
    assert_eq!(panel.left(), dock.left(), "面板左边界必须与 Dock 对齐");
    assert_eq!(panel.right(), dock.right(), "面板右边界必须与 Dock 对齐");
    assert_eq!(panel.bottom(), dock.top(), "面板底边必须紧贴 Dock 顶边");
    assert!(panel.size.height > gpui::px(0.0), "面板应有可见高度");
}
