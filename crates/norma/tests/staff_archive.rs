//! 任务归档是列表投影，不改变阶段、Worker 或问题。
use norma::protocol::{
    staff_chip_counts, staff_visible_tasks, StaffProposal, Task, TaskDisplayStatus, TaskPhase,
    Worker,
};

fn named(id: &str, title: &str) -> Task {
    Task::new(id, "ws", "project", title)
}

fn running(id: &str) -> Task {
    let mut task = named(id, "执行中的接口");
    let mut worker = Worker::new("Alan", "Codex", "Build", 3, "d-run");
    worker.prompt_delivered = true;
    task.workers.push(worker);
    task
}

fn needs_you(id: &str) -> Task {
    let mut task = named(id, "需要你决定");
    task.proposals.push(StaffProposal {
        id: 1,
        agent: "Mia".into(),
        title: "下一步".into(),
        prompt: "请确认文案".into(),
        why: "还没派发".into(),
        approved: false,
        ignored: false,
        dispatch_id: None,
        model: None,
    });
    let mut worker = Worker::new("Mia", "Claude", "Review", 4, "d-need");
    worker.needs_input = true;
    task.workers.push(worker);
    task
}

fn review(id: &str) -> Task {
    let mut task = named(id, "待验收页面");
    task.set_phase(TaskPhase::Merging, true);
    task
}

/// 场景：任何展示状态都能归档；新消息会取消归档；旧快照缺字段时视为未归档。
///
/// 数据构造：等待、执行中、需要你、待验收、已完成各一条；已完成任务同时已归档。
/// 执行过程：分别 set_archived(true)，再对已完成任务 user_message("继续")；
/// 另把一条任务序列化后删掉 archived 字段再读回。
/// 预期结果：五条都能归档且阶段不变；只有需要你、执行中、待验收在提示里仍算进行；
/// 新消息后 archived 为 false 且已完成任务回到 Active；缺字段读回为 false。
#[test]
fn any_status_can_be_archived_and_a_new_message_clears_it() {
    let waiting = named("wait", "等待拆解");
    let mut samples = [
        waiting,
        running("run"),
        needs_you("need"),
        review("review"),
        {
            let mut done = named("done", "已完成文档");
            done.set_phase(TaskPhase::Done, true);
            done
        },
    ];
    for task in &samples {
        assert!(!task.archived, "{} 新建时不能已归档", task.id);
    }
    for task in &mut samples {
        let phase = task.phase;
        task.set_archived(true);
        assert!(task.archived, "{} 必须能归档", task.id);
        assert_eq!(task.phase, phase, "归档不能改阶段");
    }
    assert!(
        !samples[0].archive_note_active(),
        "等待中归档不注明仍在进行"
    );
    assert!(samples[1].archive_note_active(), "执行中归档要注明仍在进行");
    assert!(samples[2].archive_note_active(), "需要你归档要注明仍在进行");
    assert!(samples[3].archive_note_active(), "待验收归档要注明仍在进行");
    assert!(
        !samples[4].archive_note_active(),
        "已完成归档不注明仍在进行"
    );
    samples[4].user_message("继续");
    assert!(!samples[4].archived, "已归档任务里的新消息必须取消归档");
    assert_eq!(
        samples[4].phase,
        TaskPhase::Active,
        "已完成任务的新消息仍要重新打开"
    );
    assert!(
        samples[4]
            .messages
            .iter()
            .any(|message| matches!(message, norma::protocol::TaskMessage::User { text } if text == "继续")),
        "新消息正文必须留下"
    );

    let mut value = serde_json::to_value(named("old", "旧任务")).expect("serialize");
    value.as_object_mut().expect("object").remove("archived");
    let restored: Task = serde_json::from_value(value).expect("legacy snapshot");
    assert!(!restored.archived, "旧快照缺 archived 时必须是未归档");
}

/// 场景：全部 / 需要你 / 执行中不计入已归档；已归档入口按工作空间计数，列表才吃项目筛选。
///
/// 数据构造：本项目未归档 3 条（等待、需要你、执行中），本项目已归档执行中 1 条，
/// 其他项目已归档 1 条。查询词为 "mia"。
/// 执行过程：staff_chip_counts 与 staff_visible_tasks 分别按未归档视图和已归档视图计算。
/// 预期结果：全部 3、需要你 1、执行中 1、已归档 2；未归档列表不含已归档任务；
/// 已归档视图只含已归档且仍受项目筛选与搜索约束；状态筛选在已归档视图不再生效。
#[test]
fn archived_tasks_leave_status_counts_but_stay_in_the_archive_count() {
    let mut waiting = named("wait", "等待拆解");
    waiting.updated_at = 10;
    let mut need = needs_you("need");
    need.updated_at = 30;
    let mut active = running("run");
    active.updated_at = 20;
    let mut hidden = running("hidden");
    hidden.title = "仍在跑的归档".into();
    hidden.set_archived(true);
    hidden.updated_at = 40;
    let mut other = named("other", "别的项目");
    other.project_id = "other".into();
    other.set_archived(true);
    let tasks = [waiting, need, active, hidden, other];
    let mine = |task: &Task| task.project_id == "project";
    let counts = staff_chip_counts(&tasks, mine);
    assert_eq!(counts.all, 3, "全部只数未归档且属于当前项目的任务");
    assert_eq!(counts.needs_you, 1, "需要你不含已归档");
    assert_eq!(counts.running, 1, "执行中不含已归档的执行中任务");
    assert_eq!(counts.archived, 2, "已归档计数含其他项目，不跟项目筛选");

    let open = staff_visible_tasks(&tasks, false, Some(TaskDisplayStatus::Running), "", mine);
    assert_eq!(open.len(), 1, "执行中筛选只有一条未归档任务");
    assert_eq!(open[0].id, "run", "留下的是未归档执行中任务");

    let archived = staff_visible_tasks(&tasks, true, Some(TaskDisplayStatus::Done), "", mine);
    assert_eq!(archived.len(), 1, "已归档视图忽略状态筛选，只受项目筛选");
    assert_eq!(archived[0].id, "hidden", "其他项目的归档不进当前项目视图");

    let searched = staff_visible_tasks(&tasks, false, None, "mia", mine);
    assert_eq!(searched.len(), 1, "搜索命中 Worker 名称");
    assert_eq!(searched[0].id, "need", "Mia 的需要你任务应被搜到");
    assert!(
        staff_visible_tasks(&tasks, false, None, "mia", mine)
            .iter()
            .all(|task| !task.archived),
        "搜索也不能把已归档任务带回主列表"
    );
}
