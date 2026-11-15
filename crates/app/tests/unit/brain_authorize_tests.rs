use super::*;
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
        // 当前这一轮：任务里有一条正在运行的幕僚回复。
        task.begin_reply(norma::protocol::ReplyTrigger::User);
        app.staff.tasks.insert("a".into(), task);
    });
    (entity, cx)
}

fn reply(task: &str) -> BrainOp {
    BrainOp::Task {
        op: StaffOp::Phase {
            task_id: task.into(),
            phase: norma::protocol::TaskPhase::Merging,
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
        assert_eq!(
            app.authorize_brain("staff-token", &reply("a")),
            Ok(()),
            "check: app.authorize_brain('staff-token', &reply('a'))"
        );
        assert_eq!(
            app.authorize_brain("staff-token", &reply("b")),
            Err("staff.denied.other_task".into()),
            "check: app.authorize_brain('staff-token', &reply('b'))"
        );
        let decide = BrainOp::Decide {
            approval_id: "1".into(),
            decision: BrainDecision::Allow,
            reason: "ok".into(),
        };
        assert_eq!(
            app.authorize_brain("staff-token", &decide),
            Err("staff.denied.decide".into()),
            "check: app.authorize_brain('staff-token', &decide)"
        );
        assert_eq!(
            app.authorize_brain("main-token", &decide),
            Ok(()),
            "check: app.authorize_brain('main-token', &decide)"
        );
        assert_eq!(
            app.authorize_brain("worker-guess", &reply("a")),
            Err("staff.invalid_source".into()),
            "check: app.authorize_brain('worker-guess', &reply('a'))"
        );
    });
}

/// 场景：幕僚 history 在拿不到本任务那张卡时一律拒绝；主动 report 每任务 10 分钟最多 2 条。
/// 数据构造：任务 a 派过 Simon，但 Agent 名册里没有打开的卡片；幕僚读 Simon 与 Kai；连续三次 report。
/// 预期结果：两者都被拒绝（history_scope，失败即拒绝）；第三条 report 被拒绝（report_rate）。
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
            Err("staff.denied.history_scope".into()),
            "no open card for this task: fail closed"
        );
        assert_eq!(
            app.authorize_brain("staff-token", &history("Kai")),
            Err("staff.denied.history_scope".into()),
            "check: app.authorize_brain('staff-token', &history('Kai')"
        );
        assert_eq!(
            app.authorize_brain("staff-token", &report()),
            Ok(()),
            "check: app.authorize_brain('staff-token', &report())"
        );
        assert_eq!(
            app.authorize_brain("staff-token", &report()),
            Ok(()),
            "check: app.authorize_brain('staff-token', &report())"
        );
        assert_eq!(
            app.authorize_brain("staff-token", &report()),
            Err("staff.denied.report_rate".into()),
            "check: app.authorize_brain('staff-token', &report())"
        );
        assert_eq!(
            app.authorize_brain("main-token", &report()),
            Ok(()),
            "main brain is not limited"
        );
    });
}

/// 场景：同时有两个任务在进行一轮，宿主无法确定调用者属于哪个，一律拒绝，不能随便挑一个。
/// 数据构造：在基础夹具上再让任务 b 也开始一轮。
/// 预期结果：对 a 和 b 的幕僚命令都被拒绝（other_task）；主大脑不受影响。
#[test]
fn ambiguous_active_tasks_fail_closed() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().unwrap();
    let (entity, cx) = app_with_active_task(&mut test, dir.path());
    entity.update(cx, |app, _| {
        let mut other = Task::new("b", "ws", "p", "Other");
        other.begin_reply(norma::protocol::ReplyTrigger::User);
        app.staff.tasks.insert("b".into(), other);
        for task in ["a", "b"] {
            assert_eq!(
                app.authorize_brain("staff-token", &reply(task)),
                Err("staff.denied.other_task".into()),
                "task {task}"
            );
        }
        assert_eq!(
            app.authorize_brain("main-token", &reply("a")),
            Ok(()),
            "check: app.authorize_brain('main-token', &reply('a'))"
        );
    });
}

/// 场景：幕僚自己把任务设成 done 不能紧接着归档，要等用户确认完成。
/// 数据构造：任务 a 的阶段为 Done 但不是用户手动设置；再改成用户手动设置。
/// 预期结果：前者归档被拒绝（archive_not_done），后者放行。
#[test]
fn staff_cannot_archive_a_task_it_marked_done_itself() {
    let mut test = TestAppContext::single();
    let dir = tempfile::tempdir().unwrap();
    let (entity, cx) = app_with_active_task(&mut test, dir.path());
    entity.update(cx, |app, _| {
        let archive = BrainOp::Task {
            op: StaffOp::Archive {
                task_id: "a".into(),
                archived: true,
            },
        };
        let task = app.staff.tasks.get_mut("a").unwrap();
        task.set_phase(norma::protocol::TaskPhase::Done, false);
        assert_eq!(
            app.authorize_brain("staff-token", &archive),
            Err("staff.denied.archive_not_done".into()),
            "model-set done is not the user's confirmation"
        );
        app.staff
            .tasks
            .get_mut("a")
            .unwrap()
            .set_phase(norma::protocol::TaskPhase::Done, true);
        assert_eq!(
            app.authorize_brain("staff-token", &archive),
            Ok(()),
            "check: app.authorize_brain('staff-token', &archive)"
        );
    });
}
