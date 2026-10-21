use super::{ideas_screen, relative_age, IdeaStatus};
use storage::{IdeaSource, StoredIdea};
use workspace::ideas::IdeaState;

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
    let screen = ideas_screen(&ideas, 10_000, &names);
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
            ("乙", IdeaStatus::AssignedTo("Bruno".into())),
            ("戊", IdeaStatus::Sending)
        ],
        "已指派应带执行者名字，派发中标为投递中"
    );
    assert_eq!(screen.done_count, 1, "已完成应单独计数");
    assert_eq!(screen.badge, Some(2), "角标只数待办");
    let empty = ideas_screen(&[idea("i4", "丁", IdeaState::Done, 8_000)], 10_000, &names);
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
    let screen = ideas_screen(&ideas, 0, &|_, _| None);
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
    let none = ideas_screen(&[], 0, &|_, _| None);
    assert!(
        none.subtitle.contains("记一下"),
        "空清单应教用户说“记一下”，实际：{}",
        none.subtitle
    );
}
