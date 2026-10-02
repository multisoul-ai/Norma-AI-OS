use super::load_timeline::{BrowserLoadPhase, LoadTimeline};
use std::time::{Duration, Instant};

/// L-1: 一次完整导航必须产出「创建 / 提交 / 完成」三段耗时，且只报告一次。
///
/// 数据构造（含关键数值的推导过程）：
///   t0 = 导航起点（Browser 卡片开始创建原生视图）；
///   create = 42ms（原生视图同步创建耗时，由调用方测得后传入）；
///   Started = t0 + 180ms（WebKit didCommitNavigation）；
///   Finished = t0 + 950ms（WebKit didFinishNavigation）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. begin(t0, Some(42ms)) 开始一次导航并记录创建耗时
///   2. record(Started, t0+180ms) 只记录首个提交点，不产出报告
///   3. record(Finished, t0+950ms) 产出报告
///   4. 再来一次 Finished（WebKit 对同文档跳转会重复触发）
///
/// 预期结果：
///   - 正断言：Started 之前不产出报告；Finished 时报告 committed=180ms、finished=950ms、create=42ms
///   - 负断言：第二次 Finished 不得再产出报告，避免把页内跳转误算成整页加载
#[test]
fn test_timeline_reports_create_commit_finish_once() {
    let t0 = Instant::now();
    let mut timeline = LoadTimeline::default();
    timeline.begin(t0, Some(Duration::from_millis(42)));

    let after_started = timeline.record(BrowserLoadPhase::Started, t0 + Duration::from_millis(180));
    let report = timeline
        .record(BrowserLoadPhase::Finished, t0 + Duration::from_millis(950))
        .expect("Finished 必须为已 begin 的导航产出加载报告");
    let duplicate = timeline.record(BrowserLoadPhase::Finished, t0 + Duration::from_millis(990));

    assert!(
        after_started.is_none(),
        "仅提交（Started）时页面尚未加载完成，不应产出报告"
    );
    assert_eq!(
        report.create,
        Some(Duration::from_millis(42)),
        "报告必须带上原生视图创建耗时"
    );
    assert_eq!(
        report.committed,
        Some(Duration::from_millis(180)),
        "提交耗时 = Started - 导航起点 = 180ms"
    );
    assert_eq!(
        report.finished,
        Duration::from_millis(950),
        "完成耗时 = Finished - 导航起点 = 950ms"
    );
    assert!(
        duplicate.is_none(),
        "同一次导航的重复 Finished 不得再次产出报告"
    );
}

/// L-2: 页面自行发起的跳转（未经 begin）不得被记入加载耗时。
///
/// 数据构造（含关键数值的推导过程）：
///   未调用 begin，直接送入 Started(t0) / Finished(t0+500ms)，模拟用户点击页内链接。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 空时间线收到 Started
///   2. 空时间线收到 Finished
///
/// 预期结果：
///   - 负断言：两次 record 都返回 None，没有起点的事件不能凭空生成耗时
#[test]
fn test_timeline_ignores_events_without_navigation_start() {
    let t0 = Instant::now();
    let mut timeline = LoadTimeline::default();

    let started = timeline.record(BrowserLoadPhase::Started, t0);
    let finished = timeline.record(BrowserLoadPhase::Finished, t0 + Duration::from_millis(500));

    assert!(started.is_none(), "没有 begin 的 Started 不得产出报告");
    assert!(
        finished.is_none(),
        "没有 begin 的 Finished 不得产出报告，否则页内跳转会污染首屏耗时统计"
    );
}

/// L-3: 重新导航（begin 再次调用）必须重置上一次的提交点，且重定向的多次提交只取首个。
///
/// 数据构造（含关键数值的推导过程）：
///   第一次导航 t0 起点，Started 于 +100ms；随后地址栏输入新地址，t1 = t0 + 2s 再次 begin（无创建耗时）；
///   新导航先后收到两次 Started（302 重定向）：t1+60ms、t1+200ms，Finished 于 t1+400ms。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次导航仅 Started，不完成
///   2. begin(t1, None) 重置
///   3. 两次 Started 后 Finished
///
/// 预期结果：
///   - 正断言：committed = 60ms（首个提交点，而非重定向后的 200ms，也不是第一次导航的 100ms）
///   - 正断言：finished = 400ms，create = None（复用已创建的视图）
#[test]
fn test_timeline_resets_on_new_navigation_and_keeps_first_commit() {
    let t0 = Instant::now();
    let t1 = t0 + Duration::from_secs(2);
    let mut timeline = LoadTimeline::default();
    timeline.begin(t0, Some(Duration::from_millis(30)));
    let _ = timeline.record(BrowserLoadPhase::Started, t0 + Duration::from_millis(100));

    timeline.begin(t1, None);
    let _ = timeline.record(BrowserLoadPhase::Started, t1 + Duration::from_millis(60));
    let _ = timeline.record(BrowserLoadPhase::Started, t1 + Duration::from_millis(200));
    let report = timeline
        .record(BrowserLoadPhase::Finished, t1 + Duration::from_millis(400))
        .expect("新导航完成时必须产出报告");

    assert_eq!(
        report.committed,
        Some(Duration::from_millis(60)),
        "重定向产生多次提交时只应记录首个提交点 60ms"
    );
    assert_eq!(
        report.finished,
        Duration::from_millis(400),
        "完成耗时必须相对新导航起点 t1 计算"
    );
    assert_eq!(
        report.create, None,
        "复用已有视图的导航没有创建耗时，不能沿用上一次导航的 30ms"
    );
}

/// C-1: 原生视图圆角裁剪只在半径变化时重新应用，缩放/拖拽的连续 set_bounds 不得重复改写 CALayer。
///
/// 数据构造（含关键数值的推导过程）：
///   创建时半径 = 12.0px；随后 3 次 set_bounds 半径不变（12.0）；最后一次窗口缩放使半径变为 8.0px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 首次 needs_apply(12.0) → 尚未应用过，必须应用
///   2. 连续 3 次 needs_apply(12.0) → 半径未变，全部跳过
///   3. needs_apply(8.0) → 半径变化，必须重新应用
///
/// 预期结果：
///   - 正断言：首次与半径变化时返回 true
///   - 负断言：半径不变的 3 次调用都返回 false
#[test]
fn test_corner_clip_only_reapplies_when_radius_changes() {
    let mut clip = super::load_timeline::CornerClipState::default();

    assert!(clip.needs_apply(12.0), "首次创建必须应用圆角裁剪");
    for call in 1..=3 {
        assert!(
            !clip.needs_apply(12.0),
            "第 {call} 次 set_bounds 半径未变，不得重复改写 CALayer"
        );
    }
    assert!(clip.needs_apply(8.0), "半径从 12.0 变为 8.0 时必须重新应用");
}

/// L-4: 加载期间的 bounds 变化次数必须计入报告，且每次新导航从 0 重新计数。
///
/// 数据构造（含关键数值的推导过程）：
///   第一次导航 t0：加载中窗口布局动画触发 3 次 set_bounds（420ms 动画 ≈ 25 帧，此处取 3 次代表）；
///   第二次导航 t1 = t0 + 1s：加载中只有 1 次 set_bounds。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. begin(t0) 后 note_resize 3 次，Finished 产出报告
///   2. begin(t1) 后 note_resize 1 次，Finished 产出报告
///   3. 导航完成后再 note_resize（无活动导航）
///
/// 预期结果：
///   - 正断言：第一份报告 resizes = 3，第二份 resizes = 1（重新计数，不累加）
///   - 负断言：无活动导航时 note_resize 不得使后续报告出现偏差（不 panic，且第三次导航从 0 开始）
#[test]
fn test_timeline_counts_resizes_per_navigation() {
    let t0 = Instant::now();
    let t1 = t0 + Duration::from_secs(1);
    let mut timeline = LoadTimeline::default();

    timeline.begin(t0, None);
    for _ in 0..3 {
        timeline.note_resize();
    }
    let first = timeline
        .record(BrowserLoadPhase::Finished, t0 + Duration::from_millis(300))
        .expect("第一次导航完成必须产出报告");

    timeline.begin(t1, None);
    timeline.note_resize();
    let second = timeline
        .record(BrowserLoadPhase::Finished, t1 + Duration::from_millis(200))
        .expect("第二次导航完成必须产出报告");

    timeline.note_resize();
    timeline.begin(t1 + Duration::from_secs(1), None);
    let third = timeline
        .record(BrowserLoadPhase::Finished, t1 + Duration::from_secs(2))
        .expect("第三次导航完成必须产出报告");

    assert_eq!(first.resizes, 3, "第一次导航加载期间 3 次 bounds 变化");
    assert_eq!(
        second.resizes, 1,
        "第二次导航必须重新计数为 1，而非累加成 4"
    );
    assert_eq!(
        third.resizes, 0,
        "无活动导航时的 note_resize 不得泄漏到下一次导航"
    );
}

/// L-5: 进度线只在一次导航进行中显示，完成或超时后必须消失。
///
/// 数据构造（含关键数值的推导过程）：
///   t0 = 导航起点；
///   LOAD_INDICATOR_TIMEOUT = 30s，超过后即使没有 Finished 也视为已停止，
///   避免 WebKit 加载失败（不触发 Finished）时进度线永远转动；
///   probe_before = t0 + 29s（未超时），probe_after = t0 + 31s（已超时）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 未 begin 时查询 is_loading
///   2. begin(t0) 后在 t0+1s 查询，再在 29s / 31s 查询
///   3. 重新 begin(t0+40s)，随后 Finished 产出报告，再查询
///
/// 预期结果：
///   - 正断言：begin 后 1s、29s 内 is_loading = true
///   - 负断言：从未 begin 时为 false；31s 超时后为 false；Finished 之后为 false
///
/// L-6：取消加载后进度线立即结束，且后续 Finished 不再产生报告
///
/// 执行过程：
///   1. begin(t0) → 加载中
///   2. cancel() → active 被清除
///   3. record(Finished, t0+2s) → 无 active，返回 None
///
/// 预期结果：
///   - is_loading 为 false：停止按钮点击后进度线必须消失
///   - record 返回 None：已取消的加载不应再上报耗时
#[test]
fn test_timeline_cancel_ends_loading() {
    let t0 = Instant::now();
    let mut timeline = LoadTimeline::default();
    timeline.begin(t0, None);
    assert!(
        timeline.is_loading(t0),
        "begin must mark the timeline as loading"
    );
    timeline.cancel();
    assert!(
        !timeline.is_loading(t0 + Duration::from_secs(1)),
        "cancelled load must not keep showing the progress line"
    );
    assert!(
        timeline
            .record(BrowserLoadPhase::Finished, t0 + Duration::from_secs(2))
            .is_none(),
        "finish after cancel must not emit a timing report"
    );
}

#[test]
fn test_timeline_is_loading_until_finished_or_timeout() {
    let t0 = Instant::now();
    let mut timeline = LoadTimeline::default();

    let idle = timeline.is_loading(t0);
    timeline.begin(t0, None);
    let loading_soon = timeline.is_loading(t0 + Duration::from_secs(1));
    let loading_before_timeout = timeline.is_loading(t0 + Duration::from_secs(29));
    let loading_after_timeout = timeline.is_loading(t0 + Duration::from_secs(31));

    let t1 = t0 + Duration::from_secs(40);
    timeline.begin(t1, None);
    timeline.record(BrowserLoadPhase::Finished, t1 + Duration::from_secs(2));
    let loading_after_finish = timeline.is_loading(t1 + Duration::from_secs(2));

    assert!(!idle, "从未发起导航时不得显示进度线");
    assert!(loading_soon, "导航开始 1s 后必须显示进度线");
    assert!(
        loading_before_timeout,
        "29s 仍未超过 30s 上限，必须继续显示"
    );
    assert!(
        !loading_after_timeout,
        "31s 超过上限后必须停止，避免加载失败时无限转动"
    );
    assert!(!loading_after_finish, "Finished 之后必须立即停止显示进度线");
}
