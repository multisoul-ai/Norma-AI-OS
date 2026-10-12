use super::state::{WallpaperSelection, WallpaperSettingsState};
use super::tests::TestRootGuard;
use std::time::Duration;

/// 切换静态壁纸时保留上一张图供交叉淡化，重复绑定同一选择不重启动效。
///
/// 数据构造（含关键数值的推导过程）：
///   初始壁纸 = home/earth-horizon.png；目标 = wallpapers/1350954.webp。
///   过渡总长 220ms；110ms = 220ms / 2，随后 110ms 应完成。
///
/// 执行过程：
///   1. 加载无偏好的状态，验证地球图为默认选择。
///   2. 切至 Aurora，推进 110ms，再重复设置 Aurora。
///   3. 再推进 110ms，观察过渡结束。
///
/// 预期结果：
///   - 正断言：中途保留地球图，且上一张图的透明度在 0 和 1 之间。
///   - 负断言：相同选择不重置过渡；完成后不再绘制旧壁纸。
#[test]
fn wallpaper_image_change_crossfades_without_restarting() {
    let guard = TestRootGuard::new();
    let mut state = WallpaperSettingsState::load_from_store(guard.settings_store());
    let earth = WallpaperSelection::Builtin {
        rel: crate::assets::HOME_BACKGROUND_ASSET,
    };
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };
    assert_eq!(state.selection, earth, "新 workspace 默认应使用首页地球图");

    state.set_effective_selection(aurora.clone(), false);
    assert_eq!(
        state.transition_from(),
        Some(&earth),
        "切换时必须保留上一张地球图"
    );
    state.tick_transition(Duration::from_millis(110));
    let middle = state.transition_from_opacity();
    assert!(middle > 0.0 && middle < 1.0, "过渡中应同时看见新旧图片");

    state.set_effective_selection(aurora, false);
    assert_eq!(
        state.transition_from_opacity(),
        middle,
        "重复绑定同一图片不能重启动效"
    );
    state.tick_transition(Duration::from_millis(110));
    assert_eq!(state.transition_from(), None, "过渡结束后必须移除旧图片");
    assert_eq!(
        state.transition_from_opacity(),
        0.0,
        "过渡结束后旧图片必须完全不可见"
    );
}

/// 过渡中再次切图时，当前可见的两张图继续淡出，不应突然替换旧层。
///
/// 数据构造（含关键数值的推导过程）：
///   A = Earth，B = Aurora，C = Nebula；首次过渡推进 110ms = 220ms / 2。
///   在 A→B 中途改为 C 时，出场层应是 B(1) + A(0..1)，共 2 层。
///
/// 执行过程：
///   1. A→B 并推进到中间帧，记录 A 的当前透明度。
///   2. 立即选择 C，读取新过渡的出场层与透明度。
///
/// 预期结果：
///   - 正断言：B 在 A 下方完整保留，A 保持切换瞬间的透明度。
///   - 负断言：A 不能被 B 瞬间替换，C 不能成为旧层。
#[test]
fn rapid_wallpaper_changes_keep_current_composite() {
    let guard = TestRootGuard::new();
    let mut state = WallpaperSettingsState::load_from_store(guard.settings_store());
    let earth = state.selection.clone();
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };
    let nebula = WallpaperSelection::Builtin {
        rel: "wallpapers/1358524.webp",
    };
    state.set_effective_selection(aurora.clone(), false);
    state.tick_transition(Duration::from_millis(110));
    let earth_opacity = state.transition_from_opacity();

    state.set_effective_selection(nebula.clone(), false);
    let layers: Vec<_> = state.transition_layers().collect();
    assert_eq!(layers.len(), 2, "中途切换应保留两张旧图片");
    assert_eq!(layers[0].0, &aurora, "Aurora 应作为完整的底层旧图");
    assert_eq!(layers[0].1, 1.0, "Aurora 在切换瞬间必须完全可见");
    assert_eq!(layers[1].0, &earth, "Earth 必须留在旧合成画面的顶层");
    assert_eq!(layers[1].1, earth_opacity, "Earth 的显示透明度不能跳变");
    assert_ne!(layers[0].0, &nebula, "新目标 Nebula 不能被当作旧层");
}

/// 减少动态效果时切换图片应直接显示目标图。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Earth；目标 Aurora；普通淡化时长 220ms，但 reduce_motion=true 时应为 0ms。
///
/// 执行过程：
///   1. 加载默认 Earth，启用减少动态效果并选择 Aurora。
///   2. 立即读取当前选择与旧图叠层。
///
/// 预期结果：
///   - 正断言：Aurora 立即成为当前选择。
///   - 负断言：不保留 Earth 叠层，也不请求动画帧。
#[test]
fn reduced_motion_skips_wallpaper_crossfade() {
    let guard = TestRootGuard::new();
    let mut state = WallpaperSettingsState::load_from_store(guard.settings_store());
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };

    state.set_effective_selection(aurora.clone(), true);

    assert_eq!(state.selection, aurora, "目标图片应立即成为当前选择");
    assert_eq!(
        state.transition_from(),
        None,
        "减少动态效果时不应保留旧壁纸"
    );
    assert!(!state.transition_active(), "减少动态效果时不应驱动淡化帧");
}

/// 可选图片下载完成时，地球回退画面应平滑退场。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 Earth；可选目标 Aurora；图片淡化总长 220ms，完成事件是第 0ms。
///
/// 执行过程：
///   1. 选择 Aurora 并结束首次选择过渡，模拟等待下载。
///   2. 发送 Aurora 下载完成事件，再发送无关图片完成事件。
///
/// 预期结果：
///   - 正断言：Aurora 仍为目标，Earth 作为旧画面开始淡出。
///   - 负断言：无关完成事件不得重启或改变当前过渡。
#[test]
fn downloaded_wallpaper_fades_from_fallback() {
    let guard = TestRootGuard::new();
    let mut state = WallpaperSettingsState::load_from_store(guard.settings_store());
    let aurora = WallpaperSelection::Builtin {
        rel: "wallpapers/1350954.webp",
    };
    state.set_effective_selection(aurora.clone(), false);
    state.tick_transition(Duration::from_millis(220));
    assert!(
        !state.transition_active(),
        "首次选择过渡应先结束，模拟下载等待"
    );

    state.image_resource_ready("wallpapers/1350954.webp", false);
    assert_eq!(state.selection, aurora, "下载完成不得改变用户选择");
    assert_eq!(
        state.transition_from(),
        Some(&WallpaperSelection::default()),
        "下载完成应从地球回退图开始淡化"
    );
    let opacity = state.transition_from_opacity();
    state.image_resource_ready("wallpapers/1358524.webp", false);
    assert_eq!(
        state.transition_from_opacity(),
        opacity,
        "无关图片完成事件不得重启当前壁纸过渡"
    );
}
