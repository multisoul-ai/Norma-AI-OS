//! 一条 lane 的工作线程：串行处理 prompt，会话失败时退避重建。

use super::prompt::{primer, Role};
use crate::acp::client::{AcpError, AcpSession, SessionOptions};
use crate::acp::gate::Gate;
use crate::acp::trace::{AcpTrace, TraceItem, TraceKind};
use crate::backend::{self, Backend};
use crate::event::BrainStatus;
use crate::goal::GoalStore;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const MAX_BACKOFF: Duration = Duration::from_secs(60);

pub struct Job {
    pub text: String,
    pub approval_id: Option<String>,
}

pub struct LaneEnv {
    pub role: Role,
    pub backend: Backend,
    pub model: Option<String>,
    pub gate: Gate,
    pub goal: Arc<GoalStore>,
    pub status: Sender<BrainStatus>,
    pub open: Arc<Mutex<HashSet<String>>>,
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
            program: self.backend.program.clone(),
            args: self.backend.args.clone(),
            cwd: self.gate.project_root.clone(),
            env: self.env.clone(),
            meta: backend::session_meta(self.backend.kind, self.model.as_deref()),
            mode: backend::session_mode(self.backend.kind).map(str::to_string),
            model: (self.backend.kind == backend::BackendKind::ClaudeCode)
                .then(|| self.model.clone())
                .flatten(),
            handshake_timeout: Duration::from_secs(60),
        }
    }

    fn is_open(&self, id: &str) -> bool {
        self.open.lock().expect("open lock").contains(id)
    }

    fn sleep(&self, total: Duration) {
        let end = Instant::now() + total;
        while Instant::now() < end && !self.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn trace(&self, turn: u64, item: TraceItem) {
        let _ = self.status.send(BrainStatus::Trace(AcpTrace {
            at: SystemTime::now(),
            lane: format!("{:?}", self.role),
            turn,
            item,
        }));
    }

    fn finish(&self, job: &Job) {
        if let Some(id) = &job.approval_id {
            let _ = self
                .status
                .send(BrainStatus::ApprovalTurnEnded { id: id.clone() });
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
    let mut turn_no = 0u64;
    while !env.stop.load(Ordering::Relaxed) {
        let received = jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv_timeout(Duration::from_millis(100));
        let job = match received {
            Ok(job) => job,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if job
            .approval_id
            .as_deref()
            .is_some_and(|id| !env.is_open(id))
        {
            continue;
        }
        if session.is_none() || turns >= env.max_turns {
            session = None;
            match AcpSession::start(&env.session_options()) {
                Ok(started) => {
                    session = Some(started);
                    turns = 0;
                    primed = false;
                    env.trace(
                        turn_no,
                        TraceItem::new(TraceKind::Lifecycle, "会话已创建", ""),
                    );
                    let _ = env.status.send(BrainStatus::Ready);
                }
                Err(error) => {
                    failures += 1;
                    env.trace(
                        turn_no,
                        TraceItem::new(TraceKind::Error, "会话创建失败", error.to_string()),
                    );
                    down(&env, &error);
                    env.finish(&job);
                    env.sleep(
                        (env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF),
                    );
                    continue;
                }
            }
        }
        let text = if primed {
            job.text.clone()
        } else {
            let bin = env.gate.norma_bin.display().to_string();
            format!(
                "{}\n\n{}",
                primer(env.role, &bin, env.goal.summary().as_deref()),
                job.text
            )
        };
        let active = session.as_mut().expect("会话刚创建");
        turn_no += 1;
        env.trace(
            turn_no,
            TraceItem::new(TraceKind::Prompt, "发给大脑", text.clone()),
        );
        let this_turn = turn_no;
        let result = active.prompt(
            &text,
            &env.gate,
            &env.stop,
            env.prompt_timeout,
            &mut |item| env.trace(this_turn, item),
        );
        match result {
            Ok(reply) => {
                turns += 1;
                primed = true;
                failures = 0;
                env.goal.append_decision(
                    &serde_json::json!({"lane": format!("{:?}", env.role), "reply": reply})
                        .to_string(),
                );
            }
            Err(error) => {
                session = None;
                failures += 1;
                env.trace(
                    turn_no,
                    TraceItem::new(TraceKind::Error, "本轮失败", error.to_string()),
                );
                if !matches!(error, AcpError::Cancelled) {
                    down(&env, &error);
                }
                env.finish(&job);
                env.sleep((env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF));
                continue;
            }
        }
        env.finish(&job);
    }
}

fn down(env: &LaneEnv, error: &AcpError) {
    tracing::warn!(%error, "大脑后端不可用");
    let _ = env.status.send(BrainStatus::BackendDown {
        reason: error.to_string(),
    });
}
