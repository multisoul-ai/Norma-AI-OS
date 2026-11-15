//! 幕僚任务会话：每个任务一个线程、一个后端进程；同一任务的 job 排队串行，不同任务并行。
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
}

impl TaskRunners {
    pub(super) fn new(make_env: Arc<dyn Fn() -> LaneEnv + Send + Sync>, idle: Duration) -> Self {
        Self {
            table: Table::default(),
            make_env,
            threads: Arc::default(),
            idle,
        }
    }

    /// 交给该任务自己的会话；它还没有会话（或已空闲回收）就新起一个。
    pub(super) fn submit(&self, job: Job) {
        let Some(task) = job.task_id.clone() else {
            return;
        };
        let key = (job.scope.clone(), task);
        let mut table = lock(&self.table);
        let job = match table.get(&key) {
            Some(tx) => match tx.send(job) {
                Ok(()) => return,
                // 会话线程已经退出：换一个新的。
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
        let (registry, queue) = (self.table.clone(), jobs.clone());
        let idle = Idle {
            after: self.idle,
            // 注销和「最后看一眼队列」在同一把锁里，新 job 不会丢给一个正在退出的会话。
            retire: Box::new(move || {
                let mut table = lock(&registry);
                match lock(&queue).try_recv() {
                    Ok(job) => Some(job),
                    Err(_) => {
                        table.remove(&key);
                        None
                    }
                }
            }),
        };
        let env = (self.make_env)();
        let handle = std::thread::spawn(move || worker::run(env, jobs, Some(idle)));
        let mut threads = lock(&self.threads);
        threads.retain(|thread| !thread.is_finished());
        threads.push(handle);
    }

    /// 路由被释放：丢掉它所有任务的会话入口，线程处理完手头的就退出。
    pub(super) fn release(&self, scope: &BrainScope) {
        lock(&self.table).retain(|(owner, _), _| owner != scope);
    }

    /// 关闭服务时等所有会话线程退出（它们看到 stop 标志就会退）。
    pub(super) fn join_all(&self) {
        let handles = std::mem::take(&mut *lock(&self.threads));
        for handle in handles {
            let _ = handle.join();
        }
    }
}
