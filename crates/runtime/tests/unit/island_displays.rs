use super::*;
use crate::island::{IslandEvent, IslandMode};

/// 场景：两块屏幕共享岛数据，但只在指针所在屏幕展开；旧屏幕的离开事件可迟到。
/// 数据：屏幕11/22，悬停截止160ms；第二块屏幕在200ms直接展开。
/// 执行：第一块悬停展开，第二块点击，随后旧屏幕发出离开，再移除第二块。
/// 预期：任意时刻仅一块展开；旧离开不启动收回；拔屏后转移到保留屏，状态不丢。
#[test]
fn island_display_ownership_survives_stale_pointer_events_and_unplug() {
    let mut state = IslandDisplays::default();
    state.apply(11, IslandEvent::PointerEntered, 0);
    state.apply(11, IslandEvent::Tick, 160);
    assert_eq!(state.mode_on(11), IslandMode::Expanded, "悬停屏幕应展开");
    assert_eq!(
        state.mode_on(22),
        IslandMode::Compact,
        "其他屏幕只显示收起形态"
    );
    state.apply(22, IslandEvent::Expand, 200);
    state.apply(11, IslandEvent::PointerLeft, 210);
    assert_eq!(state.mode_on(11), IslandMode::Compact, "转移后原屏幕应收起");
    assert_eq!(state.mode_on(22), IslandMode::Expanded, "新屏幕保持展开");
    assert_eq!(state.state.deadline_ms(), None, "旧窗口离开不能收起新窗口");
    state.retain_displays(&[11], 300);
    assert_eq!(
        state.mode_on(11),
        IslandMode::Expanded,
        "拔屏后保留展开状态"
    );
    assert_eq!(
        state.active_display(),
        Some(11),
        "展开所有权应转移到保留屏幕"
    );
}

/// 场景：正在输入的显示器拔出后，旧焦点和悬停不应让另一块屏幕永久展开。
/// 数据：屏11在0ms展开并聚焦，屏22保留；拔屏时间为100ms。
/// 执行：迁移到22，在519ms、520ms推进截止时间，再进入屏22。
/// 预期：100+420ms收回，旧屏离开事件不延长截止；新屏可正常重新悬停展开。
#[test]
fn island_unplug_releases_old_pointer_and_input_focus() {
    let mut state = IslandDisplays::default();
    state.apply(11, IslandEvent::PointerEntered, 0);
    state.apply(11, IslandEvent::Expand, 0);
    state.apply(11, IslandEvent::InputFocusChanged(true), 0);
    state.retain_displays(&[22], 100);
    state.apply(11, IslandEvent::PointerLeft, 110);
    assert_eq!(
        state.state.deadline_ms(),
        Some(520),
        "拔屏必须释放旧焦点并启动420ms收回"
    );
    state.apply(22, IslandEvent::Tick, 519);
    assert_eq!(state.mode_on(22), IslandMode::Expanded, "截止前保持展开");
    state.apply(22, IslandEvent::Tick, 520);
    assert_eq!(
        state.mode_on(22),
        IslandMode::Compact,
        "不能永久保留旧屏hover状态"
    );
    state.apply(22, IslandEvent::PointerEntered, 600);
    state.apply(22, IslandEvent::Tick, 760);
    assert_eq!(
        state.mode_on(22),
        IslandMode::Expanded,
        "新屏仍能正常悬停展开"
    );
}

/// 场景：A输入有焦点，悬停到B时新窗口没有实际输入焦点。
/// 数据：A在0ms展开并聚焦，100ms进入B，200ms离开B。
/// 执行：正常跨屏迁移，不拔屏、不点击B输入框，推进到620ms。
/// 预期：B离开后应按200+420ms收回，不能继承A的输入焦点。
#[test]
fn island_pointer_transfer_releases_old_input_focus() {
    let mut state = IslandDisplays::default();
    state.apply(11, IslandEvent::PointerEntered, 0);
    state.apply(11, IslandEvent::Expand, 0);
    state.apply(11, IslandEvent::InputFocusChanged(true), 0);
    state.apply(22, IslandEvent::PointerEntered, 100);
    state.apply(22, IslandEvent::PointerLeft, 200);
    assert_eq!(
        state.state.deadline_ms(),
        Some(620),
        "移屏必须释放原输入焦点"
    );
    state.apply(22, IslandEvent::Tick, 620);
    assert_eq!(
        state.mode_on(22),
        IslandMode::Compact,
        "未聚焦的新窗口按时收回"
    );
}

/// 场景：新提问在指针所在屏提醒，两屏不能同时挂载同一个可编辑表单。
/// 数据：屏11/22；指针在22时收到问题q；随后用户在11展开并输入。
/// 执行：投影提醒、跨屏展开、发送22迟到的失焦/聚焦事件，再在11离开。
/// 预期：仅22先提醒，转移后仅11展开；旧屏事件不抢焦点，11输入期间不收回。
#[test]
fn island_alert_has_one_owner_and_ignores_old_display_focus() {
    let mut displays = IslandDisplays::default();
    displays.retain_displays(&[11, 22], 0);
    let overview = crate::island::IslandOverview {
        alerts: vec![crate::island::IslandAlert {
            id: "q".into(),
            kind: crate::island::IslandAlertKind::Question,
        }],
        ..Default::default()
    };
    displays.apply(22, IslandEvent::OverviewUpdated(overview), 10);
    assert_eq!(
        displays.mode_on(22),
        IslandMode::Alert,
        "通知选择指针所在屏"
    );
    assert_eq!(
        displays.mode_on(11),
        IslandMode::Compact,
        "其他屏不能同时挂载问题输入框"
    );
    displays.apply(22, IslandEvent::PointerEntered, 11);
    displays.apply(22, IslandEvent::InputFocusChanged(true), 12);
    displays.apply(22, IslandEvent::Expand, 13);
    displays.apply(22, IslandEvent::PointerLeft, 14);
    assert_eq!(
        displays.state.deadline_ms(),
        None,
        "提醒内已有输入焦点，展开后不能收回"
    );
    displays.apply(11, IslandEvent::Expand, 20);
    displays.apply(11, IslandEvent::InputFocusChanged(true), 30);
    displays.apply(22, IslandEvent::InputFocusChanged(false), 40);
    displays.apply(11, IslandEvent::PointerLeft, 50);
    assert_eq!(
        displays.state.deadline_ms(),
        None,
        "旧屏失焦不能收起新屏正在输入的表单"
    );
    displays.apply(11, IslandEvent::InputFocusChanged(false), 60);
    displays.apply(22, IslandEvent::InputFocusChanged(true), 70);
    assert_eq!(
        displays.state.deadline_ms(),
        Some(480),
        "旧屏迟到聚焦不能阻断收回"
    );
}
