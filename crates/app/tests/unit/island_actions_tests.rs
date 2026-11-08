use super::*;
use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 场景：定位后台空间时工作窗口已关闭，必须重接原宿主而非新建Runtime。
/// 数据：一个宿主和一个后台项目Entity，先打开窗口、关窗并保留后台，再重开。
/// 执行：连续打开两次核对去重；移除旧窗口并执行关闭生命周期，再重开。
/// 预期：窗口号变化但原宿主/项目身份不变；已有窗口不会重复创建。
#[test]
fn island_reopen_reuses_retained_host_and_runtime() {
    let mut cx = TestAppContext::single();
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let project = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(&mut cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "area".into()), project.clone());
    });
    let first = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("首次开窗");
    let same = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("重复定位");
    assert!(first == same, "已有宿主窗口必须复用");
    cx.update(|cx| {
        first
            .update(cx, |_, window, _| window.remove_window())
            .expect("关闭窗口");
    });
    cx.update(|cx| crate::workspace::desktop::sessions::windows_closed(true, cx));
    let reopened = cx
        .update(|cx| crate::workspace::windowing::reopen_desktop_host(&host, cx))
        .expect("重接后台宿主");
    assert!(first != reopened, "旧窗口确实关闭后必须重开");
    host.read_with(&cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.sessions[&("B".into(), "area".into())].entity_id(),
            project.entity_id(),
            "重开不能创建新的项目Runtime"
        )
    });
}

/// 场景：两个宿主都有相同 area ID，后台 B 必须精确路由，不能按当前空间猜测。
/// 数据：宿主1的当前空间A、后台B与宿主2的B各有独立 Entity；目标携带来源和空间。
/// 执行：解析后台目标，再改变可见空间、删除原后台项目、释放来源。
/// 预期：返回始终是原 Entity；不会改 active_id，也不会回退到同名项目或另一宿主。
#[test]
fn island_action_routes_exact_background_owner_and_rejects_stale_target() {
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("创建隔离数据目录");
    let mut make = |name| {
        cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                storage::DatabasePaths::isolated_test(&temp.path().join(name)),
            )
        })
    };
    let first = make("first");
    let second = make("second");
    let a = make("a");
    let b = make("b");
    let other = make("other");
    first.update(&mut cx, |app, _| {
        app.desktop_workspace.active_id = Some("A".into());
        app.desktop_workspace
            .sessions
            .insert(("A".into(), "same".into()), a.clone());
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "same".into()), b.clone());
    });
    second.update(&mut cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("B".into(), "same".into()), other.clone());
    });
    cx.update(|cx| {
        register_source(&first, cx);
        register_source(&second, cx);
    });
    let target = IslandTarget {
        source: first.entity_id().as_u64(),
        workspace: "B".into(),
        area: "same".into(),
    };
    cx.update(|cx| {
        assert_eq!(
            target.session(cx).map(|s| s.entity_id()),
            Some(b.entity_id()),
            "必须选择后台 B 的原宿主"
        );
        assert_eq!(
            first.read(cx).desktop_workspace.active_id.as_deref(),
            Some("A"),
            "审批路由不能切换可见空间"
        );
    });
    first.update(&mut cx, |app, _| {
        app.desktop_workspace.active_id = Some("B".into());
        app.desktop_workspace
            .sessions
            .remove(&("B".into(), "same".into()));
    });
    cx.update(|cx| {
        assert!(
            target.session(cx).is_none(),
            "目标删除后不能转到另一个宿主的同名项目"
        )
    });
    cx.update(|cx| unregister_source(target.source, cx));
    cx.update(|cx| assert!(target.host(cx).is_none(), "释放来源后迟到操作必须失效"));
}

/// 场景：幕僚对话要借用已有身份的应用；工作空间身份在项目会话上，不在桌面宿主上。
/// 数据：一个已登记宿主，内含一个项目会话。
/// 预期：候选列表同时包含会话与宿主，且会话在宿主之前（先尝试带身份的会话）。
#[test]
fn staff_anchors_include_project_sessions_before_their_host() {
    let mut cx = TestAppContext::single();
    let new_app = |cx: &mut TestAppContext| {
        cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            )
        })
    };
    let host = new_app(&mut cx);
    let session = new_app(&mut cx);
    host.update(&mut cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("W".into(), "area".into()), session.clone());
    });
    cx.update(|cx| register_source(&host, cx));
    let anchors = cx.update(|cx| hosts(cx));
    let ids: Vec<_> = anchors.iter().map(|app| app.entity_id()).collect();
    assert_eq!(
        ids,
        [session.entity_id(), host.entity_id()],
        "会话应排在宿主之前"
    );
}

/// 场景：后台项目里的智能体完成后，点「查看成果」要定位到那张卡片。
/// 数据：宿主当前空间是 current；会话登记在 ws-b / area-9。完成事件从会话发出，节点 42 不在文档里。
/// 执行：会话处理 Thinking→Done，控制器记下路由，再执行查看成果。
/// 预期：来源是宿主实体而不是会话，空间是 ws-b 不是 current；卡片缺失时提示不存在，岛保持完成通知，不展开待办页。
#[test]
fn completed_session_agent_routes_view_result_to_its_card() {
    let mut cx = TestAppContext::single();
    let new_app = |cx: &mut TestAppContext| {
        cx.new(|cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            )
        })
    };
    let host = new_app(&mut cx);
    let session = new_app(&mut cx);
    let host_id = host.entity_id().as_u64();
    let session_id = session.entity_id().as_u64();
    session.update(&mut cx, |app, _| {
        app.desktop_host = Some(host.downgrade());
    });
    host.update(&mut cx, |app, _| {
        app.desktop_workspace.active_id = Some("current".into());
        app.desktop_workspace
            .sessions
            .insert(("ws-b".into(), "area-9".into()), session.clone());
    });
    host.update(&mut cx, |app, cx| {
        assert!(
            app.island_completion_route(42, cx).is_none(),
            "宿主概览上没有这张卡片，不能落到当前空间 current"
        );
    });
    let model = cx.update(crate::island::model);
    let controller = cx.new(|cx| super::super::controller::IslandController::new(model, cx));
    cx.update(|cx| register_source(&host, cx));
    session.update(&mut cx, |app, cx| {
        app.note_workspace_activity_from_frame(
            false,
            &[runtime::RuntimeEvent::AgentTaskStateChanged {
                node_id: 42,
                from: runtime::AgentTaskState::Thinking,
                to: runtime::AgentTaskState::Done,
                source_event: "fixture".into(),
            }],
            cx,
        );
    });
    controller.update(&mut cx, |state, cx| {
        let route = state
            .completion_route
            .clone()
            .expect("完成通知必须记下原卡片");
        assert_eq!(route.source, host_id, "定位来源必须是岛登记的宿主");
        assert_ne!(route.source, session_id, "后台会话实体对不上卡片来源");
        assert_eq!(route.workspace, "ws-b", "必须回到会话所在工作空间");
        assert_ne!(route.workspace, "current", "不能跳到宿主当前空间");
        assert_eq!(route.area, "area-9", "必须回到会话所在项目");
        assert_eq!(route.node, 42, "必须是刚完成的那张卡片");
        assert_eq!(
            state.displays.state.activity_id().map(str::to_owned),
            Some(format!("{host_id}:area-9:42")),
            "完成通知身份必须带上项目，避免同号节点串脸"
        );
        state.open_completed(cx);
        assert_eq!(
            state.message,
            Some(ui::i18n::LocalizedText::new("island.card_missing")),
            "卡片已不在时走定位失败"
        );
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Activity,
            "查看成果不能改去展开待办页"
        );
        assert_ne!(
            state.displays.state.tab(),
            runtime::island::IslandTab::Pending,
            "查看成果不能切到需要你处理"
        );
    });
}

/// 场景：经典画布没有桌面宿主，完成事件不能冒充一张可定位的岛卡片。
/// 数据：单独的画布实体，当前空间名叫 current，节点 42 从 Thinking 进入 Done。
/// 执行：处理完成事件，再读已订阅的岛控制器。
/// 预期：不记录路由，也不弹出完成通知；不能提示卡片不存在。
#[test]
fn canvas_without_desktop_host_does_not_publish_a_false_completion() {
    let mut cx = TestAppContext::single();
    let canvas = cx.new(|cx| {
        let mut app = MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        );
        app.desktop_workspace.active_id = Some("current".into());
        app
    });
    let model = cx.update(crate::island::model);
    let controller = cx.new(|cx| super::super::controller::IslandController::new(model, cx));
    canvas.update(&mut cx, |app, cx| {
        app.note_workspace_activity_from_frame(
            false,
            &[runtime::RuntimeEvent::AgentTaskStateChanged {
                node_id: 42,
                from: runtime::AgentTaskState::Thinking,
                to: runtime::AgentTaskState::Done,
                source_event: "fixture".into(),
            }],
            cx,
        );
    });
    controller.read_with(&cx, |state, _| {
        assert!(
            state.completion_route.is_none(),
            "没有宿主会话就不能写成可定位路由"
        );
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Compact,
            "不能弹出无法跳转的完成通知"
        );
        assert!(state.message.is_none(), "不能提示卡片不存在");
    });
}
