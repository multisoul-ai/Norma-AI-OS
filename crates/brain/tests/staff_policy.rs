use brain::staff::policy::{
    authorize_staff, decide_assign, risk_categories, AssignContext, AssignDecision, ProposeReason,
    RiskCategory, Verdict,
};
use norma::protocol::{
    AgentProfile, BrainDecision, BrainOp, QuestionAnswer, QuestionRef, QuestionRequestId,
    ReportKind, ReportPriority, StaffOp, TaskPhase,
};

/// 场景：任务文本命中四类高风险之一，中英文都要识别；普通开发任务不命中。
/// 数据构造：每类各取一句英文和一句中文，外加两句普通任务。
/// 预期结果：命中的类别与期望一致，普通任务返回空；一句话命中多类时都返回。
#[test]
fn risky_work_is_classified_by_category() {
    let cases = [
        (
            "git push origin main when green",
            vec![RiskCategory::Release],
        ),
        ("把这个版本部署到线上", vec![RiskCategory::Release]),
        (
            "rm -rf the build cache and drop table users",
            vec![RiskCategory::Destructive],
        ),
        ("清空旧的数据库", vec![RiskCategory::Destructive]),
        (
            "rotate the API key and print the password",
            vec![RiskCategory::Sensitive],
        ),
        ("读取 ~/.ssh 里的密钥", vec![RiskCategory::Sensitive]),
        (
            "send an email to the customer",
            vec![RiskCategory::External],
        ),
        ("给客户发邮件并付款", vec![RiskCategory::External]),
        ("补一个登录页的单元测试", vec![]),
        ("Rename the helper and update its callers", vec![]),
    ];
    for (text, expected) in cases {
        assert_eq!(risk_categories(text), expected, "text: {text}");
    }
    assert_eq!(
        risk_categories("deploy it and delete the old branch"),
        vec![RiskCategory::Release, RiskCategory::Destructive],
        "multiple categories are all reported, in a stable order"
    );
}

fn context(auto: bool, last: Option<&str>, count: u32) -> AssignContext<'_> {
    AssignContext {
        auto,
        last_finished_agent: last,
        auto_dispatches: count,
    }
}

/// 场景：幕僚想自己派活，四个条件缺一就降级为提议，并说明第一个不满足的原因。
/// 数据构造：基准是自动开启、上一个完成的是 Simon、已自动派发 2 次、内容无风险。
/// 预期结果：基准直接派发；逐项破坏条件各得到对应原因；多条同时不满足时先报设置，再报风险、归属、次数。
#[test]
fn assign_is_dispatched_only_when_every_condition_holds() {
    let plain = "Add the missing test";
    let base = context(true, Some("Simon"), 2);
    assert_eq!(
        decide_assign(&base, "Simon", plain),
        AssignDecision::Dispatch
    );
    assert_eq!(
        decide_assign(&context(false, Some("Simon"), 2), "Simon", plain),
        AssignDecision::Propose(ProposeReason::AutonomyAsk),
        "settings left on ask"
    );
    assert_eq!(
        decide_assign(&base, "Simon", "git push the fix"),
        AssignDecision::Propose(ProposeReason::Risk(vec![RiskCategory::Release]))
    );
    assert_eq!(
        decide_assign(&base, "Kai", plain),
        AssignDecision::Propose(ProposeReason::NotFollowUp),
        "a different agent is a new direction"
    );
    assert_eq!(
        decide_assign(&context(true, None, 0), "Simon", plain),
        AssignDecision::Propose(ProposeReason::NotFollowUp),
        "the first dispatch in a task always needs the user"
    );
    assert_eq!(
        decide_assign(&context(true, Some("Simon"), 3), "Simon", plain),
        AssignDecision::Propose(ProposeReason::LimitReached),
        "the fourth automatic dispatch is held back"
    );
    assert_eq!(
        decide_assign(&context(false, Some("Kai"), 3), "Simon", "deploy it"),
        AssignDecision::Propose(ProposeReason::AutonomyAsk)
    );
}

fn task(op: StaffOp) -> BrainOp {
    BrainOp::Task { op }
}

fn target() -> QuestionRef {
    QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: 7,
        provider: AgentProfile::Cursor,
        run_id: "run".into(),
        connection_id: "c".into(),
        session_id: "s".into(),
        turn_id: None,
        request_id: QuestionRequestId::String("q".into()),
        revision: 1,
    }
}

fn denied_code(verdict: Verdict) -> String {
    match verdict {
        Verdict::Denied { code } => code.to_owned(),
        Verdict::Allow => panic!("expected a denial"),
    }
}

/// 场景：宿主按幕僚身份检查命令；白名单里的放行，其余给出可翻译的拒绝码。
/// 数据构造：逐个构造 BrainOp；归档分别在任务未完成和已完成时检查。
/// 预期结果：只读、提议、回复、阶段、派活、report、history、答题建议放行；
/// decide（任何结论）、answer、旧 assign、任务 create/list/message/adopt、取消归档、未完成时归档被拒绝。
#[test]
fn staff_may_only_run_whitelisted_commands() {
    let allowed = [
        BrainOp::Agents,
        BrainOp::Approvals,
        BrainOp::Questions,
        BrainOp::History {
            agent: "Simon".into(),
            last: Some(3),
            grep: None,
        },
        BrainOp::Propose {
            agent: "Simon".into(),
            title: "t".into(),
            task: "x".into(),
            why: "w".into(),
        },
        BrainOp::Report {
            text: "hi".into(),
            priority: ReportPriority::Normal,
            kind: ReportKind::Status,
            reply_to: None,
            sources: vec![],
        },
        BrainOp::ProposeAnswer {
            target: target(),
            answers: vec![],
            reason: "r".into(),
        },
        task(StaffOp::Reply {
            task_id: "a".into(),
            text: "ok".into(),
        }),
        task(StaffOp::Phase {
            task_id: "a".into(),
            phase: TaskPhase::Merging,
        }),
        task(StaffOp::Propose {
            task_id: "a".into(),
            agent: "S".into(),
            title: "t".into(),
            prompt: "p".into(),
            why: "w".into(),
        }),
        task(StaffOp::Assign {
            task_id: "a".into(),
            agent: "S".into(),
            title: "t".into(),
            prompt: "p".into(),
            why: "w".into(),
        }),
    ];
    for op in &allowed {
        assert_eq!(authorize_staff(op, false), Verdict::Allow, "{op:?}");
    }
    for decision in [
        BrainDecision::Allow,
        BrainDecision::Deny,
        BrainDecision::Escalate,
    ] {
        let op = BrainOp::Decide {
            approval_id: "1".into(),
            decision,
            reason: "r".into(),
        };
        assert_eq!(
            denied_code(authorize_staff(&op, false)),
            "staff.denied.decide"
        );
    }
    let answer = BrainOp::Answer {
        target: target(),
        answers: vec![QuestionAnswer {
            key: "k".into(),
            selected: vec![],
            text: None,
        }],
    };
    assert_eq!(
        denied_code(authorize_staff(&answer, false)),
        "staff.denied.answer"
    );
    let legacy = BrainOp::Assign {
        agent: "Simon".into(),
        task: "x".into(),
    };
    assert_eq!(
        denied_code(authorize_staff(&legacy, false)),
        "staff.denied.legacy_assign"
    );
    for (op, code) in [
        (
            StaffOp::Create { title: "t".into() },
            "staff.denied.task_scope",
        ),
        (StaffOp::List, "staff.denied.task_scope"),
        (
            StaffOp::Message {
                task_id: "a".into(),
                text: "x".into(),
            },
            "staff.denied.user_message",
        ),
        (
            StaffOp::Adopt {
                task_id: "a".into(),
                proposal_id: 1,
            },
            "staff.denied.adopt",
        ),
        (
            StaffOp::Archive {
                task_id: "a".into(),
                archived: false,
            },
            "staff.denied.unarchive",
        ),
    ] {
        assert_eq!(denied_code(authorize_staff(&task(op), true)), code);
    }
    let archive = task(StaffOp::Archive {
        task_id: "a".into(),
        archived: true,
    });
    assert_eq!(
        denied_code(authorize_staff(&archive, false)),
        "staff.denied.archive_not_done"
    );
    assert_eq!(
        authorize_staff(&archive, true),
        Verdict::Allow,
        "archive after done"
    );
}

/// 场景：降级原因要以稳定的代码回给模型，模型据此决定怎么向用户解释。
/// 数据构造：四种原因，其中风险带两个类别。
/// 预期结果：代码分别为 autonomy_ask、not_follow_up、limit_reached、risk:release,destructive。
#[test]
fn downgrade_reasons_have_stable_codes() {
    assert_eq!(ProposeReason::AutonomyAsk.code(), "autonomy_ask");
    assert_eq!(ProposeReason::NotFollowUp.code(), "not_follow_up");
    assert_eq!(ProposeReason::LimitReached.code(), "limit_reached");
    assert_eq!(
        ProposeReason::Risk(vec![RiskCategory::Release, RiskCategory::Destructive]).code(),
        "risk:release,destructive"
    );
}
