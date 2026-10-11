use super::{
    dispatch::*,
    dispatch_test_support::{prepare_request, with_workspace},
};
use agent::WorkspaceTaskPlan;
use std::{
    task::Poll,
    time::{Duration, Instant},
};

/// 场景：调用方在整组持久接收后不再等待回执，投递不能只执行前半组或永久挂起。
/// 构造：Alan/Betty 两卡完整计划；在 acceptance commit 后丢弃公开 submit Task。
/// 执行：继续运行 App executor 与两卡帧循环，公开读取持久输入阶段。
/// 预期：操作自身继续收口，两张原卡各提交一次；丢弃等待句柄不是取消已提交操作。
#[test]
fn shared_service_owns_delivery_after_caller_drops_receipt_waiter() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let body = "让 Alan 和 Betty 修复登录";
        let pending = prepare_request(&host, cx, &service, &workspace.id, "dropped-waiter", body);
        let plan = WorkspaceTaskPlan::from_response(r#"{"assignments":[{"target_index":0,"text":"Alan 独有正文"},{"target_index":1,"text":"Betty 独有正文"}]}"#, &pending.cards()).expect("整组计划");
        let source = norma::workspace_operation::WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: norma::workspace_operation::WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "dropped-waiter".into(),
            message_id: "message".into(),
            original_text: body.into(),
        };
        let store = storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned());
        let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
        let deadline = Instant::now() + Duration::from_secs(5);
        cx.executor().block_test(std::future::poll_fn(|cx| {
            if store.receipt(&source).expect("公开回执").is_some() {
                return Poll::Ready(());
            }
            assert!(Instant::now() < deadline, "整组应接收");
            cx.waker().wake_by_ref();
            Poll::Pending
        }));
        drop(task);
        cx.run_until_parked();
        let now = Instant::now();
        for (_, session) in &sessions {
            session.update(cx, |app, _| {
                app.advance_card_instance_tasks_at(now);
                app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
                app.journal_workspace_card_instance_events();
            });
        }
        for (record, text) in writes.iter().zip(["Alan 独有正文", "Betty 独有正文"]) {
            assert_eq!(
                *record.lock().expect("PTY"),
                vec![text.as_bytes().to_vec(), vec![13]],
                "丢弃回执等待不能丢弃整组投递"
            );
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let receipt = store.receipt(&source).expect("回执").expect("已接收操作");
            if receipt
                .assignments
                .iter()
                .all(|item| item.input_phase == storage::WorkspaceInputPhase::EnterConfirmed)
            {
                break;
            }
            assert!(Instant::now() < deadline, "不能留下未收口项: {receipt:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    });
}

/// 场景：冷区域加载期间停止桌面或主动退出，迟到加载不得重新注册 session。
/// 构造：移除已加载 Betty，让真实 ensure 后台读取原区域；保留 host 引用。
/// 执行：在 loading 标记出现而 session 尚未安装时停止，继续执行 ensure。
/// 预期：停止与 shutdown 均拒绝迟到安装，loading 收口、session 清空且所有卡片零输入。
#[test]
fn desktop_shutdown_rejects_late_cold_dispatch_session_installation() {
    for shutdown in [false, true] {
        with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
            let area = sessions[1].0.clone();
            let key = (workspace.id.clone(), area.id.clone());
            host.update(cx, |app, _| {
                app.desktop_workspace.sessions.remove(&key);
            });
            let task = host.update(cx, |_, cx| {
                super::desktop::mobile::ensure(paths.app_db_path(), area, cx)
            });
            let async_cx = cx.to_async();
            let deadline = Instant::now() + Duration::from_secs(5);
            cx.executor().block_test(std::future::poll_fn(|poll_cx| {
                let stopped = async_cx
                    .update(|cx| {
                        if !host
                            .read(cx)
                            .desktop_workspace
                            .loading_area_ids
                            .contains(&key)
                        {
                            return false;
                        }
                        assert!(
                            !host.read(cx).desktop_workspace.sessions.contains_key(&key),
                            "必须在加载安装前停止"
                        );
                        if shutdown {
                            super::desktop::sessions::shutdown(cx);
                        } else {
                            host.update(cx, |app, cx| app.stop_desktop_sessions(cx));
                        }
                        true
                    })
                    .expect("App 有效");
                if stopped {
                    return Poll::Ready(());
                }
                assert!(Instant::now() < deadline, "必须捕获真实冷加载窗口");
                poll_cx.waker().wake_by_ref();
                Poll::Pending
            }));
            let result = cx.executor().block_test(task);
            assert!(result.is_err(), "停止后迟到 ensure 必须拒绝安装");
            host.read_with(cx, |app, _| {
                assert!(
                    app.desktop_workspace.sessions.is_empty(),
                    "停止后不能复活 session"
                );
                assert!(
                    !app.desktop_workspace.loading_area_ids.contains(&key),
                    "迟到加载必须收口"
                );
            });
            assert!(
                writes
                    .iter()
                    .all(|record| record.lock().expect("PTY").is_empty()),
                "停止过程零卡片输入"
            );
        });
    }
}
