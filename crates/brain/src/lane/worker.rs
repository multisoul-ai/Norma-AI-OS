//! 一条 lane 的工作线程：串行处理 prompt，会话失败时退避重建。

use super::prompt::{primer, primer_with_skill, Role};
use super::session::SessionToken;
pub use super::session::{Idle, Tick};
use crate::acp::client::{AcpError, AcpSession};
use crate::acp::trace::{TraceItem, TraceKind};
use crate::backend::Backend;
use crate::event::{BrainStatus, BrainStatusKind, TaskUpdate, Trigger, TriggerKind};
use crate::goal::GoalStore;
use norma::protocol::{BrainScope, ReplyStatus, ReplyTrigger};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// 与 App 进程的 launch_id 配合：重启服务和并行审批不能复用轮次号。
static NEXT_TURN: AtomicU64 = AtomicU64::new(1);

const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// 临时故障（如 429 限流）下同一条任务最多重试的次数，超过才放弃。
const MAX_TRANSIENT_RETRIES: u32 = 3;

pub struct Job {
    pub task_id: Option<String>,
    pub cancelled: Arc<AtomicBool>,
    pub scope: BrainScope,
    pub text: String,
    pub approval_id: Option<String>,
    pub trigger: Trigger,
}

#[path = "worker_env.rs"]
mod env;

pub struct LaneEnv {
    pub role: Role,
    pub backend: Backend,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub norma_bin: PathBuf,
    pub project_root: PathBuf,
    pub goal: Arc<GoalStore>,
    pub tasks: Arc<crate::staff::TaskStore>,
    pub status: Sender<BrainStatus>,
    pub open: Arc<Mutex<HashSet<(BrainScope, String)>>>,
    pub stop: Arc<AtomicBool>,
    pub(crate) scopes: crate::service::scopes::Scopes,
    pub env: Vec<(String, String)>,
    pub staff_tokens: Option<crate::staff::gate::StaffTokens>,
    pub skill_ready: bool,
    pub prompt_timeout: Option<Duration>,
    pub retry_backoff: Duration,
    /// 超过这个轮数就重建会话，避免上下文无限增长。
    pub max_turns: u32,
    /// Claude / Codex 幕僚共用的连接。OpenCode 和自检失败时为空。
    pub hub: Option<std::sync::Arc<crate::acp::hub::SharedHub>>,
    /// 只有 Claude 有。满 4 个会话时新任务在这里排队。
    pub(crate) pool: Option<std::sync::Arc<Mutex<super::staff_pool::StaffPool>>>,
}

/// 多个 worker 共用一个队列：谁空闲谁取下一条，所以并行度就是 worker 个数。
pub type SharedJobs = Arc<Mutex<Receiver<Job>>>;

pub fn run(env: LaneEnv, jobs: SharedJobs, idle: Option<Idle>) {
    let mut session_token = SessionToken {
        tokens: env.staff_tokens.clone(),
        token: None,
    };
    let mut session: Option<AcpSession> = None;
    let mut staff_cursor: Option<super::staff_prompt::PromptCursor> = None;
    let mut turns = 0u32;
    let mut session_staff = false;
    let mut primed = false;
    let mut failures = 0u32;
    let mut retrying: Option<(Job, u32)> = None;
    let mut resident: Option<(BrainScope, String)> = None;
    let mut closed_check = Instant::now();
    'lane: while !env.stop.load(Ordering::Relaxed) {
        // 回到这里就是上一条处理完了；空闲时长从此刻算起。
        let idle_since = Instant::now();
        let (job, attempts) = match retrying.take() {
            Some(again) => again,
            None => loop {
                if env.stop.load(Ordering::Relaxed) {
                    break 'lane;
                }
                let received = jobs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv_timeout(Duration::from_millis(100));
                match received {
                    Ok(job) => break (job, 0),
                    Err(RecvTimeoutError::Timeout) => {
                        let Some(idle) = idle.as_ref() else {
                            continue;
                        };
                        if let Some(poll) = &idle.poll {
                            let closed = if closed_check.elapsed() >= Duration::from_secs(1) {
                                closed_check = Instant::now();
                                resident
                                    .as_ref()
                                    .is_some_and(|(scope, id)| env.task_closed(scope, id))
                            } else {
                                false
                            };
                            match poll(closed) {
                                super::session::Tick::Job(job) => break (job, 0),
                                super::session::Tick::Exit => break 'lane,
                                super::session::Tick::Stay => {}
                            }
                        }
                        if idle_since.elapsed() < idle.after {
                            continue;
                        }
                        match (idle.retire)() {
                            Some(job) => break (job, 0),
                            None => break 'lane,
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break 'lane,
                }
            },
        };
        if let Some(task_id) = &job.task_id {
            resident = Some((job.scope.clone(), task_id.clone()));
            if let Some(pool) = &env.pool {
                pool.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .mark_busy(task_id);
            }
        }
        let route_cancelled = job.cancelled.clone();
        if route_cancelled.load(Ordering::Relaxed) {
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
        let turn_started = Instant::now();
        let trigger = if job.trigger.kind == TriggerKind::UserRequest {
            ReplyTrigger::User
        } else {
            ReplyTrigger::Event
        };
        let task = match super::staff::load(&env.tasks, &job.scope, job.task_id.as_deref(), trigger)
        {
            Ok(task) => task,
            Err(error) => {
                env.send(
                    &job.scope,
                    BrainStatusKind::TaskError {
                        reason: error.to_string(),
                    },
                );
                env.task_update(&job, turn_no, TaskUpdate::Started);
                env.finish(&job, turn_no, ReplyStatus::Failed, turn_started);
                continue;
            }
        };
        let staff_turn = task.as_ref().map(|task| {
            env.scopes
                .staff_turn(&job.scope, &task.id, task.round, &route_cancelled)
        });
        let cancelled = staff_turn
            .as_ref()
            .map_or(&route_cancelled, |turn| &turn.cancelled);
        env.task_update(&job, turn_no, TaskUpdate::Started);
        if let Some(task) = &task {
            env.send(&job.scope, BrainStatusKind::TaskChanged(task.clone()));
        }
        if session.is_none() || turns >= env.max_turns || session_staff != job.task_id.is_some() {
            session = None;
            staff_cursor = None;
            let connecting = Instant::now();
            // 每个幕僚会话为自己的任务铸一把 token；换会话时上一把随之撤销。
            let minted = job
                .task_id
                .as_deref()
                .and_then(|task| env.staff_tokens.as_ref().map(|tokens| tokens.mint(task)));
            session_token.replace(minted.clone());
            if let (Some(pool), Some(token), Some(task_id)) = (&env.pool, &minted, &job.task_id) {
                pool.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .note(task_id, token);
            }
            let started = if job.task_id.is_some() {
                let options = env.session_options(minted.as_deref(), true);
                match env.hub.as_ref() {
                    Some(hub) => match hub.open_staff(&options, cancelled) {
                        Ok(session) => Ok(session),
                        // 用户取消不能再起一个独占进程。
                        Err(AcpError::Cancelled) => Err(AcpError::Cancelled),
                        // 自检失败或连接建不起来：退回每个任务一个进程。
                        Err(_) => AcpSession::start_staff_cancellable(&options, cancelled),
                    },
                    None => AcpSession::start_staff_cancellable(&options, cancelled),
                }
            } else {
                AcpSession::start_cancellable(&env.session_options(None, false), cancelled)
            };
            match started {
                Ok(started) => {
                    session = Some(started);
                    session_staff = job.task_id.is_some();
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
                    let status = if matches!(error, AcpError::Cancelled) {
                        ReplyStatus::Stopped
                    } else {
                        ReplyStatus::Failed
                    };
                    env.finish(&job, turn_no, status, turn_started);
                    env.sleep(
                        (env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF),
                        cancelled,
                    );
                    continue;
                }
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            session = None;
            staff_cursor = None;
            env.finish(&job, turn_no, ReplyStatus::Stopped, turn_started);
            continue;
        }
        let mut next_cursor = None;
        let mut text = if let Some(task) = &task {
            let (piece, next) = super::staff_prompt::incremental(task, staff_cursor.as_ref());
            next_cursor = Some(next);
            if staff_cursor.is_none() {
                super::staff::prompt_with(task, &job.scope, &env.norma_bin)
            } else {
                piece
            }
        } else if primed {
            job.text.clone()
        } else {
            let bin = env.norma_bin.display().to_string();
            let goal = env
                .tasks
                .summary_for(&job.scope.workspace_id)
                .ok()
                .flatten()
                .or_else(|| env.goal.summary_for(&job.scope.workspace_id));
            let opening = if env.skill_ready {
                primer_with_skill(env.role, goal.as_deref())
            } else {
                primer(env.role, &bin, goal.as_deref())
            };
            format!("{opening}\n\n{}", job.text)
        };
        let images = if let Some(task) = &task {
            let attached = super::staff_images::attachments(task);
            if !attached.blocks.is_empty() {
                text.push_str(
                    "\n\n本轮用户上传的图片已作为随后的 image 内容块附上。直接查看这些内容块，不要再去读磁盘路径。",
                );
            }
            if !attached.missing.is_empty() {
                text.push_str("\n\n这些已上传图片本次没能读入，不要当作已经看过：");
                text.push_str(&attached.missing.join(" "));
            }
            attached.blocks
        } else {
            Vec::new()
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
        active.set_question_host(job.task_id.as_ref().map(|id| {
            crate::acp::questions::QuestionHost::new(
                env.tasks.clone(),
                job.scope.clone(),
                id.clone(),
                turn_no,
                env.status.clone(),
            )
        }));
        let mut writer = job.task_id.clone().filter(|_| task.is_some()).map(|id| {
            super::staff_stream::ReplyWriter::new(
                env.tasks.clone(),
                job.scope.clone(),
                id,
                env.status.clone(),
            )
        });
        let result = active.prompt_observed(
            &text,
            &images,
            cancelled,
            env.prompt_timeout,
            &mut |item| env.trace(&job.scope, this_turn, &job.trigger, item),
            &mut |item| {
                if let Some(writer) = &mut writer {
                    writer.observe(item);
                }
            },
        );
        match result {
            Ok(_) if cancelled.load(Ordering::Relaxed) => {
                staff_cursor = None;
                env.finish(&job, turn_no, ReplyStatus::Stopped, turn_started);
                continue;
            }
            Ok(reply) => {
                if let Some(next) = next_cursor.take() {
                    staff_cursor = Some(next);
                }
                if task.is_some() {
                    // Keep the already-computed reply until it is durable; never rerun model effects.
                    let saved = loop {
                        let session = active.session_id().to_owned();
                        match env.tasks.settle_reply(
                            &job.scope.workspace_id,
                            job.task_id.as_deref().unwrap_or_default(),
                            ReplyStatus::Done,
                            turn_started.elapsed().as_millis() as u64,
                            Some(&session),
                        ) {
                            Ok(saved) => {
                                if let Some(task) = saved {
                                    env.send(&job.scope, BrainStatusKind::TaskChanged(task));
                                }
                                break true;
                            }
                            Err(error) => env.send(
                                &job.scope,
                                BrainStatusKind::TaskError {
                                    reason: error.to_string(),
                                },
                            ),
                        }
                        if env.stop.load(Ordering::Relaxed) || cancelled.load(Ordering::Relaxed) {
                            break false;
                        }
                        env.sleep(env.retry_backoff, cancelled);
                    };
                    if !saved {
                        let status = if cancelled.load(Ordering::Relaxed) {
                            ReplyStatus::Stopped
                        } else {
                            ReplyStatus::Failed
                        };
                        env.finish(&job, turn_no, status, turn_started);
                        continue;
                    }
                }
                turns += 1;
                primed = task.is_none();
                failures = 0;
                env.goal.append_decision(
                    &serde_json::json!({"workspace_id": job.scope.workspace_id, "route_id": job.scope.route_id, "lane": format!("{:?}", env.role), "reply": reply})
                        .to_string(),
                );
            }
            Err(error) => {
                // 共用连接上取消只停这一轮，进程留给其他会话。独占进程取消后已经杀掉，下次重建。
                let keep_staff = session.as_ref().is_some_and(|session| session.is_shared())
                    && session_staff
                    && matches!(error, AcpError::Cancelled);
                if !keep_staff {
                    session = None;
                    staff_cursor = None;
                }
                env.trace(
                    &job.scope,
                    turn_no,
                    &job.trigger,
                    TraceItem::new(TraceKind::Error, "本轮失败", error.to_string()),
                );
                if !matches!(error, AcpError::Cancelled) {
                    down(&env, &job.scope, &error);
                }
                if matches!(error, AcpError::Cancelled) {
                    env.finish(&job, turn_no, ReplyStatus::Stopped, turn_started);
                    continue;
                }
                failures += 1;
                if matches!(error, AcpError::Transient(_)) && attempts < MAX_TRANSIENT_RETRIES {
                    // 限流等临时故障：退避后用新会话重发同一条任务，不能把它丢掉。
                    // 还没写出任何内容的回复直接撤掉，重发会开一条新的，不留一条空的“失败”。
                    if let Some(task_id) = &job.task_id {
                        if let Ok(Some(task)) = env
                            .tasks
                            .discard_empty_reply(&job.scope.workspace_id, task_id)
                        {
                            env.send(&job.scope, BrainStatusKind::TaskChanged(task));
                        }
                    }
                    env.finish(&job, turn_no, ReplyStatus::Failed, turn_started);
                    env.task_update(&job, turn_no, TaskUpdate::Queued);
                    retrying = Some((job, attempts + 1));
                } else {
                    env.finish(&job, turn_no, ReplyStatus::Failed, turn_started);
                }
                env.sleep(
                    (env.retry_backoff * 2u32.saturating_pow(failures - 1)).min(MAX_BACKOFF),
                    cancelled,
                );
                continue;
            }
        }
        env.finish(&job, turn_no, ReplyStatus::Done, turn_started);
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
