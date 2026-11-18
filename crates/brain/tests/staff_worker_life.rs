//! Worker 状态机：每次输入只走一条转换，效果由宿主执行。
use brain::staff::worker_life::{step, Effect, Input, CONFIRM_WINDOW, ENTER_DELAY, ENTER_RETRY};
use brain::staff::TaskStore;
use norma::protocol::{Task, Worker, WorkerFailure, WorkerOutcome, WorkerPhase};
use std::time::Duration;

fn tick(elapsed: Duration, accepts_input: bool) -> Input {
    Input::Tick {
        elapsed,
        accepts_input,
    }
}

/// 场景：一张新卡从预留走到确认，中间含一次补回车和超时。
/// 数据构造：阶段从 Reserved 开始；回车延迟 100 毫秒，补回车 5 秒，确认窗口 15 秒。
/// 执行过程：Spawned → OutputQuiet → 99ms → 100ms → 4s → 5s → 14s → 15s。
/// 预期结果：只有该打字、该回车、该补回车时有效果；15 秒变成未确认，不再补第二次。
#[test]
fn delivery_types_enters_once_more_then_times_out() {
    assert_eq!(
        ENTER_DELAY,
        Duration::from_millis(100),
        "回车延迟是 100 毫秒"
    );
    assert_eq!(ENTER_RETRY, Duration::from_secs(5), "只在 5 秒补一次回车");
    assert_eq!(
        CONFIRM_WINDOW,
        Duration::from_secs(15),
        "15 秒还没确认就是没送达"
    );
    let (phase, effects) = step(WorkerPhase::Reserved, Input::Spawned);
    assert_eq!(phase, WorkerPhase::Spawning, "进程起来后等它能接收输入");
    assert!(effects.is_empty(), "拉起进程是宿主在 Spawned 之前做的");
    let (phase, effects) = step(phase, Input::OutputQuiet);
    assert_eq!(phase, WorkerPhase::Typed, "可以打字了");
    assert_eq!(effects, vec![Effect::Type], "效果是把 prompt 打进去");
    let (phase, effects) = step(phase, tick(Duration::from_millis(99), true));
    assert_eq!(phase, WorkerPhase::Typed, "99 毫秒还不到回车");
    assert!(effects.is_empty(), "没到点就没有效果");
    let (phase, effects) = step(phase, tick(ENTER_DELAY, true));
    assert_eq!(phase, WorkerPhase::Submitted { retried: false }, "到点提交");
    assert_eq!(effects, vec![Effect::PressEnter], "按一次回车");
    let (waiting, effects) = step(phase, tick(Duration::from_secs(4), true));
    assert_eq!(waiting, phase, "4 秒不补回车");
    assert!(effects.is_empty(), "4 秒没有效果");
    let (phase, effects) = step(phase, tick(ENTER_RETRY, false));
    assert_eq!(
        phase,
        WorkerPhase::Submitted { retried: false },
        "卡片不收输入时不补回车"
    );
    assert!(effects.is_empty(), "拒收输入就继续等");
    let (phase, effects) = step(phase, tick(ENTER_RETRY, true));
    assert_eq!(
        phase,
        WorkerPhase::Submitted { retried: true },
        "5 秒补一次回车"
    );
    assert_eq!(effects, vec![Effect::PressEnter], "第二次也是回车");
    let (phase, effects) = step(phase, tick(Duration::from_secs(14), true));
    assert_eq!(
        phase,
        WorkerPhase::Submitted { retried: true },
        "补过之后 14 秒仍在等确认"
    );
    assert!(effects.is_empty(), "不再补第三次");
    let (phase, effects) = step(phase, tick(CONFIRM_WINDOW, false));
    assert_eq!(
        phase,
        WorkerPhase::Failed(WorkerFailure::Unconfirmed),
        "15 秒没确认就是没送达"
    );
    assert!(effects.is_empty(), "超时只改阶段，不叫醒幕僚");
}

/// 场景：确认和撤回谁先到。
/// 数据构造：已经提交、尚未 Running 的 Worker。
/// 执行过程：先确认再撤回；另一条先撤回。
/// 预期结果：确认之后的撤回被忽略并留在 Running；撤回本身是中断，新开的卡要关，复用的卡不关。
#[test]
fn withdraw_after_prompt_submit_is_ignored() {
    let (running, effects) = step(
        WorkerPhase::Submitted { retried: false },
        Input::PromptSubmitHook,
    );
    assert_eq!(running, WorkerPhase::Running, "确认之后才算在跑");
    assert!(effects.is_empty(), "确认不产生额外效果");
    let (still, effects) = step(running, Input::Withdraw { close_card: true });
    assert_eq!(still, WorkerPhase::Running, "已经在跑就撤不回");
    assert!(effects.is_empty(), "晚到的撤回没有效果");
    let (stopped, effects) = step(WorkerPhase::Typed, Input::Withdraw { close_card: true });
    assert_eq!(
        stopped,
        WorkerPhase::Finished(WorkerOutcome::Interrupted),
        "还没确认时撤回结束这一轮"
    );
    assert_eq!(
        effects,
        vec![Effect::Interrupt, Effect::CloseCard],
        "新开的卡要中断并关闭"
    );
    let (_, effects) = step(WorkerPhase::Spawning, Input::Withdraw { close_card: false });
    assert_eq!(effects, vec![Effect::Interrupt], "复用的卡只中断，不关闭");
}

/// 场景：超时之后确认才到，以及用户要求再投一次。
/// 数据构造：阶段已是 Failed(Unconfirmed)。
/// 执行过程：PromptSubmitHook，然后从同一阶段走 Redeliver。
/// 预期结果：晚到确认回到 Running；再投一次回到 Spawning，不保留失败。
#[test]
fn late_confirmation_and_redelivery_leave_the_timeout() {
    let (running, _) = step(
        WorkerPhase::Failed(WorkerFailure::Unconfirmed),
        Input::PromptSubmitHook,
    );
    assert_eq!(running, WorkerPhase::Running, "晚到的确认收回未送达");
    let (again, effects) = step(
        WorkerPhase::Failed(WorkerFailure::Unconfirmed),
        Input::Redeliver,
    );
    assert_eq!(again, WorkerPhase::Spawning, "再投一次重新等打字");
    assert!(effects.is_empty(), "再投一次先改阶段，打字等下一次静默");
    let (typed, _) = step(WorkerPhase::Typed, Input::PromptSubmitHook);
    assert_eq!(typed, WorkerPhase::Typed, "还没回车的钩子不算确认");
}

/// 场景：进程重启时任务里还有没结束的 Worker。
/// 数据构造：自动派发次数是 3；一个 Worker 在 Running，一个已经成功结束。
/// 执行过程：写入快照后用新 Store 加载，再加载一次。
/// 预期结果：Running 变成 Failed(Restarted)，成功的不变；次数仍是 3；只排队一次唤醒。
#[test]
fn restart_fails_unfinished_workers_without_counting_a_dispatch() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let mut task = Task::new("a", "ws", "project", "Ship");
    task.auto_dispatches = 3;
    let mut running = Worker::new("Alan", "Codex", "Build", 7, "d-run");
    running.phase = WorkerPhase::Running;
    let mut done = Worker::new("Bella", "Claude", "Review", 8, "d-done");
    done.phase = WorkerPhase::Finished(WorkerOutcome::Succeeded);
    task.workers.push(running);
    task.workers.push(done);
    store.create(task).expect("create");
    let loaded = TaskStore::new(dir.path().into());
    let task = loaded.get("ws", "a").expect("reload");
    assert_eq!(
        task.workers[0].phase,
        WorkerPhase::Failed(WorkerFailure::Restarted),
        "没结束的 Worker 在加载时失败"
    );
    assert_eq!(
        task.workers[1].phase,
        WorkerPhase::Finished(WorkerOutcome::Succeeded),
        "已经成功的不改"
    );
    assert_eq!(task.auto_dispatches, 3, "重启唤醒不计入自动派发次数");
    assert_eq!(
        loaded.take_restart_wakes(),
        vec![("ws".into(), "a".into())],
        "每个任务只唤醒一次"
    );
    assert!(loaded.take_restart_wakes().is_empty(), "唤醒队列取走后就空");
    let again = TaskStore::new(dir.path().into());
    assert_eq!(
        again.get("ws", "a").expect("second").workers[0].phase,
        WorkerPhase::Failed(WorkerFailure::Restarted),
        "再次加载不再改已经失败的 Worker"
    );
    assert!(
        again.take_restart_wakes().is_empty(),
        "已经是重启失败的任务不再唤醒"
    );
}

/// 场景：进程退出和主动停止。
/// 数据构造：Running 遇到 Exited 和 Stop；已经结束的再遇到 Restarted。
/// 预期结果：退出是失败并唤醒；停止是中断且不唤醒；终态被重启输入忽略。
#[test]
fn exit_wakes_and_stop_does_not_and_terminal_phases_stay() {
    let (phase, effects) = step(WorkerPhase::Running, Input::Exited);
    assert_eq!(
        phase,
        WorkerPhase::Finished(WorkerOutcome::Failed),
        "进程退出记为失败"
    );
    assert_eq!(effects, vec![Effect::WakeStaff], "退出要告诉幕僚");
    let (phase, effects) = step(WorkerPhase::Waiting, Input::Stop);
    assert_eq!(
        phase,
        WorkerPhase::Finished(WorkerOutcome::Interrupted),
        "停止记为中断"
    );
    assert!(effects.is_empty(), "停止不额外唤醒");
    let (phase, effects) = step(
        WorkerPhase::Finished(WorkerOutcome::Succeeded),
        Input::Restarted,
    );
    assert_eq!(
        phase,
        WorkerPhase::Finished(WorkerOutcome::Succeeded),
        "成功结果不被重启改写"
    );
    assert!(effects.is_empty(), "终态没有唤醒");
}
