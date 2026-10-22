use super::*;

/// 新开工作空间时先呈现完整画布，Norma 对话与右侧内容由用户按需展开。
///
/// 数据构造：使用未经过任何用户操作的抽屉默认状态。
/// 执行过程：读取初始开合状态，再触发一次左栏切换。
/// 预期结果：初始两侧均收起、左栏停在对话；用户点击后左栏仍可展开。
#[test]
fn desktop_drawers_start_collapsed_and_left_can_be_opened() {
    let initial = DesktopDrawerState::default();
    assert!(!initial.left_open, "左侧 Norma 对话默认应收起");
    assert!(!initial.right_open, "右侧内容默认应收起");
    assert_eq!(
        initial.left_view,
        DesktopLeftView::Chat,
        "左栏默认应是对话，而不是运行概览"
    );

    let opened = route_drawer(initial, DesktopDrawerEvent::ToggleLeft);
    assert!(opened.left_open, "用户点击左侧入口后应展开对话");
    assert!(!opened.right_open, "展开左栏不得连带打开右栏");
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Chat,
        "只展开左栏时不得改到运行概览"
    );
}

/// 语音对话和后台产物出现时，已收起的抽屉不得自行弹开。
///
/// 数据构造：左、右抽屉均已收起，左栏视图是对话。
/// 执行过程：触发语音、后台产物通知，再由用户显式打开产物。
/// 预期结果：前两步不改开合与视图；打开产物只展开右栏。
#[test]
fn voice_and_background_artifact_do_not_force_drawers_open() {
    let mut state = DesktopDrawerState::default();
    state = route_drawer(state, DesktopDrawerEvent::VoiceTriggered);
    state = route_drawer(state, DesktopDrawerEvent::ArtifactReady);
    assert!(!state.left_open, "语音触发不能强制打开左栏");
    assert!(!state.right_open, "后台产物不能强制打开右栏");
    assert_eq!(
        state.left_view,
        DesktopLeftView::Chat,
        "后台产物不能把左栏切到运行概览"
    );

    state = route_drawer(state, DesktopDrawerEvent::OpenArtifact);
    assert!(state.right_open, "用户打开产物后右栏应展开");
    assert!(!state.left_open, "打开产物不能顺带打开左栏");
    assert_eq!(
        state.left_view,
        DesktopLeftView::Chat,
        "打开产物不能把左栏切到运行概览"
    );
}

/// 切换运行概览会在左栏收起时先展开，再切回对话；Esc 只从概览回到对话。
///
/// 数据构造：左栏收起且当前是对话；偏好 JSON 使用字面量 `{"left_view":"overview"}`。
/// 执行过程：切换概览、再切换一次、对概览按 Esc，并把偏好做一次保存与还原。
/// 预期结果：第一次展开左栏并显示概览；第二次回到对话且左栏仍开着；
///   Esc 回到对话；坏 JSON 与对话偏好都不会被还原成概览。
#[test]
fn toggle_run_overview_opens_collapsed_left_drawer_and_escape_returns_to_chat() {
    let collapsed = DesktopDrawerState::default();
    let overview = route_drawer(collapsed, DesktopDrawerEvent::ToggleRunOverview);
    assert!(overview.left_open, "左栏收起时切到概览应先展开左栏");
    assert_eq!(
        overview.left_view,
        DesktopLeftView::Overview,
        "第一次切换应显示运行概览"
    );
    assert!(!overview.right_open, "切换运行概览不得打开右栏");

    let chat = route_drawer(overview, DesktopDrawerEvent::ToggleRunOverview);
    assert_eq!(chat.left_view, DesktopLeftView::Chat, "再次切换应回到对话");
    assert!(chat.left_open, "切回对话时不应把已经展开的左栏收起");
    assert_ne!(
        chat.left_view,
        DesktopLeftView::Overview,
        "再次切换后不能仍停在运行概览"
    );

    let escaped = route_drawer(
        route_drawer(
            chat,
            DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
        ),
        DesktopDrawerEvent::EscapeOverview,
    );
    assert_eq!(
        escaped.left_view,
        DesktopLeftView::Chat,
        "焦点在抽屉概览时 Esc 应回到对话"
    );
    assert!(escaped.left_open, "Esc 回对话不应收起左栏");
    let still_chat = route_drawer(escaped, DesktopDrawerEvent::EscapeOverview);
    assert_eq!(
        still_chat.left_view,
        DesktopLeftView::Chat,
        "已经在对话时 Esc 不应再改变视图"
    );

    let saved = left_view_preference_json(DesktopLeftView::Overview);
    assert_eq!(
        left_view_from_preference(Some(saved)),
        DesktopLeftView::Overview,
        "概览偏好应能按原样还原"
    );
    let restored = restore_left_view(DesktopDrawerState::default(), DesktopLeftView::Overview);
    assert!(restored.left_open, "重启后还原概览时，收起的左栏应先展开");
    assert_eq!(
        restored.left_view,
        DesktopLeftView::Overview,
        "重启后应还原运行概览"
    );
    assert_eq!(
        left_view_from_preference(Some("{")),
        DesktopLeftView::Chat,
        "损坏的布局偏好应回退到对话"
    );
    assert_eq!(
        left_view_from_preference(None),
        DesktopLeftView::Chat,
        "没有保存过的工作空间应保持对话"
    );
    assert_ne!(
        left_view_from_preference(Some(r#"{"left_view":"chat"}"#)),
        DesktopLeftView::Overview,
        "对话偏好不能被读成运行概览"
    );
}

/// 场景：用户拖动任一抽屉的内侧边缘，越过最小宽度时抽屉收起。
/// 数据构造：左栏宽 340px、右栏宽 420px，最小分别为 300px、370px。
/// 执行过程：向外扩大、向内缩到阈值、再继续缩过阈值。
/// 预期结果：两侧方向相反；到最小宽度仍展开，低于阈值才收起。
#[test]
fn drawer_resize_obeys_side_direction_and_collapse_threshold() {
    let left = resize_drawer(DesktopDrawerSide::Left, 340.0, 100.0, 140.0, 300.0, 600.0);
    assert_eq!(left.width, 380.0, "左栏内侧边缘向右拖应加宽");
    assert!(left.open, "扩大左栏不能收起");

    let right = resize_drawer(DesktopDrawerSide::Right, 420.0, 800.0, 760.0, 370.0, 600.0);
    assert_eq!(right.width, 460.0, "右栏内侧边缘向左拖应加宽");
    assert!(right.open, "扩大右栏不能收起");

    let edge = resize_drawer(DesktopDrawerSide::Left, 340.0, 100.0, 60.0, 300.0, 600.0);
    assert_eq!(edge.width, 300.0, "最小宽度应仍可停靠");
    assert!(edge.open, "正好到最小宽度不能收起");

    let collapsed = resize_drawer(DesktopDrawerSide::Right, 420.0, 800.0, 851.0, 370.0, 600.0);
    assert_eq!(collapsed.width, 370.0, "收起后应保留可恢复的最小宽度");
    assert!(!collapsed.open, "小于最小宽度应收起右栏");
}

/// 点图钉把概览弹出到画布，左栏回到对话；再点则收回并清掉偏移。
///
/// 数据构造：左栏已展开、当前是概览，偏移先被拖到 (-24, 12)。
/// 执行过程：点一次图钉，再点一次；中间打开产物。
/// 预期结果：第一次 floating 且左栏是对话；产物动作不收起面板；
///   第二次回到抽屉概览且偏移归零。状态显示只在悬浮时标图钉。
#[test]
fn pin_floats_overview_then_docks_it_back_into_the_drawer() {
    let drawer = route_drawer(
        DesktopDrawerState {
            left_open: true,
            ..DesktopDrawerState::default()
        },
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
    );
    let mut floating = route_drawer(drawer, DesktopDrawerEvent::ToggleOverviewPin);
    floating.overview_offset = OverviewOffset { x: -24, y: 12 };
    assert!(floating.overview_floating, "点图钉应把运行概览弹出");
    assert_eq!(
        floating.left_view,
        DesktopLeftView::Chat,
        "弹出后左栏应回到对话"
    );
    assert!(floating.left_open, "弹出后左栏保持展开，对话仍可输入");
    assert!(
        status_display_marks_pin(&floating),
        "悬浮时状态显示应标图钉"
    );
    assert_ne!(
        floating.left_view,
        DesktopLeftView::Overview,
        "悬浮后面板不该还占着抽屉概览"
    );

    let still = route_drawer(floating, DesktopDrawerEvent::OpenArtifact);
    assert!(still.overview_floating, "打开产物后悬浮面板应继续显示");
    assert!(still.right_open, "打开产物仍应展开右栏");
    assert_eq!(
        still.overview_offset,
        OverviewOffset { x: -24, y: 12 },
        "行动作不能改掉已经拖好的位置"
    );

    let docked = route_drawer(still, DesktopDrawerEvent::ToggleOverviewPin);
    assert!(!docked.overview_floating, "再点图钉应收回抽屉");
    assert_eq!(
        docked.left_view,
        DesktopLeftView::Overview,
        "收回后左栏应显示运行概览"
    );
    assert_eq!(
        docked.overview_offset,
        OverviewOffset::default(),
        "收回时应把偏移清回 (0, 0)"
    );
    assert!(docked.left_open, "收回后左栏应展开");
    assert!(
        !status_display_marks_pin(&docked),
        "收回后状态显示不再标图钉"
    );
}

/// 点「运行概览」标签在悬浮时收回，未悬浮时只切换视图。
///
/// 数据构造：已悬浮，偏移 (-8, 4)。
/// 执行过程：选择概览标签；另起一份未悬浮状态再选一次概览。
/// 预期结果：悬浮时等同收回；未悬浮时只切到概览且不开始悬浮。
#[test]
fn overview_tab_docks_a_floating_panel_and_otherwise_selects_the_view() {
    let floating = DesktopDrawerState {
        left_open: true,
        overview_floating: true,
        overview_offset: OverviewOffset { x: -8, y: 4 },
        ..DesktopDrawerState::default()
    };
    let docked = route_drawer(
        floating,
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
    );
    assert!(!docked.overview_floating, "悬浮时点概览标签应收回");
    assert_eq!(
        docked.left_view,
        DesktopLeftView::Overview,
        "收回后应显示抽屉概览"
    );
    assert_eq!(
        docked.overview_offset,
        OverviewOffset::default(),
        "从标签收回也应清偏移"
    );

    let chat = DesktopDrawerState::default();
    let overview = route_drawer(
        chat,
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
    );
    assert_eq!(
        overview.left_view,
        DesktopLeftView::Overview,
        "未悬浮时点概览标签应切到概览"
    );
    assert!(
        !overview.overview_floating,
        "未悬浮时点概览标签不能自己弹出"
    );
}

/// 拖动和方向键按 1:1 累加偏移，并限制在画布内；双击标题栏回到原点。
///
/// 数据构造：画布 1200×800。面板宽 min(360, 1200-24)=360，高 min(640, 800-150)=640。
///   默认左缘 = 1200-12-360 = 828，上缘 = 64。
///   水平限位：左缘≥8 得 x≥8-828=-820；右缘≤1192 得 x≤1192-360-828=4。
///   垂直限位：上缘≥4 得 y≥4-64=-60；下缘≤792 得 y≤792-640-64=88。
/// 执行过程：向左 16、Shift 向左再 48、向右 100、向上 100，然后双击复位。
/// 预期结果：(-16)、(-64) 原样落下；向右停在 4；向上停在 -60；复位为 (0, 0)。
#[test]
fn overview_offset_tracks_the_pointer_and_resets_on_double_click() {
    let canvas = OverviewCanvas {
        width: 1200.0,
        height: 800.0,
    };
    let origin = OverviewOffset::default();
    let step = overview_key_step(false);
    let left = move_overview_offset(origin, -(step as f32), 0.0, canvas);
    assert_eq!(left.x, -16, "方向键一步应是 16px，向左 1:1");
    assert_eq!(left.y, 0, "水平移动不能带动垂直偏移");
    assert_eq!(overview_key_step(false), 16, "未按 Shift 的步长是 16");
    assert_eq!(overview_key_step(true), 48, "按住 Shift 的步长是 48");

    let shifted = move_overview_offset(left, -48.0, 0.0, canvas);
    assert_eq!(shifted.x, -64, "Shift 步长应从当前偏移继续累加");

    let right = move_overview_offset(origin, 100.0, 0.0, canvas);
    assert_eq!(right.x, 4, "向右不能越过画布右缘 8px");
    assert_ne!(right.x, 100, "超出画布的拖动增量必须被限位裁掉");

    let up = move_overview_offset(origin, 0.0, -100.0, canvas);
    assert_eq!(up.y, -60, "上缘应停在画布顶内 4px，不能压到顶部");
    let down = move_overview_offset(origin, 0.0, 200.0, canvas);
    assert_eq!(down.y, 88, "下缘应留出 8px，面板最大高度 640");

    let placed = place_overview(right, canvas);
    assert_eq!(placed.width, 360.0, "宽画布上面板宽应为 360");
    assert_eq!(placed.left, 832.0, "默认左缘 828 再加限位后的 x=4");
    assert_eq!(placed.top, 64.0, "只限制水平时上缘仍是默认 64");

    let reset = route_drawer(
        DesktopDrawerState {
            overview_floating: true,
            overview_offset: shifted,
            ..DesktopDrawerState::default()
        },
        DesktopDrawerEvent::ResetOverviewOffset,
    );
    assert_eq!(
        reset.overview_offset,
        OverviewOffset::default(),
        "双击标题栏应把偏移复位为 (0, 0)"
    );
    assert!(reset.overview_floating, "复位不能把面板收回抽屉");
}

/// 悬浮状态和偏移按桌面工作空间往返；还原时按当前画布重新限位。
///
/// 数据构造：悬浮偏移 (-40, 20)；保存 JSON 为字面量。
///   窄画布宽 400：面板宽 min(360, 376)=360，默认左缘 400-12-360=28。
///   右缘限位 x≤(400-8-360)-28=4，所以 -40 仍合法；上缘 y=-20 在 [-60, …] 内。
///   再给一个会越界的 x=80。
/// 执行过程：序列化、解析、分别在宽画布和会裁切的偏移上还原。
/// 预期结果：往返保持悬浮、对话视图和原偏移；越界偏移被裁到 4；
///   旧的只含 left_view 的 JSON 仍还原成抽屉概览。
#[test]
fn overview_float_layout_roundtrips_and_reclamps_to_the_current_canvas() {
    let floating = DesktopDrawerState {
        left_open: true,
        left_view: DesktopLeftView::Chat,
        overview_floating: true,
        overview_offset: OverviewOffset { x: -40, y: 20 },
        ..DesktopDrawerState::default()
    };
    let saved = overview_layout_json(&floating);
    assert_eq!(
        saved, r#"{"floating":true,"left_view":"chat","offset_x":-40,"offset_y":20,"collapsed":0}"#,
        "布局 JSON 应同时写下视图、悬浮、偏移和各栏折叠位"
    );
    let parsed = overview_layout_from_preference(Some(&saved));
    assert!(parsed.floating, "读回应仍是悬浮");
    assert_eq!(parsed.left_view, DesktopLeftView::Chat, "读回应仍是对话");
    assert_eq!(
        parsed.offset,
        OverviewOffset { x: -40, y: 20 },
        "读回应保持原偏移"
    );

    let wide = OverviewCanvas {
        width: 1200.0,
        height: 800.0,
    };
    let restored = restore_overview_layout(DesktopDrawerState::default(), parsed, Some(wide));
    assert!(restored.overview_floating, "重启后应还原悬浮");
    assert!(restored.left_open, "还原悬浮时应展开左栏对话");
    assert_eq!(
        restored.left_view,
        DesktopLeftView::Chat,
        "悬浮还原后左栏是对话，不是抽屉概览"
    );
    assert_eq!(
        restored.overview_offset,
        OverviewOffset { x: -40, y: 20 },
        "宽画布上这个偏移无需裁切"
    );
    assert!(
        status_display_marks_pin(&restored),
        "还原成悬浮后状态显示仍应标图钉"
    );

    let clamped = restore_overview_layout(
        DesktopDrawerState::default(),
        OverviewLayoutPreference {
            offset: OverviewOffset { x: 80, y: 20 },
            ..parsed
        },
        Some(wide),
    );
    assert_eq!(
        clamped.overview_offset.x, 4,
        "还原到当前画布时向右越过 8px 边距的偏移应裁到 4"
    );
    assert_ne!(clamped.overview_offset.x, 80, "越界偏移不能原样回到画布外");

    let legacy = overview_layout_from_preference(Some(r#"{"left_view":"overview"}"#));
    let drawer = restore_overview_layout(DesktopDrawerState::default(), legacy, Some(wide));
    assert_eq!(
        drawer.left_view,
        DesktopLeftView::Overview,
        "旧的概览偏好仍应还原成抽屉概览"
    );
    assert!(!drawer.overview_floating, "旧偏好没有悬浮字段时不能弹出");
    assert_eq!(
        drawer.overview_offset,
        OverviewOffset::default(),
        "未悬浮的旧偏好偏移应为 (0, 0)"
    );
    assert_eq!(
        overview_layout_from_preference(Some("{")),
        OverviewLayoutPreference::default(),
        "损坏的 JSON 应回退到对话且不悬浮"
    );
}

/// Esc 在悬浮面板里只交还焦点，在抽屉概览里回到对话。
///
/// 数据构造：一份悬浮状态，一份抽屉概览状态。
/// 执行过程：分别在焦点位于面板内、外时询问 Escape。
/// 预期结果：面板内回到状态显示且不改变悬浮；抽屉概览回到对话；对话中忽略。
#[test]
fn escape_returns_floating_focus_to_the_status_display() {
    let floating = DesktopDrawerState {
        left_open: true,
        overview_floating: true,
        overview_offset: OverviewOffset { x: -12, y: 0 },
        ..DesktopDrawerState::default()
    };
    let (kept, target) = route_overview_escape(floating, true);
    assert_eq!(
        target,
        OverviewEscape::ReturnFocusToStatus,
        "焦点在悬浮面板时 Esc 应回到状态显示"
    );
    assert_eq!(kept, floating, "这次 Esc 不能收回或移动面板");

    let (ignored, target) = route_overview_escape(floating, false);
    assert_eq!(
        target,
        OverviewEscape::Ignore,
        "焦点不在悬浮面板时 Esc 不应抢走状态显示"
    );
    assert!(ignored.overview_floating, "未命中面板的 Esc 不能收回悬浮");

    let drawer = route_drawer(
        DesktopDrawerState::default(),
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
    );
    let (chat, target) = route_overview_escape(drawer, false);
    assert_eq!(
        target,
        OverviewEscape::ReturnDrawerToChat,
        "焦点在抽屉概览时 Esc 应回到对话"
    );
    assert_eq!(
        chat.left_view,
        DesktopLeftView::Chat,
        "抽屉概览的 Esc 应变回对话"
    );
    assert!(!chat.overview_floating, "抽屉 Esc 不能改成悬浮");
}
