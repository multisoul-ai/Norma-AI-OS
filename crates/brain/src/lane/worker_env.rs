use super::{Job, LaneEnv};
use crate::acp::client::SessionOptions;
use crate::acp::trace::{AcpTrace, TraceItem};
use crate::backend;
use crate::event::{BrainStatus, BrainStatusKind, TaskUpdate, Trigger};
use norma::protocol::{BrainScope, ReplyStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

impl LaneEnv {
    pub(super) fn session_options(&self, staff_token: Option<&str>, staff: bool) -> SessionOptions {
        let env = if staff {
            crate::staff::gate::staff_env(&self.env, staff_token)
        } else {
            self.env.clone()
        };
        SessionOptions {
            kind: self.backend.kind,
            program: self.backend.program.clone(),
            args: self.backend.args.clone(),
            cwd: self.project_root.clone(),
            env,
            meta: backend::session_meta(self.backend.kind, self.model.as_deref(), staff),
            staff_shell: staff.then(|| self.norma_bin.clone()),
            mode: backend::session_mode(self.backend.kind).map(str::to_string),
            model: self.model.clone(),
            effort: self.effort.clone(),
            handshake_timeout: Duration::from_secs(60),
        }
    }

    pub(super) fn task_closed(&self, scope: &BrainScope, id: &str) -> bool {
        self.tasks
            .get(&scope.workspace_id, id)
            .ok()
            .is_some_and(|task| task.archived || task.phase == norma::protocol::TaskPhase::Done)
    }

    pub(super) fn is_open(&self, scope: &BrainScope, id: &str) -> bool {
        self.open
            .lock()
            .expect("open lock")
            .contains(&(scope.clone(), id.to_owned()))
    }

    pub(super) fn sleep(&self, total: Duration, cancelled: &AtomicBool) {
        let end = Instant::now() + total;
        while Instant::now() < end
            && !self.stop.load(Ordering::Relaxed)
            && !cancelled.load(Ordering::Relaxed)
        {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    pub(super) fn send(&self, scope: &BrainScope, kind: BrainStatusKind) {
        let _ = self.status.send(BrainStatus {
            scope: scope.clone(),
            kind,
        });
    }

    pub(super) fn trace(&self, scope: &BrainScope, turn: u64, trigger: &Trigger, item: TraceItem) {
        self.send(
            scope,
            BrainStatusKind::Trace(AcpTrace {
                at: SystemTime::now(),
                lane: format!("{:?}", self.role),
                turn,
                trigger: trigger.clone(),
                item,
            }),
        );
    }

    pub(super) fn task_update(&self, job: &Job, turn: u64, update: TaskUpdate) {
        if let Some(task_id) = &job.task_id {
            self.send(
                &job.scope,
                BrainStatusKind::TaskActivity {
                    task_id: task_id.clone(),
                    turn,
                    update,
                },
            );
        }
    }

    /// 把这条任务里仍在运行的幕僚回复收尾并落盘；已经收尾过的什么也不写。
    pub(super) fn settle(
        &self,
        job: &Job,
        status: ReplyStatus,
        started: Instant,
        session_id: Option<&str>,
    ) {
        let Some(task_id) = &job.task_id else {
            return;
        };
        match self.tasks.settle_reply(
            &job.scope.workspace_id,
            task_id,
            status,
            started.elapsed().as_millis() as u64,
            session_id,
        ) {
            Ok(Some(task)) => self.send(&job.scope, BrainStatusKind::TaskChanged(task)),
            Ok(None) => {}
            Err(error) => self.send(
                &job.scope,
                BrainStatusKind::TaskError {
                    reason: error.to_string(),
                },
            ),
        }
    }

    pub(super) fn finish(&self, job: &Job, turn: u64, status: ReplyStatus, started: Instant) {
        // 成功路径已经记为 Done。提前退出在这里收口；用户取消必须传 Stopped。
        self.settle(job, status, started, None);
        let failed = status != ReplyStatus::Done;
        self.task_update(job, turn, TaskUpdate::Finished { failed });
        if let Some(id) = &job.approval_id {
            self.send(
                &job.scope,
                BrainStatusKind::ApprovalTurnEnded { id: id.clone() },
            );
        }
    }
}
