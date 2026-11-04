use super::*;
use crate::providers::local::a1::test_python as a1_python;
use crate::RecognizerFactory;

fn fixture() -> A1RecognizerFactory {
    fixture_with_pressure(|| true)
}

fn fixture_with_pressure(pressure: fn() -> bool) -> A1RecognizerFactory {
    let config = A1Config {
        protocol: 1,
        python: a1_python::executable(),
        python_sha256: String::new(),
        zipformer: std::env::temp_dir().join("unused-zipformer"),
        qwen: std::env::temp_dir().join("unused-qwen"),
    };
    let spawn = || {
        let mut cmd = std::process::Command::new(a1_python::executable());
        cmd.arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/a1_fake_worker.py"
            ))
            .arg("ok");
        Transport::spawn(cmd, "qwen", &Guard::default(), Duration::from_secs(3)).unwrap()
    };
    let engine = Engine {
        config: config.clone(),
        preview: spawn(),
        finalizer: spawn(),
        _script: None,
    };
    let host = Arc::new(A1EngineHost {
        state: Default::default(),
        pressure,
    });
    {
        let mut state = host.state.lock().unwrap();
        state.children = vec![engine.preview.control(), engine.finalizer.control()];
        state.idle = Some(engine);
    }
    A1RecognizerFactory { config, host }
}

/// 场景：多窗口不能同时启动 A1；构造共享同一宿主的 factory。
/// 执行：第一会话未结束时再次 create，结束后再 create。
/// 预期：忙时立即拒绝；成功终稿后复用同一 worker PID。
#[test]
fn shared_host_has_one_session_and_reuses_successful_workers() {
    let factory = fixture();
    let mut first = factory.create().expect("第一会话应启动");
    assert!(factory.create().is_err(), "第二窗口不得抢占第一会话");
    let pcm = audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16000, 1, vec![0, 0]).unwrap();
    first.accept_pcm(&pcm).unwrap();
    let first_pid = first.finish().unwrap().expect("应返回唯一终稿");
    assert!(first.finish().is_err(), "终态后不能再提交");
    drop(first);
    let mut second = factory.create().expect("完成后可新建会话");
    second.accept_pcm(&pcm).unwrap();
    assert_eq!(
        second.finish().unwrap(),
        Some(first_pid),
        "暖机应复用终稿模型"
    );
    drop(second);
    factory.host.shutdown();
    assert!(
        factory.host.state.lock().unwrap().idle.is_none(),
        "退出必须释放缓存模型"
    );
}

/// 场景：没有后续 PCM 的活动录音遇到资源释放；构造有真实子进程的会话。
/// 执行：保留 recognizer 的同时 shutdown，不调用 accept；预期 2 秒内 kill/wait，后续终稿失败。
#[test]
fn shutdown_reaps_active_workers_without_waiting_for_more_pcm() {
    let factory = fixture();
    let mut session = factory.create().unwrap();
    let children = factory.host.state.lock().unwrap().children.clone();
    let start = Instant::now();
    factory.host.shutdown();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "释放不应等待下一块音频"
    );
    for child in children {
        assert!(child.exited(), "活动 worker 必须已经 wait 回收");
    }
    assert_eq!(
        session.finish(),
        Err(AsrError::SessionClosed),
        "被释放的会话不能提交迟到终稿"
    );
}

/// 场景：准备或录音启动排队时被取消；构造预取消 token 和已有 warm worker。
/// 执行：create/prepare；预期拒绝且不预留活动槽，不接受旧任务。
#[test]
fn cancelled_startup_never_acquires_the_host() {
    let factory = fixture();
    let token = RecognitionCancellation::new();
    token.cancel();
    assert!(
        matches!(
            factory.create_with_cancellation(token.clone()),
            Err(AsrError::SessionClosed)
        ),
        "旧录音任务必须被拦截"
    );
    assert_eq!(
        factory.prepare_with_cancellation(token),
        Err(AsrError::SessionClosed),
        "旧准备任务必须被拦截"
    );
    assert!(
        factory.host.state.lock().unwrap().active.is_none(),
        "取消任务不得取得活动槽"
    );
    factory.host.shutdown();
}

/// 场景：释放完成但取消会话尚未 Drop；构造延后 100ms 释放的旧会话。
/// 执行恢复准备；预期等待旧槽回收，随后才做工件验证（夹具无模型，验证应失败），不得误报 busy。
#[test]
fn preparation_waits_for_cancelled_reservation_to_unwind() {
    let factory = fixture();
    let session = factory.create().unwrap();
    factory.host.shutdown();
    let cleanup = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        drop(session);
    });
    let start = Instant::now();
    let result = factory.prepare().unwrap_err();
    cleanup.join().unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(80),
        "准备必须等待取消会话释放预留槽"
    );
    assert!(
        !result.to_string().contains("another A1"),
        "不能把已取消会话误报成活动录音"
    );
    assert!(
        result.to_string().contains(if cfg!(windows) {
            "Apple Silicon"
        } else {
            "integrity"
        }),
        "槽回收后应保留平台/工件校验错误，不能伪造模型已就绪"
    );
}

/// 场景：资源压力拒绝真实宿主启动；数据为高压力监测器与真实协议 worker。
/// 执行：调用 create/prepare，再尝试已取消的 prepare。
/// 预期：普通请求明确报压力并且不占槽；取消优先收口，不能放宽生产资源护栏。
#[test]
fn elevated_pressure_rejects_start_without_reserving_and_cancel_still_wins() {
    let factory = fixture_with_pressure(|| false);
    let result = factory.create().err().expect("高压力必须拒绝启动");
    assert!(
        result.to_string().contains("memory pressure"),
        "必须保留资源拒绝原因"
    );
    assert!(
        factory
            .prepare()
            .expect_err("高压力必须拒绝准备")
            .to_string()
            .contains("memory pressure"),
        "准备也必须遵守压力护栏"
    );
    assert!(
        factory
            .host
            .state
            .lock()
            .expect("宿主状态")
            .active
            .is_none(),
        "资源拒绝不能占用活动槽"
    );
    let token = RecognitionCancellation::new();
    token.cancel();
    assert_eq!(
        factory.prepare_with_cancellation(token),
        Err(AsrError::SessionClosed),
        "取消请求必须优先结束，不受当前机器压力影响"
    );
    factory.host.shutdown();
}
