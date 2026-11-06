//! 一条 lane 的工作线程：串行处理 prompt，会话失败时退避重建。

use super::prompt::{primer, Role};
use crate::acp::client::{AcpError, AcpSession, SessionOptions};
use crate::acp::gate::Gate;
use crate::acp::trace::{AcpTrace, TraceItem, TraceKind};
use crate::backend::{self, Backend};
use crate::event::{BrainStatus, BrainStatusKind, Trigger};
use crate::goal::GoalStore;
use norma::protocol::BrainScope;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

// 与 App 进程的 launch_id 配合：重启服务和并行审批不能复用轮次号。
static NEXT_TURN: AtomicU64 = AtomicU64::new(1);

const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// 临时故障（如 429 限流）下同一条任务最多重试的次数，超过才放弃。
const MAX_TRANSIENT_RETRIES: u32 = 3;

pub struct Job {
    pub cancelled: Arc<AtomicBool>,
    pub scope: BrainScope,
    pub text: String,
    pub approval_id: Option<String>,
    pub trigger: Trigger,
}

pub struct LaneEnv {
    pub role: Role,
    pub backend: Backend,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub gate: Gate,
    pub goal: Arc<GoalStore>,
    pub status: Sender<BrainStatus>,
    pub open: Arc<Mutex<HashSet<(BrainScope, String)>>>,
    pub stop: Arc<AtomicBool>,
    pub env: Vec<(String, String)>,
    pub prompt_timeout: Duration,
    pub retry_backoff: Duration,
    /// 超过这个轮数就重建会话，避免上下文无限增长。
    pub max_turns: u32,
}

impl LaneEnv {
    fn session_options(&self) -> SessionOptions {
        SessionOptions {
            kind: self.backend.kind,
            program: self.backend.program.clone(),
            args: self.backend.args.clone(),
            cwd: self.gate.project_root.clone(),
            env: self
                .env
                .iter()
                .cloned()
                .chain(backend::session_env(self.backend.kind))
                .collect(),
            meta: backend::session_meta(self.backend.kind, self.model.as_deref()),
            mode: backend::session_mode(self.backend.kind).map(str::to_string),
            model: self.model.clone(),
            effort: self.effort.clone(),
            handshake_timeout: Duration::from_secs(60),
        }
    }

    fn is_open(&self, scope: &BrainScope, id: &str) -> bool {
        self.open
            .lock()
            .expect("open lock")
            .contains(&(scope.clone(), id.to_owned()))
    }

    fn sleep(&self, total: Duration, cancelled: &AtomicBool) {
        let end = Instant::now() + total;
        while Instant::now() < end
            && !self.stop.load(Ordering::Relaxed)
            && !cancelled.load(Ordering::Relaxed)
        {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn send(&self, scope: &BrainScope, kind: BrainStatusKind) {
        let _ = self.status.send(BrainStatus {
            scope: scope.clone(),
            kind,
        });
    }

    fn trace(&self, scope: &BrainScope, turn: u64, trigger: &Trigger, item: TraceItem) {
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

    fn finish(&self, job: &Job) {
        if let Some(id) = &job.approval_id {
            self.send(
                &job.scope,
                BrainStatusKind::ApprovalTurnEnded { id: id.clone() },
            );
        }
    }
}

/// 多个 worker 共用一个队列：谁空闲谁取下一条，所以并行度就是 worker 个数。
pub type SharedJobs = Arc<Mutex<Receiver<Job>>>;

pub fn run(env: LaneEnv, jobs: SharedJobs) {
    let mut session: Option<AcpSession> = None;
    let mut turns = 0u32;
    let mut primed = false;
    let mut failures = 0u32;
    let mut retrying: Option<(Job, u32)> = None;
    while !env.stop.load(Ordering::Relaxed) {
        let (job, attempts) = match retrying.take() {
            Some(again) => again,
            None => {
                let received = jobs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv_timeout(Duration::from_millis(100));
                match received {
                    Ok(job) => (job, 0),
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        };
        let cancelled = job.cancelled.clone();
        if cancelled.load(Ordering::Relaxed) {
            continue;
        }
        if job
            .approval_id
            .as_deref()
            .is_some_and(|id| !env.is_open(&job.scope, id))
        {
            continue;
        }
        let turn_no = NEXT_TURN.fetch_add(1, Ordering::Relaxed);
        if session.is_none() || turns >= env.max_turns {
            session = None;
            let connecting = Instant::now();
            match AcpSession::start_cancellable(&env.session_options(), &cancelled) {
                Ok(started) => {
                    session = Some(started);
                    turns = 0;
                    primed = false;
                    env.send(&job.scope, BrainStatusKind::Ready);
                }
                Err(error) => {
                    failures += 1;
                    env.trace(
                        &job.scope,
                        turn_no,
                        &job.trigger,
                        TraceItem::new(TraceKind::Error, "会话创建失败", error.to_string()),
                    );
                    env.trace(
                        &job.scope,
                        turn_no,
                        &job.trigger,
                        TraceItem::turn_end(
                            if matches!(error, AcpError::Cancelled) {
                                "cancelled"
                            } else {
                                "error"
                            },
                            connecting.elapsed().as_millis() as u64,
                        ),
                    );
                    if !matches!(error, AcpError::Cancelled) {
                        down(&env, &job.scope, &error);
                    }
                    env.finish(&job);
                    env.sleep(
                        (env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF),
                        &cancelled,
                    );
                    continue;
                }
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            session = None;
            continue;
        }
        let text = if primed {
            job.text.clone()
        } else {
            let bin = env.gate.norma_bin.display().to_string();
            format!(
                "{}\n\n{}",
                primer(
                    env.role,
                    &bin,
                    env.goal.summary_for(&job.scope.workspace_id).as_deref()
                ),
                job.text
            )
        };
        let active = session.as_mut().expect("会话刚创建");
        let identity = format!(
            "backend={:?} session_id={}",
            env.backend.kind,
            active.session_id()
        );
        tracing::info!(turn = turn_no, lane = ?env.role, backend = ?env.backend.kind,
            session_id = active.session_id(), "大脑轮次开始");
        env.trace(
            &job.scope,
            turn_no,
            &job.trigger,
            TraceItem::new(TraceKind::Lifecycle, "会话关联", identity),
        );
        env.trace(
            &job.scope,
            turn_no,
            &job.trigger,
            TraceItem::new(TraceKind::Prompt, "发给大脑", text.clone()),
        );
        let this_turn = turn_no;
        let result = active.prompt(
            &text,
            &env.gate,
            &cancelled,
            env.prompt_timeout,
            &mut |item| env.trace(&job.scope, this_turn, &job.trigger, item),
        );
        match result {
            Ok(reply) => {
                turns += 1;
                primed = true;
                failures = 0;
                env.goal.append_decision(
                    &serde_json::json!({"workspace_id": job.scope.workspace_id, "route_id": job.scope.route_id, "lane": format!("{:?}", env.role), "reply": reply})
                        .to_string(),
                );
            }
            Err(error) => {
                session = None;
                failures += 1;
                env.trace(
                    &job.scope,
                    turn_no,
                    &job.trigger,
                    TraceItem::new(TraceKind::Error, "本轮失败", error.to_string()),
                );
                if !matches!(error, AcpError::Cancelled) {
                    down(&env, &job.scope, &error);
                }
                if matches!(error, AcpError::Transient(_)) && attempts < MAX_TRANSIENT_RETRIES {
                    // 限流等临时故障：退避后用新会话重发同一条任务，不能把它丢掉。
                    retrying = Some((job, attempts + 1));
                } else {
                    env.finish(&job);
                }
                env.sleep(
                    (env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF),
                    &cancelled,
                );
                continue;
            }
        }
        env.finish(&job);
    }
}

fn down(env: &LaneEnv, scope: &BrainScope, error: &AcpError) {
    tracing::warn!(%error, "大脑后端不可用");
    env.send(
        scope,
        BrainStatusKind::BackendDown {
            reason: error.to_string(),
        },
    );
}
