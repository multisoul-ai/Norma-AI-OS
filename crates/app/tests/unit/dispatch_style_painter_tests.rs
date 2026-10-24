use super::*;

/// 确定性哈希：同输入同输出、落在 [0,1)、不同序号分布不同。
///
/// 数据构造：seed=7，序号 0..1000 共 1000 个样本；均值期望 0.5，容差 0.05。
/// 预期结果：
///   - 正断言：全部在 [0,1)；重复计算完全一致；均值在 0.45..0.55。
///   - 负断言：不同 seed 的前 8 个值不全相同。
#[test]
fn unit_hash_is_deterministic_and_well_spread() {
    let values: Vec<f32> = (0..1000).map(|i| unit(7, i)).collect();
    assert!(
        values.iter().all(|x| (0.0..1.0).contains(x)),
        "应落在 [0,1)"
    );
    assert_eq!(values[10], unit(7, 10), "同输入应得到同输出");
    let mean = values.iter().sum::<f32>() / 1000.0;
    assert!((0.45..0.55).contains(&mean), "均值 {mean} 应接近 0.5");
    assert!(
        (0..8).any(|i| unit(7, i) != unit(8, i)),
        "不同 seed 应给出不同序列"
    );
}

/// 缓动在端点精确，且单调。
///
/// 数据构造：对 6 个缓动函数各取 0、1 与 0..1 的 101 个采样点。
/// 预期结果：
///   - 正断言：f(0)=0、f(1)=1（outBack 在 1 处也为 1）；out_cubic / in_out_cubic / in_out_sine / smooth 单调不减。
///   - 负断言：超出区间的输入被夹到端点（f(-1)=0、f(2)=1）。
#[test]
fn easings_hit_endpoints_and_are_monotonic() {
    type Easing = (&'static str, fn(f32) -> f32);
    let monotone: [Easing; 5] = [
        ("out_cubic", out_cubic),
        ("in_cubic", in_cubic),
        ("in_out_cubic", in_out_cubic),
        ("in_out_sine", in_out_sine),
        ("smooth", smooth),
    ];
    for (name, f) in monotone {
        assert!(
            (f(0.0)).abs() < 1e-6 && (f(1.0) - 1.0).abs() < 1e-6,
            "{name} 端点"
        );
        assert_eq!((f(-1.0), f(2.0)), (f(0.0), f(1.0)), "{name} 应夹到端点");
        let samples: Vec<f32> = (0..=100).map(|i| f(i as f32 / 100.0)).collect();
        assert!(
            samples.windows(2).all(|w| w[1] >= w[0] - 1e-6),
            "{name} 应单调不减"
        );
    }
    assert!((out_back(1.0) - 1.0).abs() < 1e-6 && out_back(0.0).abs() < 1e-6);
    assert!(out_back(0.7) > 1.0, "outBack 应有过冲");
}

/// 按弧长取点：端点精确，中点在直线折线的几何中点，range 保留拐点。
///
/// 数据构造：折线 (0,0)→(100,0)→(100,100)，总长 200。
/// 预期结果：
///   - 正断言：at(0)=(0,0)、at(1)=(100,100)、at(0.5)=(100,0)；range(0.25,0.75) 含拐点 (100,0) 共 3 个点。
///   - 负断言：range 的起点大于终点返回空。
#[test]
fn poly_is_arc_length_parameterised() {
    let poly = Poly::new(vec![v(0.0, 0.0), v(100.0, 0.0), v(100.0, 100.0)]);
    assert_eq!(poly.total, 200.0);
    assert_eq!(poly.at(0.0).p, v(0.0, 0.0));
    assert_eq!(poly.at(1.0).p, v(100.0, 100.0));
    assert_eq!(poly.at(0.5).p, v(100.0, 0.0));
    let part = poly.range(0.25, 0.75);
    assert_eq!(part.len(), 3, "应保留中间拐点");
    assert_eq!(part[1], v(100.0, 0.0));
    assert!(poly.range(0.8, 0.2).is_empty(), "反向区间为空");
}

/// 深浅主题用不同调色板：深色高亮、浅色饱和。
///
/// 数据构造：Theme::default() 的文字色决定深浅；再把文字色改成近黑模拟浅色主题。
/// 预期结果：
///   - 正断言：两种主题的 core 色不同；浅色主题的 core 比深色暗。
///   - 负断言：accent 始终取自主题而不是写死。
#[test]
fn palette_follows_theme_darkness() {
    let light = Theme {
        text_primary: 0x1a2026ff,
        ..Theme::default()
    };
    let dark = Theme {
        text_primary: 0xeef3f7ff,
        ..Theme::default()
    };
    let (l, d) = (Palette::for_theme(light), Palette::for_theme(dark));
    assert_ne!(l.core, d.core, "深浅主题应使用不同调色板");
    assert!(
        (l.core >> 8 & 0xff) < (d.core >> 8 & 0xff),
        "浅色主题的核心色应更暗"
    );
    assert_eq!(l.accent, light.accent_blue, "accent 取自主题");
}
