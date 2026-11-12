//! 对外入口：`push` 事件进来，从 `Receiver<BrainStatus>` 读状态。

use crate::acp::gate::{Gate, ReadRoots};
use crate::backend::{self, Backend, Strategy};
use crate::event::{BrainEvent, BrainEventKind, BrainStatus};
use crate::goal::GoalStore;
use crate::lane::prompt::{self, Role};
use crate::lane::worker::{self, Job, LaneEnv};
use crate::router::{Action, Lane, RouterConfig};
use crate::scoped_router::{ScopedAction, ScopedRouter};
use crate::staff::TaskStore;
use norma::protocol::BrainScope;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct BrainConfig {
    pub backend: Backend,
    pub strategy: Strategy,
    /// 覆盖策略映射出的模型；`None` 表示按策略（非 Claude 后端始终跟随 CLI 默认）。
    pub main_model: Option<String>,
    pub approval_model: Option<String>,
    /// 审批会话的个数；大于 1 时多条审批并行判断。
    pub approval_workers: usize,
    pub norma_bin: PathBuf,
    pub project_root: PathBuf,
    pub read_roots: ReadRoots,
    pub state_dir: PathBuf,
    pub tasks_dir: PathBuf,
    pub router: RouterConfig,
    /// 传给适配器进程的环境变量，例如让 `norma` CLI 找到 App 的 socket。
    pub env: Vec<(String, String)>,
    pub prompt_timeout: Duration,
    pub retry_backoff: Duration,
    pub max_turns: u32,
}

impl BrainConfig {
    pub fn new(
        backend: Backend,
        norma_bin: PathBuf,
        project_root: PathBuf,
        state_dir: PathBuf,
    ) -> Self {
        Self {
            backend,
            strategy: Strategy::Auto,
            main_model: None,
            approval_model: None,
            approval_workers: 1,
            norma_bin,
            read_roots: ReadRoots::new(vec![project_root.clone()]),
            project_root,
            tasks_dir: state_dir.clone(),
            state_dir,
            router: RouterConfig::default(),
            env: Vec::new(),
            prompt_timeout: Duration::from_secs(180),
            retry_backoff: Duration::from_secs(1),
            max_turns: 30,
        }
    }
}

mod scopes;
mod staff;
use scopes::Scopes;

enum Command {
    Event(Box<BrainEvent>),
    Recorded(Box<BrainEvent>, crate::staff::TaskEventResult),
    Release(BrainScope),
}

pub struct BrainService {
    events: Sender<Command>,
    scopes: Scopes,
    threads: Vec<JoinHandle<()>>,
    read_roots: ReadRoots,
    tasks: Arc<TaskStore>,
}

impl BrainService {
    pub fn start(config: BrainConfig) -> (Self, Receiver<BrainStatus>) {
        let (events, inbox) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let scopes = Scopes::new(stop.clone());
        let open = Arc::new(Mutex::new(HashSet::new()));
        let goal = Arc::new(GoalStore::open(config.state_dir.clone()));
        let tasks = TaskStore::shared(config.tasks_dir.clone());
        let mut models = backend::models_for(config.backend.kind, config.strategy);
        models.main = config.main_model.clone().or(models.main);
        models.approval = config.approval_model.clone().or(models.approval);
        let gate = Gate::new(
            config.norma_bin.clone(),
            config.project_root.clone(),
            config.read_roots.clone(),
        );
        let lane = |role: Role, model: Option<String>| LaneEnv {
            role,
            backend: config.backend.clone(),
            model,
            effort: backend::effort_for(config.backend.kind, config.strategy, role == Role::Main)
                .map(str::to_string),
            gate: gate.clone(),
            goal: goal.clone(),
            tasks: tasks.clone(),
            status: status_tx.clone(),
            open: open.clone(),
            stop: stop.clone(),
            env: config.env.clone(),
            prompt_timeout: config.prompt_timeout,
            retry_backoff: config.retry_backoff,
            max_turns: config.max_turns,
        };
        let (main_tx, main_rx) = mpsc::channel();
        let (approval_tx, approval_rx) = mpsc::channel();
        let main_env = lane(Role::Main, models.main);
        let main_jobs = Arc::new(Mutex::new(main_rx));
        let mut threads = vec![std::thread::spawn(move || worker::run(main_env, main_jobs))];
        let approval_jobs = Arc::new(Mutex::new(approval_rx));
        for _ in 0..config.approval_workers.max(1) {
            let approval_env = lane(Role::Approval, models.approval.clone());
            let jobs = approval_jobs.clone();
            threads.push(std::thread::spawn(move || worker::run(approval_env, jobs)));
        }
        let dispatcher = Dispatcher {
            scopes: scopes.clone(),
            router: ScopedRouter::new(RouterConfig {
                norma_bin: config.norma_bin.display().to_string(),
                ..config.router
            }),
            goal,
            tasks: tasks.clone(),
            status: status_tx,
            retry_events: Vec::new(),
            retry_at: Instant::now(),
            open,
            main: main_tx,
            approval: approval_tx,
            stop: stop.clone(),
        };
        threads.push(std::thread::spawn(move || dispatcher.run(inbox)));
        (
            Self {
                events,
                scopes,
                threads,
                read_roots: config.read_roots,
                tasks,
            },
            status_rx,
        )
    }

    /// 非阻塞。服务已关闭时事件被丢弃。
    pub fn push(&self, event: BrainEvent) {
        if !self.scopes.token(&event.scope).load(Ordering::Relaxed) {
            let _ = self.events.send(Command::Event(Box::new(event)));
        }
    }

    /// Host has already atomically stored this Worker event; only notification remains.
    pub fn push_recorded(&self, event: BrainEvent, recorded: crate::staff::TaskEventResult) {
        if !self.scopes.token(&event.scope).load(Ordering::Relaxed) {
            let _ = self
                .events
                .send(Command::Recorded(Box::new(event), recorded));
        }
    }

    /// 只获取句柄；磁盘操作须由后台调用，不能从 GPUI render 读取。
    pub fn tasks(&self) -> Arc<TaskStore> {
        self.tasks.clone()
    }

    /// Cancel queued and in-flight work for this exact route without stopping other projects.
    pub fn release_scope(&self, scope: &BrainScope) {
        self.scopes.release(scope);
        let _ = self.events.send(Command::Release(scope.clone()));
    }

    /// Existing lanes observe added/removed projects without restarting the backend.
    pub fn update_read_roots(&self, roots: Vec<PathBuf>) {
        self.read_roots.replace(roots);
    }

    pub fn shutdown(self) {
        self.scopes.stop_all();
        drop(self.events);
        for thread in self.threads {
            let _ = thread.join();
        }
    }
}

struct Dispatcher {
    scopes: Scopes,
    router: ScopedRouter,
    goal: Arc<GoalStore>,
    tasks: Arc<TaskStore>,
    status: Sender<BrainStatus>,
    retry_events: Vec<BrainEvent>,
    retry_at: Instant,
    open: Arc<Mutex<HashSet<(BrainScope, String)>>>,
    main: Sender<Job>,
    approval: Sender<Job>,
    stop: Arc<AtomicBool>,
}

impl Dispatcher {
    fn run(mut self, inbox: Receiver<Command>) {
        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let wait = self
                .router
                .next_deadline()
                .map_or(Duration::from_millis(200), |d| {
                    d.saturating_duration_since(now)
                })
                .min(Duration::from_millis(200));
            match inbox.recv_timeout(wait) {
                Ok(Command::Event(event)) => self.handle(*event, None),
                Ok(Command::Recorded(event, result)) => self.handle(*event, Some(result)),
                Ok(Command::Release(scope)) => {
                    self.router.remove(&scope);
                    self.open
                        .lock()
                        .expect("open lock")
                        .retain(|(source, _)| source != &scope);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let due = self.router.poll(Instant::now());
            self.dispatch(due);
            if Instant::now() >= self.retry_at {
                self.retry_at = Instant::now() + Duration::from_secs(1);
                for event in std::mem::take(&mut self.retry_events) {
                    self.handle(event, None);
                }
            }
        }
    }

    fn handle(&mut self, event: BrainEvent, recorded: Option<crate::staff::TaskEventResult>) {
        let BrainEvent { scope, kind: event } = event;
        if self.scopes.token(&scope).load(Ordering::Relaxed) {
            return;
        }
        if !self.record_task(&scope, &event, recorded) {
            return;
        }
        let approval_id = match &event {
            BrainEventKind::ApprovalRequested(request) => Some(request.id.clone()),
            BrainEventKind::ApprovalClosed { id } => Some(id.clone()),
            _ => None,
        };
        match &event {
            BrainEventKind::Goal { agent, text } => {
                self.goal.record_for(&scope.workspace_id, agent, text);
            }
            BrainEventKind::ProposalDeclined { agent, title } => {
                self.goal.decline_for(&scope.workspace_id, agent, title)
            }
            _ => {}
        }
        let actions = self.router.push(scope.clone(), event, Instant::now());
        if let Some(id) = approval_id {
            let mut open = self.open.lock().expect("open lock");
            if self.router.is_open(&scope, &id) {
                open.insert((scope.clone(), id));
            } else {
                open.remove(&(scope, id));
            }
        }
        self.dispatch(actions);
    }

    fn dispatch(&self, actions: Vec<ScopedAction>) {
        for ScopedAction {
            scope,
            action:
                Action {
                    lane,
                    text,
                    approval_id,
                    trigger,
                },
        } in actions
        {
            let cancelled = self.scopes.token(&scope);
            if cancelled.load(Ordering::Relaxed) {
                continue;
            }
            let tx = if lane == Lane::Approval {
                &self.approval
            } else {
                &self.main
            };
            // 主会话的每条提示都带上当前目标，不只在首条说明里说一次。
            let text = match (
                &lane,
                self.tasks
                    .summary_for(&scope.workspace_id)
                    .ok()
                    .flatten()
                    .or_else(|| self.goal.summary_for(&scope.workspace_id)),
            ) {
                (Lane::Main, Some(goals)) => prompt::with_goals(&text, &goals),
                _ => text,
            };
            let text = match (&lane, self.goal.declined_summary_for(&scope.workspace_id)) {
                (Lane::Main, Some(declined)) => prompt::with_declined(&text, &declined),
                _ => text,
            };
            let _ = tx.send(Job {
                task_id: match lane {
                    Lane::Task(id) => Some(id),
                    _ => None,
                },
                cancelled,
                scope,
                text,
                approval_id,
                trigger,
            });
        }
    }
}
