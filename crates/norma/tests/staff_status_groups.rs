use norma::protocol::{
    staff_status_groups, staff_visible_tasks, Task, TaskDisplayStatus, TaskPhase,
};

fn task(id: &str, phase: TaskPhase, updated_at: u64) -> Task {
    let mut task = Task::new(id, "ws", "project", id);
    task.phase = phase;
    task.updated_at = updated_at;
    task
}

/// 场景：侧栏按状态给任务分组，组内保持按更新时间从新到旧。
/// 数据构造：两个已完成（更新时间 5、9）、一个合并中（7）、一个进行中无 Worker（8）。
/// 执行过程：先用 staff_visible_tasks 排序，再分组。
/// 预期结果：按「等待、合并、完成」的顺序出现三组；完成组里新的 d9 在前、d5 在后。
#[test]
fn consecutive_statuses_form_groups_in_list_order() {
    let tasks = vec![
        task("d5", TaskPhase::Done, 5),
        task("m7", TaskPhase::Merging, 7),
        task("a8", TaskPhase::Active, 8),
        task("d9", TaskPhase::Done, 9),
    ];
    let visible = staff_visible_tasks(&tasks, false, None, "", |_| true);
    let groups = staff_status_groups(&visible);
    let statuses: Vec<_> = groups.iter().map(|(status, _)| *status).collect();
    assert_eq!(
        statuses,
        [
            TaskDisplayStatus::Waiting,
            TaskDisplayStatus::Merging,
            TaskDisplayStatus::Done
        ],
        "check: group order"
    );
    let done: Vec<_> = groups[2].1.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(done, ["d9", "d5"], "check: newest first inside a group");
}

/// 场景：没有任务。
/// 数据构造：空列表。
/// 预期结果：没有分组。
#[test]
fn empty_list_has_no_groups() {
    assert!(staff_status_groups(&[]).is_empty(), "check: groups");
}
