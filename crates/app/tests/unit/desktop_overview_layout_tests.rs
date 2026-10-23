use super::*;

/// 概览四栏的顺序和基准高度是固定的：需要你处理 → 运行中 → 待开始 → 想法。
///
/// 预期结果：
///   - 正断言：ORDER 恰为四栏且顺序固定；基准高度依次 200 / 260 / 200 / 250。
///   - 负断言：任何栏的基准高度都不低于最小高度，保证至少容得下标题和一行内容。
#[test]
fn groups_keep_a_fixed_order_and_base_heights() {
    assert_eq!(
        OverviewGroup::ORDER,
        [
            OverviewGroup::Attention,
            OverviewGroup::Running,
            OverviewGroup::Idle,
            OverviewGroup::Ideas
        ],
        "栏目顺序是产品约定，不随内容变化"
    );
    let heights = OverviewGroup::ORDER.map(OverviewGroup::base_height);
    assert_eq!(heights, [200.0, 260.0, 200.0, 250.0]);
    assert!(
        heights.iter().all(|height| *height >= MIN_GROUP_HEIGHT),
        "基准高度不能低于最小高度"
    );
}

/// 折叠状态按栏记录，互不影响，且可反复切换。
///
/// 数据构造：全部展开的初始状态。
/// 执行过程：折叠“运行中”，再折叠“待开始”，再展开“运行中”。
/// 预期结果：
///   - 正断言：每一步只有被操作的那一栏状态改变。
///   - 负断言：未操作的栏（需要你处理、想法）始终保持展开。
#[test]
fn collapse_state_is_tracked_per_group() {
    let mut state = OverviewCollapse::default();
    assert!(OverviewGroup::ORDER
        .iter()
        .all(|group| !state.is_collapsed(*group)));
    state.toggle(OverviewGroup::Running);
    state.toggle(OverviewGroup::Idle);
    assert!(state.is_collapsed(OverviewGroup::Running));
    assert!(state.is_collapsed(OverviewGroup::Idle));
    state.toggle(OverviewGroup::Running);
    assert!(
        !state.is_collapsed(OverviewGroup::Running),
        "再次点击应展开"
    );
    assert!(state.is_collapsed(OverviewGroup::Idle));
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
    state.toggle(OverviewGroup::Idle);
    state.reveal(OverviewGroup::Ideas);
    assert!(!state.is_collapsed(OverviewGroup::Ideas));
    assert!(state.is_collapsed(OverviewGroup::Idle), "其他栏保持折叠");
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

/// 折叠位固定编号并写进持久化状态：升级后旧偏好的含义要延续。
///
/// 数据构造：旧版偏好里“当前项目”折叠对应位 1（0b0010），“其他项目”折叠对应位 3（0b1000）。
/// 执行过程：用旧位还原折叠状态。
/// 预期结果：
///   - 正断言：旧“当前项目”位仍折叠“运行中”；旧“其他项目”位折叠“待开始”（项目行已并入这一组）；
///     四个栏的折叠位互不重复，且都在运行时保留的低四位以内。
///   - 负断言：这两个旧位不会折叠“需要你处理”和“想法”。
#[test]
fn legacy_collapse_bits_keep_their_meaning() {
    let legacy_current = OverviewCollapse::from_bits(0b0010);
    assert!(legacy_current.is_collapsed(OverviewGroup::Running));
    assert!(!legacy_current.is_collapsed(OverviewGroup::Idle));
    let legacy_other = OverviewCollapse::from_bits(0b1000);
    assert!(legacy_other.is_collapsed(OverviewGroup::Idle));
    for state in [legacy_current, legacy_other] {
        assert!(!state.is_collapsed(OverviewGroup::Attention));
        assert!(!state.is_collapsed(OverviewGroup::Ideas));
    }
    let mut seen = 0u8;
    for group in OverviewGroup::ORDER {
        let mut state = OverviewCollapse::default();
        state.toggle(group);
        assert_eq!(seen & state.bits(), 0, "折叠位不能重复");
        assert_eq!(state.bits() & !0b1111, 0, "折叠位必须落在低四位");
        seen |= state.bits();
    }
}

/// 状态栏起始高度按内容定：内容少就紧凑，内容多封顶到基准高度，想法栏不按内容。
///
/// 数据构造：一行 48px 的待处理、一行 72px 的智能体、七行 72px 的智能体、没有行。
/// 执行过程：分别取各栏的起始高度。
/// 预期结果：
///   - 正断言：待处理一行不超过基准且不低于最小高度；待开始一行恰好容得下标题加一整行（34 + 74）；
///     七行封顶到基准高度；空栏按空态高度但不超过基准。
///   - 负断言：想法栏不随行高变化，始终是基准高度；任何栏都不低于最小高度。
#[test]
fn basis_height_follows_content_and_is_capped_by_base() {
    assert_eq!(
        OverviewGroup::Attention.basis_height(&[48.0]),
        MIN_GROUP_HEIGHT
    );
    assert_eq!(OverviewGroup::Idle.basis_height(&[72.0]), 34.0 + 72.0 + 2.0);
    assert_eq!(
        OverviewGroup::Running.basis_height(&[72.0; 7]),
        OverviewGroup::Running.base_height(),
        "内容多了封顶，超出部分栏内滚动"
    );
    assert_eq!(
        OverviewGroup::Idle.basis_height(&[]),
        OverviewGroup::Idle.base_height().min(120.0)
    );
    assert_eq!(
        OverviewGroup::Ideas.basis_height(&[72.0; 7]),
        OverviewGroup::Ideas.base_height()
    );
    for group in OverviewGroup::ORDER {
        assert!(group.basis_height(&[]) >= MIN_GROUP_HEIGHT);
        assert!(group.basis_height(&[10.0]) >= MIN_GROUP_HEIGHT);
    }
}
