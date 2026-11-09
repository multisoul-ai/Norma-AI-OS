use super::*;

fn agent(id: &str, slot: SlotKind) -> SlotAgent {
    SlotAgent {
        id: id.to_owned(),
        slot,
    }
}

// 场景：8 个智能体里 4 个在跑、3 个待处理、1 个排队。
// 数据：运行 a–d，待处理 p q r，排队 z。同一标识不会重复。
// 执行：compact_roster。
// 预期：左槽露出 3 个并溢出 +1，右槽露出 2 个、总数 3；排队不出现在任一侧。
#[test]
fn eight_agents_split_into_three_plus_overflow_and_two_pending() {
    let agents = vec![
        agent("a", SlotKind::Running),
        agent("b", SlotKind::Running),
        agent("c", SlotKind::Running),
        agent("d", SlotKind::Running),
        agent("p", SlotKind::Pending),
        agent("q", SlotKind::Pending),
        agent("r", SlotKind::Pending),
        agent("z", SlotKind::Hidden),
    ];
    let roster = compact_roster(&agents);
    assert_eq!(
        roster.running,
        vec!["a".to_owned(), "b".into(), "c".into()],
        "左槽按输入顺序只留 3 个"
    );
    assert_eq!(roster.running_overflow, 1, "第 4 个运行中折成 +1");
    assert_eq!(roster.running_total(), 4, "溢出仍计入运行总数");
    assert_eq!(
        roster.pending,
        vec!["p".to_owned(), "q".into()],
        "右槽只留前 2 个待处理"
    );
    assert_eq!(roster.pending_total, 3, "徽章用待处理总数 3");
    assert!(
        !roster.running.iter().any(|id| id == "z") && !roster.pending.iter().any(|id| id == "z"),
        "排队的 z 不进左右槽"
    );
    let left: std::collections::BTreeSet<_> = roster.running.iter().collect();
    assert!(
        roster.pending.iter().all(|id| !left.contains(id)),
        "露出的左右槽没有同一个智能体"
    );
}

// 场景：一个智能体既在跑又在等审批。
// 数据：a 先以运行出现，再以待处理出现；b 只在跑。
// 执行：compact_roster。
// 预期：a 只在右边，左边只剩 b。
#[test]
fn an_agent_waiting_on_the_user_is_not_also_on_the_left() {
    let roster = compact_roster(&[
        agent("a", SlotKind::Running),
        agent("a", SlotKind::Pending),
        agent("b", SlotKind::Running),
    ]);
    assert_eq!(roster.running, vec!["b".to_owned()], "a 已归到待处理");
    assert_eq!(roster.pending, vec!["a".to_owned()], "a 只出现一次");
    assert_eq!(roster.running_overflow, 0, "没有被折掉的运行中");
    assert_eq!(roster.pending_total, 1, "待处理总数不含重复");
}

// 场景：收起宽度的边界。
// 数据：运行数与待处理数之和为 3 和 4。
// 执行：needs_wide_capsule。
// 预期：3 保持 330 的窄胶囊，4 起加宽。1+1 窄，2+2 宽，4+0 宽，0+0 窄。
#[test]
fn capsule_widens_when_running_plus_pending_reaches_four() {
    assert!(!needs_wide_capsule(0, 0), "空闲保持窄胶囊");
    assert!(!needs_wide_capsule(1, 1), "2 个智能体仍是 330");
    assert!(!needs_wide_capsule(3, 0), "3 个运行中还没到 4");
    assert!(needs_wide_capsule(2, 2), "2+2 是加宽边界");
    assert!(
        needs_wide_capsule(4, 3),
        "8 人夹具里 4 运行 + 3 待处理要加宽"
    );
    assert!(needs_wide_capsule(1, 3), "待处理也能单独把和推过 4");
}

// 场景：四种活动对应四种表情，排队变淡。
// 数据：运行、等你、完成、排队。
// 执行：avatar_face。
// 预期：Working / Waiting / Responded / Idle，只有排队的不透明度是 0.55。
#[test]
fn queued_agents_are_idle_at_fifty_five_percent() {
    let running = avatar_face(AgentActivity::Running);
    let waiting = avatar_face(AgentActivity::Waiting);
    let done = avatar_face(AgentActivity::Done);
    let queued = avatar_face(AgentActivity::Queued);
    assert_eq!(running.mood, Mood::Working, "运行中是专注表情");
    assert_eq!(waiting.mood, Mood::Waiting, "等你是摇头表情");
    assert_eq!(done.mood, Mood::Responded, "完成是笑眼");
    assert_eq!(queued.mood, Mood::Idle, "排队不扮演其他情绪");
    assert_eq!(running.opacity, 1., "运行中不透明");
    assert_eq!(waiting.opacity, 1., "等你不透明");
    assert_eq!(done.opacity, 1., "完成不透明");
    assert!(
        (queued.opacity - 0.55).abs() < f32::EPSILON,
        "排队不透明度是 0.55，实际 {}",
        queued.opacity
    );
}

// 场景：运行中栏的筛选计数必须等于筛完的行数。
// 数据：4 运行、2 等你、1 完成、1 排队，共 8。
// 执行：filter_counts，再按 accepts 过滤。
// 预期：全部 8、运行 4、等你 2、完成 1；排队只出现在全部里。
#[test]
fn filter_counts_match_the_rows_each_chip_would_show() {
    let activities = [
        AgentActivity::Running,
        AgentActivity::Running,
        AgentActivity::Running,
        AgentActivity::Running,
        AgentActivity::Waiting,
        AgentActivity::Waiting,
        AgentActivity::Done,
        AgentActivity::Queued,
    ];
    let counts = filter_counts(&activities);
    assert_eq!(counts.all, 8, "全部含排队");
    assert_eq!(counts.running, 4, "运行中 4");
    assert_eq!(counts.waiting, 2, "等你 2，不含已完成的成果");
    assert_eq!(counts.done, 1, "完成 1");
    for (filter, expected) in [
        (RunFilter::All, 8),
        (RunFilter::Running, 4),
        (RunFilter::Waiting, 2),
        (RunFilter::Done, 1),
    ] {
        let shown = activities
            .iter()
            .filter(|activity| accepts(filter, **activity))
            .count();
        assert_eq!(shown, expected, "{filter:?} 的行数要和计数一致");
    }
    assert!(
        !accepts(RunFilter::Running, AgentActivity::Queued),
        "排队不是运行中"
    );
    assert!(
        accepts(RunFilter::All, AgentActivity::Queued),
        "全部要留下排队"
    );
}

// 场景：审批队列点掉当前卡之后轮到下一个。
// 数据：之前是 a b c，处理后剩下 a c。当前是 b。
// 执行：pending_successor。
// 预期：下一个是 c，不是回到 a。只剩最后一项被处理时回到前一个。队列不足 2 个不显示。
#[test]
fn resolving_the_current_card_advances_to_the_next_pending() {
    let previous = vec!["a".to_owned(), "b".into(), "c".into()];
    let remaining = vec!["a".to_owned(), "c".into()];
    assert_eq!(
        pending_successor(&previous, &remaining, "b").as_deref(),
        Some("c"),
        "处理 b 之后轮到 c"
    );
    assert_eq!(
        pending_successor(&previous, &remaining, "a").as_deref(),
        Some("a"),
        "a 还在就不要跳走"
    );
    assert_eq!(
        pending_successor(&previous, &["a".into(), "b".into()], "c").as_deref(),
        Some("b"),
        "最后一张走了就回到前一张"
    );
    assert_eq!(
        pending_successor(&previous, &[], "b"),
        None,
        "没有剩余则收起队列"
    );
    assert!(pending_queue_visible(2), "两件待处理才出现队列");
    assert!(!pending_queue_visible(1), "只有一件时不画队列");
}

/// 场景：展开运行页右侧一览要把每张 20pt 脸排进父级宽度。
/// 数据：0、1、8 张脸。重叠是 2pt，所以步长 18pt。
/// 执行：overview_strip_width。
/// 预期：空条 0；一张 20；八张 20+7×18=146。父级按这个宽度占位，才不会被挤出窗口。
#[test]
fn overview_strip_reserves_the_overlapped_face_width() {
    assert_eq!(overview_strip_width(0), 0., "没有头像时一览不占宽");
    assert_eq!(overview_strip_width(1), 20., "一张头像就是 20pt");
    assert_eq!(
        overview_strip_width(8),
        146.,
        "8 张 20pt 头像、相邻重叠 2pt：20 + 7×18 = 146"
    );
}

// 场景：完成通知底部的 3 秒倒计时。
// 数据：截止 3000，窗口就是 ACTIVITY_HOLD_MS。
// 执行：起点、过半、结束，以及减少动态效果。
// 预期：1、0.5、0；减少动态效果时始终是 1，线不缩短。
#[test]
fn countdown_shrinks_across_three_seconds_unless_motion_is_reduced() {
    assert_eq!(ACTIVITY_HOLD_MS, 3_000, "停留窗口是 3 秒");
    assert_eq!(countdown_scale(0, 3_000, false), 1., "刚弹出是满线");
    assert!(
        (countdown_scale(1_500, 3_000, false) - 0.5).abs() < 1e-9,
        "1.5 秒时剩下一半"
    );
    assert_eq!(countdown_scale(3_000, 3_000, false), 0., "到点线归零");
    assert_eq!(
        countdown_scale(1_500, 3_000, true),
        1.,
        "减少动态效果时不播放缩短动画"
    );
}
