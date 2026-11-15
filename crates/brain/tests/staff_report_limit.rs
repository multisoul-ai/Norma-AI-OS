use brain::staff::gate::ReportLimiter;
use std::time::{Duration, Instant};

/// 场景：幕僚对同一任务 10 分钟内最多主动 report 2 条，超出的要写进任务对话，不出声。
/// 数据构造：窗口 10 分钟、上限 2；任务 a 连续 3 次，任务 b 1 次，之后过了窗口再发 a。
/// 预期结果：a 前两次放行、第三次拒绝；b 不受影响；窗口过后 a 又可以发。
#[test]
fn proactive_reports_are_capped_per_task_within_a_window() {
    let start = Instant::now();
    let mut limiter = ReportLimiter::new(Duration::from_secs(600), 2);
    assert!(limiter.allow("a", start));
    assert!(limiter.allow("a", start + Duration::from_secs(10)));
    assert!(
        !limiter.allow("a", start + Duration::from_secs(20)),
        "third in the window"
    );
    assert!(
        limiter.allow("b", start + Duration::from_secs(20)),
        "other tasks are independent"
    );
    assert!(
        limiter.allow("a", start + Duration::from_secs(700)),
        "window has passed"
    );
}
