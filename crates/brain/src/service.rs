//! 对外入口：`push` 事件进来，从 `Receiver<BrainStatus>` 读状态。

use crate::acp::gate::Gate;
use crate::backend::{self, Backend, Strategy};
use crate::event::{BrainEvent, BrainStatus};
use crate::goal::GoalStore;
use crate::lane::prompt::{self, Role};
use crate::lane::worker::{self, Job, LaneEnv};
use crate::router::{Action, Lane, Router, RouterConfig};
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
    pub state_dir: PathBuf,
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
            project_root,
            state_dir,
            router: RouterConfig::default(),
            env: Vec::new(),
            prompt_timeout: Duration::from_secs(180),
            retry_backoff: Duration::from_secs(1),
            max_turns: 30,
        }
    }
}

pub struct BrainService {
    events: Sender<BrainEvent>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl BrainService {
    pub fn start(config: BrainConfig) -> (Self, Receiver<BrainStatus>) {
        let (events, inbox) = mpsc::channel();
        let (status_tx, status_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let open = Arc::new(Mutex::new(HashSet::new()));
        let goal = Arc::new(GoalStore::open(config.state_dir.clone()));
        let mut models = backend::models_for(config.backend.kind, config.strategy);
        if config.backend.kind == backend::BackendKind::ClaudeCode {
            models.main = config.main_model.clone().or(models.main);
            models.approval = config.approval_model.clone().or(models.approval);
        }
        let gate = Gate {
            norma_bin: config.norma_bin.clone(),
            project_root: config.project_root.clone(),
        };
        let lane = |role: Role, model: Option<String>| LaneEnv {
            role,
            backend: config.backend.clone(),
            model,
            gate: gate.clone(),
            goal: goal.clone(),
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
            router: Router::new(
                RouterConfig {
                    norma_bin: config.norma_bin.display().to_string(),
                    ..config.router
                },
                Instant::now(),
            ),
            goal,
            open,
            main: main_tx,
            approval: approval_tx,
            stop: stop.clone(),
        };
        threads.push(std::thread::spawn(move || dispatcher.run(inbox)));
        (
            Self {
                events,
                stop,
                threads,
            },
            status_rx,
        )
    }

    /// 非阻塞。服务已关闭时事件被丢弃。
    pub fn push(&self, event: BrainEvent) {
        let _ = self.events.send(event);
    }

    pub fn shutdown(self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.events);
        for thread in self.threads {
            let _ = thread.join();
        }
    }
}

struct Dispatcher {
    router: Router,
    goal: Arc<GoalStore>,
    open: Arc<Mutex<HashSet<String>>>,
    main: Sender<Job>,
    approval: Sender<Job>,
    stop: Arc<AtomicBool>,
}

impl Dispatcher {
    fn run(mut self, inbox: Receiver<BrainEvent>) {
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
                Ok(event) => self.handle(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let due = self.router.poll(Instant::now());
            self.dispatch(due);
        }
    }

    fn handle(&mut self, event: BrainEvent) {
        let approval_id = match &event {
            BrainEvent::ApprovalRequested(request) => Some(request.id.clone()),
            BrainEvent::ApprovalClosed { id } => Some(id.clone()),
            _ => None,
        };
        if let BrainEvent::Goal { agent, text } = &event {
            self.goal.record(agent, text);
        }
        let actions = self.router.push(event, Instant::now());
        if let Some(id) = approval_id {
            let mut open = self.open.lock().expect("open lock");
            if self.router.is_open(&id) {
                open.insert(id);
            } else {
                open.remove(&id);
            }
        }
        self.dispatch(actions);
    }

    fn dispatch(&self, actions: Vec<Action>) {
        for Action {
            lane,
            text,
            approval_id,
        } in actions
        {
            let tx = if lane == Lane::Main {
                &self.main
            } else {
                &self.approval
            };
            // 主会话的每条提示都带上当前目标，不只在首条说明里说一次。
            let text = match (lane, self.goal.summary()) {
                (Lane::Main, Some(goals)) => prompt::with_goals(&text, &goals),
                _ => text,
            };
            let _ = tx.send(Job { text, approval_id });
        }
    }
}
