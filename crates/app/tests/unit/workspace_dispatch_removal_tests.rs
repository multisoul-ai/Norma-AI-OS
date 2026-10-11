use super::*;
use crate::workspace::{
    dispatch::*,
    dispatch_test_support::{prepare_betty, with_workspace},
};
use agent::WorkspaceTaskPlan;
use std::{
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};

/// 场景：确认删区域/空间后，后台 catalog 删除尚未回调，旧任务不能继续 Enter。
/// 构造：Betty 已写正文；用真实桌面删除入口，保留 session 引用观察 100ms 延时窗口。
/// 执行：确认删除后立即尝试注册同 scope 新 lease、推进 Enter，再运行删除后台任务。
/// 预期：scope 立即关闭，新旧输入均撤权；仅原正文、待核对；删除成功后 scope 永久关闭。
#[test]
fn desktop_removal_closes_scope_before_catalog_await_and_never_reopens_deleted_scope() {
    for remove_workspace in [false, true] {
        with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
            let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
            let pending = prepare_betty(&host, cx, &service, &workspace.id, "remove-operation");
            let card = pending.cards()[0].clone();
            let plan = WorkspaceTaskPlan::from_response(
                r#"{"assignments":[{"target_index":0,"text":"删除前正文"}]}"#,
                &pending.cards(),
            )
            .expect("完整计划");
            let capture = Arc::new(Mutex::new(None));
            let output = capture.clone();
            let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
            cx.spawn(async move |_| {
                *output.lock().expect("结果") = Some(task.await);
            })
            .detach();
            cx.run_until_parked();
            capture
                .lock()
                .expect("结果")
                .take()
                .expect("提交")
                .expect("接收");
            let now = Instant::now();
            sessions[1]
                .1
                .update(cx, |app, _| app.advance_card_instance_tasks_at(now));
            host.update(cx, |app, cx| {
                app.desktop_workspace.pending_delete = Some(if remove_workspace {
                    DesktopDeleteTarget::Workspace {
                        id: workspace.id.clone(),
                        name: "空间".into(),
                    }
                } else {
                    DesktopDeleteTarget::Area {
                        workspace_id: workspace.id.clone(),
                        area_id: card.area_id.clone(),
                        name: "区域".into(),
                    }
                });
                app.confirm_desktop_delete(cx);
            });
            let late = authority::register(paths.app_db_path(), &card);
            assert!(
                !late.load(Ordering::Acquire),
                "删除 await 窗口不能注册有效新权限"
            );
            sessions[1].1.update(cx, |app, _| {
                app.advance_card_instance_tasks_at(now + Duration::from_millis(101))
            });
            assert_eq!(
                *writes[1].lock().expect("B"),
                vec!["删除前正文".as_bytes().to_vec()],
                "确认删除后不得发送 Enter"
            );
            assert!(writes[0].lock().expect("A").is_empty(), "非目标零输入");
            cx.run_until_parked();
            assert!(
                !authority::register(paths.app_db_path(), &card).load(Ordering::Acquire),
                "已删除 scope 永不自动开启"
            );
            assert!(!late.load(Ordering::Acquire), "等待窗口中的旧权限永不复活");
            let areas = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned())
                .list_projects(&workspace.id)
                .expect("目录读取");
            assert!(
                !areas.iter().any(|area| area.id == card.area_id),
                "真实 catalog 删除已经完成"
            );
        });
    }
}

/// 场景：区域删除事务明确失败，原角色仍在，但旧输入权限必须保持关闭。
/// 构造：真实 catalog 上用 SQLite trigger 拒绝删除，保存确认删除前的原卡 lease。
/// 执行：走桌面确认删除入口，验证等待窗口关闭，再等待事务失败与错误反馈。
/// 预期：catalog/角色未删除，新尝试可重新授权；旧 lease 永不复活且所有 PTY 零输入。
#[test]
fn failed_desktop_removal_allows_new_attempts_without_reviving_old_input() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let card = sessions[1].1.read_with(cx, |app, _| {
            app.workspace_card_ref(app.runtime.document.nodes()[0].id)
                .expect("原卡")
        });
        let old = authority::register(paths.app_db_path(), &card);
        let blocker = rusqlite::Connection::open(paths.app_db_path()).expect("故障连接");
        blocker.execute_batch("CREATE TRIGGER reject_area_removal BEFORE DELETE ON desktop_project_areas BEGIN SELECT RAISE(ABORT, 'fixture deletion failure'); END;").expect("真实事务失败 fixture");
        host.update(cx, |app, cx| {
            app.desktop_workspace.pending_delete = Some(DesktopDeleteTarget::Area {
                workspace_id: workspace.id.clone(),
                area_id: card.area_id.clone(),
                name: "区域".into(),
            });
            app.confirm_desktop_delete(cx);
        });
        assert!(!old.load(Ordering::Acquire), "确认删除立即关闭旧输入");
        let during = authority::register(paths.app_db_path(), &card);
        assert!(!during.load(Ordering::Acquire), "删除等待中不发新权限");
        cx.run_until_parked();
        host.read_with(cx, |app, _| {
            assert!(
                app.desktop_workspace.error.is_some(),
                "真实删除失败必须反馈"
            );
            assert!(!app.desktop_workspace.busy, "删除失败结束等待");
        });
        assert!(
            authority::register(paths.app_db_path(), &card).load(Ordering::Acquire),
            "明确失败后允许新尝试"
        );
        assert!(!old.load(Ordering::Acquire), "事务失败不复活旧输入");
        assert!(!during.load(Ordering::Acquire), "等待窗口内权限不复活");
        let areas = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned())
            .list_projects(&workspace.id)
            .expect("真实目录");
        assert!(
            areas.iter().any(|area| area.id == card.area_id),
            "失败后原区域仍在"
        );
        assert!(
            writes
                .iter()
                .all(|record| record.lock().expect("PTY").is_empty()),
            "失败过程零输入"
        );
    });
}
