use super::embedded_presentation_layout;
use crate::canvas::layout_strategy::CanvasLayoutStrategy;
use canvas::{CanvasNodeKind, CanvasRect, Document, TermSpec};

/// 场景：Cmd+M 主从布局下打开右抽屉（画布变窄），master 不得被拍平成与其他卡片等大的单列。
///
/// 数据构造：
///   三张终端 a / master / b，各 350×280；
///   抽屉后嵌入画布宽 472px（≤ 650px，触发窄画布分支），高 710px。
///   主从布局：master 宽 = 内容宽 × 2/3，detail 占剩余列；
///   detail 在窄画布中会变小，但必须仍落在画布内且宽高 > 0。
///
/// 执行过程：
///   1. 传入 master=Some(master) 计算展示布局 → 走主从布局
///   2. 传入 master=None 作对照 → 走等宽单列堆叠
///
/// 预期结果：
///   - master 面积严格大于 detail a：主从层级在抽屉打开后仍保留
///   - master 面积严格大于 detail b：同上
///   - 对照组 a 与 master 宽度相同：证明上面的断言能区分两种布局
#[test]
fn narrow_presentation_keeps_master_layout() {
    let mut document = Document::new();
    let a = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(0.0, 0.0, 350.0, 280.0),
    );
    let master = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(400.0, 0.0, 350.0, 280.0),
    );
    let b = document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(800.0, 0.0, 350.0, 280.0),
    );
    let area = CanvasRect::new(0.0, 0.0, 472.0, 710.0);
    let strategy = CanvasLayoutStrategy::default();
    let size = |r: &CanvasRect| r.size.x * r.size.y;

    let with_master =
        embedded_presentation_layout(document.nodes(), area, 1.0, strategy, Some(master));
    let m = size(&with_master[&master]);
    assert!(
        m > size(&with_master[&a]),
        "master 应大于 detail a，否则主从布局被拍平"
    );
    assert!(
        m > size(&with_master[&b]),
        "master 应大于 detail b，否则主从布局被拍平"
    );

    for (name, id) in [("a", a), ("b", b), ("master", master)] {
        let r = with_master[&id];
        assert!(r.size.x > 0.0 && r.size.y > 0.0, "{name} 展示尺寸必须为正");
        assert!(
            r.origin.x >= 0.0 && r.origin.x + r.size.x <= area.size.x,
            "{name} 不得超出 472px 抽屉画布"
        );
    }

    let tiled = embedded_presentation_layout(document.nodes(), area, 1.0, strategy, None);
    assert_eq!(
        tiled[&a].size.x, tiled[&master].size.x,
        "对照：无 master 时窄画布应为等宽单列"
    );
}
