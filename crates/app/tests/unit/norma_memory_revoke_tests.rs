use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 场景：关闭记忆召回必须撤销 Live 原生会话中已交付的资料；关闭保存不影响通话。
/// 构造：隔离数据库、生产记忆设置订阅和已启动的 Live 状态，不使用麦克风或云端。
/// 执行：经生产 toggle 依次关闭保存与召回，等待设置写入和事件投递。
/// 预期：保存开关不取消通话；召回关闭后通话及子任务取消，持久偏好确认为关闭。
#[test]
fn memory_recall_revocation_stops_live_but_save_toggle_keeps_call() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let (view, call, child) = entity.update_in(cx, |app, window, cx| {
        app.ensure_norma_memory(window, cx);
        let (_, call) = app.gpt_live.begin();
        let child = call.child_token();
        (app.norma_memory.clone().unwrap(), call, child)
    });
    // Same-database detached project + embedded session, and an unrelated database.
    let other_dir = tempfile::tempdir().unwrap();
    let other_paths = storage::DatabasePaths::isolated_test(other_dir.path());
    let peers = [paths.clone(), paths.clone(), other_paths.clone()].map(|paths| {
        cx.new(|cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        })
    });
    cx.update(|_, cx| {
        crate::workspace::sessions::register(
            paths.app_db_path().to_owned(),
            "same-database".into(),
            peers[0].clone(),
            cx,
        );
        crate::workspace::sessions::register(
            other_paths.app_db_path().to_owned(),
            "other-database".into(),
            peers[2].clone(),
            cx,
        );
    });
    entity.update(cx, |app, _| {
        app.desktop_workspace
            .sessions
            .insert(("workspace".into(), "embedded".into()), peers[1].clone());
    });
    let peer_calls = peers
        .each_ref()
        .map(|peer| peer.update(cx, |app, _| app.gpt_live.begin().1));
    let settle = |cx: &mut gpui::VisualTestContext| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            if !view.read_with(cx, |view, _| view.busy) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "记忆设置应在五秒内完成"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    settle(cx);
    view.update(cx, |view, cx| view.toggle(false, false, cx));
    settle(cx);
    assert!(!call.is_cancelled(), "仅关闭保存不能打断通话");
    assert!(
        peer_calls.iter().all(|call| !call.is_cancelled()),
        "保存开关不能取消其他会话"
    );
    assert!(
        entity.read_with(cx, |app, _| app.gpt_live.is_active()),
        "通话应仍有效"
    );

    view.update(cx, |view, cx| view.toggle(true, false, cx));
    settle(cx);
    let preferences = storage::MemoryAccess::new(paths, None)
        .preferences()
        .unwrap();
    assert!(
        !preferences.recall && !preferences.save,
        "两个开关应已真实保存"
    );
    assert!(
        call.is_cancelled(),
        "关闭召回必须撤销包含旧记忆的 Live 通话"
    );
    assert!(child.is_cancelled(), "通话的后台子任务也必须收到取消");
    assert!(peer_calls[0].is_cancelled(), "同库后台项目也必须撤销旧会话");
    assert!(peer_calls[1].is_cancelled(), "同库嵌入项目也必须撤销旧会话");
    assert!(
        !peer_calls[2].is_cancelled(),
        "不同数据库的通话不能被误取消"
    );
    assert!(
        !entity.read_with(cx, |app, _| app.gpt_live.is_active()),
        "不能继续沿用旧原生会话"
    );
}

/// 场景：召回关闭写入尚在后台排队时，来源设置视图被释放。
/// 构造：隔离数据库、已订阅的设置视图、同库注册的后台 Live 项目；调度器暂不推进。
/// 执行：调用 toggle 后释放视图，确认弱引用失效，再推进写库任务及回调。
/// 预期：偏好仍成功落库，后台项目也收到撤销；广播不能依赖来源视图存活。
#[test]
fn memory_recall_revocation_survives_settings_view_release() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let view = source.update_in(cx, |app, window, cx| {
        app.ensure_norma_memory(window, cx);
        app.norma_memory.clone().unwrap()
    });
    cx.run_until_parked();
    assert!(
        !view.read_with(cx, |view, _| view.busy),
        "初始加载应完成，toggle 不会被忽略"
    );
    let peer = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    cx.update(|_, cx| {
        crate::workspace::sessions::register(
            paths.app_db_path().to_owned(),
            "peer".into(),
            peer.clone(),
            cx,
        )
    });
    let call = peer.update(cx, |app, _| app.gpt_live.begin().1);
    let weak = view.downgrade();
    view.update(cx, |view, cx| view.toggle(true, false, cx));
    source.update(cx, |app, _| app.norma_memory = None);
    drop(view);
    assert!(
        weak.upgrade().is_none(),
        "来源视图必须在后台回填之前确实释放"
    );
    cx.run_until_parked();
    assert!(
        !storage::MemoryAccess::new(paths, None)
            .preferences()
            .unwrap()
            .recall,
        "视图释放不能撤销已提交的设置写入"
    );
    assert!(
        call.is_cancelled(),
        "写入成功后的整库撤销不能随视图释放而丢失"
    );
}
