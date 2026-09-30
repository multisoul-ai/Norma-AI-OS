use super::*;

/// BC-1: 未激活呼吸（Inactive）时直接回退到主题静态描边色。
///
/// 数据构造：theme = glass_dark()，node_stroke = 0xffffff24。
///
/// 执行过程：调用 breathing_color_for_phase(任意 phase, Inactive, theme)。
///
/// 预期结果：
///   - 正断言：返回值精确等于 theme.node_stroke
///   - 负断言：不应等于任何 accent 色（说明没有误入色相循环分支）
#[test]
fn test_breathing_color_inactive_falls_back_to_static_stroke() {
    let theme = Theme::glass_dark();
    let color = breathing_color_for_phase(0.37, BreathingState::Inactive, theme);
    assert_eq!(
        color, theme.node_stroke,
        "Inactive 应直接返回静态 node_stroke，不参与呼吸"
    );
    assert_ne!(color, theme.accent_cyan, "Inactive 不应误入冷色组色相循环");
}

/// BC-2: Cool 色组 phase=0.0 时落在循环起点 cyan，alpha 处于呼吸中点。
///
/// 数据构造（含关键数值推导）：
///   sin(0 * 2π) = 0 → t = (0+1)/2 = 0.5
///   alpha = 0x66 + 0.5 * (0xFF - 0x66) = 102 + 76.5 = 178.5 → round = 179 = 0xB3
///   phase=0.0 时 seg=1/3，idx=floor(0/0.3333)=0，local_t=0 → 颜色精确等于 accent_cyan 的 RGB
///
/// 执行过程：调用 breathing_color_for_phase(0.0, Cool, theme)。
///
/// 预期结果：
///   - 正断言：RGB 部分精确等于 accent_cyan 的 RGB
///   - 正断言：alpha 字节精确等于 0xB3
///   - 负断言：不应直接复用 accent_cyan 原始 alpha（0xff）
#[test]
fn test_breathing_color_cool_phase_zero_starts_at_cyan_with_midpoint_alpha() {
    let theme = Theme::glass_dark();
    let color = breathing_color_for_phase(0.0, BreathingState::Cool, theme);
    assert_eq!(
        color & 0xffffff00,
        theme.accent_cyan & 0xffffff00,
        "phase=0.0 时 Cool 色组应精确从 accent_cyan 起步，实际 {color:#010x}"
    );
    assert_eq!(
        (color & 0xff) as u8,
        0xB3,
        "phase=0.0 呼吸 alpha 应为 sin 中点 0xB3，实际 {:#04x}",
        color & 0xff
    );
    assert_ne!(
        (color & 0xff) as u8,
        0xff,
        "呼吸 alpha 不应直接沿用 accent_cyan 原始不透明 alpha"
    );
}

/// BC-3: Cool 色组 phase=0.5 时位于 violet→blue 分段中点，alpha 回到呼吸中点。
///
/// 数据构造（含关键数值推导）：
///   accent_violet = 0x9b55ffff (R155,G85,B255)
///   accent_blue   = 0x4b70ffff (R75,G112,B255)
///   seg = 1/3 ≈ 0.3333，idx = floor(0.5/0.3333) = 1，local_t = (0.5-0.3333)/0.3333 = 0.5
///   R = round((155+75)/2) = 115 = 0x73
///   G = round((85+112)/2) = round(98.5) = 99 = 0x63
///   B = 255 = 0xff
///   alpha：sin(0.5*2π) = sin(π) = 0 → t = 0.5 → alpha = 179 = 0xB3（与 phase=0.0 相同的呼吸中点）
///
/// 执行过程：调用 breathing_color_for_phase(0.5, Cool, theme)。
///
/// 预期结果：
///   - 正断言：颜色精确等于推导出的 0x7363ffb3
///   - 负断言：不应停留在 violet 或 blue 任一端点
#[test]
fn test_breathing_color_cool_phase_half_mixes_violet_and_blue() {
    let theme = Theme::glass_dark();
    let color = breathing_color_for_phase(0.5, BreathingState::Cool, theme);
    assert_eq!(
        color, 0x7363ffb3,
        "phase=0.5 应精确落在 violet/blue 中点叠加呼吸中点 alpha，实际 {color:#010x}"
    );
    assert_ne!(
        color & 0xffffff00,
        theme.accent_violet & 0xffffff00,
        "phase=0.5 不应停留在 violet 端点"
    );
    assert_ne!(
        color & 0xffffff00,
        theme.accent_blue & 0xffffff00,
        "phase=0.5 不应停留在 blue 端点"
    );
}

/// BC-4: Cool 色组 phase=0.75 时位于 blue→cyan 回环分段，alpha 处于呼吸谷值。
///
/// 数据构造（含关键数值推导）：
///   accent_blue = 0x4b70ffff (R75,G112,B255)，accent_cyan = 0x27d7eaff (R39,G215,B234)
///   seg=1/3，idx=floor(0.75/0.3333)=2，local_t=(0.75-0.6667)/0.3333=0.25
///   R = round(75+(39-75)*0.25) = round(66) = 66 = 0x42
///   G = round(112+(215-112)*0.25) = round(137.75) = 138 = 0x8a
///   B = round(255+(234-255)*0.25) = round(249.75) = 250 = 0xfa
///   alpha：sin(0.75*2π) = sin(1.5π) = -1 → t=0 → alpha = 0x66（呼吸谷值，最暗）
///
/// 执行过程：调用 breathing_color_for_phase(0.75, Cool, theme)。
///
/// 预期结果：
///   - 正断言：颜色精确等于推导出的 0x428afa66
///   - 负断言：alpha 不应是呼吸峰值 0xff
#[test]
fn test_breathing_color_cool_phase_three_quarters_wraps_toward_cyan_at_dim_alpha() {
    let theme = Theme::glass_dark();
    let color = breathing_color_for_phase(0.75, BreathingState::Cool, theme);
    assert_eq!(
        color, 0x428afa66,
        "phase=0.75 应精确落在 blue→cyan 回环分段且 alpha 为谷值，实际 {color:#010x}"
    );
    assert_ne!(
        (color & 0xff) as u8,
        0xff,
        "phase=0.75 处于呼吸谷值，alpha 不应是峰值 0xff"
    );
}

/// BC-5: Warm 色组 phase=0.0 时精确从 accent_warm 起步。
///
/// 数据构造（含关键数值推导）：
///   accent_warm = 0xf2c26bff (R242,G194,B107)
///   alpha 推导与 BC-2 相同：phase=0.0 → alpha = 0xB3
///
/// 执行过程：调用 breathing_color_for_phase(0.0, Warm, theme)。
///
/// 预期结果：
///   - 正断言：颜色精确等于 0xf2c26bb3
///   - 负断言：不应等于 Cool 色组在同一相位的结果（两色组不应混淆）
#[test]
fn test_breathing_color_warm_phase_zero_starts_at_warm() {
    let theme = Theme::glass_dark();
    let warm_color = breathing_color_for_phase(0.0, BreathingState::Warm, theme);
    let cool_color = breathing_color_for_phase(0.0, BreathingState::Cool, theme);
    assert_eq!(
        warm_color, 0xf2c26bb3,
        "phase=0.0 时 Warm 色组应精确从 accent_warm 起步，实际 {warm_color:#010x}"
    );
    assert_ne!(
        warm_color, cool_color,
        "Warm 与 Cool 色组在同一相位不应给出相同颜色"
    );
}

/// BC-6: Warm 色组 phase=0.5（分段边界）精确落在 accent_orange。
///
/// 数据构造（含关键数值推导）：
///   accent_orange = 0xff7932ff (R255,G121,B50)
///   Warm 只有两色，seg=0.5，phase=0.5 时 idx=min(floor(1.0),1)=1，local_t=(0.5-0.5)/0.5=0
///   → mix(colors[1]=orange, colors[0]=warm, t=0) 精确等于 orange
///   alpha 推导与 BC-3 相同：phase=0.5 → alpha = 0xB3
///
/// 执行过程：调用 breathing_color_for_phase(0.5, Warm, theme)。
///
/// 预期结果：
///   - 正断言：颜色精确等于 0xff7932b3
///   - 负断言：不应等于 phase=0.0 时的 warm 起点颜色
#[test]
fn test_breathing_color_warm_phase_half_lands_on_orange_boundary() {
    let theme = Theme::glass_dark();
    let color = breathing_color_for_phase(0.5, BreathingState::Warm, theme);
    assert_eq!(
        color, 0xff7932b3,
        "phase=0.5 应精确落在 accent_orange 分段边界，实际 {color:#010x}"
    );
    assert_ne!(
        color, 0xf2c26bb3,
        "phase=0.5 不应仍停留在 phase=0.0 的 warm 起点颜色"
    );
}

/// BC-7: 呼吸周期映射——冷色组 2.5s，暖色组 1.2s（更急促），未激活无周期。
///
/// 数据构造：SPEC 验收标准要求冷色组周期 ≈2.5s，暖色组周期 ≈1.2s 且明显更快。
///
/// 执行过程：分别读取三种 BreathingState 的 period()。
///
/// 预期结果：
///   - 正断言：Cool=2500ms，Warm=1200ms，Inactive=0ms
///   - 负断言：Warm 周期不应大于或等于 Cool 周期（暖色组必须更急促）
#[test]
fn test_breathing_period_mapping_matches_spec() {
    assert_eq!(
        BreathingState::Cool.period(),
        std::time::Duration::from_millis(2500),
        "冷色组（运行中）呼吸周期应为 2.5s"
    );
    assert_eq!(
        BreathingState::Warm.period(),
        std::time::Duration::from_millis(1200),
        "暖色组（等待批准）呼吸周期应为 1.2s"
    );
    assert_eq!(
        BreathingState::Inactive.period(),
        std::time::Duration::ZERO,
        "未激活状态不应有呼吸周期"
    );
    assert!(
        BreathingState::Warm.period() < BreathingState::Cool.period(),
        "等待批准的暖色组节奏必须比运行中的冷色组更急促"
    );
}
