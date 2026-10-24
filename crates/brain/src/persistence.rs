//! 幕僚状态的原子替换与有序后台写入。

use std::{
    io::{self, Write},
    path::Path,
};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        OnceLock,
    },
};

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

struct Job {
    path: Option<PathBuf>,
    text: String,
    append: bool,
    done: Sender<Result<(), String>>,
}

/// 同一拥有者的写入按提交顺序完成；文件 IO 和等待均只发生在后台。
#[derive(Clone, Debug)]
pub struct Writer {
    jobs: Sender<Job>,
}

impl Default for Writer {
    fn default() -> Self {
        let (jobs, receiver) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            for job in receiver {
                let result = match job.path.as_deref() {
                    None => Ok(()),
                    Some(path) if job.append => append_line(path, &job.text),
                    Some(path) => atomic_write(path, job.text.as_bytes()),
                }
                .map_err(|error| error.to_string());
                if let Err(error) = &result {
                    tracing::warn!(%error, path = ?job.path, "保存幕僚状态失败");
                }
                let _ = job.done.send(result);
            }
        });
        Self { jobs }
    }
}

impl Writer {
    pub fn save(&self, path: PathBuf, text: String) -> Receiver<Result<(), String>> {
        self.enqueue(Some(path), text, false)
    }

    pub fn append(&self, path: PathBuf, text: String) -> Receiver<Result<(), String>> {
        self.enqueue(Some(path), text, true)
    }

    /// 后台读盘前等待队列屏障，保证重开的拥有者看见之前提交的全部快照。
    pub fn flush(&self) -> Receiver<Result<(), String>> {
        self.enqueue(None, String::new(), false)
    }

    fn enqueue(
        &self,
        path: Option<PathBuf>,
        text: String,
        append: bool,
    ) -> Receiver<Result<(), String>> {
        let (done, receiver) = mpsc::channel();
        let _ = self.jobs.send(Job {
            path,
            text,
            append,
            done,
        });
        receiver
    }
}

pub fn shared_writer() -> &'static Writer {
    static WRITER: OnceLock<Writer> = OnceLock::new();
    WRITER.get_or_init(Writer::default)
}

fn append_line(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", text.replace('\n', " "))
}

#[cfg(test)]
#[path = "../tests/unit/persistence_tests.rs"]
mod persistence_tests;
