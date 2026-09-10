use agent::{PlanTodoChoice, ProjectPlanDraft};
use serde_json::json;
use storage::{
    AppStore, MobileCommandStore, NewMobileCommand, PlanTodoReservation, ProjectPlanStore,
};

fn fixture() -> (
    tempfile::TempDir,
    MobileCommandStore,
    NewMobileCommand,
    ProjectPlanDraft,
) {
    let directory = tempfile::tempdir().expect("隔离项目");
    let app = AppStore::new(directory.path().join("app.db"));
    let workspace = app
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Todo")
        .expect("项目")
        .workspace_id;
    let target = json!({"node_id":7,"profile":"Codex","title":"测试","mobile_control":true,"state":"ready","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null});
    let draft: ProjectPlanDraft = serde_json::from_value(json!({"summary":"下一步","todos":[
        {"target":target,"title":"登录测试","instruction":"检查登录边界。"},
        {"target":target,"title":"补文档","instruction":"写清登录行为。"}]}))
    .expect("建议");
    ProjectPlanStore::new(app.path().into())
        .publish("plan-a", &workspace, "route-a", "先稳定登录", &draft)
        .expect("保存计划");
    let command = NewMobileCommand {
        device_id: "phone-a".into(),
        operation_id: "accept-a".into(),
        workspace_id: workspace,
        route_id: "route-a".into(),
        owner_id: "boot-a".into(),
        request_hash: "a".repeat(64),
    };
    (
        directory,
        MobileCommandStore::new(app.path().into()),
        command,
        draft,
    )
}

/// 两部手机竞争同一Todo，只能产生一条关联执行命令，重复/重开不能再次领取。
///
/// 数据构造：plan-a第0项指向node7/run-a；phone-a/op-a和phone-b/op-b并发，实际指令固定来自计划。
/// 执行过程：并发确认 → 查看唯一决定/命令 → 同请求回查 → 另一决定拒绝 → 换项目读取。
/// 预期结果：一个New、一个AlreadyDecided，败方无命令；原建议和准确原文保留，跨项目不可见。
#[test]
fn todo_claim_and_command_commit_once_across_devices() {
    let (_directory, store, first, draft) = fixture();
    let mut second = first.clone();
    second.device_id = "phone-b".into();
    second.operation_id = "accept-b".into();
    let choice = PlanTodoChoice::Accept {
        target: draft.todos[0].target.clone(),
        queue: false,
    };
    let a = (store.clone(), first.clone(), choice.clone());
    let b = (store.clone(), second.clone(), choice.clone());
    let left = std::thread::spawn(move || {
        a.0.reserve_plan_todo(&a.1, "plan-a", 0, &a.2, 2)
            .expect("A确认")
    });
    let right = std::thread::spawn(move || {
        b.0.reserve_plan_todo(&b.1, "plan-a", 0, &b.2, 2)
            .expect("B确认")
    });
    let results = [left.join().expect("A"), right.join().expect("B")];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, PlanTodoReservation::New(_)))
            .count(),
        1,
        "只允许一条真实执行来源"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, PlanTodoReservation::AlreadyDecided))
            .count(),
        1,
        "另一部手机必须看见已决定"
    );
    let decisions = store
        .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-a")
        .expect("决定");
    assert_eq!(decisions.len(), 1, "决定不因两次点击重复");
    let winner = if matches!(results[0], PlanTodoReservation::New(_)) {
        assert!(
            store
                .get("phone-a", "accept-a", &first.workspace_id, "boot-a")
                .expect("A回执")
                .is_some(),
            "A胜出必须有命令"
        );
        assert!(
            store
                .get("phone-b", "accept-b", &first.workspace_id, "boot-a")
                .expect("B回执")
                .is_none(),
            "B败方不能留下可执行命令"
        );
        first.clone()
    } else {
        assert!(
            store
                .get("phone-b", "accept-b", &first.workspace_id, "boot-a")
                .expect("B回执")
                .is_some(),
            "B胜出必须有命令"
        );
        assert!(
            store
                .get("phone-a", "accept-a", &first.workspace_id, "boot-a")
                .expect("A回执")
                .is_none(),
            "A败方不能留下可执行命令"
        );
        second
    };
    assert_eq!(decisions[0].todo_index, 0, "准确第0项被确认");
    assert!(
        matches!(
            store
                .reserve_plan_todo(&winner, "plan-a", 0, &choice, 2)
                .expect("原请求重试"),
            PlanTodoReservation::Existing(_)
        ),
        "重试只回查原记录"
    );
    let mut changed = winner.clone();
    changed.request_hash = "b".repeat(64);
    assert!(
        matches!(
            store
                .reserve_plan_todo(&changed, "plan-a", 0, &PlanTodoChoice::Reject, 2)
                .expect("改决定"),
            PlanTodoReservation::Conflict
        ),
        "同操作ID不能改为拒绝"
    );
    assert!(
        store
            .plan_todo_decisions("another-project", "plan-a", "boot-a")
            .expect("隔离查询")
            .is_empty(),
        "另一项目不得得到决定来源"
    );
    let restarted = store
        .plan_todo_decisions(&first.workspace_id, "plan-a", "boot-b")
        .expect("重开");
    assert_eq!(
        restarted[0].receipt.state, "outcome_unknown",
        "重开不能认领未确认执行"
    );
}

/// 拒绝与排队和原始Todo原子关联；调用方不能篡改原指令或指向另一节点。
///
/// 数据构造：第0项拒绝，第1项排队；queue要求已有run/session，revision=2对应明确控制授权。
/// 执行过程：拒绝落盘 → 错目标排队失败 → 正确目标排队 → 查询队列 → 完成真实发送回执。
/// 预期结果：拒绝无队列，错误目标无命令；仅第1项含原始文案，sent回执绑定实际turn。
#[test]
fn todo_rejection_and_queue_preserve_original_instruction() {
    let (_directory, store, mut command, draft) = fixture();
    assert!(
        matches!(
            store
                .reserve_plan_todo(&command, "plan-a", 0, &PlanTodoChoice::Reject, 2)
                .expect("拒绝"),
            PlanTodoReservation::New(_)
        ),
        "首次拒绝应保存"
    );
    let rejected = store
        .plan_todo_decisions(&command.workspace_id, "plan-a", "boot-a")
        .expect("拒绝记录");
    assert_eq!(rejected[0].receipt.state, "succeeded", "决定保存成功");
    assert_eq!(
        rejected[0].receipt.result.as_ref().expect("决定身份")["status"],
        "declined",
        "不能冒充任务完成"
    );
    assert!(rejected[0].queued.is_none(), "拒绝不能创建队列");
    command.operation_id = "queue-a".into();
    let mut wrong = draft.todos[1].target.clone();
    wrong.node_id = 8;
    assert!(
        store
            .reserve_plan_todo(
                &command,
                "plan-a",
                1,
                &PlanTodoChoice::Accept {
                    target: wrong,
                    queue: true
                },
                2
            )
            .is_err(),
        "不能改派其他卡片"
    );
    assert!(
        store
            .get("phone-a", "queue-a", &command.workspace_id, "boot-a")
            .expect("失败回查")
            .is_none(),
        "错误目标不占用命令ID"
    );
    let choice = PlanTodoChoice::Accept {
        target: draft.todos[1].target.clone(),
        queue: true,
    };
    store
        .reserve_plan_todo(&command, "plan-a", 1, &choice, 2)
        .expect("排队");
    let decisions = store
        .plan_todo_decisions(&command.workspace_id, "plan-a", "boot-a")
        .expect("两个决定");
    assert_eq!(decisions.len(), 2, "应分别保留拒绝与接受");
    let queued = decisions[1].queued.as_ref().expect("真正的已有队列");
    assert_eq!(
        queued.message.text, "写清登录行为。",
        "只派发持久化建议原文"
    );
    assert_eq!(queued.message.node_id, 7, "准确分配给node7");
    assert_eq!(queued.state, "queued", "持久化不等于Agent已经收到");
    assert!(
        store.claim_queued_message(&command).expect("现有队列领取"),
        "复用已有派发领取"
    );
    store.finish_queued_message(&command,Ok(json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-real"}))).expect("实际发送回执");
    let sent = store
        .plan_todo_decisions(&command.workspace_id, "plan-a", "boot-a")
        .expect("已发送");
    assert_eq!(
        sent[1].queued.as_ref().expect("队列").state,
        "sent",
        "跟随已有派发真实状态"
    );
    assert_eq!(
        sent[1]
            .queued
            .as_ref()
            .expect("队列")
            .result
            .as_ref()
            .expect("真实回合")["turn_id"],
        "turn-real",
        "必须能回到实际任务"
    );
}

#[path = "project_todo_decisions/retry.rs"]
mod retry;
