use super::*;
use gpui::{point, px, Bounds, Size};

fn approx(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.001
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

/// IA-A2：Agent 资源在最后稳定帧同时保留中性人物轮廓和紫色 X 发饰。
///
/// 数据构造（含关键数值推导）：
///   帧数为 32，最后稳定帧 index = 32 - 1 = 31。
///   100×41 网格共 4100 格，需至少各有一个 normal/accent 非空格。
///
/// 执行过程：
///   1. 读取 Agent 第 31 帧。
///   2. 遍历全部格子，统计中性和 accent 的可见字符数。
///
/// 预期结果：
///   - 正断言：中性层包含人物轮廓字符。
///   - 正断言：accent 层包含从白色 X 发饰派生的字符。
///   - 负断言：Agent 最终帧不得退化为纯空格或纯单色。
#[test]
fn test_agent_intro_asset_stable_frame_contains_neutral_portrait_and_accent_x() {
    let frame = intro_frame(
        InitializationAnimationKind::Agent,
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

/// IA-A3：100×41 ASCII 画面按内容区等比居中，不借用或覆盖标题栏坐标。
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
