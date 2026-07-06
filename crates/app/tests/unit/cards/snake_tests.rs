use super::*;
use gpui::size;

fn test_bounds(width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)))
}

fn test_snake(head_phase: f32) -> WorkingSnake {
    WorkingSnake {
        head_phase,
        tail_fraction: 0.24,
        head_color: 0x27d7eae6,
        tail_color: 0x9b55ffe6,
        glow_color: 0x27d7ea66,
        inset_px: 3.0,
        stroke_px: 3.0,
    }
}

/// SP-1: 非跨零 snake segment 只应生成一条渐变路径，不应额外生成 glow 路径。
///
/// 数据构造（含关键数值的推导过程）：
///   bounds = 320x220 px，node_radius = 22px，zoom = 1.0。
///   head_phase = 0.40，tail_fraction = 0.24：
///     tail = 0.40 - 0.24 = 0.16
///     tail <= head，因此 snake_ranges 只生成一个连续区间 [0.16, 0.40]。
///   性能目标是每个区间只绘制主渐变 stroke：
///     paths = 1 range * 1 primary path = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造不跨零的 WorkingSnake。
///   2. 调用 build_snake_paths 生成可绘制路径。
///   3. 读取 paths.len() 判断是否还存在额外 glow path。
///
/// 预期结果：
///   - 正断言：paths.len() 精确等于 1，说明只绘制主渐变 segment。
///   - 负断言：paths.len() 不应等于 2，否则仍在为同一区间额外绘制 glow。
#[test]
fn test_non_wrapping_snake_segment_builds_only_primary_path() {
    let paths = build_snake_paths(test_bounds(320.0, 220.0), test_snake(0.40), 1.0, 22.0);

    assert_eq!(
        paths.len(),
        1,
        "非跨零 snake 区间应只生成 1 条主渐变 path，实际为 {}",
        paths.len()
    );
    assert_ne!(
        paths.len(),
        2,
        "非跨零 snake 区间不应同时生成 glow 和主线两条 path"
    );
}

/// SP-2: 跨零 snake segment 应按两个路径区间绘制，但每个区间仍只保留主渐变路径。
///
/// 数据构造（含关键数值的推导过程）：
///   bounds = 320x220 px，node_radius = 22px，zoom = 1.0。
///   head_phase = 0.10，tail_fraction = 0.24：
///     raw_tail = 0.10 - 0.24 = -0.14
///     rem_euclid 后 tail = 0.86，tail > head，因此拆成 [0.86, 1.0] 与 [0.0, 0.10] 两段。
///   性能目标是每个区间只绘制主渐变 stroke：
///     paths = 2 ranges * 1 primary path = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造跨零的 WorkingSnake。
///   2. 调用 build_snake_paths 生成可绘制路径。
///   3. 读取 paths.len() 判断跨零场景是否还为每段额外绘制 glow。
///
/// 预期结果：
///   - 正断言：paths.len() 精确等于 2，说明跨零拆段正常。
///   - 负断言：paths.len() 不应等于 4，否则两段都还在额外绘制 glow。
#[test]
fn test_wrapping_snake_segment_builds_one_primary_path_per_range() {
    let paths = build_snake_paths(test_bounds(320.0, 220.0), test_snake(0.10), 1.0, 22.0);

    assert_eq!(
        paths.len(),
        2,
        "跨零 snake 区间应生成 2 条主渐变 path，实际为 {}",
        paths.len()
    );
    assert_ne!(
        paths.len(),
        4,
        "跨零 snake 区间不应为两段都额外生成 glow path"
    );
}
