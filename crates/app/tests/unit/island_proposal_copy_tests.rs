use super::{block_key, brief_why, IslandProposalBlock};

/// 场景：提议理由在卡片上只留一行，按 Unicode 字符截断。
///
/// 数据构造：
///   短句 = 2 个汉字「先收」
///   正好一行 = 42 个「日」（42 是卡片一行的上限）
///   超一行 = 43 个「日」（42 + 1，多出来的那一个必须变成省略号，不能占第 43 格）
///
/// 执行：brief_why。
///
/// 预期结果：
///   - 短句和正好 42 字原样保留，没有省略号
///   - 43 字变成 42 字加「…」，不再包含第 43 个「日」
#[test]
fn brief_why_keeps_forty_two_characters_and_ellipsizes_the_next() {
    assert_eq!(brief_why("先收"), "先收", "短句不能被截断");
    assert_eq!(brief_why(""), "", "空理由保持空");
    let exact: String = "日".repeat(42);
    assert_eq!(brief_why(&exact), exact, "正好 42 个字符要原样保留");
    assert!(!brief_why(&exact).contains('…'), "未超长时不能加省略号");
    let over = format!("{exact}日");
    let shown = brief_why(&over);
    assert_eq!(shown, format!("{exact}…"), "第 43 个字符要换成省略号");
    assert_eq!(
        shown.chars().filter(|c| *c == '日').count(),
        42,
        "省略后仍是 42 个字"
    );
    assert!(!shown.contains(&over), "不能把第 43 个日留在卡片上");
}

/// 场景：四种派发阻断各自对应一条岛上文案键。
///
/// 数据：WaitingConfirmation、Busy、Missing、Disabled 各一次。
/// 执行：block_key。
/// 预期：键互不相同，且 Missing 用未找到 Agent 的键，不能落到忙碌键。
#[test]
fn block_key_maps_each_reason_to_its_own_sentence() {
    assert_eq!(
        block_key(IslandProposalBlock::WaitingConfirmation),
        "island.proposal_waiting",
        "等确认要用等待键"
    );
    assert_eq!(
        block_key(IslandProposalBlock::Busy),
        "island.proposal_busy",
        "正忙要用忙碌键"
    );
    assert_eq!(
        block_key(IslandProposalBlock::Missing),
        "island.proposal_missing",
        "找不到 Agent 不能写成正忙"
    );
    assert_eq!(
        block_key(IslandProposalBlock::Disabled),
        "island.proposal_disabled",
        "停用要用停用键"
    );
    assert_ne!(
        block_key(IslandProposalBlock::Missing),
        block_key(IslandProposalBlock::Busy),
        "未找到和正忙必须是两句"
    );
}
