use super::*;
use std::collections::HashSet;

fn spawn(
    store: &mut dispatch_receipt::DispatchEffectStore,
    task_ref: &str,
    style: DispatchEffectStyle,
) -> DispatchEffectSpawnOutcome {
    store.spawn_styled(dispatch_receipt::DispatchSpawn {
        task_ref: task_ref.into(),
        origin: DispatchVisualOrigin::CommandDock,
        target_node_id: 7,
        status: DispatchReceiptVisualStatus::Submitted,
        style,
    })
}

fn tick_ms(store: &mut dispatch_receipt::DispatchEffectStore, total_ms: u64) {
    for _ in 0..total_ms / 50 {
        store.tick(Duration::from_millis(50));
    }
}

/// 每款特效的飞行/命中时长来自原型时间轴，速度倍率按毫秒四舍五入。
///
/// 数据构造：8 款 × (飞行, 命中) = 16 个数；倍率 0.5 → 流星 900ms 飞行 = 450ms。
/// 执行过程：读取 timing(1.0) 与 timing(0.5)。
/// 预期结果：
///   - 正断言：表中 16 个毫秒数逐个相等；0.5 倍的流星飞行 450ms、命中 215ms。
///   - 负断言：倍率 0.5 时不再保持原本的 900ms 飞行时长。
#[test]
fn test_style_timings_and_speed_scale() {
    use DispatchEffectStyle::*;
    let table = [
        (Meteor, 900, 430),
        (Ion, 480, 430),
        (Plane, 1020, 600),
        (Ribbon, 660, 700),
        (Firefly, 920, 660),
        (Fireworks, 600, 1300),
        (Lightning, 170, 590),
        (Vortex, 1420, 340),
    ];
    for (style, flight, impact) in table {
        let timing = style.timing(1.0);
        assert_eq!(
            (timing.flight.as_millis(), timing.impact.as_millis()),
            (flight, impact),
            "{style:?} 的时长应与原型一致"
        );
    }
    let half = Meteor.timing(0.5);
    assert_eq!(half.flight.as_millis(), 450, "0.5 倍流星飞行应为 450ms");
    assert_eq!(half.impact.as_millis(), 215, "0.5 倍流星命中应为 215ms");
    assert_ne!(half.flight.as_millis(), 900, "速度倍率必须实际影响飞行时长");
}

/// 款式 id 与存储层字符串一一对应，未知 id 不解析。
///
/// 数据构造：8 个款式 id + 4 个不认识的字符串（"nope"、""、"random"、"pulse"）。
/// 预期结果：
///   - 正断言：8 个 id 往返相等。
///   - 负断言：四个字符串都返回 None（"random" 是偏好而不是可绘制款式）。
#[test]
fn test_style_ids_round_trip() {
    use DispatchEffectStyle::*;
    for style in [
        Meteor, Ion, Plane, Ribbon, Firefly, Fireworks, Lightning, Vortex,
    ] {
        assert_eq!(DispatchEffectStyle::from_id(style.id()), Some(style));
    }
    for unknown in ["nope", "", "random", "pulse"] {
        assert_eq!(
            DispatchEffectStyle::from_id(unknown),
            None,
            "{unknown:?} 不应解析为可绘制款式"
        );
    }
}

/// 随机款式由 task_ref 决定：同一次派发结果固定，且覆盖全部 7 款、不含离子/脉冲。
///
/// 数据构造：400 个不同 task_ref；期望集合 = 流星/纸飞机/声纹/萤火/烟花/闪电/星漩 = 7 款。
/// 预期结果：
///   - 正断言：同一 task_ref 连续两次结果相同；400 次抽取覆盖 7 款（均匀哈希下漏掉任一款的概率可忽略）。
///   - 负断言：结果集合不含 Ion，也不含 7 款之外的款式。
#[test]
fn test_random_style_is_deterministic_and_covers_all_drawn_styles() {
    use DispatchEffectStyle::*;
    assert_eq!(
        DispatchEffectStyle::random_for("idea:a"),
        DispatchEffectStyle::random_for("idea:a"),
        "同一次派发的随机款式必须固定"
    );
    let seen: HashSet<_> = (0..400)
        .map(|i| DispatchEffectStyle::random_for(&format!("task-{i}")))
        .collect();
    let expected: HashSet<_> = [Meteor, Plane, Ribbon, Firefly, Fireworks, Lightning, Vortex]
        .into_iter()
        .collect();
    assert_eq!(seen, expected, "随机应覆盖且仅覆盖 7 款可绘制特效");
}

/// 未显式指定款式时沿用现状：想法/语音起点是流星，Agent 交接是离子，时长不变。
///
/// 数据构造：Dock 起点与 AgentNode 起点各 1 条 effect。
/// 预期结果：
///   - 正断言：Dock → Meteor，Agent → Ion。
///   - 负断言：离子飞行 480ms 时已进入命中；流星同一时刻仍在飞行。
#[test]
fn test_default_styles_keep_existing_behavior() {
    let mut store = MotionStore::new(false);
    store.spawn_dispatch_receipt("dock", 1, DispatchReceiptVisualStatus::Submitted);
    store.spawn_dispatch_receipt_from(
        "agent",
        DispatchVisualOrigin::AgentNode(2),
        3,
        DispatchReceiptVisualStatus::Submitted,
    );
    // 后发的 effect 有 40ms 错峰，先 tick 一次让两条都进入 active。
    store.tick(Duration::from_millis(50));
    let shown = store.dispatch_receipt_presentations();
    assert_eq!(shown[0].style, DispatchEffectStyle::Meteor, "语音默认流星");
    assert_eq!(shown[1].style, DispatchEffectStyle::Ion, "交接默认离子");
    for _ in 0..10 {
        store.tick(Duration::from_millis(48));
    }
    let shown = store.dispatch_receipt_presentations();
    assert_eq!(
        shown[0].phase,
        DispatchEffectPhase::Flight,
        "流星 480ms 仍在飞行"
    );
    assert_eq!(
        shown[1].phase,
        DispatchEffectPhase::Impact,
        "离子 480ms 已命中"
    );
}

/// 飞行时长取自款式并受速度倍率影响，飞行中改倍率不影响已发出的 effect 的款式。
///
/// 数据构造：烟花飞行 600ms；tick 步长 50ms；倍率 1.0 与 0.5。
/// 预期结果：
///   - 正断言：1.0 倍 550ms 仍飞行、600ms 命中；0.5 倍 250ms 仍飞行、300ms 命中。
///   - 负断言：550ms 时不应已命中。
#[test]
fn test_flight_duration_follows_style_and_speed() {
    let mut store = dispatch_receipt::DispatchEffectStore::default();
    spawn(&mut store, "a", DispatchEffectStyle::Fireworks);
    tick_ms(&mut store, 550);
    assert_eq!(
        store.presentations()[0].phase,
        DispatchEffectPhase::Flight,
        "550ms 仍应在飞行"
    );
    tick_ms(&mut store, 50);
    assert_eq!(
        store.presentations()[0].phase,
        DispatchEffectPhase::Impact,
        "600ms 应命中"
    );

    let mut fast = dispatch_receipt::DispatchEffectStore::default();
    fast.set_speed(0.5);
    spawn(&mut fast, "b", DispatchEffectStyle::Fireworks);
    tick_ms(&mut fast, 250);
    assert_eq!(
        fast.presentations()[0].phase,
        DispatchEffectPhase::Flight,
        "0.5 倍 250ms 仍在飞行"
    );
    tick_ms(&mut fast, 50);
    assert_eq!(
        fast.presentations()[0].phase,
        DispatchEffectPhase::Impact,
        "0.5 倍 300ms 应命中"
    );
}

/// 连续派发和减少动态效果都必须保留用户选定的完整特效。
///
/// 数据构造：8 款飞行动画 × 减少动态效果的 2 种状态 × 语音/想法/交接 3 种来源。
/// 粒子预算沿用 20 个主粒子 + 8 个次级火花 = 28；40ms 为既有发射错峰间隔。
/// 执行过程：按设置选择款式并立即派发到两个目标，再推进 40ms 发射错峰使第二条入场。
/// 预期结果：两条均保持指定款式、Flight 阶段与 28 粒子预算，不能跳过飞行或变为脉冲。
#[test]
fn test_dispatch_always_preserves_selected_effect() {
    use DispatchEffectStyle::*;
    for reduce_motion in [false, true] {
        for style in [
            Meteor, Ion, Plane, Ribbon, Firefly, Fireworks, Lightning, Vortex,
        ] {
            for origin in [DOCK, IDEA, AGENT] {
                let mut store = MotionStore::new(reduce_motion);
                store.set_dispatch_style_selection(StyleSelection::from_preference(&pref(
                    style.id(),
                    None,
                    None,
                    None,
                )));
                for target in [1, 2] {
                    store.spawn_dispatch_receipt_from(
                        format!("selected:{target}"),
                        origin,
                        target,
                        DispatchReceiptVisualStatus::Submitted,
                    );
                }
                store.tick(Duration::from_millis(40));
                let shown = store.dispatch_receipt_presentations();
                assert_eq!(shown.len(), 2, "40ms 错峰后两条派发都应入场");
                for effect in shown {
                    assert_eq!(effect.style, style, "连续派发必须保持指定款式 {style:?}");
                    assert_eq!(effect.phase, DispatchEffectPhase::Flight, "不得跳过飞行");
                    assert_eq!(effect.particle_count, 28, "必须保留完整的 28 粒子预算");
                }
            }
        }
    }
}

fn pref(
    style: &str,
    voice: Option<&str>,
    idea: Option<&str>,
    handoff: Option<&str>,
) -> storage::DispatchEffectPreference {
    storage::DispatchEffectPreference {
        style: style.into(),
        voice: voice.map(Into::into),
        idea: idea.map(Into::into),
        handoff: handoff.map(Into::into),
    }
}

const DOCK: DispatchVisualOrigin = DispatchVisualOrigin::CommandDock;
const IDEA: DispatchVisualOrigin = DispatchVisualOrigin::IdeaRow { x: 1, y: 2 };
const AGENT: DispatchVisualOrigin = DispatchVisualOrigin::AgentNode(9);

/// 没有任何设置时沿用现状：语音和想法是流星，交接是离子。
///
/// 数据构造：默认选择（未设置）× 3 种起点 = 3 个结果。
/// 预期结果：
///   - 正断言：Dock/IdeaRow → Meteor，AgentNode → Ion。
///   - 负断言：交接不会因为默认流星而变成流星。
#[test]
fn test_selection_default_keeps_existing_behavior() {
    let selection = StyleSelection::default();
    assert_eq!(selection.style_for(DOCK, "t"), DispatchEffectStyle::Meteor);
    assert_eq!(selection.style_for(IDEA, "t"), DispatchEffectStyle::Meteor);
    assert_eq!(
        selection.style_for(AGENT, "t"),
        DispatchEffectStyle::Ion,
        "交接默认仍是离子"
    );
}

/// 设置了款式后，三种来源默认跟随；来源覆盖只影响自己，None 表示跟随。
///
/// 数据构造：base=ribbon；覆盖组合 4 组 × 3 个来源 = 12 个断言。
/// 预期结果：
///   - 正断言：无覆盖时三者都是 ribbon；voice=fireworks 只改语音；handoff=ion 只改交接；idea=lightning 只改想法。
///   - 负断言：未被覆盖的来源不受影响。
#[test]
fn test_selection_follow_and_source_overrides() {
    use DispatchEffectStyle::*;
    let cases = [
        (pref("ribbon", None, None, None), [Ribbon, Ribbon, Ribbon]),
        (
            pref("ribbon", Some("fireworks"), None, None),
            [Fireworks, Ribbon, Ribbon],
        ),
        (
            pref("ribbon", None, None, Some("ion")),
            [Ribbon, Ribbon, Ion],
        ),
        (
            pref("ribbon", None, Some("lightning"), None),
            [Ribbon, Lightning, Ribbon],
        ),
    ];
    for (preference, [voice, idea, handoff]) in cases {
        let selection = StyleSelection::from_preference(&preference);
        assert_eq!(selection.style_for(DOCK, "t"), voice, "{preference:?} 语音");
        assert_eq!(selection.style_for(IDEA, "t"), idea, "{preference:?} 想法");
        assert_eq!(
            selection.style_for(AGENT, "t"),
            handoff,
            "{preference:?} 交接"
        );
    }
}

/// 随机在派发时按 task_ref 固定；未知 id 当作未设置，回落到默认而不是报错。
///
/// 数据构造：base=random；另一组 base=future_style（未知）+ voice=unknown。
/// 预期结果：
///   - 正断言：随机结果与 `random_for(task_ref)` 一致且重复调用不变；未知 id 的结果与默认一致。
///   - 负断言：随机结果不是 Ion。
#[test]
fn test_selection_random_and_unknown_ids() {
    let random = StyleSelection::from_preference(&pref("random", None, None, None));
    for task in ["a", "b", "c"] {
        let style = random.style_for(DOCK, task);
        assert_eq!(style, DispatchEffectStyle::random_for(task));
        assert_eq!(style, random.style_for(DOCK, task), "同一次派发结果固定");
        assert_ne!(style, DispatchEffectStyle::Ion, "随机不应选中交接专用离子");
    }
    let unknown = StyleSelection::from_preference(&pref("future_style", Some("nope"), None, None));
    assert_eq!(unknown, StyleSelection::default(), "未知 id 等同未设置");
}

/// MotionStore 在 spawn 时按选择解析款式；之后改设置不影响已在飞的 effect。
///
/// 数据构造：选择 ribbon；先后派发两次（不同 task）。
/// 预期结果：
///   - 正断言：第一次 presentation.style 为 Ribbon。
///   - 负断言：把选择改成 fireworks 后，已发出的那条仍是 Ribbon。
#[test]
fn test_motion_store_resolves_style_at_spawn() {
    let mut store = MotionStore::new(false);
    store.set_dispatch_style_selection(StyleSelection::from_preference(&pref(
        "ribbon", None, None, None,
    )));
    store.spawn_dispatch_receipt("one", 1, DispatchReceiptVisualStatus::Submitted);
    store.set_dispatch_style_selection(StyleSelection::from_preference(&pref(
        "fireworks",
        None,
        None,
        None,
    )));
    assert_eq!(
        store.dispatch_receipt_presentations()[0].style,
        DispatchEffectStyle::Ribbon
    );
    store.tick(Duration::from_millis(50));
    store.spawn_dispatch_receipt("two", 2, DispatchReceiptVisualStatus::Submitted);
    store.tick(Duration::from_millis(50));
    let styles: Vec<_> = store
        .dispatch_receipt_presentations()
        .iter()
        .map(|p| p.style)
        .collect();
    assert_eq!(
        styles,
        [DispatchEffectStyle::Ribbon, DispatchEffectStyle::Fireworks]
    );
}

/// 脉冲不再提供给用户：旧版本存过的 "pulse" 被当作未设置，仍走各来源的默认款式。
///
/// 数据构造：偏好 style="pulse"，语音覆盖 "pulse"。
/// 执行过程：解析旧偏好并读取语音与交接来源的实际款式。
/// 预期结果：
///   - 正断言：选择等同默认（语音流星、交接离子）；from_id("pulse") 也不再解析出可绘制款式。
///   - 负断言：不会变成 Pulse。
#[test]
fn test_pulse_is_not_user_selectable() {
    let selection = StyleSelection::from_preference(&pref("pulse", Some("pulse"), None, None));
    assert_eq!(
        selection,
        StyleSelection::default(),
        "旧脉冲偏好应等同未设置"
    );
    assert_eq!(
        selection.style_for(DOCK, "t"),
        DispatchEffectStyle::Meteor,
        "语音应恢复原有流星"
    );
    assert_eq!(
        selection.style_for(AGENT, "t"),
        DispatchEffectStyle::Ion,
        "交接应恢复原有离子"
    );
    assert_eq!(
        DispatchEffectStyle::from_id("pulse"),
        None,
        "旧脉冲 id 不应再解析为可绘制款式"
    );
}
