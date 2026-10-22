use super::{CardTarget, DragEvent, DragOutcome, DragTargets, DropTarget, IdeaDrag, Rect, RowSlot};

fn card(area: &str, node: u64, x: f32, y: f32) -> CardTarget {
    CardTarget {
        area_id: area.into(),
        node_id: node,
        rect: Rect {
            x,
            y,
            w: 200.0,
            h: 100.0,
        },
    }
}

/// 画布 400..1200 × 60..800；卡片 A(500,100)、B(800,100)（各 200×100）；新建区 (600,700) 220×42。
fn targets() -> DragTargets {
    DragTargets {
        cards: vec![card("a1", 7, 500.0, 100.0), card("a1", 8, 800.0, 100.0)],
        new_zone: Rect {
            x: 600.0,
            y: 700.0,
            w: 220.0,
            h: 42.0,
        },
        canvas: Rect {
            x: 400.0,
            y: 60.0,
            w: 800.0,
            h: 740.0,
        },
        // 想法栏 0..360 × 100..500，三条待办各 52 高、间隔 4：i1 在 y=110，i2 在 166，i3 在 222。
        list: Rect {
            x: 0.0,
            y: 100.0,
            w: 360.0,
            h: 400.0,
        },
        rows: ["i1", "i2", "i3"]
            .iter()
            .enumerate()
            .map(|(index, id)| RowSlot {
                id: (*id).into(),
                rect: Rect {
                    x: 8.0,
                    y: 110.0 + 56.0 * index as f32,
                    w: 344.0,
                    h: 52.0,
                },
            })
            .collect(),
    }
}

fn drag() -> IdeaDrag {
    // 想法行左上角 (20,200)，按下点 (60,220) → 抓取偏移 (40,20)。
    IdeaDrag::new("i1".into(), (60.0, 220.0), (20.0, 200.0))
}

/// 起拖阈值：位移不足 5px 仍算点击，达到 5px（含）才开始拖拽。
///
/// 数据构造：按下点 (60,220)；先移到 (63,223)（位移 √18≈4.24）；再移到 (63,224)（位移 3-4-5 的斜边 = 5.0）。
/// 预期结果：
///   - 正断言：5.0 时返回 Started，并且幽灵预览左上角 = 当前位置 − 抓取偏移 = (63−40, 224−20) = (23,204)。
///   - 负断言：4.24 时返回 None，且未进入拖拽（松手即为 Click）。
#[test]
fn drag_starts_only_after_five_pixel_threshold() {
    let t = targets();
    let mut d = drag();
    assert_eq!(
        d.moved(63.0, 223.0, &t),
        DragEvent::None,
        "位移 4.24px 不应起拖"
    );
    assert!(!d.is_active(), "未过阈值不应处于拖拽中");
    assert_eq!(
        d.moved(63.0, 224.0, &t),
        DragEvent::Started,
        "位移恰好 5px 应起拖"
    );
    assert!(d.is_active(), "过阈值后应处于拖拽中");
    assert_eq!(
        d.ghost_origin(),
        (23.0, 204.0),
        "预览应保持抓取偏移：(63,224) − (40,20)"
    );
    let pending = drag();
    assert_eq!(
        pending.released(63.0, 223.0, &t),
        DragOutcome::Click,
        "未过阈值松手应当作点击"
    );
}

/// 悬停命中：卡片之间切换、离开、同卡内移动分别给出不同事件，供调用方只在变化时重绘。
///
/// 数据构造：先越过阈值；依次移到 A 内、A 内另一点、B 内、空白。
/// 预期结果：
///   - 正断言：进入 A → HoverChanged 且 hover=Existing(a1,7)；A 内再移动 → Moved；到 B → HoverChanged；到空白 → HoverChanged 且 hover=None。
///   - 负断言：同一卡片内移动不得报告 HoverChanged（否则每次鼠标移动都重绘）。
#[test]
fn hover_reports_changes_only_when_target_changes() {
    let t = targets();
    let mut d = drag();
    d.moved(70.0, 230.0, &t);
    assert_eq!(
        d.moved(550.0, 150.0, &t),
        DragEvent::HoverChanged,
        "进入 A 应报告变化"
    );
    assert_eq!(
        d.hover(),
        Some(&DropTarget::Existing {
            area_id: "a1".into(),
            node_id: 7
        }),
        "应悬停在 A"
    );
    assert_eq!(
        d.moved(560.0, 160.0, &t),
        DragEvent::Moved,
        "A 内移动只算 Moved"
    );
    assert_eq!(
        d.moved(850.0, 150.0, &t),
        DragEvent::HoverChanged,
        "切到 B 应报告变化"
    );
    assert_eq!(
        d.moved(1000.0, 500.0, &t),
        DragEvent::HoverChanged,
        "离开所有目标应报告变化"
    );
    assert_eq!(d.hover(), None, "空白处不应有悬停目标");
}

/// 命中优先级与可见性：新建区压在卡片上时优先；画布外或被抽屉遮住的卡片不可命中。
///
/// 数据构造：新建区与卡片 C(650,690) 重叠；卡片 D(100,100) 在画布左侧外（x<400，被左抽屉盖住）。
/// 预期结果：
///   - 正断言：重叠处命中 NewAgent。
///   - 负断言：画布外的卡片 D 内部点（150,150）不命中任何目标。
#[test]
fn new_zone_wins_overlap_and_offcanvas_cards_are_not_hittable() {
    let mut t = targets();
    t.cards.push(card("a1", 9, 650.0, 690.0));
    t.cards.push(card("a1", 10, 1250.0, 100.0));
    assert_eq!(
        t.hit(700.0, 710.0),
        Some(DropTarget::NewAgent),
        "重叠处新建区优先"
    );
    assert_eq!(
        t.hit(1300.0, 150.0),
        None,
        "画布外的卡片不可命中，即使指针在卡片矩形内"
    );
}

/// 松手结果：落在目标上投递（带松手点），落空弹回，Esc 取消弹回。
///
/// 数据构造：越过阈值后分别在 B 上、空白处松手；另一次拖拽中取消；再有一次未过阈值取消。
/// 预期结果：
///   - 正断言：B 上松手 → Deliver{Existing(a1,8), at=(900,150), from=按下点(60,220)}（流星从想法行出发，不是从松手点）；空白松手 → Return；活动中取消 → Return。
///   - 负断言：未过阈值取消只是 Click，不触发弹回动画。
#[test]
fn release_delivers_on_target_otherwise_returns() {
    let t = targets();
    let mut on_card = drag();
    on_card.moved(100.0, 260.0, &t);
    assert_eq!(
        on_card.released(900.0, 150.0, &t),
        DragOutcome::Deliver {
            target: DropTarget::Existing {
                area_id: "a1".into(),
                node_id: 8
            },
            at: (900.0, 150.0),
            from: (60.0, 220.0)
        },
        "在 B 上松手应投递并带上松手点"
    );
    let mut blank = drag();
    blank.moved(100.0, 260.0, &t);
    assert_eq!(
        blank.released(1000.0, 500.0, &t),
        DragOutcome::Return,
        "落空应弹回"
    );
    let mut cancelled = drag();
    cancelled.moved(100.0, 260.0, &t);
    assert_eq!(cancelled.cancel(), DragOutcome::Return, "拖拽中取消应弹回");
    assert_eq!(
        drag().cancel(),
        DragOutcome::Click,
        "未过阈值取消不应触发弹回"
    );
}

/// 栏内拖动是排序：指针在哪行上半部就插到该行之前，下半部落到下一行之前，最后一行下半部为末尾。
///
/// 数据构造：行中线 i1=136、i2=192、i3=248；指针 y 依次为 120、140、200、260。
/// 预期结果：
///   - 正断言：120 → 插到 i1 前；140 → 插到 i2 前（过了 i1 中线）；200 → 插到 i3 前；260 → 末尾（None）。
///   - 负断言：栏外下方（y=600）与画布卡片区都不产生排序；松手结果为 Reorder 而不是 Deliver。
#[test]
fn pointer_inside_the_list_reorders_by_row_midpoints() {
    let t = targets();
    let before = |y: f32| match t.hit(100.0, y) {
        Some(DropTarget::Reorder { before }) => before,
        other => panic!("y={y} 应是排序目标，实际 {other:?}"),
    };
    assert_eq!(
        before(120.0),
        Some("i1".to_owned()),
        "i1 上半部应插到 i1 前"
    );
    assert_eq!(
        before(140.0),
        Some("i2".to_owned()),
        "过了 i1 中线应插到 i2 前"
    );
    assert_eq!(
        before(200.0),
        Some("i3".to_owned()),
        "过了 i2 中线应插到 i3 前"
    );
    assert_eq!(before(260.0), None, "过了最后一行中线应排到末尾");
    assert_eq!(t.hit(100.0, 600.0), None, "栏外下方不是目标");
    assert_eq!(t.hit(900.0, 400.0), None, "画布空白处不是排序目标");
    let mut dragging = drag();
    dragging.moved(100.0, 260.0, &t);
    assert_eq!(
        dragging.released(100.0, 200.0, &t),
        DragOutcome::Reorder {
            before: Some("i3".to_owned())
        },
        "栏内松手应是 Reorder"
    );
}

/// 没有展开项目时只有想法栏：仍可在栏内排序，但没有任何派发目标（含窗口原点）。
///
/// 数据构造：用 `targets()` 的想法栏与行，清掉卡片、新建区与画布（`DragTargets::list_only`）。
/// 预期结果：
///   - 正断言：栏内 y=140 → 插到 i2 前。
///   - 负断言：窗口原点 (0,0)、画布位置、栏外都不是目标；松手栏外不产生任何投递。
#[test]
fn without_a_project_only_reordering_is_possible() {
    let full = targets();
    let t = DragTargets::list_only(full.list, full.rows.clone());
    assert_eq!(
        t.hit(100.0, 140.0),
        Some(DropTarget::Reorder {
            before: Some("i2".to_owned())
        })
    );
    assert_eq!(t.hit(0.0, 0.0), None, "窗口原点不是新建区");
    assert_eq!(t.hit(900.0, 400.0), None, "没有画布，也没有卡片");
    assert_eq!(t.hit(100.0, 600.0), None, "栏外不是目标");
}
