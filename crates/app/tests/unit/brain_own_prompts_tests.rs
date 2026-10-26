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

/// 场景：幕僚派发的任务带约束后超过 60 字符，hook 只回传截断标题。
/// 构造：记住“任务+约束块”长文本，取 extract_prompt_text 同样的 60 字符加省略号。
/// 执行：对截断标题与不相关的 60 字符标题调用 contains。
/// 预期：截断标题识别为幕僚自己的提示；无关标题不识别。
#[test]
fn own_prompt_matches_hook_truncated_title_of_handed_off_task() {
    let mut own = OwnPrompts::default();
    let full = format!(
        "检查边界条件\n\n当前用户目标与约束：{}",
        "只修改前端".repeat(20)
    );
    own.remember(&full);
    let title = format!("{}…", full.chars().take(60).collect::<String>());
    assert!(own.contains(&title), "截断后的自派任务必须仍被识别");
    assert!(
        !own.contains(&format!("{}…", "别的".repeat(30))),
        "无关提示不能误判"
    );
}
