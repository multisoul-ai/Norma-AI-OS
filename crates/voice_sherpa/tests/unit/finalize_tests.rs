use super::{
    run_finalize_for_test, FinalizeOutcome, FinalizeRecognizer, PcmEvent, FINALIZE_TIMEOUT_MS,
    PCM_QUEUE_CHUNKS, TAP_DRAIN_WAIT_MS,
};
use std::thread;
use std::time::Duration;

#[derive(Default)]
struct FakeRecognizer {
    accepted: Vec<Vec<f32>>,
    input_finished: bool,
    decode_calls: usize,
    ready_before_final: usize,
    result: String,
}

impl FakeRecognizer {
    fn with_result(result: &str) -> Self {
        Self {
            result: result.to_string(),
            ..Self::default()
        }
    }

    fn with_ready_result(result: &str, ready_before_final: usize) -> Self {
        Self {
            ready_before_final,
            result: result.to_string(),
            ..Self::default()
        }
    }
}

impl FinalizeRecognizer for FakeRecognizer {
    fn accept_samples(&mut self, samples: &[f32]) {
        self.accepted.push(samples.to_vec());
    }

    fn input_finished(&mut self) {
        self.input_finished = true;
    }

    fn is_ready(&self) -> bool {
        self.input_finished && self.decode_calls < self.ready_before_final
    }

    fn decode(&mut self) {
        self.decode_calls += 1;
    }

    fn result_text(&self) -> String {
        self.result.clone()
    }
}

/// VF-1: finish 收到 EndOfAudio 后必须消费已排队 PCM，然后再完成 final decode。
///
/// 数据构造（含关键数值的推导过程）：
///   samples_a = [0.1, 0.2]（第一段 PCM，2 samples）
///   samples_b = [0.3, 0.4, 0.5]（第二段 PCM，3 samples）
///   EndOfAudio 位于两段 PCM 之后，表示音频输入结束
///   ready_before_final = 2，因此 final 阶段应调用 decode 2 次后收敛
///
/// 执行过程（逐步说明系统如何处理）：
///   1. recognizer 接收 samples_a → accepted.len() = 1
///   2. recognizer 接收 samples_b → accepted.len() = 2
///   3. 收到 EndOfAudio → 调用 input_finished()
///   4. is_ready 为 true 两次 → decode 两次
///   5. 返回 final transcript
///
/// 预期结果：
///   - 正断言：两段 PCM 都被 accept，证明 backlog 未被 stop 丢弃
///   - 正断言：input_finished 在返回前被调用
///   - 正断言：final decode 调用 2 次
///   - 负断言：结果不应 timeout，否则 finish barrier 未正常收敛
#[test]
fn test_finalize_consumes_all_samples_before_final_decode() {
    let (tx, rx) = crossbeam_channel::bounded(8);
    tx.send(PcmEvent::Samples(vec![0.1, 0.2]))
        .expect("第一段 PCM 应能入队");
    tx.send(PcmEvent::Samples(vec![0.3, 0.4, 0.5]))
        .expect("第二段 PCM 应能入队");
    tx.send(PcmEvent::EndOfAudio).expect("EndOfAudio 应能入队");
    drop(tx);

    let mut recognizer = FakeRecognizer::with_ready_result("打开 terminal", 2);
    let outcome = run_finalize_for_test(&mut recognizer, &rx, Duration::from_millis(250))
        .expect("finalize 应返回 transcript");

    assert_eq!(
        recognizer.accepted.len(),
        2,
        "finish 必须消费 EndOfAudio 前所有 queued PCM，不能提前丢 backlog"
    );
    assert_eq!(
        recognizer.accepted[0],
        vec![0.1, 0.2],
        "第一段 PCM 应按入队顺序进入 recognizer"
    );
    assert_eq!(
        recognizer.accepted[1],
        vec![0.3, 0.4, 0.5],
        "第二段 PCM 应按入队顺序进入 recognizer"
    );
    assert!(
        recognizer.input_finished,
        "收到 EndOfAudio 后必须调用 input_finished 才能生成 final result"
    );
    assert_eq!(
        recognizer.decode_calls, 2,
        "input_finished 后应 decode 到 is_ready=false，预期 2 次"
    );
    assert_eq!(
        outcome,
        FinalizeOutcome::Final("打开 terminal".to_string()),
        "正常收口应返回 Final，而不是 timeout 或空结果"
    );
    assert_ne!(
        outcome,
        FinalizeOutcome::TimedOut("打开 terminal".to_string()),
        "未超时时不应标记为 TimedOut"
    );
}

/// VF-2: finalization 超时但已有 best transcript 时返回 TimedOut(best)。
///
/// 数据构造（含关键数值的推导过程）：
///   samples = [1.0; 160]（约 10ms @16kHz，足以触发一次 accept）
///   timeout = 20ms（人为设小，用于测试超时分支）
///   ready_before_final = usize::MAX，表示 fake recognizer 一直 ready，不会自然收敛
///   result = "创建节点"，作为 best transcript
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 消费 samples
///   2. 收到 EndOfAudio 并调用 input_finished
///   3. decode 循环无法在 20ms 内收敛
///   4. 超时后读取 best transcript 并返回 TimedOut(best)
///
/// 预期结果：
///   - 正断言：返回 TimedOut("创建节点")
///   - 正断言：至少调用过一次 decode，证明确实进入 final decode
///   - 负断言：不应返回 Final，避免把超时路径伪装为正常完成
#[test]
fn test_finalize_timeout_returns_best_transcript() {
    let (tx, rx) = crossbeam_channel::bounded(8);
    tx.send(PcmEvent::Samples(vec![1.0; 160]))
        .expect("PCM 应能入队");
    tx.send(PcmEvent::EndOfAudio).expect("EndOfAudio 应能入队");
    drop(tx);

    let mut recognizer = FakeRecognizer::with_ready_result("创建节点", usize::MAX);
    let outcome = run_finalize_for_test(&mut recognizer, &rx, Duration::from_millis(20))
        .expect("best transcript 非空时超时也应返回");

    assert_eq!(
        outcome,
        FinalizeOutcome::TimedOut("创建节点".to_string()),
        "finalization 超时应返回 TimedOut(best transcript)"
    );
    assert!(
        recognizer.decode_calls > 0,
        "超时前应至少尝试 decode，一次都没有说明未进入 final 阶段"
    );
    assert_ne!(
        outcome,
        FinalizeOutcome::Final("创建节点".to_string()),
        "超时路径必须可观测，不应伪装为正常 Final"
    );
}

/// VF-3: abort 信号优先终止 finalization，cancel/drop 不等待 backlog。
///
/// 数据构造（含关键数值的推导过程）：
///   samples_a = [0.1]（已排队 PCM）
///   未发送 EndOfAudio，表示 graceful finish 条件尚未满足
///   abort_tx 在测试线程中 10ms 后发送 Abort
///   timeout = 500ms，远大于 abort 延迟，若等待到 timeout 说明 cancel 不即时
///
/// 执行过程（逐步说明系统如何处理）：
///   1. recognizer 消费 samples_a
///   2. 无 EndOfAudio，理论上 graceful finish 会继续等待
///   3. abort 信号到达
///   4. finalization 立即返回 Aborted
///
/// 预期结果：
///   - 正断言：返回 Aborted
///   - 正断言：耗时小于 200ms，证明没有等待完整 timeout
///   - 负断言：不应调用 input_finished，cancel/drop 不是 graceful finalization
#[test]
fn test_abort_exits_without_graceful_finalization() {
    let (tx, rx) = crossbeam_channel::bounded(8);
    tx.send(PcmEvent::Samples(vec![0.1])).expect("PCM 应能入队");
    let _tx = tx;

    let (abort_tx, abort_rx) = crossbeam_channel::bounded(1);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        abort_tx.send(()).expect("abort 应能发送");
    });

    let start = std::time::Instant::now();
    let mut recognizer = FakeRecognizer::with_result("不会提交");
    let outcome = super::run_finalize_with_abort_for_test(
        &mut recognizer,
        &rx,
        &abort_rx,
        Duration::from_millis(500),
    )
    .expect("abort 应返回 outcome");

    assert_eq!(
        outcome,
        FinalizeOutcome::Aborted,
        "abort 信号应立即结束 finalization"
    );
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "abort 应快速返回，不应等待 500ms timeout，实际 {:?}",
        start.elapsed()
    );
    assert!(
        !recognizer.input_finished,
        "cancel/drop 路径不应调用 input_finished，否则变成 graceful finish"
    );
}

/// VF-4: 有界队列常量必须能限制 backlog，且 tap/finalize 超时为有限值。
///
/// 数据构造（含关键数值的推导过程）：
///   每个 tap chunk ≈ 1600 samples @16kHz = 100ms
///   PCM_QUEUE_CHUNKS = 30 → 约 30 * 100ms = 3000ms backlog 上限
///   TAP_DRAIN_WAIT_MS = 200ms，用于等待最后 callback 入队
///   FINALIZE_TIMEOUT_MS = 3000ms，防止 final decode 无限等待
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取三个常量
///   2. 校验 bounded backlog 不为 0 且不无限
///   3. 校验 drain/finalize 超时均为正且 finalize 不低于 1000ms
///
/// 预期结果：
///   - 正断言：队列容量等于 30，约束 backlog 为约 3 秒
///   - 正断言：drain 与 finalize timeout 均为正
///   - 负断言：finalize timeout 不应小于 1000ms，否则快语速 backlog 仍易截断
#[test]
fn test_backlog_and_timeout_constants_are_bounded_and_adequate() {
    assert_eq!(
        PCM_QUEUE_CHUNKS, 30,
        "PCM_QUEUE_CHUNKS=30 对应约 3 秒 100ms chunk backlog，不应回退到 unbounded"
    );
    assert!(
        TAP_DRAIN_WAIT_MS > 0,
        "TAP_DRAIN_WAIT_MS 必须大于 0，确保最后 tap callback 有机会入队"
    );
    assert!(
        FINALIZE_TIMEOUT_MS >= 1_000,
        "FINALIZE_TIMEOUT_MS 不应小于 1000ms，否则识别稍慢仍会提前截断"
    );
    assert!(
        FINALIZE_TIMEOUT_MS <= 10_000,
        "FINALIZE_TIMEOUT_MS 必须有限，避免 recognizer 异常时卡住 app"
    );
}
