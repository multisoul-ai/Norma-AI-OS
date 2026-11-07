use super::*;
use crate::island::{geometry::*, IslandMode};

fn layout(mode: IslandMode) -> IslandLayout {
    IslandScreen {
        frame: IslandFrame {
            x: 0.,
            y: 0.,
            width: 1440.,
            height: 900.,
        },
        visible_top: 875.,
        safe_top: 0.,
        left_max_x: 0.,
        right_min_x: 0.,
    }
    .layout(mode)
}

/// 场景：从胶囊展开后中途反向收起，黑色表面不跳帧，原生窗只在结束后缩小。
/// 数据：1440×900无刘海屏，330×34胶囊与660×562面板（52点标题行加510点正文），180ms进入、120ms退出。
/// 执行：展开90ms时采样并反向，在最后一帧前后采样。
/// 预期：先预留大窗、命中只覆盖当前黑色形状；反向起点连续，结束后窗口释放透明区域且无动画。
#[test]
fn island_transition_reverses_from_presentation_and_defers_native_shrink() {
    let compact = layout(IslandMode::Compact);
    let expanded = layout(IslandMode::Expanded);
    let mut motion = IslandTransition::new(compact);
    motion.retarget(expanded, 100, false, false);
    let start = motion.sample(100);
    assert_eq!(start.surface, compact, "第一帧从实际胶囊开始");
    assert_eq!(start.window, expanded.frame, "展开先扩大原生窗口");
    assert!(
        !start.surface.contains(720., 400.),
        "透明预留区域不能获得命中"
    );
    let middle = motion.sample(190);
    assert!(
        middle.surface.frame.height > compact.frame.height
            && middle.surface.frame.height < expanded.frame.height,
        "中间帧在两个形态之间"
    );
    motion.retarget(compact, 190, false, false);
    assert_eq!(
        motion.sample(190).surface,
        middle.surface,
        "反向从当前屏幕值开始"
    );
    assert!(
        motion.sample(309).window.height > compact.frame.height,
        "收起结束前不能裁掉形变"
    );
    let end = motion.sample(310);
    assert_eq!(end.surface, compact, "最终表面准确收起");
    assert_eq!(end.window, compact.frame, "最后一帧释放原生透明区域");
    assert!(!end.animating, "空闲后不再申请重绘");
}

/// 场景：减少动态效果采用淡入，键盘开合立即完成，不移动黑色表面。
/// 数据：同一胶囊/展开布局，100ms淡入时限。
/// 执行：分别以reduce-motion与即时模式展开，比较首帧与结束帧。
/// 预期：减少动态效果首帧即为目标几何但透明度未完成；键盘首帧全显且不动画。
#[test]
fn island_transition_reduced_motion_fades_and_keyboard_is_immediate() {
    let compact = layout(IslandMode::Compact);
    let expanded = layout(IslandMode::Expanded);
    let mut motion = IslandTransition::new(compact);
    motion.retarget(expanded, 0, true, false);
    let start = motion.sample(0);
    assert_eq!(start.surface, expanded, "减少动态效果不做形变");
    assert!(start.opacity < 1., "保留温和淡入反馈");
    let middle_opacity = motion.sample(50).opacity;
    motion.retarget(layout(IslandMode::Alert), 50, true, false);
    assert_eq!(
        motion.sample(50).opacity,
        middle_opacity,
        "中途切换形态必须续接当前亮度"
    );
    assert_eq!(motion.sample(150).opacity, 1., "再次淡入100ms后完成");
    assert!(!motion.sample(150).animating, "淡入结束后静止");
    motion.retarget(compact, 160, false, true);
    let keyboard = motion.sample(160);
    assert_eq!(keyboard.surface, compact, "键盘操作立即到目标");
    assert_eq!(keyboard.window, compact.frame, "键盘操作立即缩原生窗");
    assert_eq!(keyboard.opacity, 1., "键盘不淡入");
    assert!(!keyboard.animating, "键盘不触发动画帧");
}

/// 场景：首次键盘展开先用估计高度，下一帧收到实际内容高度。
/// 数据：660×544展开目标，实际内容200pt；保持相同顶部和宽度。
/// 执行：键盘展开后16ms重测，再以悬停收起并重新展开/重测。
/// 预期：键盘重测立即完成，不复活动画；悬停重测保留平滑形变。
#[test]
fn island_transition_measurement_preserves_keyboard_intent() {
    let compact = layout(IslandMode::Compact);
    let expanded = layout(IslandMode::Expanded);
    let mut measured = expanded;
    measured.frame.y += measured.frame.height - 200.;
    measured.frame.height = 200.;
    let mut motion = IslandTransition::new(compact);
    motion.retarget(expanded, 0, false, true);
    motion.retarget_content(measured, 16, false);
    let result = motion.sample(16);
    assert_eq!(result.surface, measured, "键盘展开的内容测量立即应用");
    assert_eq!(result.window, measured.frame, "原生窗口没有多余透明区域");
    assert!(!result.animating, "内容测量不能让键盘展开重新播放动画");
    motion.retarget(compact, 100, false, false);
    motion.retarget(expanded, 300, false, false);
    motion.retarget_content(measured, 316, false);
    assert!(motion.sample(316).animating, "之后的悬停形变仍可动画");
    assert_eq!(motion.sample(500).surface, measured, "悬停也收敛到实际高度");
}

/// 场景：固定目标宽度的内容遇到缩放中的圆角容器，矩形裁剪不能露出底角。
/// 数据：330×34胶囊向660×544展开，180ms中每一毫秒的实际表面。
/// 执行：检查页签中央带底角、body裁剪底边的左右极端点是否在黑色轮廓内。
/// 预期：动画中全都命中黑色表面；结束后恢复原型16pt边距。
#[test]
fn island_transition_content_stays_inside_rounded_intermediate_frames() {
    let mut motion = IslandTransition::new(layout(IslandMode::Compact));
    motion.retarget(layout(IslandMode::Expanded), 0, false, false);
    for now in 1..180 {
        let p = motion.sample(now);
        let f = p.surface.frame;
        let inset = p.content_inset();
        for x in [f.x + inset, f.x + f.width - inset] {
            assert!(
                p.surface.contains(x, f.y + 0.5),
                "中间帧{now}的页签中央带底角须在轮廓内"
            );
        }
        if f.height > p.surface.header_height + inset {
            for x in [f.x + 0.5, f.x + f.width - 0.5] {
                assert!(
                    p.surface.contains(x, f.y + inset + 0.5),
                    "中间帧{now}的body裁剪底边须在轮廓内"
                );
            }
        }
    }
    assert_eq!(
        motion.sample(180).content_inset(),
        16.,
        "稳定后恢复原型边距"
    );
}

/// 场景：形变交给原生合成器，纯逻辑只给出“一次性动画计划”，不再逐帧采样。
/// 数据：胶囊→展开（180ms）、展开→胶囊（120ms）、键盘即时、减少动态效果。
/// 执行：每次 retarget 读取返回的计划；再取内容布局（动画期间排版用较大一端）。
/// 预期：展开/收起各返回一次计划且起点为当前形状；即时与重复目标没有计划；
/// 减少动态效果计划为 fade_only；动画期间内容按较大一端排版，结束后按目标排版。
#[test]
fn island_transition_hands_native_a_one_shot_morph_plan() {
    let compact = layout(IslandMode::Compact);
    let expanded = layout(IslandMode::Expanded);
    let mut motion = IslandTransition::new(compact);
    let open = motion
        .retarget(expanded, 0, false, false)
        .expect("展开应给出形变计划");
    assert_eq!(
        (open.from, open.to, open.duration_ms),
        (compact, expanded, 180)
    );
    assert!(!open.fade_only);
    assert_eq!(motion.content(true), expanded, "展开期间按目标大小排版");
    assert!(
        motion.retarget(expanded, 10, false, false).is_none(),
        "同一目标不得重复计划"
    );
    let close = motion
        .retarget(compact, 90, false, false)
        .expect("收起应给出形变计划");
    assert_eq!(close.to, compact);
    assert_eq!(close.duration_ms, 120, "收起 120ms");
    assert!(
        close.from.frame.height > compact.frame.height,
        "起点是当前中途形状"
    );
    assert_eq!(
        motion.content(true).frame.height,
        close.from.frame.height,
        "收起期间按较大一端（当前形状）排版，直到结束"
    );
    assert_eq!(motion.content(false), compact, "结束后按目标排版");
    assert!(
        motion.envelope().height >= close.from.frame.height,
        "原生窗预留较大一端"
    );
    assert!(
        motion.retarget(expanded, 300, false, true).is_none(),
        "键盘即时没有形变计划"
    );
    let reduced = motion
        .retarget(compact, 400, true, false)
        .expect("减少动态效果仍要淡入");
    assert!(reduced.fade_only);
    assert_eq!(reduced.duration_ms, 100);
}

/// 场景：展开动画还在播时，正文高度测量到了新值。
/// 数据：胶囊在 t=0 展开，180ms；测量发生在 t=40。
/// 执行：询问这一刻是否还挡住测量。
/// 预期：播放中挡住，避免再开一段形变；180ms 结束后不再挡。
#[test]
fn island_transition_blocks_remeasure_until_the_morph_ends() {
    let mut motion = IslandTransition::new(layout(IslandMode::Compact));
    motion.retarget(layout(IslandMode::Expanded), 0, false, false);
    assert!(
        motion.blocks_content_measurement(40),
        "40ms 仍在 180ms 形变内，测量不能再触发一段动画"
    );
    assert!(
        !motion.blocks_content_measurement(180),
        "形变结束后测量可以落成最终尺寸"
    );
}
