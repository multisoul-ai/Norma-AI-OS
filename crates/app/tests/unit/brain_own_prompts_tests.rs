use super::*;

/// 大脑自己派的任务要认得出来，前后空白不影响；只记最近 10 条。
///
/// 数据构造：记 12 条不同任务，其中第一条带前后空白。
/// 预期结果：
///   - 正断言：最近的任务认得出来（含去掉空白后的比较）
///   - 负断言：最早两条已被挤掉；用户自己的话认不成大脑派的
#[test]
fn own_tasks_are_recognised_and_capped() {
    let mut own = OwnPrompts::default();
    own.remember("  补上登录页的测试  ");
    for n in 1..=11 {
        own.remember(&format!("任务{n}"));
    }
    assert!(own.contains("任务11"));
    assert!(own.contains(" 任务2 "), "空白不影响比较");
    assert!(!own.contains("补上登录页的测试"), "最早的已被挤掉");
    assert!(!own.contains("用户自己说的话"));
}
