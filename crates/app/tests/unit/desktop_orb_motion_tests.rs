use super::orb;
use std::time::Duration;

/// 场景：原型极光亮弧持续缓慢流动，并遵守系统减少动态效果设置。
/// 数据构造：原型 shader 亮弧每秒转动 0.28 弧度，约 22.44 秒一周；从 0.1、0.9 相位各推进 5.61 秒。
/// 执行过程：分别计算普通、跨周期及减少动态效果下的新相位。
/// 预期结果：普通状态推进四分之一周，跨周期回到 0.15；减少动态效果时保持原相位。
#[test]
fn assistant_orb_phase_flows_slowly_and_stops_for_reduced_motion() {
    let quarter = Duration::from_millis(5610);
    let flowing = orb::advance_orb_phase(0.1, quarter, false);
    let wrapped = orb::advance_orb_phase(0.9, quarter, false);
    let reduced = orb::advance_orb_phase(0.1, quarter, true);
    assert!(
        (flowing - 0.35).abs() < 0.001,
        "5.61 秒应约推进原型亮弧周期的四分之一"
    );
    assert!(
        (wrapped - 0.15).abs() < 0.001,
        "光环旋转经过整周后应平滑回绕"
    );
    assert_eq!(reduced, 0.1, "减少动态效果时光环不得持续转动");
    assert_ne!(flowing, reduced, "普通与减少动态效果必须有不同的运动结果");
}

/// 场景：GPUI 0.2.2 会把 SVG 变成单色遮罩，因此原型 WebGL 极光须保留为 RGBA 帧。
/// 数据构造：加载一张随程序内嵌的完整 2× 144px 光学体帧，读取中心雾光和四向环边像素。
/// 执行过程：检查原型的暗蓝中心、柔光边缘，再计算相邻帧混合权重。
/// 预期结果：资源保留原型 WebGL 画布的透明留白，中心有暗蓝雾光，边缘保留蓝紫亮弧。
#[test]
fn assistant_orb_asset_preserves_color_and_blends_adjacent_frames() {
    let frame = image::load_from_memory(include_bytes!("../../assets/icons/norma-orb-00.png"))
        .expect("应能解码预渲染光环 PNG")
        .into_rgba8();
    assert_eq!(
        frame.dimensions(),
        (288, 288),
        "光环资源应以完整 2× 分辨率呈现原型 144px 画布"
    );
    let center = frame.get_pixel(144, 144);
    assert!(center[3] > 30, "原型光环中心应有微弱的暗蓝雾光");
    assert!(center[2] > center[0], "原型光环中心应偏蓝而非灰黑");
    let top = frame.get_pixel(144, 48);
    let right = frame.get_pixel(240, 144);
    let bottom = frame.get_pixel(144, 240);
    assert_eq!(frame.get_pixel(0, 0)[3], 0, "原型画布角落应保留透明留白");
    assert!(top[2] > top[0], "圆环顶部应偏蓝，不得退化成白灰色");
    assert!(right[0] > top[0], "圆环右侧应有比顶部更明显的紫色红通道");
    assert!(top[3] > 80 && right[3] > 80, "圆环彩色像素必须足够可见");
    assert!(bottom[0] > top[0], "原型底部移动亮弧应比顶部更偏白");
    let (first, second, weight) = orb::orb_frames_for_phase(0.5 / 32.0);
    assert_eq!((first, second), (0, 1), "半帧相位应混合第 0、1 帧");
    assert!((weight - 0.5).abs() < 0.001, "相邻帧应以各 50% 平滑混合");
    assert_ne!(first, second, "动画混合不得重复同一帧");
}

/// 场景：原型在聆听、思考、播报时提高光场能量，切换状态仍连续过渡。
/// 数据构造：从待机能量 0.1 切到聆听目标 0.8，推进 50ms；减少动态效果单独计算。
/// 执行过程：按原型 180ms 指数响应推进，再读取聆听与播报资源。
/// 预期结果：普通模式只前进一部分，减少动态效果立即到目标，活跃帧比待机更亮。
#[test]
fn assistant_orb_energy_tracks_voice_mode_and_active_asset() {
    let next = orb::advance_orb_energy(
        0.1,
        orb::OrbMode::Listening,
        Duration::from_millis(50),
        false,
    );
    assert!(next > 0.1 && next < 0.8, "聆听光场应从待机亮度平滑提升");
    assert_eq!(
        orb::advance_orb_energy(0.1, orb::OrbMode::Listening, Duration::ZERO, true),
        0.8,
        "减少动态效果时应直接呈现聆听状态"
    );
    assert_eq!(
        orb::OrbMode::Thinking.target_energy(),
        0.45,
        "思考目标须与原型一致"
    );
    assert_eq!(
        orb::OrbMode::Speaking.target_energy(),
        0.65,
        "播报目标须与原型一致"
    );
    let idle = image::load_from_memory(include_bytes!("../../assets/icons/norma-orb-00.png"))
        .expect("应读取待机光环")
        .into_rgba8();
    let active =
        image::load_from_memory(include_bytes!("../../assets/icons/norma-orb-active-00.png"))
            .expect("应读取聆听光环")
            .into_rgba8();
    assert_eq!(
        active.dimensions(),
        idle.dimensions(),
        "状态帧与待机帧应同尺寸对齐"
    );
    assert!(
        active.get_pixel(144, 144)[3] > idle.get_pixel(144, 144)[3]
            && active.get_pixel(144, 48)[2] > idle.get_pixel(144, 48)[2],
        "聆听光场中心雾光与环边蓝光应随能量增强"
    );
}
