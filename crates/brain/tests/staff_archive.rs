use brain::staff::TaskStore;
use norma::protocol::{StaffOp, Task, TaskPhase};

/// 场景：归档写入任务快照，新消息取消归档，旧文件缺字段时仍能读。
///
/// 数据构造：工作空间 ws 的任务 keep，阶段 Done；快照先按当前格式保存，再删掉 archived 字段。
/// 执行过程：Archive(true) → 新 Store 读取 → Message("继续") → 再读；另用缺字段文件加载。
/// 预期结果：归档后阶段仍是 Done 且重载为 true；消息后 archived 为 false 且阶段回到 Active；
/// 缺字段文件读回为未归档，不把任务丢掉。
#[test]
fn archive_survives_reload_until_a_new_message() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    let mut task = Task::new("keep", "ws", "project", "保留下来的任务");
    task.set_phase(TaskPhase::Done, true);
    store.create(task).expect("create");
    store
        .execute(
            "ws",
            StaffOp::Archive {
                task_id: "keep".into(),
                archived: true,
            },
        )
        .expect("archive");
    let archived = TaskStore::new(dir.path().to_owned())
        .get("ws", "keep")
        .expect("reload archived");
    assert!(archived.archived, "归档必须写入 tasks.json");
    assert_eq!(archived.phase, TaskPhase::Done, "归档不能改阶段");
    let path = store.path("ws");
    let saved = std::fs::read(&path).expect("snapshot");
    let mut legacy_value: serde_json::Value = serde_json::from_slice(&saved).expect("json");
    legacy_value["tasks"][0]
        .as_object_mut()
        .expect("task")
        .remove("archived");
    std::fs::write(&path, serde_json::to_vec(&legacy_value).expect("legacy"))
        .expect("write legacy");
    let legacy = TaskStore::new(dir.path().to_owned())
        .get("ws", "keep")
        .expect("legacy load");
    assert!(!legacy.archived, "旧快照缺 archived 时必须当作未归档");
    assert_eq!(legacy.id, "keep", "缺字段不能让任务消失");
    assert_eq!(legacy.phase, TaskPhase::Done, "缺字段不能改阶段");
    std::fs::write(&path, saved).expect("restore archived snapshot");
    store
        .execute(
            "ws",
            StaffOp::Message {
                task_id: "keep".into(),
                text: "继续".into(),
            },
        )
        .expect("message");
    let reopened = store.get("ws", "keep").expect("after message");
    assert!(!reopened.archived, "新消息必须取消归档并落盘");
    assert_eq!(
        reopened.phase,
        TaskPhase::Active,
        "已完成任务的新消息仍要重新打开"
    );
    let durable = TaskStore::new(dir.path().to_owned())
        .get("ws", "keep")
        .expect("reload after message");
    assert!(!durable.archived, "取消归档必须写回磁盘");
}
