use super::{TextRole, TextTone};

/// Settings 字级必须是一套清晰、有限的阶梯，而不是每页自造近似值。
///
/// 场景：所有 Settings 页面通过 `TextRole` 取字号；字号集合必须收敛到少数几级。
///
/// 数据构造（含关键数值的推导过程）：
///   2026-10-06 按产品要求适度放大：title = 18，headline = 14，
///   body/row title/control/subtitle = 12，helper/label = 11，caption/eyebrow = 10，badge = 9；
///   长文阅读 = 13；一次性配对码 = 26。
///   去重后期望字号集合 = {9, 10, 11, 12, 13, 14, 18, 26}，共 8 级。
///   旧实现中出现过的近似值：15.75、15.5、18.5、16。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 遍历 `TextRole::ALL` 读取每个角色的 spec。
///   2. 收集去重后的字号并排序。
///
/// 预期结果：
///   - 正断言：字号集合精确为 9/10/11/12/13/14/18/26。
///   - 负断言：不得出现 15.75/15.5/18.5/16 等旧的近似字号。
#[test]
fn settings_scale_uses_a_small_fixed_set_of_sizes() {
    let mut sizes: Vec<f32> = TextRole::ALL.iter().map(|role| role.spec().size).collect();
    sizes.sort_by(f32::total_cmp);
    sizes.dedup();

    assert_eq!(
        sizes,
        vec![9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 18.0, 26.0],
        "Settings 字级必须收敛为 8 级清晰阶梯"
    );
    let roles = [
        (TextRole::Title, 18.0),
        (TextRole::Subtitle, 12.0),
        (TextRole::Eyebrow, 10.0),
        (TextRole::Headline, 14.0),
        (TextRole::RowTitle, 12.0),
        (TextRole::Body, 12.0),
        (TextRole::Reading, 13.0),
        (TextRole::Helper, 11.0),
        (TextRole::Label, 11.0),
        (TextRole::Control, 12.0),
        (TextRole::Caption, 10.0),
        (TextRole::Badge, 9.0),
        (TextRole::Code, 26.0),
    ];
    assert_eq!(roles.len(), TextRole::ALL.len(), "字号表覆盖每个角色");
    for (role, size) in roles {
        assert_eq!(role.spec().size, size, "{role:?} 应使用指定字号");
    }
    for legacy in [15.75, 15.5, 18.5, 16.0] {
        assert!(
            !sizes.contains(&legacy),
            "旧的近似字号 {legacy}px 不得回到 Settings 字级中"
        );
    }
}

/// 层级必须单调：标题 > 区块标题 > 行标题/正文 > 说明 > 元信息 > 徽标。
///
/// 场景：同一页面内用户靠字号与字重判断层级，任意两级倒置都会让结构读错。
///
/// 数据构造（含关键数值的推导过程）：
///   Title 18 > Headline 14 > RowTitle 12 = Body 12 > Helper 11 > Caption 10 > Badge 9。
///   Caption 10px 恢复高于 Badge 9px，徽标字重仍为 600。
///   RowTitle 与 Body 同字号，层级靠字重区分：SEMIBOLD(600) > NORMAL(400)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次读取上述角色的字号。
///   2. 比较相邻角色，以及 RowTitle 与 Body 的字重。
///
/// 预期结果：
///   - 正断言：字号严格递减链成立，RowTitle 字重高于 Body。
///   - 负断言：Eyebrow（导航分组）不得大于正文，避免 "Norma" 分组标题比其他分组大一号。
#[test]
fn settings_scale_hierarchy_is_monotonic() {
    let size = |role: TextRole| role.spec().size;
    let chain = [
        TextRole::Title,
        TextRole::Headline,
        TextRole::Body,
        TextRole::Helper,
        TextRole::Caption,
        TextRole::Badge,
    ];

    for pair in chain.windows(2) {
        assert!(
            size(pair[0]) > size(pair[1]),
            "{:?}({}px) 必须大于 {:?}({}px)",
            pair[0],
            size(pair[0]),
            pair[1],
            size(pair[1])
        );
    }
    assert!(
        size(TextRole::Caption) > size(TextRole::Badge)
            && TextRole::Badge.spec().weight > TextRole::Caption.spec().weight,
        "元信息必须严格大于徽标；徽标字重保持高于元信息"
    );
    assert_eq!(
        size(TextRole::RowTitle),
        size(TextRole::Body),
        "行标题与正文同字号，层级由字重表达"
    );
    assert!(
        TextRole::RowTitle.spec().weight > TextRole::Body.spec().weight,
        "行标题字重必须高于正文"
    );
    assert!(
        size(TextRole::Eyebrow) < size(TextRole::Body),
        "导航分组/区段标签不得大于正文，避免单个分组标题被放大"
    );
}

/// 行高随字号反向收紧：标题紧、正文松，且每级都不裁切字形。
///
/// 场景：apple-design 排版原则要求标题行距紧凑、正文行距宽松；GPUI 默认 phi(≈1.618)
/// 行高会让紧凑面板的标题过松，因此每个角色显式给出像素行高。
///
/// 数据构造（含关键数值的推导过程）：
///   字号与行高同比放大，Title 23.625/18 ≈ 1.31；Headline (238/13)/14 ≈ 1.31；
///   Body (192/11)/12 ≈ 1.45；Reading (247/12)/13 ≈ 1.58。
///   全部角色行高比例范围限定在 [1.15, 1.60]。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算每个角色的 line_height / size。
///   2. 比较 Title 与 Body、Reading 的比例。
///
/// 预期结果：
///   - 正断言：每个比例都在 [1.15, 1.60]；Title 比例 < Body 比例 < Reading 比例。
///   - 负断言：任何角色的行高不得小于字号（会裁切字形）。
#[test]
fn settings_scale_leading_tightens_as_size_grows() {
    let ratio = |role: TextRole| role.spec().line_height / role.spec().size;

    for role in TextRole::ALL {
        let spec = role.spec();
        assert!(
            (1.15..=1.60).contains(&ratio(role)),
            "{role:?} 行高比例 {:.2} 必须位于 1.15–1.60",
            ratio(role)
        );
        assert!(
            spec.line_height >= spec.size,
            "{role:?} 行高 {} 不得小于字号 {}，否则裁切字形",
            spec.line_height,
            spec.size
        );
    }
    assert!(
        ratio(TextRole::Title) < ratio(TextRole::Body),
        "标题行距必须比正文更紧"
    );
    assert!(
        ratio(TextRole::Body) < ratio(TextRole::Reading),
        "长文阅读行距必须比 UI 正文更松"
    );
}

/// 每个角色只有一个字重，且只用系统字重档位 400/500/600/700。
///
/// 场景：design.md 规定"同一层级只使用一个字号与一个主要字重"；旧实现中同为区块标题
/// 的文字分别使用 BOLD 与 SEMIBOLD，同为行标题的文字分别使用 BOLD、SEMIBOLD、MEDIUM。
///
/// 数据构造（含关键数值的推导过程）：
///   Title/Headline/RowTitle/Eyebrow/Badge = SEMIBOLD(600)；
///   Label/Control/Caption = MEDIUM(500)；Subtitle/Body/Reading/Helper = NORMAL(400)；
///   Code = BOLD(700)，仅用于一次性配对码这一展示型数字。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取各角色字重。
///   2. 与期望映射逐一比较。
///
/// 预期结果：
///   - 正断言：映射与上表完全一致。
///   - 负断言：区块标题与行标题不得使用 BOLD（700）。
#[test]
fn settings_scale_assigns_one_weight_per_role() {
    let expected = [
        (TextRole::Title, 600.0),
        (TextRole::Subtitle, 400.0),
        (TextRole::Eyebrow, 600.0),
        (TextRole::Headline, 600.0),
        (TextRole::RowTitle, 600.0),
        (TextRole::Body, 400.0),
        (TextRole::Reading, 400.0),
        (TextRole::Helper, 400.0),
        (TextRole::Label, 500.0),
        (TextRole::Control, 500.0),
        (TextRole::Caption, 500.0),
        (TextRole::Badge, 600.0),
        (TextRole::Code, 700.0),
    ];

    assert_eq!(
        expected.len(),
        TextRole::ALL.len(),
        "期望表必须覆盖全部 Settings 文本角色"
    );
    for (role, weight) in expected {
        assert_eq!(
            role.spec().weight,
            weight,
            "{role:?} 字重不符合 Settings 字级"
        );
    }
    for role in [TextRole::Headline, TextRole::RowTitle] {
        assert_ne!(
            role.spec().weight,
            700.0,
            "{role:?} 不得使用 BOLD，避免与同级 SEMIBOLD 混用"
        );
    }
}

/// 场景：角色使用现有三级文字色语义，不把强调色作为默认正文色。
/// 数据构造：标题/正文为 Primary；说明/控件为 Secondary；副标题/版本为 Tertiary。
/// 执行过程：遍历角色表，比较每个角色的默认色调。
/// 预期结果：全部角色符合语义层级；说明文字不使用可读性较低的 Tertiary。
#[test]
fn settings_scale_tones_preserve_text_emphasis() {
    let expected = [
        (TextRole::Title, TextTone::Primary),
        (TextRole::Subtitle, TextTone::Tertiary),
        (TextRole::Eyebrow, TextTone::Secondary),
        (TextRole::Headline, TextTone::Primary),
        (TextRole::RowTitle, TextTone::Primary),
        (TextRole::Body, TextTone::Primary),
        (TextRole::Reading, TextTone::Primary),
        (TextRole::Helper, TextTone::Secondary),
        (TextRole::Label, TextTone::Secondary),
        (TextRole::Control, TextTone::Secondary),
        (TextRole::Caption, TextTone::Tertiary),
        (TextRole::Badge, TextTone::Secondary),
        (TextRole::Code, TextTone::Primary),
    ];
    assert_eq!(expected.len(), TextRole::ALL.len(), "色调表覆盖全部角色");
    for (role, tone) in expected {
        assert_eq!(
            role.spec().tone,
            tone,
            "{role:?} 必须保持统一的默认文字强调"
        );
    }
    assert_ne!(
        TextRole::Helper.spec().tone,
        TextTone::Tertiary,
        "说明文字不得降为三级色调"
    );
}
