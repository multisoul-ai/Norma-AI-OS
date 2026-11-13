use brain::staff::TaskStore;
use norma::protocol::{
    QuestionItem, QuestionOption, StaffQuestion, StaffQuestionState, Task, TaskMessage,
};

fn question(id: &str) -> StaffQuestion {
    StaffQuestion::new(
        id.into(),
        vec![QuestionItem {
            key: "theme".into(),
            provider_id: None,
            prompt: "Theme?".into(),
            options: vec![QuestionOption {
                key: "dark".into(),
                label: "Dark".into(),
                description: None,
            }],
            multiple: false,
            allow_custom: true,
            secret: false,
        }],
        None,
    )
}

/// 场景：重启后不保留已失去投递所有者的待回答卡。
/// 数据构造：一个已回答卡和一个尚未到125秒期限的待答卡，快照由真实TaskStore写入。
/// 执行过程：保存 → 用新的Store加载 → 再次重载磁盘。
/// 预期结果：仅待答卡变为TimedOut并存盘，已回答卡保持；不会再次要求用户回答。
#[test]
fn restarting_expires_pending_questions_durably() {
    let root = tempfile::tempdir().expect("root");
    let store = TaskStore::new(root.path().into());
    let mut task = Task::new("t", "w", "p", "Ship");
    let mut answered = question("answered");
    answered.state = StaffQuestionState::Answered;
    task.messages.push(TaskMessage::Question {
        question: Box::new(answered),
    });
    task.messages.push(TaskMessage::Question {
        question: Box::new(question("pending")),
    });
    store.create(task).expect("save");
    let loaded = TaskStore::new(root.path().into())
        .get("w", "t")
        .expect("restart");
    assert_eq!(loaded.pending_question_count(), 0, "重启不应遗留待答卡");
    assert!(
        matches!(&loaded.messages[0], TaskMessage::Question { question } if question.state == StaffQuestionState::Answered),
        "已回答终态保持不变"
    );
    assert!(
        matches!(&loaded.messages[1], TaskMessage::Question { question } if question.state == StaffQuestionState::TimedOut),
        "失去所有者的题立即过期"
    );
    let disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.path("w")).expect("snapshot")).expect("JSON");
    assert_eq!(
        disk["tasks"][0]["messages"][1]["Question"]["question"]["state"], "TimedOut",
        "重启过期必须落盘"
    );
}

/// 场景：同一待答卡的重复点击/并发提交只有一次能成为用户消息，秘密值不落盘。
/// 数据构造：一个 secret 自定义问题；两个线程同时提交相同答案。
/// 执行过程：空答案先失败且不消费 → 两线程竞争提交 → 读取任务和原始磁盘。
/// 预期结果：恰好一个提交获胜、恰好一条用户消息，状态Answered；不能在JSON里找到秘密值。
#[test]
fn answer_claim_is_atomic_and_secret_values_stay_out_of_storage() {
    use norma::protocol::QuestionAnswer;
    use std::sync::Arc;
    let root = tempfile::tempdir().expect("root");
    let store = Arc::new(TaskStore::new(root.path().into()));
    store
        .create(Task::new("t", "w", "p", "Ship"))
        .expect("create");
    let mut q = question("q");
    q.items[0].secret = true;
    store.add_question("w", "t", q.clone()).expect("question");
    store
        .add_question("w", "t", q)
        .expect("duplicate notification");
    assert!(
        store
            .resolve_question("w", "t", "q", StaffQuestionState::Answered, &[])
            .is_err(),
        "不完整答案不得消费原题"
    );
    assert_eq!(
        store.get("w", "t").expect("task").pending_question_count(),
        1,
        "无效答案后仍可回答，重复通知没有第二张卡"
    );
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .resolve_question(
                        "w",
                        "t",
                        "q",
                        StaffQuestionState::Answered,
                        &[QuestionAnswer {
                            key: "theme".into(),
                            selected: vec![],
                            text: Some("sensitive-test-value".into()),
                        }],
                    )
                    .expect("answer")
                    .1
            })
        })
        .collect();
    let wins = threads
        .into_iter()
        .map(|handle| usize::from(handle.join().expect("join")))
        .sum::<usize>();
    assert_eq!(wins, 1, "首答者生效，不能重复写用户消息");
    let task = store.get("w", "t").expect("task");
    assert_eq!(
        task.messages
            .iter()
            .filter(|m| matches!(m, TaskMessage::User { .. }))
            .count(),
        1,
        "只有一条回答摘要"
    );
    assert_eq!(task.pending_question_count(), 0, "已回答不再阻塞");
    let disk = std::fs::read_to_string(store.path("w")).expect("JSON");
    assert!(
        !disk.contains("sensitive-test-value"),
        "秘密不得写进问答字段或用户摘要"
    );
    assert!(
        disk.contains("Answered") && disk.contains("••••"),
        "终态和脱敏摘要仍保留"
    );
}

/// 场景：忽略只影响点击的提议，恢复只能发生在没有更新提议时。
/// 数据构造：按顺序生成编号1、2的提议；全程无Worker派发。
/// 执行过程：忽略1 → 恢复1 → 再忽略1 → 创建2 → 尝试恢复1和批准1。
/// 预期结果：初次恢复成功；有2后拒绝恢复1，2不被忽略；忽略的提议不可直接批准。
#[test]
fn ignore_is_per_proposal_and_restore_rejects_superseded_proposals() {
    use norma::protocol::StaffOp;
    let root = tempfile::tempdir().expect("root");
    let store = TaskStore::new(root.path().into());
    store
        .create(Task::new("t", "w", "p", "Ship"))
        .expect("create");
    let propose = |prompt: &str| StaffOp::Propose {
        task_id: "t".into(),
        agent: "Simon".into(),
        title: "Ship".into(),
        prompt: prompt.into(),
        why: "Ready".into(),
    };
    store.execute("w", propose("first")).expect("first");
    store
        .set_proposal_ignored("w", "t", 1, true)
        .expect("ignore");
    let restored = store
        .set_proposal_ignored("w", "t", 1, false)
        .expect("restore latest");
    assert!(!restored.proposals[0].ignored, "最新提议可恢复");
    store
        .set_proposal_ignored("w", "t", 1, true)
        .expect("ignore again");
    store.execute("w", propose("second")).expect("second");
    assert!(
        store.set_proposal_ignored("w", "t", 1, false).is_err(),
        "已有更新提议时不得恢复旧提议"
    );
    assert!(
        store.approve("w", "t", 1).is_err(),
        "已忽略提议不能绕过恢复直接采用"
    );
    let task = store.get("w", "t").expect("task");
    assert!(task.proposals[0].ignored, "只忽略编号1");
    assert!(!task.proposals[1].ignored, "编号2保持待决定");
    assert!(task.workers.is_empty(), "忽略/恢复不能派发Worker");
}

/// 场景：跳过与超时都是终态，迟到的点击不能改写为已回答。
/// 数据构造：两张独立待答卡，分别以Skipped和TimedOut结算。
/// 执行过程：结算 → 送达合法但迟到的答案 → 读取最终对话。
/// 预期结果：原终态保留、迟到提交失败、没有伪造用户消息。
#[test]
fn skipped_and_timed_out_questions_reject_late_answers() {
    use norma::protocol::QuestionAnswer;
    let root = tempfile::tempdir().expect("root");
    let store = TaskStore::new(root.path().into());
    store
        .create(Task::new("t", "w", "p", "Ship"))
        .expect("create");
    for (id, state) in [
        ("skip", StaffQuestionState::Skipped),
        ("timeout", StaffQuestionState::TimedOut),
    ] {
        store
            .add_question("w", "t", question(id))
            .expect("question");
        assert!(
            store
                .resolve_question("w", "t", id, state, &[])
                .expect("settle")
                .1,
            "待答卡可进入终态"
        );
        let (_, accepted) = store
            .resolve_question(
                "w",
                "t",
                id,
                StaffQuestionState::Answered,
                &[QuestionAnswer {
                    key: "theme".into(),
                    selected: vec!["dark".into()],
                    text: None,
                }],
            )
            .expect("late answer");
        assert!(!accepted, "迟到答案不能覆盖{state:?}");
    }
    let task = store.get("w", "t").expect("task");
    assert_eq!(task.pending_question_count(), 0, "没有待答卡");
    assert!(
        !task
            .messages
            .iter()
            .any(|m| matches!(m, TaskMessage::User { .. })),
        "跳过和超时没有用户答案"
    );
    assert!(
        matches!(&task.messages[0], TaskMessage::Question{question} if question.state==StaffQuestionState::Skipped),
        "跳过终态不变"
    );
    assert!(
        matches!(&task.messages[1], TaskMessage::Question{question} if question.state==StaffQuestionState::TimedOut),
        "超时终态不变"
    );
}
