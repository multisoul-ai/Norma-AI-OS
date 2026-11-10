use super::scene::{GlowFrame, GlowPrim, Rect};
use super::styles::{halo, ribbon, stardust::StardustPool};
use super::*;

fn frame(phase: VoiceGlowPhase, energy: f32) -> GlowFrame {
    GlowFrame {
        width: 1200.0,
        height: 700.0,
        dock: Rect {
            x: 380.0,
            y: 626.0,
            w: 440.0,
            h: 52.0,
        },
        energy,
        intensity: 1.0,
        phase,
        t_ms: 1000.0,
        reduce_motion: false,
        gain: 1.0,
    }
}

fn max_blur(prims: &[GlowPrim]) -> f32 {
    prims
        .iter()
        .filter_map(|prim| match prim {
            GlowPrim::Glow { blur, .. } => Some(*blur),
            _ => None,
        })
        .fold(0.0, f32::max)
}

/// 场景：风格 id 持久化往返，未知值回落到经典（升级后效果不变）。
/// 数据构造：遍历 ALL 的 id，再给出 None 与未知字符串。
/// 执行过程：from_id(id) 还原；from_id 读取异常输入。
/// 预期结果：全部往返相等；None 与未知值得到经典。
#[test]
fn style_id_roundtrips_and_unknown_falls_back_to_classic() {
    for style in VoiceGlowStyle::ALL {
        assert_eq!(
            VoiceGlowStyle::from_id(Some(style.id())),
            style,
            "id 应可往返"
        );
    }
    assert_eq!(
        VoiceGlowStyle::from_id(None),
        VoiceGlowStyle::Classic,
        "未配置用经典"
    );
    assert_eq!(
        VoiceGlowStyle::from_id(Some("nope")),
        VoiceGlowStyle::Classic,
        "未知值用经典"
    );
}

/// 场景：旧开关 `voice.window_inset_enabled` 迁移成风格。
/// 数据构造：旧值分别为 Some(true)、Some(false)、None。
/// 执行过程：调用 migrate_legacy。
/// 预期结果：开与未设置为经典；关闭过的用户保持关闭，不被升级重新打开。
#[test]
fn legacy_toggle_migrates_to_style() {
    assert_eq!(
        VoiceGlowStyle::migrate_legacy(Some(true)),
        VoiceGlowStyle::Classic,
        "旧开关开 → 经典"
    );
    assert_eq!(
        VoiceGlowStyle::migrate_legacy(None),
        VoiceGlowStyle::Classic,
        "未设置 → 经典"
    );
    assert_eq!(
        VoiceGlowStyle::migrate_legacy(Some(false)),
        VoiceGlowStyle::Off,
        "旧开关关 → 关闭"
    );
}

/// 场景：相位按偏好折叠——经典只在回复时出现，关闭永不出现，开关关闭后新风格仅回复。
/// 数据构造：经典 / 关闭 / 光环（开关开、关）× 四种相位。
/// 执行过程：effective_phase。
/// 预期结果：见各断言；回复相位在非关闭风格下始终保留。
#[test]
fn effective_phase_follows_prefs() {
    use VoiceGlowPhase::*;
    let prefs = |style, show| VoiceGlowPrefs {
        style,
        gain: Gain::Standard,
        show_listening_thinking: show,
    };
    let classic = prefs(VoiceGlowStyle::Classic, true);
    assert_eq!(classic.effective_phase(Listening), Idle, "经典忽略聆听");
    assert_eq!(classic.effective_phase(Speaking), Speaking, "经典保留回复");
    let off = prefs(VoiceGlowStyle::Off, true);
    assert_eq!(off.effective_phase(Speaking), Idle, "关闭永不显示");
    let halo_on = prefs(VoiceGlowStyle::Halo, true);
    assert_eq!(
        halo_on.effective_phase(Listening),
        Listening,
        "开关开：聆听可见"
    );
    assert_eq!(
        halo_on.effective_phase(Thinking),
        Thinking,
        "开关开：思考可见"
    );
    let halo_off = prefs(VoiceGlowStyle::Halo, false);
    assert_eq!(
        halo_off.effective_phase(Thinking),
        Idle,
        "开关关：思考折叠为空闲"
    );
    assert_eq!(
        halo_off.effective_phase(Speaking),
        Speaking,
        "开关关：回复仍可见"
    );
}

/// 场景：强度档位读取，未知值为标准。
/// 数据构造：三个已知 key 与一个未知 key。
/// 执行过程：Gain::from_key 并取 multiplier。
/// 预期结果：0.55 / 0.8 / 1.0；未知为 0.8。
#[test]
fn gain_keys_map_to_multipliers() {
    assert_eq!(Gain::from_key(Some("soft")).multiplier(), 0.55, "柔和");
    assert_eq!(Gain::from_key(Some("standard")).multiplier(), 0.8, "标准");
    assert_eq!(Gain::from_key(Some("vivid")).multiplier(), 1.0, "鲜明");
    assert_eq!(Gain::from_key(Some("x")).multiplier(), 0.8, "未知为标准");
}

/// 场景：光环厚度随能量单调增加，聆听比回复更薄，强度 0 时不出图。
/// 数据构造：同一帧在能量 0 / 1、回复 / 聆听下构建。
/// 执行过程：halo::build 后取最大模糊半径作厚度。
/// 预期结果：回复厚度 e1 > e0；聆听 e1 < 回复 e1；intensity=0 得到空列表。
#[test]
fn halo_thickens_with_energy_and_is_thinner_when_listening() {
    let quiet = max_blur(&halo::build(&frame(VoiceGlowPhase::Speaking, 0.0)));
    let loud = max_blur(&halo::build(&frame(VoiceGlowPhase::Speaking, 1.0)));
    let listening = max_blur(&halo::build(&frame(VoiceGlowPhase::Listening, 1.0)));
    assert!(loud > quiet, "能量越大光越厚：{loud} 应大于 {quiet}");
    assert!(listening < loud, "聆听比回复薄：{listening} 应小于 {loud}");
    let mut hidden = frame(VoiceGlowPhase::Speaking, 1.0);
    hidden.intensity = 0.0;
    assert!(halo::build(&hidden).is_empty(), "强度 0 不产生任何图元");
}

/// 场景：减少动态效果下光环不流动（颜色相位不随时间变化），只随能量变亮。
/// 数据构造：reduce_motion=true，t=0 与 t=5000ms 两帧。
/// 执行过程：构建并比较颜色序列。
/// 预期结果：两帧图元完全一致；而 reduce_motion=false 时颜色会变化。
#[test]
fn halo_reduce_motion_freezes_flow() {
    let colors = |f: &GlowFrame| {
        halo::build(f)
            .iter()
            .filter_map(|p| match p {
                GlowPrim::Glow { rgba, .. } => Some(*rgba),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut a = frame(VoiceGlowPhase::Speaking, 0.5);
    a.reduce_motion = true;
    a.t_ms = 0.0;
    let mut b = a.clone();
    b.t_ms = 5000.0;
    assert_eq!(colors(&a), colors(&b), "减少动态效果：颜色相位不推进");
    a.reduce_motion = false;
    b.reduce_motion = false;
    assert_ne!(colors(&a), colors(&b), "正常模式：颜色随时间流动");
}

/// 场景：声纹丝带两端收细为零，振幅随能量增加。
/// 数据构造：回复相位，能量 0 与 1。
/// 执行过程：取第一条填充多边形的点集，量测首尾点离基线的距离与整体最大偏移。
/// 预期结果：首尾点正好落在基线（偏移 0）；能量 1 的最大偏移大于能量 0。
#[test]
fn ribbon_tapers_to_zero_and_grows_with_energy() {
    let spread = |energy: f32| {
        let prims = ribbon::build(&frame(VoiceGlowPhase::Speaking, energy));
        let GlowPrim::Fill { pts, .. } = &prims[0] else {
            panic!("回复相位第一个图元应为丝带填充");
        };
        let base = 626.0 - 40.0;
        let ends = (pts[0].1 - base)
            .abs()
            .max((pts[pts.len() / 2 - 1].1 - base).abs());
        let peak = pts.iter().map(|p| (p.1 - base).abs()).fold(0.0, f32::max);
        (ends, peak)
    };
    let (ends_quiet, peak_quiet) = spread(0.0);
    let (_, peak_loud) = spread(1.0);
    assert!(
        ends_quiet < 0.01,
        "丝带两端应收细到基线，实际偏移 {ends_quiet}"
    );
    assert!(
        peak_loud > peak_quiet,
        "能量越大振幅越大：{peak_loud} 应大于 {peak_quiet}"
    );
}

/// 场景：声纹在聆听时是一条细线，不是填充丝带。
/// 数据构造：聆听相位。
/// 执行过程：构建。
/// 预期结果：含描边，不含填充。
#[test]
fn ribbon_listening_is_a_single_line() {
    let prims = ribbon::build(&frame(VoiceGlowPhase::Listening, 0.6));
    assert!(
        prims.iter().any(|p| matches!(p, GlowPrim::Stroke { .. })),
        "聆听应有描边线"
    );
    assert!(
        !prims.iter().any(|p| matches!(p, GlowPrim::Fill { .. })),
        "聆听不应有填充丝带"
    );
}

/// 场景：星尘粒子池——静默不生成、上限 220、固定容量、减少动态效果为零。
/// 数据构造：能量 0 与能量 1 的回复帧，各步进 10s（每步 16ms）。
/// 执行过程：StardustPool::step。
/// 预期结果：能量 0 且无历史时为空；能量 1 时数量不超过 220 且容量不增长；reduce_motion 时为 0。
#[test]
fn stardust_pool_is_bounded_and_quiet_when_silent() {
    let mut silent = StardustPool::default();
    for _ in 0..600 {
        silent.step(16.0, &frame(VoiceGlowPhase::Speaking, 0.0));
    }
    assert!(
        silent.is_empty(),
        "能量 0 不应生成粒子，实际 {}",
        silent.len()
    );
    let mut loud = StardustPool::default();
    let capacity = loud.capacity();
    for _ in 0..600 {
        loud.step(16.0, &frame(VoiceGlowPhase::Speaking, 1.0));
    }
    assert!(
        loud.len() > 20,
        "高能量应有可见数量粒子，实际 {}",
        loud.len()
    );
    assert!(loud.len() <= 220, "粒子数上限 220，实际 {}", loud.len());
    assert_eq!(loud.capacity(), capacity, "固定容量，帧内不重新分配");
    let mut reduced = StardustPool::default();
    let mut f = frame(VoiceGlowPhase::Speaking, 1.0);
    f.reduce_motion = true;
    for _ in 0..600 {
        reduced.step(16.0, &f);
    }
    assert!(reduced.is_empty(), "减少动态效果不生成粒子");
}
