use super::{
    canonical_expanded_content_height_for, canonical_provider_row_layout,
    canonical_provider_row_rhythm, canonical_provider_typography, warning_status_color,
};
use norma::orchestration::OrchestrationProvider;
use ui::Theme;

/// Canonical 展开态高度：五个 Provider 与 Cursor 四组件必须同时留在 golden 面板内。
///
/// 数据构造（含关键数值的推导过程）：
///   header = 119px
///   notice = 47px
///   provider rows = 92 + 84 + 80 + 85 + 85 = 426px
///   Cursor components = 4 × 44px = 176px
///   detail bottom margin = 14px
///   total = 119 + 47 + 426 + 176 + 14 = 782px
///   logical panel height = 820px；remaining = 820 - 782 = 38px
///   0.854 display scale 下 outer = 820 × 0.854 ≈ 700px，与 699px golden 光栅外框一致。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算标题、提示、五个 Provider 行和四个详情行的固定高度。
///   2. 与 782px 精确逻辑预算及 820px Settings 面板上限比较。
///
/// 预期结果：
///   - 正断言：固定内容预算精确为 782px，光栅化后与 golden 行距一致。
///   - 负断言：不得超过 820px 并把 Grok 推出面板。
#[test]
fn canonical_expanded_layout_keeps_grok_inside_golden_panel() {
    let height = canonical_expanded_content_height_for(&[
        OrchestrationProvider::Codex,
        OrchestrationProvider::Claude,
        OrchestrationProvider::Cursor,
        OrchestrationProvider::OpenCode,
        OrchestrationProvider::Grok,
    ]);

    assert_eq!(
        height, 782.0,
        "Orchestration 展开态固定内容应为 782px，保留 38px 逻辑余量"
    );
    assert!(
        height <= 820.0,
        "Orchestration 固定内容不得超过 820px Settings 逻辑面板高度"
    );
    assert_ne!(
        height, 800.0,
        "固定内容不得退化为 800px 临界预算并挤压底部安全余量"
    );
}

/// 修复状态语义色：Needs repair 必须保持 warning orange，不能跟随主题主强调色变蓝。
///
/// 数据构造（含关键数值的推导过程）：
///   canonical warning = glass-dark accent_orange = 0xff7932ff
///   SapphireNight primary accent = 0x54a2ffff
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 获取 Orchestration 的 warning status token。
///   2. 与 canonical orange 和 SapphireNight blue 分别比较。
///
/// 预期结果：
///   - 正断言：Needs repair 使用现有 glass-dark orange token。
///   - 负断言：warning 不得退化为 SapphireNight 的蓝色主强调色。
#[test]
fn needs_repair_uses_semantic_orange_instead_of_primary_accent() {
    let warning = warning_status_color();

    assert_eq!(
        warning,
        Theme::glass_dark().accent_orange,
        "Needs repair 必须复用 canonical Settings warning orange token"
    );
    assert_ne!(
        warning,
        Theme::sapphire_night().accent_blue,
        "Needs repair 不得与 Repair 按钮共用蓝色主强调色"
    );
}

/// Settings 标题栏固定高度：119px header 不得在纵向 flex 中被压回旧尺寸
///
/// 数据构造（含关键数值的推导过程）：
///   canonical header = 119px。
///   old header = 118px；修正量 = 119 - 118 = 1px。
///   0.854 display scale 下 header = 119 × 0.854 ≈ 102px，与 golden 分隔线一致。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Settings content 的生产渲染源码。
///   2. 定位 `min_h(119)` 标题栏约束。
///   3. 验证该约束紧接 `flex_none()`，禁止父级在内容密集时收缩标题栏。
///
/// 预期结果：
///   - 正断言：119px 与不可收缩约束同时存在。
///   - 负断言：不得保留 118px 标题栏并让分隔线上移 1px。
#[test]
fn settings_content_header_preserves_canonical_height_under_flex_pressure() {
    let source = include_str!("../../src/settings/content.rs");

    assert!(
        source.contains(".min_h(px(119.0))\n        .flex_none()"),
        "Settings 119px header must be flex-none so dense Orchestration content cannot shrink it"
    );
    assert!(
        !source.contains(".min_h(px(118.0))\n        .flex_none()"),
        "Settings header must not retain the one-pixel-short 118px layout"
    );
}

/// 场景：Orchestration 与所有 Settings 页面共用紧凑字级，不再按旧 golden 自造近似值。
/// 数据构造：行标题 = 12px；说明/状态/组件/提示 = 11px，来自统一 RowTitle/Helper/Label。
/// 执行过程：读取生产代码使用的五类文字角色，比较字号。
/// 预期结果：正断言为 12/11/11/11/11；负断言不保留 18.5/15.5/16/16/14 的旧字号。
#[test]
fn provider_typography_uses_shared_settings_scale() {
    let actual = canonical_provider_typography();
    assert_eq!(
        actual,
        (12.0, 11.0, 11.0, 11.0, 11.0),
        "Provider 排版使用共享 Settings 字级"
    );
    assert_ne!(
        actual,
        (18.5, 15.5, 16.0, 16.0, 14.0),
        "不得恢复旧的独立 Provider 排版"
    );
}

/// Canonical Provider 横向列：状态左移、按钮右移后必须与 golden 锚点一致。
///
/// 数据构造（含关键数值的推导过程）：
///   row gap = 6px。
///   provider text gap = 0px（字体自身行高已提供间隔）。
///   status width = 259px。
///   action width = 112px。
///   section right inset = 7px；row outdent = 23px。
///   component label column = 152px。
///   effective row extension = 23 - 7 = 16px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Provider 行间距、状态列宽、按钮宽度和左右出血量。
///   2. 计算行相对详情容器的右向扩展量。
///
/// 预期结果：
///   - 正断言：列 token 精确为 6/0/259/112/7/23/152px，扩展量为 16px。
///   - 负断言：不得恢复 18px gap、180px 状态列和 116px 按钮的向内挤压布局。
#[test]
fn provider_columns_match_canonical_golden_anchors() {
    let (
        gap,
        text_gap,
        status_width,
        action_width,
        section_inset,
        row_outdent,
        component_label_width,
    ) = canonical_provider_row_layout();
    let extension = row_outdent - section_inset;

    assert_eq!(gap, 6.0, "Provider 行列间距必须为 6px");
    assert_eq!(
        text_gap, 0.0,
        "Provider 标题与说明不得叠加字体行高之外的间距"
    );
    assert_eq!(status_width, 259.0, "Provider 状态列必须为 259px");
    assert_eq!(action_width, 112.0, "Provider 操作按钮必须为 112px");
    assert_eq!(section_inset, 7.0, "Provider 分隔线右侧必须内收 7px");
    assert_eq!(row_outdent, 23.0, "Provider 操作区必须向右出血 23px");
    assert_eq!(
        component_label_width, 152.0,
        "组件状态必须锚定在 152px 标签列之后"
    );
    assert_eq!(
        extension, 16.0,
        "Provider 行相对滚动内容必须净向右扩展 16px"
    );
    assert_ne!(
        (gap, status_width, action_width),
        (18.0, 180.0, 116.0),
        "Provider 横向布局不得恢复状态与按钮同时向中央挤压的旧 token"
    );
}

/// Canonical Provider 纵向节奏：首三行必须在 Cursor 详情顶边前抵消累计偏移。
///
/// 数据构造（含关键数值的推导过程）：
///   Codex = 92px；Claude = 84px；expanded Cursor = 80px。
///   OpenCode = 85px；Grok = 85px。
///   total = 92 + 84 + 80 + 85 + 85 = 426px。
///   相对等高方案 = 5 × 85 = 425px，净修正 = 426 - 425 = 1px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取五个 Provider 在 canonical 展开态的行高。
///   2. 逐一比较首三行和末两行，再计算总高。
///
/// 预期结果：
///   - 正断言：行高精确为 92/84/80/85/85px，总高 426px。
///   - 负断言：不得回退为全部 85px 并在 Codex/Claude/Cursor 中累积 4–6px 双边。
#[test]
fn provider_rows_follow_canonical_golden_vertical_rhythm() {
    let (codex, claude, cursor, opencode, grok) = canonical_provider_row_rhythm();
    let total = codex + claude + cursor + opencode + grok;

    assert_eq!(codex, 92.0, "Codex 行高必须为 92px");
    assert_eq!(claude, 84.0, "Claude Code 行高必须为 84px");
    assert_eq!(cursor, 80.0, "展开 Cursor 主行高必须为 80px");
    assert_eq!(opencode, 85.0, "OpenCode 行高必须保持 85px");
    assert_eq!(grok, 85.0, "Grok Build 行高必须保持 85px");
    assert_eq!(total, 426.0, "五个 Provider 行总高必须为 426px");
    assert_ne!(
        (codex, claude, cursor),
        (85.0, 85.0, 85.0),
        "首三行不得退回等高并重现累计偏移"
    );
}

/// Canonical 文字对齐：副标题、组件内边距与状态字重必须消除最后可见双边。
///
/// 数据构造（含关键数值的推导过程）：
///   subtitle = 13px，统一由 Subtitle 角色控制。
///   component left padding = 20px；旧值 18px，位置补偿 = 20 - 18 = 2px。
///   provider/component status weight = MEDIUM，原型中语义色不以细字重呈现。
///   Cursor title raster offset = 1px；修正前上缘偏差 3px，修正后 = 3 - 1 = 2px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Settings header 与 Orchestration 生产渲染源码。
///   2. 检查副标题字号、组件左内边距和两处状态字重。
///
/// 预期结果：
///   - 正断言：Subtitle 角色、20px 和两处 Label 角色约束都存在。
///   - 正断言：Cursor 标题存在 1px 光栅补偿。
///   - 负断言：不得保留 15.5px 副标题或对称 18px 组件内边距。
#[test]
fn canonical_text_alignment_removes_final_visible_double_edges() {
    let content_source = include_str!("../../src/settings/content.rs");
    let orchestration_source = include_str!("../../src/settings/orchestration.rs");
    let medium_status_count = orchestration_source
        .matches(".settings_font(TextRole::Label)")
        .count();
    let symmetric_eighteen_padding_count = orchestration_source.matches(".px(px(18.0))").count();

    assert!(
        content_source.contains(".settings_text(TextRole::Subtitle, theme)"),
        "Settings 副标题必须使用共享 Subtitle 角色"
    );
    assert!(
        orchestration_source.contains(".pl(px(20.0))\n        .pr(px(18.0))"),
        "组件内容必须左缩进 20px、右缩进 18px"
    );
    assert_eq!(
        medium_status_count, 2,
        "Provider 与组件状态必须各有一处 MEDIUM 字重"
    );
    assert!(
        orchestration_source.contains(
            ".when(provider == OrchestrationProvider::Cursor, |title| {\n                            title.relative().top(px(1.0))"
        ),
        "Cursor Agent CLI 标题必须下移 1px 将字形上缘偏差收敛到 2px"
    );
    assert!(
        !content_source.contains(".text_size(px(15.5))"),
        "Settings 副标题不得保留宽度短约 7px 的 15.5px 字号"
    );
    assert_eq!(
        symmetric_eighteen_padding_count, 1,
        "仅错误详情可保留对称 18px，组件行不得继续左偏 2px"
    );
}

/// 场景：ALL 扩到 9 个 provider 后总高度与行节奏正确，且页面可滚动不会溢出。
/// 数据构造：旧 5 项 = 782px（见上）；新增 Kimi/ZCode/Pi/Hermes 各 85px（默认行高）
/// 共 4 × 85 = 340px；total = 782 + 340 = 1122px，大于 820px 面板，因此必须依赖滚动。
/// 过程：对完整 ALL 计算高度；读取 content.rs 源码确认 Orchestration 内容容器 overflow_y_scroll。
/// 预期：正断言高度 1122 且新行均为 85；content.rs 含 overflow_y_scroll；
/// 负断言：1122 不得被误当作需放进 820 面板的固定高度（超出时由滚动承载）。
#[test]
fn nine_provider_layout_is_scrollable_with_85px_rows() {
    let height = canonical_expanded_content_height_for(&OrchestrationProvider::ALL);

    assert_eq!(height, 1122.0, "9 个 provider 总高应为 782 + 4×85");
    for provider in [
        OrchestrationProvider::Kimi,
        OrchestrationProvider::ZCode,
        OrchestrationProvider::Pi,
        OrchestrationProvider::Hermes,
    ] {
        assert_eq!(
            super::provider_row_min_height(provider, false),
            85.0,
            "{provider:?} 行高应为默认 85px"
        );
    }
    assert!(height > 820.0, "总高超过面板，必须依赖滚动");
    let content = include_str!("../../src/settings/content.rs");
    assert!(
        content.contains("overflow_y_scroll()"),
        "Settings 内容区必须可滚动，9 个 provider 才不会溢出"
    );
    let fixed = content
        .split("let fixed_height_section = matches!(")
        .nth(1)
        .and_then(|rest| rest.split(");").next())
        .expect("应找到固定高度分区列表");
    assert!(
        !fixed.contains("SettingsSection::Orchestration"),
        "Orchestration 不得是固定高度（不可滚动）分区"
    );
}
