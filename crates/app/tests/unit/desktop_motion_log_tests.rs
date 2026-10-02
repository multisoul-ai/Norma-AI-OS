use super::*;
use camera::DesktopCanvasSubject;
use std::time::Duration;

/// 切换日志保留未截断的回调间隔，并隔离连续改目标和减少动态效果的统计。
///
/// 数据构造（含关键数值的推导过程）：
///   A 的回调间隔为16、34、150ms，总计200ms；在220ms改目标。
///   长间隔阈值为2 / 60秒≈33.33ms，因此34和150ms计为2次；
///   相机每次最多推进100ms，因此150ms产生1次截断，但日志仍记录150ms。
///   B 在220ms开始，236ms收到回调，250ms结束：间隔16ms，总耗时30ms。
///   C 在250ms开始并立即因减少动态效果结束：0次回调、0ms耗时。
/// 执行过程：
///   1. 安装TRACE级局部日志收集器，开始A并推进三次回调。
///   2. 开始B以结束A，再推进并完成B；重复结束不得产生额外日志。
///   3. 开始并跳过C，读取六条开始／结束事件。
///
/// 预期结果：
///   - A明确以retargeted结束，B以completed结束，C以reduced_motion结束。
///   - A记录150ms而非相机截断后的100ms；B不继承A的慢回调计数。
///   - 每次切换有独立关联ID，改目标关联上一段；不得逐帧输出或记录NaN。
#[test]
fn desktop_motion_logs_bound_output_and_preserve_raw_callback_gaps() {
    let temp = tempfile::tempdir().expect("create isolated log directory");
    let log_path = temp.path().join("motion.log");
    let file = std::fs::File::create(&log_path).expect("create test log");
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(Mutex::new(file))
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let mut state = DesktopWorkspaceUiState::empty(DesktopWorkspaceStore::new(
            temp.path().join("unused.db"),
        ));
        let start = Instant::now();
        let rect = canvas::CanvasRect::new(0.0, 0.0, 1200.0, 700.0);
        let subject = DesktopCanvasSubject::Workspace("test-workspace".into());
        state.start_motion_log("workspace_expand", &subject, rect, rect, start);
        for (elapsed_ms, gap_ms) in [(16, 16), (50, 34), (200, 150)] {
            state
                .motion_log
                .as_mut()
                .expect("A is active")
                .record_callback(
                    start + Duration::from_millis(elapsed_ms),
                    Duration::from_millis(gap_ms),
                );
        }
        state.start_motion_log(
            "workspace_overview",
            &subject,
            rect,
            rect,
            start + Duration::from_millis(220),
        );
        state
            .motion_log
            .as_mut()
            .expect("B is active")
            .record_callback(
                start + Duration::from_millis(236),
                Duration::from_millis(16),
            );
        state.finish_motion_log("completed", start + Duration::from_millis(250));
        state.finish_motion_log("completed", start + Duration::from_millis(250));
        state.start_motion_log(
            "project_switch",
            &subject,
            rect,
            rect,
            start + Duration::from_millis(250),
        );
        state.finish_motion_log("reduced_motion", start + Duration::from_millis(250));
    });
    let logs = std::fs::read_to_string(log_path).expect("read captured motion logs");
    assert_eq!(
        logs.lines().count(),
        6,
        "三段只能各输出开始和结束，不能逐帧输出"
    );
    let interrupted = logs
        .lines()
        .find(|line| line.contains("outcome=\"retargeted\""))
        .expect("A must report interruption");
    assert!(
        interrupted.contains("elapsed_ms=220"),
        "A应包含末帧到改目标的20ms"
    );
    assert!(interrupted.contains("callbacks=3"), "A应收集三次回调");
    assert!(
        interrupted.contains("first_callback_ms=Some(16"),
        "首回调延迟应独立可读"
    );
    assert!(
        interrupted.contains("max_callback_gap_ms=150"),
        "卡顿不能被积分器的100ms上限掩盖"
    );
    assert!(
        interrupted.contains("long_callback_gaps=2"),
        "仅34ms和150ms超过阈值"
    );
    assert!(
        interrupted.contains("clamped_callback_gaps=1"),
        "仅150ms超过积分器上限"
    );
    let completed = logs
        .lines()
        .find(|line| line.contains("outcome=\"completed\""))
        .expect("B must report completion");
    assert!(completed.contains("elapsed_ms=30"), "B耗时从220ms重新开始");
    assert!(completed.contains("callbacks=1"), "B不能继承A的回调数");
    assert!(
        completed.contains("long_callback_gaps=0"),
        "B不能继承A的慢回调"
    );
    let skipped = logs
        .lines()
        .find(|line| line.contains("outcome=\"reduced_motion\""))
        .expect("C must explain why no animation ran");
    assert!(skipped.contains("callbacks=0"), "跳过动画不能虚报帧数");
    assert!(
        skipped.contains("first_callback_ms=None"),
        "未调度不能虚报首帧延迟"
    );
    assert!(!logs.contains("NaN"), "零帧摘要不能除零");
    let id = interrupted
        .split("transition_id=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert!(
        logs.contains(&format!("previous_transition_id=Some({id})")),
        "B应关联被中断的A"
    );
    let next_id = completed
        .split("transition_id=")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert_ne!(id, next_id, "连续改目标必须有不同的日志关联ID");
}
