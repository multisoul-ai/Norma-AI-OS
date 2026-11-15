use super::*;
use brain::event::TaskUpdate;
use gpui::TestAppContext;
use norma::protocol::{BrainDecision, ReportKind, ReportPriority, StaffOp, Task, Worker};

fn app_with_active_task<'a>(
    test: &'a mut TestAppContext,
    dir: &std::path::Path,
) -> (gpui::Entity<MultisoulApp>, &'a mut gpui::VisualTestContext) {
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, _| Err(terminal::PtyError::Io(std::io::Error::other("unused")))),
        )
    });
    entity.update(cx, |app, cx| {
        app.database_paths = storage::DatabasePaths::isolated_test(dir);
        app.enter_canvas_for_test(dir, cx);
        let (_, status) = std::sync::mpsc::channel();
        app.install_brain_host_for_test(super::super::BrainHost {
            service: None,
            status,
            tokens: brain::staff::gate::Tokens::new("main-token", "staff-token"),
        });
        let mut task = Task::new("a", "ws", "p", "Ship");
        task.workers
            .push(Worker::new("Simon", "Codex", "Build", 10, "d-1"));
        app.staff.tasks.insert("a".into(), task);
        app.staff
            .activity
            .entry("a".into())
            .or_default()
            .apply(1, TaskUpdate::Started { message_count: 0 });
    });
    (entity, cx)
}

fn reply(task: &str) -> BrainOp {
    BrainOp::Task {
        op: StaffOp::Reply {
            task_id: task.into(),
            text: "ok".into(),
        },
    }
}

fn report() -> BrainOp {
    BrainOp::Report {
        text: "hi".into(),
        priority: ReportPriority::Normal,
        kind: ReportKind::Status,
        reply_to: None,
        sources: vec![],
    }
}

/// 场景：宿主入口按 token 角色授权，幕僚被限制在白名单和当前这一轮的任务里。
/// 数据构造：主大脑 token 与幕僚 token；任务 a 正在进行一轮，任务 b 没有；陌生 token。
/// 预期结果：幕僚回复 a 放行、回复 b 与 decide 被拒绝并带拒绝码；主大脑 decide 放行；陌生 token 无效。
#[test]
fn host_authorizes_by_token_role_and_active_task() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().unwrap();
    let (entity, cx) = app_with_active_task(&mut test, dir.path());
    entity.update(cx, |app, _| {
        assert_eq!(app.authorize_brain("staff-token", &reply("a")), Ok(()));
        assert_eq!(
            app.authorize_brain("staff-token", &reply("b")),
            Err("staff.denied.other_task".into())
        );
        let decide = BrainOp::Decide {
            approval_id: "1".into(),
            decision: BrainDecision::Allow,
            reason: "ok".into(),
        };
        assert_eq!(
            app.authorize_brain("staff-token", &decide),
            Err("staff.denied.decide".into())
        );
        assert_eq!(app.authorize_brain("main-token", &decide), Ok(()));
        assert_eq!(
            app.authorize_brain("worker-guess", &reply("a")),
            Err("staff.invalid_source".into())
        );
    });
}

/// 场景：幕僚 history 只能读本任务派出过的 Agent；主动 report 每任务 10 分钟最多 2 条。
/// 数据构造：任务 a 只派过 Simon；幕僚读 Simon 与 Kai 的对话；连续三次 report。
/// 预期结果：Simon 放行、Kai 被拒绝（history_scope）；第三条 report 被拒绝（report_rate）。
#[test]
fn staff_history_is_task_scoped_and_reports_are_rate_limited() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().unwrap();
    let (entity, cx) = app_with_active_task(&mut test, dir.path());
    entity.update(cx, |app, _| {
        let history = |agent: &str| BrainOp::History {
            agent: agent.into(),
            last: None,
            grep: None,
        };
        assert_eq!(
            app.authorize_brain("staff-token", &history("Simon")),
            Ok(())
        );
        assert_eq!(
            app.authorize_brain("staff-token", &history("Kai")),
            Err("staff.denied.history_scope".into())
        );
        assert_eq!(app.authorize_brain("staff-token", &report()), Ok(()));
        assert_eq!(app.authorize_brain("staff-token", &report()), Ok(()));
        assert_eq!(
            app.authorize_brain("staff-token", &report()),
            Err("staff.denied.report_rate".into())
        );
        assert_eq!(
            app.authorize_brain("main-token", &report()),
            Ok(()),
            "main brain is not limited"
        );
    });
}
