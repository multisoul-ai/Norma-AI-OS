use agent::{AdapterCommand, AdapterResult};
use storage::{LiveOperationRequest, LiveOperationReservation, LiveOperationStore};

/// AC07/AC10/AC11：任务终态事实跨断线保存，使用稳定task_ref避免重启后相同数字串号。
/// 构造：同项目boot-a:7和boot-b:7两份真实accepted回执，以及另一个项目同名任务。
/// 执行：落账→重建Store→按project查询watch→排队同一终态两次→按ACK确认其中一条。
/// 预期：三份任务独立；同一事实只有一条；ACK仅代表服务端接收，未ACK事实可重连补交。
#[test]
fn live_task_facts_persist_deduplicate_and_isolate_runtime_and_project() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = LiveOperationStore::new(path.clone());
    for (project, intent, task_ref) in [
        ("/a", "first", "boot-a:7"),
        ("/a", "second", "boot-b:7"),
        ("/b", "third", "boot-a:7"),
    ] {
        let command = AdapterCommand::AssignAgentTask {
            agent: "Simon".into(),
            task: "执行测试".into(),
        };
        let LiveOperationReservation::Claimed(lease) = store
            .reserve(&LiveOperationRequest {
                project_id: project,
                intent_id: intent,
                operation_id: "step-1",
                owner_id: "owner",
                command: &command,
            })
            .expect("预留")
        else {
            panic!("首次认领")
        };
        let result = AdapterResult::accepted("已接收")
            .with_task_id(7)
            .with_node_id(9)
            .with_tasks(vec![
                serde_json::json!({"task_id":7,"task_ref":task_ref,"node_id":9}),
            ]);
        store
            .record(&lease, &Ok(result))
            .expect("落账并追踪稳定身份");
    }
    let restarted = LiveOperationStore::new(path);
    let watches = restarted.task_watches("/a", 0).expect("重启后追踪");
    assert_eq!(watches.len(), 2, "重启后的相同数字task_id不能串号");
    for watch in &watches {
        restarted
            .queue_task_fact(watch, "succeeded", "任务已完成；是否完整播报未确认")
            .expect("真实终态排队");
        restarted
            .queue_task_fact(watch, "succeeded", "任务已完成；是否完整播报未确认")
            .expect("重复事实幂等");
    }
    let pending = restarted.pending_task_facts("/a").expect("待交事实");
    assert_eq!(
        pending.len(),
        2,
        "同一project/task_ref/terminal只能一条事实"
    );
    assert!(
        restarted
            .pending_task_facts("/b")
            .expect("其他项目")
            .is_empty(),
        "其他项目没有排队事实"
    );
    assert!(
        !restarted
            .acknowledge_task_fact("/b", &pending[0].event_id)
            .expect("错项目ACK"),
        "ACK不能跨项目"
    );
    assert_eq!(
        restarted
            .pending_task_facts("/a")
            .expect("事实仍保留")
            .len(),
        2,
        "跨项目ACK不能消耗事实"
    );
    assert!(
        restarted
            .acknowledge_task_fact("/a", &pending[0].event_id)
            .expect("同项目ACK"),
        "仅服务端接收确认后停止重交"
    );
    assert_eq!(
        restarted.pending_task_facts("/a").expect("剩余事实").len(),
        1,
        "未ACK事实跨重连保留"
    );
    assert_eq!(
        restarted.task_watches("/a", 0).expect("剩余任务").len(),
        1,
        "已ACK终态不反复扫描"
    );
}
