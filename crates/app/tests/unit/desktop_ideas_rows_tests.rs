use super::{ideas_screen, relative_age, IdeaStatus};
use storage::{IdeaSource, StoredIdea};
use workspace::ideas::IdeaState;
use workspace::ideas::TaskPhase;

fn idea(id: &str, body: &str, state: IdeaState, created: u64) -> StoredIdea {
    StoredIdea {
        id: id.into(),
        workspace_id: "w1".into(),
        body: body.into(),
        source: IdeaSource::Chat,
        state,
        task_ref: None,
        assignee_area_id: None,
        assignee_node_id: None,
        position: 0,
        created_at_unix: created,
        updated_at_unix: created,
    }
}

/// 想法按状态分成待办、已指派、已完成三组，保持记录顺序，角标只数待办。
///
/// 数据构造：now = 10_000 秒。五条想法依次为 待办甲、已指派乙（交给 Bruno）、待办丙、已完成丁、派发中戊。
/// 执行过程：用名字查找函数（area-b/7 → Bruno）生成视图。
/// 预期结果：
///   - 正断言：待办为 [甲, 丙]；已指派为 [乙, 戊]（派发中归入已指派并标“投递中”）；已完成 1 条；角标为 2。
///   - 负断言：已完成不进入待办或已指派；没有待办时角标为 None。
#[test]
fn ideas_are_grouped_by_state_in_record_order() {
    let mut assigned = idea("i2", "乙", IdeaState::Assigned, 9_000);
    assigned.assignee_area_id = Some("area-b".into());
    assigned.assignee_node_id = Some(7);
    let ideas = vec![
        idea("i1", "甲", IdeaState::Todo, 9_900),
        assigned,
        idea("i3", "丙", IdeaState::Todo, 9_000),
        idea("i4", "丁", IdeaState::Done, 8_000),
        idea("i5", "戊", IdeaState::Dispatching, 9_990),
    ];
    let names = |area: &str, node: u64| (area == "area-b" && node == 7).then(|| "Bruno".to_owned());
    let screen = ideas_screen(&ideas, 10_000, &names, &|_| None);
    let todo: Vec<_> = screen.todo.iter().map(|row| row.title.as_str()).collect();
    assert_eq!(todo, ["甲", "丙"], "待办应保持记录顺序");
    let assigned: Vec<_> = screen
        .assigned
        .iter()
        .map(|row| (row.title.as_str(), row.status.clone()))
        .collect();
    assert_eq!(
        assigned,
        [
            (
                "乙",
                IdeaStatus::AssignedTo {
                    name: "Bruno".into(),
                    phase: None
                }
            ),
            ("戊", IdeaStatus::Sending)
        ],
        "已指派应带执行者名字，派发中标为投递中"
    );
    assert_eq!(screen.done.len(), 1, "已完成应单独成组");
    assert_eq!(screen.badge, Some(2), "角标只数待办");
    let empty = ideas_screen(
        &[idea("i4", "丁", IdeaState::Done, 8_000)],
        10_000,
        &names,
        &|_| None,
    );
    assert_eq!(empty.badge, None, "没有待办时不显示角标");
    assert!(
        empty.todo.is_empty() && empty.assigned.is_empty(),
        "已完成不应回到待办或已指派"
    );
}

/// 多行或过长的想法在列表里只显示首行并截断，完整正文保留给派发。
///
/// 数据构造：正文三行；一条 50 个字符的单行（按字符不是字节，上限 40）。
/// 预期结果：
///   - 正断言：多行取首行；长行截成 40 字符加省略号；两种情形 body 都是完整原文。
///   - 负断言：恰好 40 字符的正文不加省略号。
#[test]
fn long_or_multiline_ideas_show_first_line_but_keep_full_body() {
    let multi = "第一行\n第二行\n第三行";
    let long = "想".repeat(50);
    let exact = "想".repeat(40);
    let ideas = vec![
        idea("a", multi, IdeaState::Todo, 0),
        idea("b", &long, IdeaState::Todo, 0),
        idea("c", &exact, IdeaState::Todo, 0),
    ];
    let screen = ideas_screen(&ideas, 0, &|_, _| None, &|_| None);
    assert_eq!(screen.todo[0].title, "第一行", "多行正文应只显示首行");
    assert_eq!(screen.todo[0].body, multi, "完整正文应原样保留");
    assert_eq!(
        screen.todo[1].title,
        format!("{}…", "想".repeat(40)),
        "超过 40 字符应截断并加省略号"
    );
    assert_eq!(screen.todo[1].body, long, "截断只影响显示，不影响正文");
    assert_eq!(screen.todo[2].title, exact, "恰好 40 字符不应加省略号");
}

/// 相对时间的边界：按秒换算，未来时间不出现负数。
///
/// 数据构造（now − created，单位秒）：59、60、3599、3600、86399、86400、created 在未来 5 秒。
/// 推导：60 秒 = 1 分钟；3600 秒 = 1 小时；86400 秒 = 1 天。
/// 预期结果：
///   - 正断言：刚刚、1 分钟前、59 分钟前、1 小时前、23 小时前、1 天前、刚刚。
///   - 负断言：59 秒不是“0 分钟前”；未来时间不显示“-5 秒”之类。
#[test]
fn relative_age_uses_clear_boundaries() {
    let now = 1_000_000;
    let age = |delta: i64| relative_age((now as i64 - delta) as u64, now);
    assert_eq!(age(59), "刚刚", "不足 1 分钟应为刚刚");
    assert_eq!(age(60), "1 分钟前", "60 秒应为 1 分钟前");
    assert_eq!(age(3_599), "59 分钟前", "3599 秒应为 59 分钟前");
    assert_eq!(age(3_600), "1 小时前", "3600 秒应为 1 小时前");
    assert_eq!(age(86_399), "23 小时前", "86399 秒应为 23 小时前");
    assert_eq!(age(86_400), "1 天前", "86400 秒应为 1 天前");
    assert_eq!(age(-5), "刚刚", "created 在未来时按刚刚处理");
}

/// 副标题随清单状态变化，空清单教用户怎么记。
///
/// 预期结果：
///   - 正断言：有待办时副标题含数量；空时提示“记一下”。
///   - 负断言：有待办时不显示空状态提示。
#[test]
fn subtitle_reports_count_or_teaches_capture() {
    let some = ideas_screen(
        &[
            idea("a", "甲", IdeaState::Todo, 0),
            idea("b", "乙", IdeaState::Todo, 0),
        ],
        0,
        &|_, _| None,
        &|_| None,
    );
    assert!(
        some.subtitle.contains("2 条待办"),
        "副标题应包含待办数量，实际：{}",
        some.subtitle
    );
    assert!(
        !some.subtitle.contains("记一下"),
        "有待办时不应显示空状态提示"
    );
    let none = ideas_screen(&[], 0, &|_, _| None, &|_| None);
    assert!(
        none.subtitle.contains("记一下"),
        "空清单应教用户说“记一下”，实际：{}",
        none.subtitle
    );
}

/// 已指派行按任务阶段显示状态，失败时给出重试，已完成的单独成组并保留标题。
///
/// 数据构造：三条已指派想法，task_ref 分别是 t-run、t-queue、t-fail、t-none（无记录）；一条已完成。
/// 执行过程：阶段查找函数 t-run→运行中、t-queue→排队、t-fail→失败，t-none→None。
/// 预期结果：
///   - 正断言：四行阶段依次为 Running、Queued、Failed、None；只有失败行 retryable。
///   - 负断言：运行中与排队行不可重试；已完成行不进入已指派，标题保留在 done 里。
#[test]
fn assigned_rows_show_task_phase_and_only_failed_is_retryable() {
    let mk = |id: &str, task_ref: &str| {
        let mut idea = idea(id, id, IdeaState::Assigned, 0);
        idea.task_ref = Some(task_ref.into());
        idea.assignee_area_id = Some("a".into());
        idea.assignee_node_id = Some(1);
        idea
    };
    let mut done = idea("d", "已完成的事", IdeaState::Done, 0);
    done.task_ref = Some("t-done".into());
    let ideas = vec![
        mk("r", "t-run"),
        mk("q", "t-queue"),
        mk("f", "t-fail"),
        mk("n", "t-none"),
        done,
    ];
    let phase = |task_ref: &str| match task_ref {
        "t-run" => Some(TaskPhase::Running),
        "t-queue" => Some(TaskPhase::Queued),
        "t-fail" => Some(TaskPhase::Failed),
        _ => None,
    };
    let screen = ideas_screen(&ideas, 0, &|_, _| Some("Bella".into()), &phase);
    let phases: Vec<_> = screen
        .assigned
        .iter()
        .map(|row| match &row.status {
            IdeaStatus::AssignedTo { phase, .. } => *phase,
            IdeaStatus::Sending => None,
        })
        .collect();
    assert_eq!(
        phases,
        [
            Some(TaskPhase::Running),
            Some(TaskPhase::Queued),
            Some(TaskPhase::Failed),
            None
        ],
        "各行应带对应的任务阶段"
    );
    let retryable: Vec<_> = screen.assigned.iter().map(|row| row.retryable).collect();
    assert_eq!(retryable, [false, false, true, false], "只有失败的行可重试");
    assert_eq!(
        screen
            .done
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["已完成的事"],
        "已完成单独成组并保留标题"
    );
    assert_eq!(screen.assigned.len(), 4, "已完成不应混进已指派");
}
