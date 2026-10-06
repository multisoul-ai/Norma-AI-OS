use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, NodeRunState, RuntimeEvent};

fn native_mobile_card(
    runtime: &mut Runtime,
) -> (NodeId, crossbeam_channel::Sender<terminal::PtyEvent>) {
    let (capture, output) = crossbeam_channel::bounded(1);
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        capture.send(tx.clone()).expect("原 PTY 输出入口");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0., 0., 640., 400.),
        )
        .expect("iOS 创建原生卡");
    runtime
        .start_native_agent_terminal(node, 80, 24)
        .expect("启动原 PTY");
    (node, output.try_recv().expect("取得原 PTY sender"))
}

fn ready_native_card(runtime: &mut Runtime, node: NodeId) {
    let start = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node,
        serde_json::json!({"hook_event_name":"SessionStart","session_id":format!("session-{node}")}),
    )
    .expect("真实 SessionStart 格式");
    assert!(runtime.observe_mobile_hook(&start), "原卡应绑定真实会话");
    assert!(
        runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id: node,
            profile: AgentProfile::Codex,
            provider_event: "SessionStart".into(),
            to: AgentTaskState::Ready,
            task_title: None,
            session_id: start.session_id,
        }),
        "SessionStart 应使原卡就绪"
    );
}

/// iOS 创建的原生卡异常退出后显示失败，同项目另一张就绪卡不受影响。
///
/// 数据构造：2 张 80×24 原生卡，分别绑定真实 SessionStart；退出码 1 ≠ 0，表示启动失败。
/// 执行过程：创建并启动两卡 → 收到 Ready → 清空事件 → A 输出 Exited(1) → drain 两次。
/// 预期结果：A 的运行和任务状态均失败，手机不再显示 ready；B 仍 ready；重复 drain 不重复发事件。
#[test]
fn native_mobile_exit_replaces_ready_with_failure_once() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (failed, output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, failed);
    let (healthy, _healthy_output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, healthy);
    runtime.take_events();
    output
        .send(terminal::PtyEvent::Exited { code: Some(1) })
        .expect("原生进程启动失败");
    runtime.drain_all();
    assert_eq!(
        runtime.node_run_state(failed),
        Some(NodeRunState::Error {
            message: "process exited with status 1".into()
        }),
        "进程状态必须保留非零退出码"
    );
    assert_eq!(
        runtime.agent_task_state(failed),
        Some(AgentTaskState::Failed {
            message: "process exited with status 1".into()
        }),
        "原生进程退出必须同步任务状态"
    );
    let cards = runtime.mobile_card_snapshots();
    let failed_card = cards
        .iter()
        .find(|c| c.node_id == failed)
        .expect("失败原卡");
    assert_eq!(failed_card.state, "failed", "手机必须显示失败");
    assert_ne!(failed_card.state, "ready", "已退出的原卡不能残留就绪");
    assert_eq!(
        cards
            .iter()
            .find(|c| c.node_id == healthy)
            .expect("另一张原卡")
            .state,
        "ready",
        "同项目其他活动卡不能被结束"
    );
    assert_eq!(
        runtime.node_run_state(healthy),
        Some(NodeRunState::Running),
        "另一张 PTY 应继续运行"
    );
    let events = runtime.take_events();
    assert!(
        events.iter().any(|event| matches!(event, RuntimeEvent::AgentTaskStateChanged { node_id, from: AgentTaskState::Ready, to: AgentTaskState::Failed { .. }, .. } if *node_id == failed)),
        "手机订阅必须收到原卡 Ready 到 Failed 的状态事件"
    );
    runtime.drain_all();
    assert!(
        runtime.take_events().is_empty(),
        "重复 drain 不能重复发布退出事件"
    );
}

/// 启动失败即使没有 SessionStart 也应显示失败，就绪卡正常退出应结束会话。
///
/// 数据构造：2 张 80×24 原卡；失败卡退出码 1 ≠ 0，就绪卡退出码 0 = 正常退出。
/// 执行过程：A 启动但不发 Hook → Exited(1) → B 启动 → SessionStart → Exited(0)。
/// 预期结果：A 为 failed，B 为 ended；两卡都不能残留 ready，不伪造 Provider 会话。
#[test]
fn native_mobile_bootstrap_failure_and_clean_exit_follow_process_lifecycle() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (node, output) = native_mobile_card(&mut runtime);
    output
        .send(terminal::PtyEvent::Exited { code: Some(1) })
        .expect("bootstrap 失败");
    runtime.drain_all();
    assert_eq!(
        runtime.mobile_card_snapshots()[0].state,
        "failed",
        "无 Hook 的启动失败也必须可见"
    );
    assert!(
        runtime.agent_session_id(node).is_none(),
        "启动失败不能伪造 Provider 会话"
    );
    let (clean, output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, clean);
    output
        .send(terminal::PtyEvent::Exited { code: Some(0) })
        .expect("正常退出");
    runtime.drain_all();
    assert_eq!(
        runtime.agent_task_state(clean),
        Some(AgentTaskState::Ended),
        "正常退出应结束会话"
    );
    let cards = runtime.mobile_card_snapshots();
    let clean_card = cards
        .iter()
        .find(|c| c.node_id == clean)
        .expect("正常退出原卡");
    assert_eq!(clean_card.state, "ended", "手机必须看到已结束");
    assert_ne!(clean_card.state, "ready", "退出后不再就绪");
    assert_eq!(
        cards
            .iter()
            .find(|c| c.node_id == node)
            .expect("失败原卡")
            .state,
        "failed",
        "另一张卡退出不能覆盖原卡失败"
    );
}

/// 用户重新发送前可显式重开启动失败的原卡，但新进程加载中不能沿用旧任务状态。
///
/// 数据构造：1 张 80×24 原卡；旧进程 Exited(1) 后新进程仍使用同一 node，且无 SessionStart。
///   新首屏 5 行 < 24 行，bracketed paste 开启，真实模型页脚不含旧 model: 头部。
/// 执行过程：原卡启动失败 → 显式重新启动原卡 → 校验加载中拒收 → PTY 输出就绪首屏 → 校验可发送。
/// 预期结果：原卡可恢复，不另建卡；加载中 unknown 且拒收，就绪后 ready 且接受手机消息。
#[test]
fn native_mobile_failed_card_can_restart_without_stale_readiness() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (node, output) = native_mobile_card(&mut runtime);
    output
        .send(terminal::PtyEvent::Exited { code: Some(1) })
        .expect("首次 bootstrap 失败");
    runtime.drain_all();
    let (capture, output) = crossbeam_channel::bounded(1);
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        capture.send(tx.clone()).expect("重开原卡的 PTY");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    assert!(
        runtime.start_native_agent_terminal(node, 80, 24).is_ok(),
        "已退出的 PTY 不能阻止用户重开原卡"
    );
    let loading = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(loading.state, "unknown", "新进程不能沿用旧失败或就绪状态");
    assert_eq!(
        runtime.validate_workspace_message(&loading, "hi"),
        Err("state_not_ready"),
        "新进程仍在加载时不能输入"
    );
    output
        .try_recv()
        .expect("新 PTY 输出入口")
        .send(terminal::PtyEvent::Output(
            "\x1b[?2004h\r\n› Ask Codex to do anything\r\n\r\nGPT-6.1-Sol xhigh · /tmp\r\n"
                .as_bytes()
                .to_vec(),
        ))
        .expect("新进程已加载的首屏");
    runtime.drain_all();
    let ready = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(ready.state, "ready", "新的真实输入首屏应就绪");
    assert_eq!(
        runtime.validate_workspace_message(&ready, "hi"),
        Ok(()),
        "恢复后的原卡应接受手机消息"
    );
    assert_eq!(ready.node_id, node, "重开不能替换卡片身份");
    assert_eq!(
        runtime.mobile_card_snapshots().len(),
        1,
        "恢复不能创建重复卡片"
    );
}

/// 重开已有 Codex 会话时，CLI 延迟 SessionStart 不应使手机永远无法发送下一条消息。
///
/// 数据构造：1 张 80×24 原卡，已收到 SessionStart/UserPromptSubmit；/resume 首屏 5 行 < 24，无 model: 头部。
/// 执行过程：记录原会话 → 原进程退出 → 显式重开 → PTY 输出已加载页脚 → 手机校验。
/// 预期结果：原卡 ready 且可输入，保存的 session 保留；不得等待只有输入后才出现的 Hook。
#[test]
fn native_mobile_resumed_session_is_ready_before_delayed_start_hook() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (node, output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, node);
    let session = format!("session-{node}");
    assert!(
        runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id: node,
            profile: AgentProfile::Codex,
            provider_event: "UserPromptSubmit".into(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: Some(session.clone()),
        }),
        "真实输入后应持久绑定原会话"
    );
    output
        .send(terminal::PtyEvent::Exited { code: Some(0) })
        .expect("退出原进程");
    runtime.drain_all();
    let (capture, output) = crossbeam_channel::bounded(1);
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        capture.send(tx.clone()).expect("恢复原会话的 PTY");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    runtime
        .start_native_agent_terminal(node, 80, 24)
        .expect("恢复原会话");
    output
        .try_recv()
        .expect("恢复进程的输出入口")
        .send(terminal::PtyEvent::Output(
            "\x1b[?2004h\r\n› Ask Codex to do anything\r\n\r\nGPT-6.1-Sol xhigh · /tmp\r\n"
                .as_bytes()
                .to_vec(),
        ))
        .expect("恢复后的真实就绪首屏");
    runtime.drain_all();
    let card = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(card.state, "ready", "已加载的恢复会话不能永远停在 unknown");
    assert_eq!(
        card.session_id.as_deref(),
        Some(session.as_str()),
        "恢复必须保留原会话身份"
    );
    assert_eq!(
        runtime.validate_workspace_message(&card, "hi"),
        Ok(()),
        "SessionStart 延迟不能阻止恢复会话继续输入"
    );
    assert_ne!(
        card.state, "preparing",
        "已就绪的恢复首屏不能继续显示准备中"
    );
}

/// 原生进程收尾不能覆盖 Provider 已报告的任务成果或失败原因。
///
/// 数据构造：2 张 80×24 原卡；A 已 Done，B 已 Failed("provider failure")，随后均正常退出。
/// 执行过程：绑定会话 → 应用真实 Stop/StopFailure 任务状态 → 发送 Exited(0) → 投影手机状态。
/// 预期结果：A 保留 done，B 保留具体失败；退出不能将已知成果降为 ended。
#[test]
fn native_mobile_exit_preserves_provider_outcomes() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let (done, done_output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, done);
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: done,
        profile: AgentProfile::Codex,
        provider_event: "Stop".into(),
        to: AgentTaskState::Done,
        task_title: None,
        session_id: Some(format!("session-{done}")),
    });
    let (failed, failed_output) = native_mobile_card(&mut runtime);
    ready_native_card(&mut runtime, failed);
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: failed,
        profile: AgentProfile::Codex,
        provider_event: "StopFailure".into(),
        to: AgentTaskState::Failed {
            message: "provider failure".into(),
        },
        task_title: None,
        session_id: Some(format!("session-{failed}")),
    });
    done_output
        .send(terminal::PtyEvent::Exited { code: Some(0) })
        .expect("完成卡退出");
    failed_output
        .send(terminal::PtyEvent::Exited { code: Some(0) })
        .expect("失败卡退出");
    runtime.drain_all();
    assert_eq!(
        runtime.agent_task_state(done),
        Some(AgentTaskState::Done),
        "已完成的真实成果应保留"
    );
    assert_eq!(
        runtime.agent_task_state(failed),
        Some(AgentTaskState::Failed {
            message: "provider failure".into()
        }),
        "Provider 失败原因不能被退出状态覆盖"
    );
    let cards = runtime.mobile_card_snapshots();
    assert_eq!(
        cards
            .iter()
            .find(|c| c.node_id == done)
            .expect("完成原卡")
            .state,
        "done",
        "手机应保留任务完成"
    );
    assert_eq!(
        cards
            .iter()
            .find(|c| c.node_id == failed)
            .expect("失败原卡")
            .state,
        "failed",
        "手机应保留任务失败"
    );
    assert_ne!(
        runtime.agent_task_state(done),
        Some(AgentTaskState::Ended),
        "进程收尾不能丢失已知成果"
    );
}

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
