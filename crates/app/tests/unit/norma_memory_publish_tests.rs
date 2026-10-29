use super::*;
use crate::app::MultisoulApp;
use gpui::TestAppContext;
use std::time::{Duration, Instant};

/// 场景：设置页纠正长期记忆后，含旧资料的原生会话必须停止，写入失败则保留通话。
/// 构造：真实隔离记忆库、设置页、同库与异库 Live 状态；旧主题发布版本为一。
/// 执行：先通过 save 提交无效主题，再纠正已有主题，推进后台写入与广播。
/// 预期：失败不取消；成功后同库媒体和子任务取消，异库保留，新正文实际落库。
#[test]
fn memory_settings_publication_retires_old_context_only_after_possible_commit() {
    exercise_publication(false, false);
}

/// 场景：记忆保存提交后设置页释放，旧原生上下文仍须失效。
/// 构造：与正常保存相同的隔离库与宿主，保存后移除设置页唯一强引用。
/// 执行：确认弱引用已失效，再等待真实发布与数据库级撤销广播。
/// 预期：新正文保留，同库通话取消；广播不依赖设置页存活，不影响异库。
#[test]
fn memory_settings_publication_survives_view_release() {
    exercise_publication(true, false);
}

/// 场景：正文发布成功但回执写入失败，旧会话不能因结果为 Unknown 而继续使用旧资料。
/// 构造：真实 SQLite 触发器仅阻断 memory_operations 回执 UPDATE，Markdown 发布仍执行。
/// 执行：分别保留和释放设置页，提交纠正，读回新版本并观察原生通话取消。
/// 预期：已发布新正文、同库通话取消；页面存活时仍显示未知结果并保留待核验提交。
#[test]
fn memory_settings_publication_without_receipt_still_retires_old_context() {
    for release_view in [false, true] {
        exercise_publication(release_view, true);
    }
}

fn exercise_publication(release_view: bool, fail_receipt: bool) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut access = storage::MemoryAccess::new(paths.clone(), None);
    let saved = access
        .settings_save(
            MemoryScopeAlias::Personal,
            MemoryRevision::INITIAL,
            vec![("preference".into(), "OLD-FACT".into())],
            "原始记忆",
            "seed",
        )
        .unwrap();
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
    settle(&view, cx);
    let other_dir = tempfile::tempdir().unwrap();
    let other_paths = storage::DatabasePaths::isolated_test(other_dir.path());
    let peers = [paths.clone(), other_paths.clone()].map(|paths| {
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
            "peer".into(),
            peers[0].clone(),
            cx,
        );
        crate::workspace::sessions::register(
            other_paths.app_db_path().to_owned(),
            "other".into(),
            peers[1].clone(),
            cx,
        );
        if release_view {
            crate::workspace::sessions::register(
                paths.app_db_path().to_owned(),
                "source".into(),
                source.clone(),
                cx,
            );
        }
    });
    let call = source.update(cx, |app, _| app.gpt_live.begin().1);
    let child = call.child_token();
    let peer_calls = peers
        .each_ref()
        .map(|peer| peer.update(cx, |app, _| app.gpt_live.begin().1));
    view.update_in(cx, |view, window, cx| {
        view.imports = vec![("../invalid".into(), "不能发布".into())];
        view.save(window, cx);
    });
    settle(&view, cx);
    assert!(
        view.read_with(cx, |view, _| view.error),
        "无效主题应经真实 save 返回失败"
    );
    assert!(
        !call.is_cancelled() && !peer_calls[0].is_cancelled(),
        "未提交的失败不能停止同库通话"
    );
    let weak = view.downgrade();
    if fail_receipt {
        rusqlite::Connection::open(paths.app_db_path()).unwrap().execute_batch(
            "CREATE TRIGGER fail_memory_receipt BEFORE UPDATE OF receipt ON memory_operations BEGIN SELECT RAISE(ABORT, 'injected_receipt_failure'); END;"
        ).unwrap();
    }
    view.update_in(cx, |view, window, cx| {
        view.imports = vec![("preference".into(), "NEW-FACT".into())];
        view.save(window, cx);
    });
    if release_view {
        source.update(cx, |app, _| app.norma_memory = None);
        drop(view);
        assert!(weak.upgrade().is_none(), "保存完成前设置页应确实释放");
    } else {
        settle(&view, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.error),
            fail_receipt,
            "回执写入失败不能显示已确认保存，正常保存不能显示错误"
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.pending_save.is_some()),
            fail_receipt,
            "未知结果保留待核验提交，已确认结果清理提交"
        );
    }
    // 五秒是后台保存与撤销的测试上限，不等待网络或真实音频。
    let deadline = Instant::now() + Duration::from_secs(5);
    while !peer_calls[0].is_cancelled() && Instant::now() < deadline {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    let snapshot = access
        .settings_snapshot(MemoryScopeAlias::Personal)
        .unwrap();
    assert_ne!(
        snapshot.revision(),
        saved.revision,
        "取消检查前必须已真实发布新版本"
    );
    let current = storage::MemoryStore::open(&paths)
        .unwrap()
        .read(
            &snapshot,
            &agent::MemoryEntryId::parse("preference").unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        current.markdown, "NEW-FACT",
        "真实发布正文应为用户纠正后的内容"
    );
    assert!(
        call.is_cancelled() && child.is_cancelled(),
        "来源通话和后台子任务必须撤销旧上下文"
    );
    assert!(peer_calls[0].is_cancelled(), "同库其他通话必须撤销旧上下文");
    assert!(!peer_calls[1].is_cancelled(), "异库通话不能被取消");
    assert!(
        !source.read_with(cx, |app, _| app.gpt_live.is_active()),
        "不能自动重启麦克风"
    );
}

fn settle(view: &Entity<MemoryView>, cx: &mut gpui::VisualTestContext) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if !view.read_with(cx, |view, _| view.busy) {
            return;
        }
        assert!(Instant::now() < deadline, "设置页操作应在五秒内完成");
        std::thread::sleep(Duration::from_millis(1));
    }
}
