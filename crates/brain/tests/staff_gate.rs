use brain::staff::gate::{authorize, staff_env, BrainRole, GateContext, Tokens};
use brain::staff::policy::Verdict;
use norma::protocol::{BrainOp, StaffOp};

fn tokens() -> Tokens {
    Tokens::new("main-token", "staff-token")
}

fn reply(task_id: &str) -> BrainOp {
    BrainOp::Task {
        op: StaffOp::Reply {
            task_id: task_id.into(),
            text: "ok".into(),
        },
    }
}

fn context<'a>(active: Option<&'a str>, done: bool) -> GateContext<'a> {
    GateContext {
        active_task: active,
        task_done: done,
    }
}

/// 场景：宿主靠 token 认出调用者是主大脑还是幕僚。
/// 数据构造：主大脑 token、幕僚 token、陌生 token、空 token。
/// 预期结果：前两个各得其角色，陌生和空 token 没有角色。
#[test]
fn tokens_identify_the_caller_role() {
    let tokens = tokens();
    assert_eq!(tokens.role("main-token"), Some(BrainRole::Main));
    assert_eq!(tokens.role("staff-token"), Some(BrainRole::Staff));
    assert_eq!(tokens.role("worker-guess"), None);
    assert_eq!(tokens.role(""), None);
}

/// 场景：幕僚只能操作自己当前这一轮的任务；主大脑不受白名单约束。
/// 数据构造：当前活跃任务 a；幕僚对 a 和 b 回复；没有活跃任务时回复；主大脑 decide。
/// 预期结果：a 放行；b 与无活跃任务都被拒绝（other_task，失败即拒绝）；主大脑的 decide 放行。
#[test]
fn staff_is_bound_to_its_active_task_and_main_is_unrestricted() {
    assert_eq!(
        authorize(BrainRole::Staff, &reply("a"), &context(Some("a"), false)),
        Verdict::Allow
    );
    for (op, active) in [(reply("b"), Some("a")), (reply("a"), None)] {
        assert_eq!(
            authorize(BrainRole::Staff, &op, &context(active, false)),
            Verdict::Denied {
                code: "staff.denied.other_task"
            },
            "{op:?} with active {active:?}"
        );
    }
    let decide = BrainOp::Decide {
        approval_id: "1".into(),
        decision: norma::protocol::BrainDecision::Allow,
        reason: "ok".into(),
    };
    assert_eq!(
        authorize(BrainRole::Main, &decide, &context(None, false)),
        Verdict::Allow
    );
    assert_eq!(
        authorize(BrainRole::Staff, &decide, &context(Some("a"), false)),
        Verdict::Denied {
            code: "staff.denied.decide"
        }
    );
}

/// 场景：归档只在任务已完成后允许，且任务要是当前活跃任务。
/// 数据构造：幕僚归档 a，done 与未 done 各一次。
/// 预期结果：未完成被拒绝（archive_not_done），完成后放行。
#[test]
fn staff_archive_requires_a_done_active_task() {
    let archive = BrainOp::Task {
        op: StaffOp::Archive {
            task_id: "a".into(),
            archived: true,
        },
    };
    assert_eq!(
        authorize(BrainRole::Staff, &archive, &context(Some("a"), false)),
        Verdict::Denied {
            code: "staff.denied.archive_not_done"
        }
    );
    assert_eq!(
        authorize(BrainRole::Staff, &archive, &context(Some("a"), true)),
        Verdict::Allow
    );
}

/// 场景：幕僚会话进程拿到的是幕僚 token，而不是主大脑的 token。
/// 数据构造：基础环境含主大脑 token 和另外两个变量。
/// 预期结果：其他变量原样保留，NORMA_BRAIN_TOKEN 只出现一次且是幕僚 token。
#[test]
fn staff_sessions_get_the_staff_token_only() {
    let base = vec![
        ("PATH".to_owned(), "/bin".to_owned()),
        ("NORMA_BRAIN_TOKEN".to_owned(), "main-token".to_owned()),
        ("NORMA_SOCKET".to_owned(), "/tmp/s".to_owned()),
    ];
    let env = staff_env(&base, "staff-token");
    let tokens: Vec<_> = env
        .iter()
        .filter(|(key, _)| key == "NORMA_BRAIN_TOKEN")
        .collect();
    assert_eq!(
        tokens,
        [&("NORMA_BRAIN_TOKEN".to_owned(), "staff-token".to_owned())]
    );
    assert!(env.contains(&("PATH".to_owned(), "/bin".to_owned())));
    assert!(env.contains(&("NORMA_SOCKET".to_owned(), "/tmp/s".to_owned())));
}
