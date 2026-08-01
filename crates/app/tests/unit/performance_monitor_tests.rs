use std::time::Duration;

use super::{PerformanceRecorder, PerformanceState, ProcessCounters, RawPerformanceSample};

/// macOS 进程采样：libproc 必须直接返回当前主进程的 CPU、phys_footprint 与线程数
///
/// 数据构造（含关键数值的推导过程）：
///   observed_at = 7s（显式单调时间输入）
///   repaint_count = 58；render_count = 57（显式 UI 累计计数输入）
///   queried pid = std::process::id()（当前 app 单测主进程）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 macOS `proc_pidinfo(PROC_PIDTASKINFO)` 读取 CPU 累计时间和线程数。
///   2. 调用 `proc_pid_rusage(RUSAGE_INFO_V2)` 读取 phys_footprint 和 wakeups。
///   3. 将系统计数与显式 repaint/render 计数组合为 ProcessCounters。
///
/// 预期结果：
///   - 正断言：当前测试进程存在非零 CPU 时间、phys_footprint 和线程数。
///   - 负断言：调用方传入的 repaint/render 计数不得被系统查询覆盖或清零。
#[cfg(target_os = "macos")]
#[test]
fn macos_sampler_reads_current_process_without_spawning_commands() {
    let counters = super::macos::read_process_counters(Duration::from_secs(7), 58, 57)
        .expect("macOS libproc 应能读取当前 app 单测进程");

    assert_eq!(
        counters.observed_at,
        Duration::from_secs(7),
        "采样器必须保留调用方提供的单调观测时间"
    );
    assert!(
        counters.cpu_time_ns > 0,
        "已运行的 app 单测进程必须具有非零累计 CPU 时间"
    );
    assert!(
        counters.phys_footprint_bytes > 0,
        "当前 app 单测进程必须具有非零 phys_footprint"
    );
    assert!(
        counters.thread_count > 0,
        "当前 app 单测进程必须至少包含一个线程"
    );
    assert_eq!(
        counters.repaint_count, 58,
        "libproc 查询不得覆盖调用方传入的 58 次 repaint"
    );
    assert_ne!(
        counters.repaint_count, 0,
        "repaint 计数不得在组合进程样本时被清零"
    );
    assert_eq!(
        counters.render_count, 57,
        "libproc 查询不得覆盖调用方传入的 57 次 render"
    );
    assert_ne!(
        counters.render_count, 0,
        "render 计数不得在组合进程样本时被清零"
    );
}

/// 采样差值换算：一秒内 CPU、wakeups、repaint 和 render 增量必须换算为每秒指标
///
/// 数据构造（含关键数值的推导过程）：
///   elapsed       = 2s - 1s = 1s
///   cpu delta     = 2.5s - 2.0s = 0.5s → 0.5 / 1 × 100 = 50%
///   wakeups delta = 242 - 100 = 142 → 142 / 1 = 142/s
///   repaints delta = 68 - 10 = 58 → 58 / 1 = 58/s
///   renders delta  = 77 - 20 = 57 → 57 / 1 = 57/s
///   memory         = 316 × 1024 × 1024 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造前后两次单调递增的进程累计计数。
///   2. 以 terminal_active 状态计算两个观测点之间的原始样本。
///   3. 逐项读取换算后的 CPU、内存、wakeups、线程、repaint 和 render。
///
/// 预期结果：
///   - 正断言：所有速率都与一秒增量完全一致，内存保留 phys_footprint 字节值。
///   - 负断言：CPU 不得被错误缩小为 5%，状态不得退化成 idle。
#[test]
fn raw_sample_converts_monotonic_counter_deltas_to_per_second_metrics() {
    let previous = ProcessCounters {
        observed_at: Duration::from_secs(1),
        cpu_time_ns: 2_000_000_000,
        phys_footprint_bytes: 300 * 1024 * 1024,
        wakeup_count: 100,
        thread_count: 10,
        repaint_count: 10,
        render_count: 20,
    };
    let current = ProcessCounters {
        observed_at: Duration::from_secs(2),
        cpu_time_ns: 2_500_000_000,
        phys_footprint_bytes: 316 * 1024 * 1024,
        wakeup_count: 242,
        thread_count: 12,
        repaint_count: 68,
        render_count: 77,
    };

    let sample = RawPerformanceSample::between(
        1_780_000_000_000,
        PerformanceState::TerminalActive,
        previous,
        current,
    )
    .expect("一秒有效观测区间应生成性能样本");

    assert_eq!(
        sample.cpu_percent, 50.0,
        "0.5 CPU 秒除以 1 秒应等于单核 50%"
    );
    assert_ne!(sample.cpu_percent, 5.0, "CPU 百分比不得少乘一个数量级");
    assert_eq!(
        sample.phys_footprint_bytes,
        316 * 1024 * 1024,
        "内存必须使用当前时刻的 phys_footprint 字节值"
    );
    assert_eq!(
        sample.wakeups_per_second, 142.0,
        "142 次增量应换算为 142 wakeups/s"
    );
    assert_eq!(sample.thread_count, 12, "线程数必须采用当前观测值 12");
    assert_eq!(
        sample.repaints_per_second, 58.0,
        "58 次 repaint 增量应换算为 58/s"
    );
    assert_eq!(
        sample.renders_per_second, 57.0,
        "57 次 render 增量应换算为 57/s"
    );
    assert_eq!(
        sample.state,
        PerformanceState::TerminalActive,
        "终端活动样本必须保留 terminal_active 状态"
    );
    assert_ne!(
        sample.state,
        PerformanceState::Idle,
        "终端活动样本不得误标成 idle"
    );
}

/// 十秒摘要：第十个样本才输出一条结构化汇总，并计算 median/P95 与关联指标
///
/// 数据构造（含关键数值的推导过程）：
///   sample count = 10 × 1s = 10s
///   CPU values   = 1..10 → median = (5 + 6) / 2 = 5.5
///   P95 nearest-rank index = ceil(10 × 0.95) - 1 = 9 → value 10
///   memory       = 每个样本 316 MiB → latest/peak 都为 316 MiB
///   wakeups      = 每个样本 142/s；repaints = 58/s；renders = 57/s
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续推入前九个 terminal_active 样本并检查没有摘要。
///   2. 推入第十个样本触发唯一一次摘要。
///   3. 读取 CPU 分位数以及内存、wakeups、线程、repaint/render 关联指标。
///
/// 预期结果：
///   - 正断言：摘要包含十个样本和可复算的统计值。
///   - 负断言：九秒时不得提前写日志，低于 active 20% 门槛不得产生诊断文件。
#[test]
fn recorder_emits_one_summary_after_ten_samples_with_correlated_metrics() {
    let mut recorder = PerformanceRecorder::default();
    let mut tenth_outcome = None;

    for second in 1..=10 {
        let sample = RawPerformanceSample {
            timestamp_epoch_ms: 1_780_000_000_000 + second * 1_000,
            state: PerformanceState::TerminalActive,
            cpu_percent: second as f64,
            phys_footprint_bytes: 316 * 1024 * 1024,
            wakeups_per_second: 142.0,
            thread_count: 12,
            repaints_per_second: 58.0,
            renders_per_second: 57.0,
        };
        let outcome = recorder.push(sample);
        if second < 10 {
            assert!(
                outcome.summary.is_none(),
                "第 {second} 秒尚未满 10 个样本，不得提前输出摘要"
            );
            assert!(
                outcome.diagnostic.is_none(),
                "CPU 最高 9% 低于 active 20% 门槛，不得输出诊断"
            );
        } else {
            tenth_outcome = Some(outcome);
        }
    }

    let outcome = tenth_outcome.expect("第十秒必须返回 recorder outcome");
    let summary = outcome.summary.expect("第十个样本必须触发一次摘要");
    assert_eq!(
        summary.sample_count, 10,
        "十秒摘要必须包含恰好 10 个原始样本"
    );
    assert_eq!(
        summary.state,
        PerformanceState::TerminalActive,
        "摘要状态应保留 terminal_active"
    );
    assert_eq!(summary.cpu_median, 5.5, "CPU 1..10 的 median 应为 5.5");
    assert_eq!(
        summary.cpu_p95, 10.0,
        "CPU 1..10 的 nearest-rank P95 应为 10"
    );
    assert_eq!(
        summary.memory_mb, 316.0,
        "最新 phys_footprint 应换算为 316 MiB"
    );
    assert_eq!(summary.memory_peak_mb, 316.0, "十秒内内存峰值应为 316 MiB");
    assert_eq!(
        summary.wakeups_per_second, 142.0,
        "wakeups 十秒均值应保持 142/s"
    );
    assert_eq!(summary.thread_count, 12, "线程摘要应采用最新观测值 12");
    assert_eq!(
        summary.repaints_per_second, 58.0,
        "repaint 十秒均值应保持 58/s"
    );
    assert_eq!(
        summary.renders_per_second, 57.0,
        "render 十秒均值应保持 57/s"
    );
    assert!(
        outcome.diagnostic.is_none(),
        "terminal_active CPU 最高 10% 低于 20% 门槛，不得触发诊断"
    );
}

/// 连续高 CPU 捕获：第五个超阈值样本触发一次最近两分钟诊断，并保持环形容量
///
/// 数据构造（含关键数值的推导过程）：
///   ring capacity = 120 samples × 1s = 120s = 2min
///   initial low samples = 120，时间戳 0..119
///   high idle CPU = 10% ≥ idle threshold 5%
///   trigger streak = 5 samples，加入时间戳 120..124 后保留 5..124 共 120 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 推入 120 个 CPU=1% 的 idle 样本填满环形缓冲。
///   2. 逐一推入四个 CPU=10% 样本，确认尚未达到连续五秒。
///   3. 推入第五个高样本并读取诊断快照，再推入第六个高样本检查去重。
///
/// 预期结果：
///   - 正断言：第五秒触发的诊断包含 120 条、首尾时间戳 5 和 124。
///   - 负断言：前四秒和同一高 CPU episode 的第六秒都不得重复触发。
#[test]
fn recorder_captures_bounded_two_minute_ring_once_per_high_cpu_episode() {
    let mut recorder = PerformanceRecorder::default();
    for timestamp in 0..120 {
        let outcome = recorder.push(sample_at(timestamp, PerformanceState::Idle, 1.0));
        assert!(
            outcome.diagnostic.is_none(),
            "CPU=1% 低于 idle 5% 门槛，填充环形缓冲时不得触发诊断"
        );
    }

    for timestamp in 120..124 {
        let outcome = recorder.push(sample_at(timestamp, PerformanceState::Idle, 10.0));
        assert!(
            outcome.diagnostic.is_none(),
            "连续高 CPU 第 {} 秒尚未达到五秒，不得提前触发",
            timestamp - 119
        );
    }

    let trigger = recorder.push(sample_at(124, PerformanceState::Idle, 10.0));
    let diagnostic = trigger
        .diagnostic
        .expect("连续五个 idle 高 CPU 样本必须触发诊断快照");
    assert_eq!(
        diagnostic.samples.len(),
        120,
        "诊断快照必须严格限制为最近 120 个样本"
    );
    assert_eq!(
        diagnostic
            .samples
            .first()
            .map(|sample| sample.timestamp_epoch_ms),
        Some(5_000),
        "容量截断后首个样本应是第 5 秒"
    );
    assert_eq!(
        diagnostic
            .samples
            .last()
            .map(|sample| sample.timestamp_epoch_ms),
        Some(124_000),
        "触发样本第 124 秒必须保留在诊断末尾"
    );
    assert_eq!(
        diagnostic.trigger_state,
        PerformanceState::Idle,
        "诊断必须记录触发状态 idle"
    );
    assert_eq!(
        diagnostic.cpu_threshold_percent, 5.0,
        "idle CPU 门槛必须记录为 5%"
    );
    assert_eq!(
        diagnostic.consecutive_high_samples, 5,
        "诊断必须记录连续五个高样本"
    );

    let repeated = recorder.push(sample_at(125, PerformanceState::Idle, 10.0));
    assert!(
        repeated.diagnostic.is_none(),
        "同一段连续高 CPU 的第六秒不得重复写诊断文件"
    );
}

fn sample_at(
    timestamp_seconds: u64,
    state: PerformanceState,
    cpu_percent: f64,
) -> RawPerformanceSample {
    RawPerformanceSample {
        timestamp_epoch_ms: timestamp_seconds * 1_000,
        state,
        cpu_percent,
        phys_footprint_bytes: 316 * 1024 * 1024,
        wakeups_per_second: 142.0,
        thread_count: 12,
        repaints_per_second: 58.0,
        renders_per_second: 57.0,
    }
}
