use super::*;
use gpui::{point, px, Bounds, Size};

fn approx(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.001
}

fn visible_display_ratio(kind: InitializationAnimationKind, frame_index: usize) -> Option<f32> {
    let frame = intro_frame(kind, frame_index);
    let mut minimum_column = INTRO_ANIMATION_COLUMNS;
    let mut maximum_column = 0usize;
    let mut minimum_row = INTRO_ANIMATION_ROWS;
    let mut maximum_row = 0usize;
    let mut has_visible_cell = false;

    for row in 0..INTRO_ANIMATION_ROWS {
        for column in 0..INTRO_ANIMATION_COLUMNS {
            if frame.cell(row, column).glyph == ' ' {
                continue;
            }
            minimum_column = minimum_column.min(column);
            maximum_column = maximum_column.max(column);
            minimum_row = minimum_row.min(row);
            maximum_row = maximum_row.max(row);
            has_visible_cell = true;
        }
    }

    has_visible_cell.then(|| {
        let visible_columns = maximum_column - minimum_column + 1;
        let visible_rows = maximum_row - minimum_row + 1;
        visible_columns as f32 * 8.0 / (visible_rows as f32 * 16.0)
    })
}

/// IA-A1：Terminal 资源保留 Ghostty 官方第 17 帧的原始蓝色边框字符。
///
/// 数据构造（含关键数值推导）：
///   使用 0-based 的初始化帧 0，对应上游 `frame_017.txt`。
///   第 2 行第 38 列是首个蓝色 `+`；上游 HTML 以 `<span class="b">` 标记它。
///
/// 执行过程：
///   1. 读取 Terminal 第 0 帧。
///   2. 取得 row=2、column=38 的紧凑单元格。
///
/// 预期结果：
///   - 正断言：字符仍为 `+` 且保留 accent 标记。
///   - 负断言：该格不应被转成空格或 Norma 紫色资源。
#[test]
fn test_terminal_intro_asset_keeps_upstream_frame_17_blue_border_cell() {
    let frame = intro_frame(InitializationAnimationKind::Terminal, 0);
    let cell = frame.cell(2, 38);

    assert_eq!(
        cell.glyph, '+',
        "上游 frame_017 的蓝色边框首字符应保留为 '+'"
    );
    assert!(cell.accent, "上游 <span class=\"b\"> 应编码为 accent 标记");
    assert_ne!(
        cell.glyph, ' ',
        "Ghostty 边框字符不应在紧凑资源预处理时丢失为普通空格"
    );
}

/// IA-A2：OpenCode 继续使用现有 Norma 资源，最后稳定帧保留中性人物轮廓和紫色 X 发饰。
///
/// 数据构造（含关键数值推导）：
///   帧数为 32，最后稳定帧 index = 32 - 1 = 31。
///   100×41 网格共 4100 格，需至少各有一个 normal/accent 非空格。
///
/// 执行过程：
///   1. 读取 OpenCode 第 31 帧。
///   2. 遍历全部格子，统计中性和 accent 的可见字符数。
///
/// 预期结果：
///   - 正断言：中性层包含人物轮廓字符。
///   - 正断言：accent 层包含从白色 X 发饰派生的字符。
///   - 负断言：Agent 最终帧不得退化为纯空格或纯单色。
#[test]
fn test_agent_intro_asset_stable_frame_contains_neutral_portrait_and_accent_x() {
    let frame = intro_frame(
        InitializationAnimationKind::OpenCode,
        super::super::INTRO_ANIMATION_FRAME_COUNT - 1,
    );
    let mut neutral_visible = 0usize;
    let mut accent_visible = 0usize;
    for row in 0..INTRO_ANIMATION_ROWS {
        for column in 0..INTRO_ANIMATION_COLUMNS {
            let cell = frame.cell(row, column);
            if cell.glyph == ' ' {
                continue;
            }
            if cell.accent {
                accent_visible += 1;
            } else {
                neutral_visible += 1;
            }
        }
    }

    assert!(
        neutral_visible > 0,
        "Norma ASCII 肖像应保留至少一个中性轮廓字符，实际为 {neutral_visible}"
    );
    assert!(
        accent_visible > 0,
        "Norma ASCII 肖像应保留至少一个 X 发饰 accent 字符，实际为 {accent_visible}"
    );
    assert_ne!(
        neutral_visible, 0,
        "Agent 最终帧不应退化为只含紫色 accent 的单色图形"
    );
    assert_ne!(
        accent_visible, 0,
        "Agent 最终帧不应退化为完全没有 X 发饰 accent 的纯中性图形"
    );
}

/// IA-A3：Codex 与 Claude Code 使用从各自品牌图标转化的独立双色 ASCII 帧。
///
/// 数据构造（含关键数值的推导过程）：
///   frame_index = 32 - 1 = 31（动画最后稳定帧）；每张图为 100×41 = 4100 个 ASCII 单元。
///   Codex 输入图包含蓝紫终端轮廓；Claude Code 输入图包含橙色机器人和黑色眼睛。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Codex、Claude Code 与 OpenCode 的第 31 帧，统计中性/强调字符数量。
///   2. 读取三个资源名，验证 Codex/Claude Code 是独立 ASCII 资产而非通用 Norma 资源。
///
/// 预期结果：
///   - 断言 A：Codex 与 Claude Code 各自都有中性与强调 ASCII 字符。
///   - 断言 B：Codex 与 Claude Code 使用不同资源名。
///   - 断言 C：两者均不应回退到 OpenCode 的 norma-agent-duotone 资源。
#[test]
fn test_brand_intro_assets_use_distinct_duotone_ascii_frames() {
    let final_frame = super::super::INTRO_ANIMATION_FRAME_COUNT - 1;
    let (codex_neutral, codex_accent) =
        super::super::intro_visible_cell_counts(InitializationAnimationKind::Codex, final_frame);
    let (claude_neutral, claude_accent) =
        super::super::intro_visible_cell_counts(InitializationAnimationKind::Claude, final_frame);
    let codex_asset = super::super::intro_asset_name(InitializationAnimationKind::Codex);
    let claude_asset = super::super::intro_asset_name(InitializationAnimationKind::Claude);

    assert!(
        codex_neutral > 0,
        "Codex ASCII 图标必须保留至少一个中性轮廓字符，实际为 {codex_neutral}"
    );
    assert!(
        codex_accent > 0,
        "Codex ASCII 图标必须保留至少一个蓝紫强调字符，实际为 {codex_accent}"
    );
    assert!(
        claude_neutral > 0,
        "Claude Code ASCII 图标必须保留至少一个黑色眼睛/轮廓字符，实际为 {claude_neutral}"
    );
    assert!(
        claude_accent > 0,
        "Claude Code ASCII 图标必须保留至少一个橙色强调字符，实际为 {claude_accent}"
    );
    assert_ne!(
        codex_asset, claude_asset,
        "Codex 与 Claude Code 必须映射到不同的 ASCII 资源，不能显示同一张图"
    );
    assert_eq!(
        codex_asset, "codex-brand-ascii",
        "Codex 必须使用从品牌图标生成的 codex-brand-ascii 紧凑帧资源"
    );
    assert_eq!(
        claude_asset, "claude-code-brand-ascii",
        "Claude Code 必须使用从品牌图标生成的 claude-code-brand-ascii 紧凑帧资源"
    );
    assert_ne!(
        codex_asset, "norma-agent-duotone",
        "Codex 不应回退到 OpenCode 使用的通用 Norma ASCII 肖像"
    );
    assert_ne!(
        claude_asset, "norma-agent-duotone",
        "Claude Code 不应回退到 OpenCode 使用的通用 Norma ASCII 肖像"
    );
}

/// IA-A4：Cursor 使用由黑色六边形箭头图标生成的独立双色 ASCII 初始化资源。
///
/// 数据构造（含关键数值的推导过程）：
///   最后稳定帧 index = 32 - 1 = 31；资源网格为 100×41 = 4100 个 ASCII 单元。
///   Cursor 图标由中性六边形轮廓和 accent 箭头两层组成，均至少应有 1 个可见字符。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Cursor 稳定帧并统计中性/强调字符数。
///   2. 读取 Cursor 的资源名，与通用 OpenCode 资源名比较。
///
/// 预期结果：
///   - 断言 A：中性六边形和 accent 箭头都保留为可见字符。
///   - 断言 B：Cursor 精确使用 `cursor-brand-ascii` 资源。
///   - 断言 C：Cursor 不得回退到通用 Norma Agent 或 Codex 资源。
#[test]
fn test_cursor_intro_asset_uses_distinct_hexagon_arrow_frames() {
    let final_frame = super::super::INTRO_ANIMATION_FRAME_COUNT - 1;
    let (neutral, accent) =
        super::super::intro_visible_cell_counts(InitializationAnimationKind::Cursor, final_frame);
    let asset = super::super::intro_asset_name(InitializationAnimationKind::Cursor);

    assert!(
        neutral > 0,
        "Cursor 稳定帧必须保留至少一个六边形中性轮廓字符，实际为 {neutral}"
    );
    assert!(
        accent > 0,
        "Cursor 稳定帧必须保留至少一个箭头 accent 字符，实际为 {accent}"
    );
    assert_eq!(
        asset, "cursor-brand-ascii",
        "Cursor 必须使用由指定图标生成的 cursor-brand-ascii 紧凑帧资源"
    );
    assert_ne!(
        asset, "norma-agent-duotone",
        "Cursor 不应回退到 OpenCode 使用的通用 Norma ASCII 肖像"
    );
    assert_ne!(
        asset, "codex-brand-ascii",
        "Cursor 不应错误复用 Codex 的品牌初始化资源"
    );
}

/// IA-A5：Codex 与 Claude Code 的 ASCII 品牌图按终端字符的真实 8×16 像素比例等比显示。
///
/// 数据构造（含关键数值的推导过程）：
///   最后稳定帧 index = 32 - 1 = 31；终端字符单元物理尺寸为 8×16px。
///   Codex 源图的可见终端标志接近 1:1，允许离散 ASCII 采样误差后显示比例落在 [0.85, 1.15]。
///   Claude Code 源图机器人约为 495:404 = 1.225，允许轮廓采样误差后显示比例落在 [0.95, 1.45]。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取两个品牌在稳定帧中的所有可见 ASCII 字符。
///   2. 求其最小包围盒，并以 columns×8 / rows×16 计算屏幕显示比例。
///   3. 将结果与各自源图的允许比例区间比较。
///
/// 预期结果：
///   - 断言 A：Codex 可见区域存在，且不再被纵向拉伸。
///   - 断言 B：Claude Code 可见区域存在，且不再被纵向拉伸。
///   - 断言 C：两者均不得退化为无可见字符的空白帧。
#[test]
fn test_brand_ascii_frames_preserve_source_display_aspect_ratio() {
    let final_frame = super::super::INTRO_ANIMATION_FRAME_COUNT - 1;
    let codex_ratio = visible_display_ratio(InitializationAnimationKind::Codex, final_frame);
    let claude_ratio = visible_display_ratio(InitializationAnimationKind::Claude, final_frame);

    assert!(
        codex_ratio.is_some(),
        "Codex 稳定帧必须有可见 ASCII 字符，不能退化为空白画面"
    );
    assert!(
        claude_ratio.is_some(),
        "Claude Code 稳定帧必须有可见 ASCII 字符，不能退化为空白画面"
    );

    let codex_ratio = codex_ratio.expect("上方断言已确认 Codex 稳定帧含有可见字符");
    let claude_ratio = claude_ratio.expect("上方断言已确认 Claude Code 稳定帧含有可见字符");

    assert!(
        (0.85..=1.15).contains(&codex_ratio),
        "Codex 的显示比例应接近 1:1，实际为 {codex_ratio:.3}；偏小表示被纵向拉伸"
    );
    assert!(
        (0.95..=1.45).contains(&claude_ratio),
        "Claude Code 的显示比例应接近源图 1.225，实际为 {claude_ratio:.3}；偏小表示被纵向拉伸"
    );
    assert_ne!(
        codex_ratio, 0.0,
        "Codex 的可见包围盒比例不能为 0，否则说明没有宽度或高度"
    );
    assert_ne!(
        claude_ratio, 0.0,
        "Claude Code 的可见包围盒比例不能为 0，否则说明没有宽度或高度"
    );
}

/// IA-A5：100×41 ASCII 画面按内容区等比居中，不借用或覆盖标题栏坐标。
///
/// 数据构造（含关键数值推导）：
///   - 内容区 bounds = 800×400px（不包含 card 标题栏）。
///   - 逻辑画面尺寸 = 100×8 by 41×16 = 800×656px。
///   - 高度约束 scale = 400/656 = 0.609756；画面宽 = 800×0.609756 = 487.8048px。
///   - 居中 x = (800-487.8048)/2 = 156.0976px；y = 0px。
///
/// 执行过程：
///   1. 以内容区 bounds 计算 intro metrics。
///   2. 读取首个 glyph 的绘制原点、单元宽度和行高。
///
/// 预期结果：
///   - 正断言：画面在内容区水平居中，且恰好填满内容区高度。
///   - 正断言：等比缩放后 cell/line 的比例仍为 8:16。
///   - 负断言：原点不应为标题栏坐标或左侧 0px，避免动画跑进标题栏/贴边。
#[test]
fn test_intro_metrics_center_full_grid_inside_card_content_area() {
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: Size {
            width: px(800.0),
            height: px(400.0),
        },
    };
    let metrics = centered_metrics(bounds);

    assert!(
        approx(f32::from(metrics.origin.x), 156.0976),
        "800×400 内容区中的等比 100×41 画面应水平居中于 x≈156.098，实际为 {}",
        f32::from(metrics.origin.x)
    );
    assert!(
        approx(f32::from(metrics.origin.y), 0.0),
        "高度受限时动画应恰好填满内容区高度，y 应为 0，实际为 {}",
        f32::from(metrics.origin.y)
    );
    assert!(
        approx(
            f32::from(metrics.line_height) / f32::from(metrics.cell_width),
            2.0
        ),
        "等比缩放后 line_height/cell_width 应保持 16/8=2，实际为 {}",
        f32::from(metrics.line_height) / f32::from(metrics.cell_width)
    );
    assert_ne!(
        f32::from(metrics.origin.x),
        0.0,
        "动画不应贴在内容区左边；这会表明未做居中布局或错误使用标题栏坐标"
    );
}
