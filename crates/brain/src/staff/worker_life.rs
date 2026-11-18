//! Worker 生命周期。纯函数：给定当前阶段和一次输入，得到下一阶段和要执行的效果。
use norma::protocol::{WorkerFailure, WorkerOutcome, WorkerPhase};
use std::time::Duration;

/// 打完字后等多久按回车。与 Agent 输入的共享延迟一致。
pub const ENTER_DELAY: Duration = Duration::from_millis(100);
/// 回车后仍无确认，补一次回车的等待。
pub const ENTER_RETRY: Duration = Duration::from_secs(5);
/// 第一次回车后仍无 UserPromptSubmit，就视为没送达。
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Input {
    /// 进程已经拉起来，可以开始等它能接收输入。
    Spawned,
    /// 宿主判断现在可以打字（Ready、输出静默，或启动宽限到了）。
    OutputQuiet,
    PromptSubmitHook,
    Stop,
    Exited,
    /// `elapsed` 是进入当前阶段之后过去的时间。
    Tick {
        elapsed: Duration,
        accepts_input: bool,
    },
    /// 超时后用户要求在同一张卡上再投一次。
    Redeliver,
    /// `close_card`：这张卡是这次派活新开的，撤回时关掉。
    Withdraw {
        close_card: bool,
    },
    Restarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    CreateCard,
    Spawn,
    Type,
    PressEnter,
    Interrupt,
    CloseCard,
    WakeStaff,
}

pub fn step(phase: WorkerPhase, input: Input) -> (WorkerPhase, Vec<Effect>) {
    match (phase, input) {
        (WorkerPhase::Reserved, Input::Spawned) => (WorkerPhase::Spawning, Vec::new()),
        (
            WorkerPhase::Submitted { .. } | WorkerPhase::Failed(WorkerFailure::Unconfirmed),
            Input::Spawned,
        ) => (WorkerPhase::Spawning, Vec::new()),
        (WorkerPhase::Spawning, Input::OutputQuiet) => (WorkerPhase::Typed, vec![Effect::Type]),
        (
            WorkerPhase::Typed,
            Input::Tick {
                elapsed,
                accepts_input,
            },
        ) if accepts_input && elapsed >= ENTER_DELAY => (
            WorkerPhase::Submitted { retried: false },
            vec![Effect::PressEnter],
        ),
        (WorkerPhase::Submitted { retried: false }, Input::Tick { elapsed, .. })
            if elapsed >= CONFIRM_WINDOW =>
        {
            (WorkerPhase::Failed(WorkerFailure::Unconfirmed), Vec::new())
        }
        (
            WorkerPhase::Submitted { retried: false },
            Input::Tick {
                elapsed,
                accepts_input,
            },
        ) if accepts_input && elapsed >= ENTER_RETRY => (
            WorkerPhase::Submitted { retried: true },
            vec![Effect::PressEnter],
        ),
        (WorkerPhase::Submitted { retried: true }, Input::Tick { elapsed, .. })
            if elapsed >= CONFIRM_WINDOW =>
        {
            (WorkerPhase::Failed(WorkerFailure::Unconfirmed), Vec::new())
        }
        (
            WorkerPhase::Submitted { .. } | WorkerPhase::Failed(WorkerFailure::Unconfirmed),
            Input::PromptSubmitHook,
        ) => (WorkerPhase::Running, Vec::new()),
        (WorkerPhase::Failed(WorkerFailure::Unconfirmed), Input::Redeliver) => {
            (WorkerPhase::Spawning, Vec::new())
        }
        (phase, Input::Withdraw { close_card }) if before_running(phase) => {
            let mut effects = vec![Effect::Interrupt];
            if close_card {
                effects.push(Effect::CloseCard);
            }
            (WorkerPhase::Finished(WorkerOutcome::Interrupted), effects)
        }
        (phase, Input::Restarted) if unfinished(phase) => (
            WorkerPhase::Failed(WorkerFailure::Restarted),
            vec![Effect::WakeStaff],
        ),
        (phase, Input::Exited) if unfinished(phase) => (
            WorkerPhase::Finished(WorkerOutcome::Failed),
            vec![Effect::WakeStaff],
        ),
        (WorkerPhase::Running | WorkerPhase::Waiting, Input::Stop) => (
            WorkerPhase::Finished(WorkerOutcome::Interrupted),
            Vec::new(),
        ),
        (phase, Input::Stop) if before_running(phase) => (
            WorkerPhase::Finished(WorkerOutcome::Interrupted),
            vec![Effect::Interrupt],
        ),
        (phase, _) => (phase, Vec::new()),
    }
}

pub fn before_running(phase: WorkerPhase) -> bool {
    matches!(
        phase,
        WorkerPhase::Reserved
            | WorkerPhase::Spawning
            | WorkerPhase::Typed
            | WorkerPhase::Submitted { .. }
    )
}

pub(crate) fn unfinished(phase: WorkerPhase) -> bool {
    !matches!(phase, WorkerPhase::Finished(_) | WorkerPhase::Failed(_))
}

/// 把还没结束的 Worker 收成重启失败。有变化时返回 true；不改自动派发次数。
pub(crate) fn fail_restarted(task: &mut norma::protocol::Task) -> bool {
    let mut changed = false;
    for worker in &mut task.workers {
        let (next, _) = step(worker.phase, Input::Restarted);
        if next != worker.phase {
            worker.phase = next;
            changed = true;
        }
    }
    changed
}
