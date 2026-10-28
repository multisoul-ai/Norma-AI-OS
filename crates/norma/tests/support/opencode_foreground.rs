use norma::opencode::native_questions::{OpenCodeNativeEvent, OpenCodeQuestionHost};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub fn route(path: &std::path::Path, revision: u64, session: Option<&str>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let pending = path.with_extension("pending");
    std::fs::write(
        &pending,
        json!({"nonce":"owned", "revision":revision,"session":session,"updatedAt":now}).to_string(),
    )
    .unwrap();
    std::fs::rename(pending, path).unwrap();
}
pub struct Server {
    pub endpoint: String,
    stop: Arc<AtomicBool>,
    pub posts: Arc<AtomicUsize>,
    pub offline: Arc<AtomicBool>,
    pub consumed: Arc<AtomicBool>,
    pub hide_a: Arc<AtomicBool>,
    pub pending_gate: Arc<AtomicUsize>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn new(drop_ack: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let posts = Arc::new(AtomicUsize::new(0));
        let offline = Arc::new(AtomicBool::new(false));
        let worker_offline = offline.clone();
        let consumed = Arc::new(AtomicBool::new(false));
        let worker_consumed = consumed.clone();
        let hide_a = Arc::new(AtomicBool::new(false));
        let worker_hide_a = hide_a.clone();
        let pending_gate = Arc::new(AtomicUsize::new(0));
        let worker_gate = pending_gate.clone();
        let (done, count) = (stop.clone(), posts.clone());
        let worker = std::thread::spawn(move || {
            'connections: while !done.load(Ordering::Acquire) {
                let mut socket = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                // HTTP reconnects can open then abandon a socket before writing a request.
                if reader.read_line(&mut first).is_err() || first.is_empty() {
                    continue;
                }
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        continue 'connections;
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(length < 8192, "请求体应有界");
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if first.starts_with("GET /question?")
                    && worker_gate
                        .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    while worker_gate.load(Ordering::Acquire) == 2 && !done.load(Ordering::Acquire)
                    {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                if first.starts_with("GET /question?") && worker_offline.load(Ordering::Acquire) {
                    let _ = write!(socket,"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let body = if first.starts_with("GET /question?") {
                    let mut a: Value = serde_json::from_str(include_str!(
                        "../fixtures/questions/opencode-request.json"
                    ))
                    .unwrap();
                    a["sessionID"] = "ses_a".into();
                    a["id"] = "que_a".into();
                    let mut b = a.clone();
                    b["sessionID"] = "ses_b".into();
                    b["id"] = "que_b".into();
                    if worker_consumed.load(Ordering::Acquire)
                        || worker_hide_a.load(Ordering::Acquire)
                    {
                        json!([b])
                    } else {
                        json!([a, b])
                    }
                } else if first.starts_with("POST /question/") {
                    count.fetch_add(1, Ordering::AcqRel);
                    if drop_ack {
                        continue;
                    }
                    json!(true)
                } else if first.starts_with("GET /session/ses_a/")
                    && worker_consumed.load(Ordering::Acquire)
                {
                    let mut part: Value = serde_json::from_str(include_str!(
                        "../fixtures/questions/opencode-consumed.json"
                    ))
                    .unwrap();
                    part["sessionID"] = "ses_a".into();
                    json!({"parts":[part]})
                } else {
                    json!({"parts":[]})
                }
                .to_string();
                let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len());
            }
        });
        Self {
            endpoint,
            offline,
            consumed,
            hide_a,
            pending_gate,
            stop,
            posts,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
pub fn question(host: &OpenCodeQuestionHost) -> norma::agent_hook::HookQuestion {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if let Ok(OpenCodeNativeEvent::Question(q)) =
            host.events().recv_timeout(Duration::from_millis(100))
        {
            return q;
        }
    }
    panic!("当前前台会话必须发现原题");
}
