use super::*;

/// 场景：点击右上角状态胶囊切换左栏，悬浮时只聚焦悬浮面板。
///
/// 数据构造：三种起点——左栏收起、左栏已开且停在概览、左栏已开且停在对话。
///   悬浮由点击参数传入，和抽屉状态里的 overview_floating 分开，避免点击本身移动面板。
/// 执行过程：分别用指针和键盘触发，再对悬浮状态触发一次。
/// 预期结果：收起时先展开并进入概览；已在概览时回到对话且左栏仍开着；
///   键盘进入概览时要求焦点落到第一行；悬浮时不改左栏，效果是聚焦悬浮面板。
#[test]
fn status_capsule_click_opens_overview_returns_to_chat_or_focuses_floating() {
    let collapsed = DesktopDrawerState::default();
    let (opened, effect) = route_status_capsule(
        collapsed,
        StatusCapsuleClick {
            floating: false,
            from_keyboard: false,
        },
    );
    assert!(opened.left_open, "左栏收起时点击应先展开");
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Overview,
        "展开后应显示运行概览"
    );
    assert_eq!(
        effect,
        StatusCapsuleEffect::ShowOverview {
            focus_first_row: false
        },
        "指针点击展开概览时不把焦点送进第一行"
    );
    assert!(!opened.right_open, "点击胶囊不能打开右栏");

    let (back, back_effect) = route_status_capsule(
        opened,
        StatusCapsuleClick {
            floating: false,
            from_keyboard: true,
        },
    );
    assert_eq!(
        back.left_view,
        DesktopLeftView::Chat,
        "已在概览时再点应回到对话"
    );
    assert!(back.left_open, "回到对话时不应收起已经展开的左栏");
    assert_eq!(
        back_effect,
        StatusCapsuleEffect::ReturnToChat,
        "已显示概览时的点击是回对话，不是再聚焦"
    );
    assert_ne!(
        back.left_view,
        DesktopLeftView::Overview,
        "回对话后不能仍停在概览"
    );

    let chatting = DesktopDrawerState {
        left_open: true,
        right_open: true,
        left_view: DesktopLeftView::Chat,
        ..DesktopDrawerState::default()
    };
    let (overview, keyboard) = route_status_capsule(
        chatting,
        StatusCapsuleClick {
            floating: false,
            from_keyboard: true,
        },
    );
    assert_eq!(
        keyboard,
        StatusCapsuleEffect::ShowOverview {
            focus_first_row: true
        },
        "键盘触发进入概览时焦点应进入第一行"
    );
    assert!(overview.right_open, "切换左栏视图不能收起右栏");

    let (unchanged, floating) = route_status_capsule(
        chatting,
        StatusCapsuleClick {
            floating: true,
            from_keyboard: true,
        },
    );
    assert_eq!(
        floating,
        StatusCapsuleEffect::FocusFloating,
        "概览已悬浮时点击应聚焦悬浮面板"
    );
    assert_eq!(
        unchanged, chatting,
        "聚焦悬浮面板时不能改动抽屉开合或左栏视图"
    );
    assert_ne!(
        floating,
        StatusCapsuleEffect::ShowOverview {
            focus_first_row: true
        },
        "悬浮时不能再把左栏切去概览"
    );
}
