use super::*;

/// 后台启动的原终端仍支持进程观察，取消后不能观察已退出的进程。
///
/// 数据构造：1个真实 cat PTY，80×24为原终端默认网格；无Agent调用。
/// 执行过程：启动cat → 取得观察器 → 取消 → 等待读线程wait退出（2s上限）→ 再准备观察。
/// 预期结果：运行时观察有效；取消后观察失败，不误认复用的PID。
#[test]
#[cfg(target_os = "macos")]
fn native_launch_preserves_process_observation() {
    use terminal::PtyBackend;
    let control = launch::LaunchControl::default();
    let (tx, rx) = crossbeam_channel::unbounded();
    let pty = control
        .spawn_pty(PtyCommand::new("/bin/cat"), 80, 24, tx)
        .expect("测试PTY");
    let mut observer = pty
        .child_process_observer()
        .expect("原终端观察器不能被包装丢失");
    assert!(observer.prepare(), "运行中的原PTY应该可观察");
    control.cancel();
    // 2s = 正常本机wait余量；读取Exited意味着PTY读线程已经wait完成。
    assert!(
        matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Ok(terminal::PtyEvent::Exited { .. })
        ),
        "取消后必须回收子进程"
    );
    assert!(!observer.prepare(), "结束后不能继续观察旧PID");
}

/// 项目停止或卡片被删除后，迟到启动不能创建或重新附着PTY。
///
/// 数据构造：2张独立Claude空卡；80×24为默认网格，使用计数echo工厂，无真实Provider。
/// 执行过程：预留A→停项目→尝试spawn；预留B→spawn完成→删除B→尝试附着。
/// 预期结果：A工厂从未调用；B的迟到结果被拒绝；两侧均不产生新的活动终端。
#[test]
fn native_launch_cancellation_blocks_spawn_and_late_attach() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let node = runtime
        .create_mobile_agent_card(
            AgentProfile::Claude,
            None,
            None,
            CanvasRect::new(0., 0., 640., 400.),
        )
        .expect("原卡");
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let recorded = calls.clone();
    let factory: crate::PtyFactory = Box::new(move |_, _, _, tx| {
        recorded.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    });
    let launch = runtime
        .prepare_native_agent_terminal(node, 80, 24)
        .expect("预留");
    runtime.kill_all();
    assert!(
        launch.spawn_with_factory(Some(&factory)).is_err(),
        "退出后预留不能spawn"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "退出后不能调用PTY工厂");
    assert_eq!(runtime.live_len(), 0, "停止后没有活动终端");
    runtime.remove_node(node);
    let second = runtime
        .create_mobile_agent_card(
            AgentProfile::Claude,
            None,
            None,
            CanvasRect::new(0., 0., 640., 400.),
        )
        .expect("另一张卡");
    let launch = runtime
        .prepare_native_agent_terminal(second, 80, 24)
        .expect("预留B");
    let run = launch.run_id().to_owned();
    let prepared = launch.spawn_with_factory(Some(&factory));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "B只启动一次");
    runtime.remove_node(second);
    assert!(
        runtime
            .complete_native_agent_terminal(second, &run, prepared)
            .is_err(),
        "删除卡片后不能接回迟到PTY"
    );
    assert!(
        runtime.document.node(second).is_none(),
        "迟到回调不能复活卡片"
    );
    assert_eq!(runtime.live_len(), 0, "迟到结果不能注册活动终端");
}
