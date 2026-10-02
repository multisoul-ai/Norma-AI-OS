use super::*;
use crate::{AvailableSpace, TestAppContext, TextStyleRefinement, div, point, px, size};

/// 单行标题在多次布局测量中改变可用宽度时，省略号必须随宽度重算。
///
/// 数据构造：40 个中文字符（“工作空间”×10，共 120 UTF-8 bytes）；20px 字号，
///   初始 2000px > 40×20px 的文本宽度，窄列 100px 仅容纳约 5 字，最终恢复 2000px。
/// 执行过程：同一 TextLayout 测量宽列 → 窄列 → 再次宽列，覆盖 flex 的重复测量。
/// 预期结果：窄列以省略号结尾而非整段溢出；恢复宽列后文字完整且不残留省略号，run 长度不能沿用裁剪值。
#[test]
fn nowrap_ellipsis_tracks_measurement_width() {
    let mut test = TestAppContext::single();
    let cx = test.add_empty_window();
    let text = "工作空间".repeat(10);
    let layout = TextLayout::default();
    cx.draw(
        point(px(0.), px(0.)),
        size(px(2000.), px(40.)),
        |window, cx| {
            let id = window.with_text_style(
                Some(TextStyleRefinement {
                    font_size: Some(px(20.).into()),
                    white_space: Some(WhiteSpace::Nowrap),
                    text_overflow: Some(TextOverflow::Truncate("…".into())),
                    ..Default::default()
                }),
                |window| layout.layout(text.clone().into(), None, window, cx),
            );
            let space = |width| {
                size(
                    AvailableSpace::Definite(px(width)),
                    AvailableSpace::MaxContent,
                )
            };
            window.compute_layout(id, space(2000.), cx);
            assert_eq!(layout.text(), text, "宽列应显示完整工作空间名称");
            window.compute_layout(id, space(100.), cx);
            assert!(layout.text().ends_with('…'), "窄列必须重新测量并绘制省略号");
            assert_ne!(layout.text(), text, "不能复用宽列布局而仅靠父容器裁切文字");
            window.compute_layout(id, space(2000.), cx);
            assert_eq!(layout.text(), text, "再次变宽应恢复原始文本和完整样式 runs");
            assert!(!layout.text().ends_with('…'), "宽度恢复后不能保留旧省略号");
            div()
        },
    );
}
