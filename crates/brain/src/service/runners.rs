//! 幕僚任务会话：每个任务一个线程；Claude 最多 4 个会话，满了就排队。
use crate::event::{BrainStatus, BrainStatusKind, TaskUpdate};
use crate::lane::staff_pool::{StaffPool, Submit};
use crate::lane::worker::Tick;
use crate::lane::worker::{self, Idle, Job, LaneEnv, SharedJobs};
use norma::protocol::BrainScope;
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

type Key = (BrainScope, String);
type Table = Arc<Mutex<HashMap<Key, Sender<Job>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone)]
pub(super) struct TaskRunners {
    table: Table,
    make_env: Arc<dyn Fn() -> LaneEnv + Send + Sync>,
    threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
    idle: Duration,
    pool: Option<Arc<Mutex<StaffPool>>>,
}

impl TaskRunners {
    pub(super) fn new(
        make_env: Arc<dyn Fn() -> LaneEnv + Send + Sync>,
        idle: Duration,
        pool: Option<Arc<Mutex<StaffPool>>>,
    ) -> Self {
        Self {
            table: Table::default(),
            make_env,
            threads: Arc::default(),
            idle,
            pool,
        }
    }

    /// 交给该任务自己的会话；它还没有会话（或已空闲回收）就新起一个。
    /// Claude 已有 4 个忙会话时先排队，面板显示「排队中」。
    ///
    /// 加锁顺序固定为 table → pool。投递（`send`）和占位（`pool.submit`）在同一段临界区里完成，
    /// 空闲线程检查队列、让出位置也在同一对锁下进行，所以不会出现「刚塞进队列就被驱逐」。
    pub(super) fn submit(&self, job: Job) {
        let Some(task) = job.task_id.clone() else {
            return;
        };
        let key = (job.scope.clone(), task);
        let mut table = lock(&self.table);
        let mut pool = self.pool.as_ref().map(|pool| lock(pool));
        let outcome = match pool.as_mut() {
            Some(pool) => pool.submit(job),
            None => Submit::Run {
                job: Box::new(job),
                evicted: None,
            },
        };
        let job = match outcome {
            Submit::Wait => {
                drop(pool);
                drop(table);
                let env = (self.make_env)();
                let _ = env.status.send(BrainStatus {
                    scope: key.0,
                    kind: BrainStatusKind::TaskActivity {
                        task_id: key.1,
                        turn: 0,
                        update: TaskUpdate::SessionWait,
                    },
                });
                return;
            }
            Submit::Run { job, evicted } => {
                if let Some(evicted) = evicted {
                    table.remove(&evicted);
                }
                *job
            }
        };
        let job = match table.get(&key) {
            Some(tx) => match tx.send(job) {
                Ok(()) => return,
                Err(mpsc::SendError(job)) => job,
            },
            None => job,
        };
        self.spawn(&mut table, key, job);
    }

    fn spawn(&self, table: &mut HashMap<Key, Sender<Job>>, key: Key, job: Job) {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(job);
        table.insert(key.clone(), tx);
        let jobs: SharedJobs = Arc::new(Mutex::new(rx));
        let idle_after = self.idle;
        let retire = {
            let registry = self.table.clone();
            let queue = jobs.clone();
            let pool = self.pool.clone();
            let runners = self.clone();
            let key = key.clone();
            Box::new(move || {
                let mut table = lock(&registry);
                let mut pool = pool.as_ref().map(|pool| lock(pool));
                if let Ok(job) = lock(&queue).try_recv() {
                    return Some(job);
                }
                table.remove(&key);
                let next = pool.as_mut().and_then(|pool| pool.close(&key.1));
                drop(pool);
                drop(table);
                if let Some(next) = next {
                    runners.submit(next);
                }
                None
            })
        };
        let poll = self.pool.as_ref().map(|pool| {
            let pool = pool.clone();
            let runners = self.clone();
            let registry = self.table.clone();
            let queue = jobs.clone();
            let key = key.clone();
            Box::new(move |closed: bool| -> Tick {
                let mut table = lock(&registry);
                let mut pool = lock(&pool);
                if let Ok(job) = lock(&queue).try_recv() {
                    pool.mark_busy(&key.1);
                    return Tick::Job(job);
                }
                if closed || !pool.holds(&key.0, &key.1) {
                    table.remove(&key);
                    let next = pool.close(&key.1);
                    drop(pool);
                    drop(table);
                    if let Some(next) = next {
                        runners.submit(next);
                    }
                    return Tick::Exit;
                }
                let Some((evicted, next)) = pool.on_idle(&key.1) else {
                    return Tick::Stay;
                };
                table.remove(&evicted);
                drop(pool);
                drop(table);
                runners.submit(next);
                if evicted == key {
                    Tick::Exit
                } else {
                    Tick::Stay
                }
            }) as Box<dyn Fn(bool) -> Tick + Send>
        });
        let idle = Idle {
            after: idle_after,
            retire,
            poll,
        };
        let env = (self.make_env)();
        let handle = std::thread::spawn(move || worker::run(env, jobs, Some(idle)));
        let mut threads = lock(&self.threads);
        threads.retain(|thread| !thread.is_finished());
        threads.push(handle);
    }

    /// 路由被释放：丢掉它所有任务的会话入口和池里的名额，线程处理完手头的就退出。
    pub(super) fn release(&self, scope: &BrainScope) {
        let mut table = lock(&self.table);
        table.retain(|(owner, _), _| owner != scope);
        let mut pool = self.pool.as_ref().map(|pool| lock(pool));
        let next = pool
            .as_mut()
            .map(|pool| pool.release_scope(scope))
            .unwrap_or_default();
        drop(pool);
        drop(table);
        for job in next {
            self.submit(job);
        }
    }

    /// 关闭服务时等所有会话线程退出（它们看到 stop 标志就会退）。
    pub(super) fn join_all(&self) {
        let handles = std::mem::take(&mut *lock(&self.threads));
        for handle in handles {
            let _ = handle.join();
        }
    }
}
