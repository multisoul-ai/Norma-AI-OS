use super::display_lines;

/// 场景：Worker 结果折行后超过摘要行数，必须判为过长以折叠并显示展开按钮。
/// 数据构造：线上出现过的结果——一段约 85 字（折 3 行）、空行、一句短话。
/// 预期结果：显示行数大于 3；单行短文本仍为 1 行。
#[test]
fn wrapped_paragraphs_count_as_multiple_display_lines() {
    let body = format!("{}\n\n用浏览器直接打开即可游玩。", "字".repeat(85));
    assert_eq!(
        display_lines(&body),
        3 + 1 + 1,
        "折行段 3 + 空行 1 + 短句 1"
    );
    assert_eq!(display_lines("ok"), 1, "短文本一行");
}
