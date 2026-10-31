//! 跑测生命周期证据；失败/取消不能留下 running 状态。
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct PressureWatch {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<io::Result<()>>>,
}
impl PressureWatch {
    pub(super) fn start(path: &Path) -> io::Result<Self> {
        Self::with_probe(path, Duration::from_millis(100), system_pressure)
    }
    fn with_probe(
        path: &Path,
        interval: Duration,
        probe: impl Fn() -> Option<i32> + Send + 'static,
    ) -> io::Result<Self> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let start = Instant::now();
        let initial = probe();
        serde_json::to_writer(&mut file, &json!({"level":initial,"elapsed_ms":0}))?;
        writeln!(file)?;
        file.flush()?;
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let thread = thread::Builder::new().name("voice_eval_pressure".into()).spawn(move || {
            let mut previous = initial;
            while !done.load(Ordering::Relaxed) {
                thread::sleep(interval);
                let next = probe();
                if next != previous {
                    serde_json::to_writer(&mut file, &json!({"level":next,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0}))?;
                    writeln!(file)?;
                    file.flush()?;
                    previous = next;
                }
            }
            file.flush()
        })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
    pub(super) fn stop(mut self) -> io::Result<()> {
        self.join()
    }
    fn join(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            return thread
                .join()
                .map_err(|_| io::Error::other("pressure observer panicked"))?;
        }
        Ok(())
    }
}
impl Drop for PressureWatch {
    fn drop(&mut self) {
        if let Err(error) = self.join() {
            eprintln!("voice-eval: pressure logger failed: {error}");
        }
    }
}

fn system_pressure() -> Option<i32> {
    let mut level = 0_i32;
    let mut size = std::mem::size_of_val(&level);
    let status = unsafe {
        libc::sysctlbyname(
            c"kern.memorystatus_vm_pressure_level".as_ptr(),
            (&mut level as *mut i32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0).then_some(level)
}

pub(super) fn start_manifest(path: &Path, value: &Value) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)
}

pub(super) struct RunGuard {
    path: PathBuf,
    complete: bool,
}

impl RunGuard {
    pub(super) fn start(path: &Path, value: &Value) -> io::Result<Self> {
        start_manifest(path, value)?;
        Ok(Self {
            path: path.to_owned(),
            complete: false,
        })
    }

    pub(super) fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        let result = (|| -> io::Result<()> {
            let mut value: Value = serde_json::from_slice(&fs::read(&self.path)?)?;
            value["status"] = "failed-incomplete".into();
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_millis();
            value["ended_at_unix_ms"] = u64::try_from(millis).map_err(io::Error::other)?.into();
            fs::write(&self.path, serde_json::to_vec_pretty(&value)?)
        })();
        if let Err(error) = result {
            eprintln!(
                "voice-eval: cannot mark failed manifest {}: {error}",
                self.path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 场景：模拟识别会话取消导致提前退出，原 manifest 为 running。
    /// 构造独立结果目录，启动 guard 后不调用 complete；执行析构。
    /// 预期：状态为 failed-incomplete，配置哈希保持不变，不能伪装 complete。
    #[test]
    fn early_exit_marks_the_run_failed_instead_of_running() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a0-manifest.json");
        let guard =
            RunGuard::start(&path, &json!({"status":"running","config_sha256":"abc"})).unwrap();
        drop(guard);
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            value["status"], "failed-incomplete",
            "取消后不能残留 running"
        );
        assert_eq!(value["config_sha256"], "abc", "失败也要保留原配置身份");
        assert_ne!(value["status"], "complete", "缺失结果不能当完整评测");
    }

    /// 场景：识别跑测正常完成；构造完整状态并显式提交完成。
    /// 执行析构；预期保持 complete，不被失败保护误改。
    #[test]
    fn completed_run_keeps_its_terminal_status() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("a0-manifest.json");
        let mut guard = RunGuard::start(&path, &json!({"status":"complete"})).unwrap();
        guard.complete();
        drop(guard);
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["status"], "complete", "成功提交后必须保留完成状态");
    }

    /// 场景：内存压力瞬时由 NORMAL(1) 到 WARN(2)，观测器不改变系统状态。
    /// 构造原子探针和 1ms 采样，仅测试工具记录通道；执行状态变化后停止。
    /// 预期：本地日志保留 1→2，变化有单调时间，无重复采样刷盘。
    #[test]
    fn pressure_changes_are_recorded_without_controlling_the_host() {
        use std::sync::atomic::AtomicI32;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resource-events.jsonl");
        let level = Arc::new(AtomicI32::new(1));
        let probe = level.clone();
        let watch = PressureWatch::with_probe(&path, Duration::from_millis(1), move || {
            Some(probe.load(Ordering::Relaxed))
        })
        .unwrap();
        level.store(2, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_millis(200);
        while !fs::read_to_string(&path).unwrap().contains("\"level\":2")
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        watch.stop().unwrap();
        let rows: Vec<Value> = fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(
            rows.iter().map(|r| r["level"].as_i64()).collect::<Vec<_>>(),
            vec![Some(1), Some(2)],
            "应只写状态变化"
        );
        assert!(
            rows[1]["elapsed_ms"].as_f64().unwrap() >= 0.0,
            "应记录同一单调时钟的时间"
        );
    }
}
