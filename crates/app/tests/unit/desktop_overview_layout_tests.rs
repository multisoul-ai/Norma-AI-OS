use super::*;

/// 概览四栏的顺序和基准高度是固定的：需要你处理 → 当前项目 → 想法 → 其他项目。
///
/// 预期结果：
///   - 正断言：ORDER 恰为四栏且顺序固定；基准高度依次 190 / 210 / 236 / 88。
///   - 负断言：任何栏的基准高度都不低于最小高度，保证至少容得下标题和一行内容。
#[test]
fn groups_keep_a_fixed_order_and_base_heights() {
    assert_eq!(
        OverviewGroup::ORDER,
        [
            OverviewGroup::Attention,
            OverviewGroup::Current,
            OverviewGroup::Ideas,
            OverviewGroup::Other
        ],
        "栏目顺序是产品约定，不随内容变化"
    );
    let heights = OverviewGroup::ORDER.map(OverviewGroup::base_height);
    assert_eq!(heights, [190.0, 210.0, 236.0, 88.0]);
    assert!(
        heights.iter().all(|height| *height >= MIN_GROUP_HEIGHT),
        "基准高度不能低于最小高度"
    );
}

/// 折叠状态按栏记录，互不影响，且可反复切换。
///
/// 数据构造：全部展开的初始状态。
/// 执行过程：折叠“当前项目”，再折叠“其他项目”，再展开“当前项目”。
/// 预期结果：
///   - 正断言：每一步只有被操作的那一栏状态改变。
///   - 负断言：未操作的栏（需要你处理、想法）始终保持展开。
#[test]
fn collapse_state_is_tracked_per_group() {
    let mut state = OverviewCollapse::default();
    assert!(OverviewGroup::ORDER
        .iter()
        .all(|group| !state.is_collapsed(*group)));
    state.toggle(OverviewGroup::Current);
    state.toggle(OverviewGroup::Other);
    assert!(state.is_collapsed(OverviewGroup::Current));
    assert!(state.is_collapsed(OverviewGroup::Other));
    state.toggle(OverviewGroup::Current);
    assert!(
        !state.is_collapsed(OverviewGroup::Current),
        "再次点击应展开"
    );
    assert!(state.is_collapsed(OverviewGroup::Other));
    assert!(!state.is_collapsed(OverviewGroup::Attention));
    assert!(!state.is_collapsed(OverviewGroup::Ideas));
}

/// 想法栏被要求“露出来”时必须展开；其他栏保持原状态。
///
/// 预期结果：
///   - 正断言：想法栏折叠后调用 reveal 即展开。
///   - 负断言：其他已折叠的栏不会被顺带展开。
#[test]
fn revealing_ideas_expands_only_the_ideas_group() {
    let mut state = OverviewCollapse::default();
    state.toggle(OverviewGroup::Ideas);
    state.toggle(OverviewGroup::Other);
    state.reveal(OverviewGroup::Ideas);
    assert!(!state.is_collapsed(OverviewGroup::Ideas));
    assert!(state.is_collapsed(OverviewGroup::Other), "其他栏保持折叠");
}

/// 每栏都有空态文案，且标题与提示都不为空。
///
/// 预期结果：
///   - 正断言：四栏都有非空的空态标题和提示。
///   - 负断言：四个空态标题互不相同，不会出现两栏文案雷同。
#[test]
fn every_group_has_a_distinct_empty_state() {
    let titles: Vec<&str> = OverviewGroup::ORDER
        .iter()
        .map(|group| {
            let (title, hint) = group.empty_copy();
            assert!(!title.is_empty() && !hint.is_empty());
            title
        })
        .collect();
    for (index, title) in titles.iter().enumerate() {
        assert!(!titles[..index].contains(title), "空态标题重复：{title}");
    }
}
