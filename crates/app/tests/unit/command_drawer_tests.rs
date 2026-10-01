use super::*;

/// 场景：抽屉展开、筛选改变高度、中途收回再展开均从当前画面连续过渡。
///
/// 数据构造：单条建议高度 = 40 + 54 + 8 = 102px；5 条建议 = 40 + 5×54 + 8 = 318px。
///   展开 220ms，半程 110ms；收回 160ms，半程 80ms；cubic-out(0.5) = 0.875。
/// 执行过程：展开半程 → 相同目标刷新 → 筛选为单条 → 收回半程 → 反向展开 → 收回结束。
/// 预期结果：首帧不是最终高度；相同目标不重播；改变目标无跳跃；收回途中仍存在；结束停止动画。
#[test]
fn drawer_retargets_without_jumping_or_replaying() {
    let mut drawer = CommandDrawer::default();
    let expanded = 40.0 + 5.0 * 54.0 + 8.0;
    let filtered = 40.0 + 54.0 + 8.0;
    drawer.retarget(expanded);
    assert_eq!(drawer.height(false), 0.0, "展开首帧不能跳到最终高度");
    assert!(drawer.is_active(), "展开应请求后续动画帧");
    drawer.tick(Duration::from_millis(110), false);
    let middle = expanded * 0.875;
    assert_eq!(
        drawer.height(false),
        middle,
        "展开半程应使用快速起步、逐渐减速曲线"
    );
    assert!(
        drawer.opacity() > 0.0 && drawer.opacity() < 1.0,
        "展开中应淡入但尚未完全不透明"
    );
    drawer.retarget(expanded);
    assert_eq!(
        drawer.progress.value, 0.5,
        "继续输入但高度不变时不能重播动画"
    );
    drawer.retarget(filtered);
    assert_eq!(
        drawer.height(false),
        middle,
        "过滤建议后不能突然改变当前高度"
    );
    drawer.tick(Duration::from_millis(220), false);
    assert_eq!(
        drawer.height(false),
        filtered,
        "过滤后的抽屉应收至单条建议高度"
    );
    assert!(!drawer.is_active(), "到达目标后不能继续请求帧");
    drawer.retarget(0.0);
    assert_eq!(drawer.height(false), filtered, "关闭首帧应保留原高度");
    drawer.tick(Duration::from_millis(80), false);
    let closing = filtered * (1.0 - 0.875);
    assert_eq!(drawer.height(false), closing, "收回必须向底边逐渐收缩");
    assert!(drawer.opacity() > 0.0, "退出中不得提前移除抽屉");
    drawer.retarget(expanded);
    assert_eq!(
        drawer.height(false),
        closing,
        "快速重新输入应从收回中途反向展开"
    );
    drawer.tick(Duration::from_millis(220), false);
    assert_eq!(drawer.height(false), expanded, "重新展开应回到完整高度");
    drawer.retarget(0.0);
    drawer.tick(Duration::from_millis(160), false);
    assert_eq!(drawer.height(false), 0.0, "关闭后不能留下可见高度");
    assert_eq!(drawer.opacity(), 0.0, "关闭后应完全不可见");
    assert!(!drawer.is_active(), "关闭结束不能持续重绘");
}

/// 场景：减少动态效果时只淡入淡出，不移动抽屉边缘。
///
/// 数据构造：单条建议 = 40 + 54 + 8 = 102px；淡入淡出 160ms，半程 = 160 / 2 = 80ms。
/// 执行过程：展开 → 半程 → 结束 → 关闭半程 → 结束。
/// 预期结果：淡入淡出期间高度固定；透明度变化；最终透明且无后续帧。
#[test]
fn reduced_motion_keeps_drawer_geometry_still() {
    let mut drawer = CommandDrawer::default();
    let height = 40.0 + 54.0 + 8.0;
    drawer.retarget(height);
    drawer.tick(Duration::from_millis(80), true);
    assert_eq!(
        drawer.height(true),
        height,
        "减少动态效果不能从底部移动边缘"
    );
    assert!(
        drawer.opacity() > 0.0 && drawer.opacity() < 1.0,
        "减少动态效果仍应保留淡入反馈"
    );
    drawer.tick(Duration::from_millis(80), true);
    assert_eq!(drawer.opacity(), 1.0, "淡入结束应完整可见");
    drawer.retarget(0.0);
    drawer.tick(Duration::from_millis(80), true);
    assert_eq!(drawer.height(true), height, "退出只能淡出，不能滑动");
    assert!(drawer.opacity() < 1.0, "退出应逐渐降低透明度");
    drawer.tick(Duration::from_millis(80), true);
    assert_eq!(drawer.opacity(), 0.0, "退出结束应完全透明");
    assert!(!drawer.is_active(), "减少动态效果完成后不能持续重绘");
}
