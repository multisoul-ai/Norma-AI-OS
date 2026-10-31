#[path = "../support/drawer_locale.rs"]
mod drawer_locale;
use drawer_locale::expected_copy;

use super::{clamp_cursor, ideas_screen, relative_age, IdeaStatus, IdeasTab};
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
        images: Vec::new(),
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
/// 数据构造：正文三行；一条 50 个字符的单行（按字符不是字节，上限 40）；同样的正文进入已指派和已完成。
/// 预期结果：
///   - 正断言：多行取首行；长行截成 40 字符加省略号；所有状态保留完整原文供悬停查看。
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
        idea("d", multi, IdeaState::Assigned, 0),
        idea("e", &long, IdeaState::Done, 0),
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
    assert_eq!(
        screen.assigned[0].body, multi,
        "已指派想法的悬停内容须保留后续行"
    );
    assert_eq!(
        screen.done[0].body, long,
        "已完成想法的悬停内容须保留第 40 字符后的正文"
    );
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
    assert_eq!(
        age(59),
        expected_copy("刚刚", "Just now"),
        "不足 1 分钟应为刚刚"
    );
    assert_eq!(
        age(60),
        expected_copy("1 分钟前", "1 min ago"),
        "60 秒应为 1 分钟前"
    );
    assert_eq!(
        age(3_599),
        expected_copy("59 分钟前", "59 min ago"),
        "3599 秒应为 59 分钟前"
    );
    assert_eq!(
        age(3_600),
        expected_copy("1 小时前", "1 hr ago"),
        "3600 秒应为 1 小时前"
    );
    assert_eq!(
        age(86_399),
        expected_copy("23 小时前", "23 hr ago"),
        "86399 秒应为 23 小时前"
    );
    assert_eq!(
        age(86_400),
        expected_copy("1 天前", "1 d ago"),
        "86400 秒应为 1 天前"
    );
    assert_eq!(
        age(-5),
        expected_copy("刚刚", "Just now"),
        "created 在未来时按刚刚处理"
    );
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
        some.subtitle
            .contains(expected_copy("2 条待办", "To do: 2")),
        "副标题应包含待办数量，实际：{}",
        some.subtitle
    );
    assert!(
        !some
            .subtitle
            .contains(expected_copy("记一下", "Remember this")),
        "有待办时不应显示空状态提示"
    );
    let none = ideas_screen(&[], 0, &|_, _| None, &|_| None);
    assert!(
        none.subtitle
            .contains(expected_copy("记一下", "Remember this")),
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

/// 刷新后条数变少，键盘光标必须收敛回有效范围，否则回车会索引越界。
///
/// 数据构造：光标在第 2 行（下标 2）；总数分别为 3、2、0。
/// 预期结果：
///   - 正断言：总数 3 时保持；总数 2 时光标失效为 None；
///   - 负断言：总数 0 时一定是 None（空清单不能留着旧光标）。
#[test]
fn stale_keyboard_cursor_is_dropped_when_rows_shrink() {
    assert_eq!(clamp_cursor(Some(2), 3), Some(2), "仍在范围内应保持");
    assert_eq!(clamp_cursor(Some(2), 2), None, "光标超出末行应失效");
    assert_eq!(clamp_cursor(Some(0), 0), None, "空清单不能留着旧光标");
    assert_eq!(clamp_cursor(None, 5), None, "没有光标仍是 None");
}

/// 多行、带链接、带图片的想法在待办行里的呈现：标题取第一行，链接与图片数单独给出。
///
/// 数据构造：一条“两行正文 + 一个链接 + 2 张图片”的待办；一条只有图片（正文为空）的待办。
/// 执行过程：生成视图。
/// 预期结果：
///   - 正断言：标题只取第一行；links 为正文里的链接；image_names 保持顺序；纯图片想法的标题为“图片”。
///   - 负断言：第二行不进入标题；正文无链接时 links 为空。
#[test]
fn todo_row_shows_first_line_links_and_images() {
    let mut rich = idea(
        "i1",
        "看这个设计\n细节在 https://example.com/spec 里",
        IdeaState::Todo,
        9_000,
    );
    rich.images = vec!["a.png".into(), "b.jpg".into()];
    let mut only_image = idea("i2", "", IdeaState::Todo, 9_000);
    only_image.images = vec!["c.png".into()];
    let screen = ideas_screen(&[rich, only_image], 10_000, &|_, _| None, &|_| None);
    let first = &screen.todo[0];
    assert_eq!(first.title, "看这个设计", "标题只取第一行");
    assert_eq!(
        first.links,
        ["https://example.com/spec"],
        "应识别正文里的链接"
    );
    assert_eq!(first.image_names, ["a.png", "b.jpg"], "图片应保持顺序");
    let second = &screen.todo[1];
    assert_eq!(
        second.title,
        expected_copy("图片", "Images"),
        "纯图片想法用“图片”作标题"
    );
    assert!(second.links.is_empty(), "没有链接时 links 应为空");
}

/// 分段按钮默认停在「待办」，三段顺序固定，标签与键互不重复。
///
/// 数据构造：取 `IdeasTab::default()` 与 `IdeasTab::ALL`。
/// 执行过程：读取默认值、逐段取 label 与 key。
/// 预期结果：
///   - 正断言：默认是 Todo；顺序为 待办、已指派、已完成。
///   - 负断言：三个 key 互不相同，避免元素 ID 冲突。
#[test]
fn ideas_tabs_default_to_todo_with_stable_order() {
    assert_eq!(IdeasTab::default(), IdeasTab::Todo, "首次进入应先看到待办");
    let labels: Vec<_> = IdeasTab::ALL.iter().map(|tab| tab.label()).collect();
    assert_eq!(
        labels,
        [
            expected_copy("待办", "To do"),
            expected_copy("已指派", "Assigned"),
            expected_copy("已完成", "Done")
        ],
        "分段顺序应固定"
    );
    let mut keys: Vec<_> = IdeasTab::ALL.iter().map(|tab| tab.key()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 3, "三段的 key 不能重复");
}
