use super::*;

/// TextRange normalize：反向拖动时 anchor/head 应归一为从小到大的范围。
///
/// 数据构造（含关键数值推导）：
///   anchor = row 3, col 8（鼠标先按在较后的 grid 坐标）
///   head   = row 1, col 2（鼠标再拖到较前的 grid 坐标）
///   视觉选择应覆盖 row 1 col 2 到 row 3 col 8
///
/// 执行过程：
///   1. 构造 TextRange(anchor=(3,8), head=(1,2))
///   2. 调用 normalized()
///   3. 读取 start/end
///
/// 预期结果：
///   - 正断言：start 是 (1,2)，end 是 (3,8)
///   - 负断言：start 不应保留反向拖动的 anchor=(3,8)
#[test]
fn test_text_range_normalizes_reverse_drag() {
    let range = TextRange::new(TextPoint::new(3, 8), TextPoint::new(1, 2));

    let normalized = range.normalized();

    assert_eq!(
        normalized.start,
        TextPoint::new(1, 2),
        "反向拖动后 start 应归一为较小的文本坐标 (1,2)"
    );
    assert_eq!(
        normalized.end,
        TextPoint::new(3, 8),
        "反向拖动后 end 应归一为较大的文本坐标 (3,8)"
    );
    assert_ne!(
        normalized.start,
        TextPoint::new(3, 8),
        "start 不应保留鼠标按下时的 anchor，否则复制范围会反向"
    );
}

/// TextRange empty：anchor/head 相同表示没有可复制选区。
///
/// 数据构造（含关键数值推导）：
///   anchor = row 2, col 4
///   head   = row 2, col 4
///   两点相同，因此选区宽度 = 4 - 4 = 0
///
/// 执行过程：
///   1. 构造 TextRange(anchor=(2,4), head=(2,4))
///   2. 调用 is_empty()
///
/// 预期结果：
///   - 正断言：is_empty() 为 true
///   - 负断言：相同坐标不应被当作可复制选区
#[test]
fn test_text_range_reports_empty_when_anchor_equals_head() {
    let range = TextRange::new(TextPoint::new(2, 4), TextPoint::new(2, 4));

    assert!(
        range.is_empty(),
        "anchor/head 坐标相同时应视为空选区，Cmd+C 不应写剪贴板"
    );
    assert!(
        range.is_empty(),
        "零宽选区不应被当作可复制内容，否则单击正文会误复制"
    );
}

/// SelectionSpan 展开：跨三行选择应生成每行一个连续 span。
///
/// 数据构造（含关键数值推导）：
///   range = row 1 col 3 → row 3 col 2
///   terminal cols = 10
///   第一行 span = 3..10，中间行 span = 0..10，末行 span = 0..2
///
/// 执行过程：
///   1. 构造跨三行 TextRange
///   2. 调用 spans_for_columns(cols=10)
///   3. 检查每行 span
///
/// 预期结果：
///   - 正断言：三行 span 均存在且列范围正确
///   - 负断言：不应生成第四行或把第一行起点误设为 0
#[test]
fn test_text_range_expands_to_one_span_per_selected_line() {
    let range = TextRange::new(TextPoint::new(1, 3), TextPoint::new(3, 2));

    let spans = range.spans_for_columns(10);

    assert_eq!(
        spans,
        vec![
            SelectionSpan::new(1, 3, 10),
            SelectionSpan::new(2, 0, 10),
            SelectionSpan::new(3, 0, 2),
        ],
        "跨三行选区应展开为每行一个连续 span，首尾行保留各自列边界"
    );
    assert_eq!(
        spans.len(),
        3,
        "row 1..3 的闭区间选择只应生成 3 个 span，不能多画额外高亮行"
    );
    assert_ne!(
        spans[0],
        SelectionSpan::new(1, 0, 10),
        "第一行起点应保持 start_col=3，不能扩展到整行开头"
    );
}
