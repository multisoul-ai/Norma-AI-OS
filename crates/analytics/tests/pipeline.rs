//! 队列、重试退避、离线补发、身份与总开关的行为测试（假 Sink，无网络）。

use std::sync::{Arc, Mutex};

use analytics::*;

#[derive(Clone, Default)]
struct FakeSink {
    outcomes: Arc<Mutex<Vec<SendOutcome>>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl FakeSink {
    fn scripted(outcomes: &[SendOutcome]) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(outcomes.to_vec())),
            ..Self::default()
        }
    }
    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
}

impl Sink for FakeSink {
    fn send(&mut self, batch: &[Queued]) -> SendOutcome {
        let outcome = {
            let mut o = self.outcomes.lock().unwrap();
            if o.is_empty() {
                SendOutcome::Sent
            } else {
                o.remove(0)
            }
        };
        if outcome == SendOutcome::Sent {
            self.sent
                .lock()
                .unwrap()
                .extend(batch.iter().map(|q| q.uuid.clone()));
        }
        outcome
    }
}

fn item(n: usize) -> Queued {
    Queued {
        uuid: format!("u{n}"),
        event: "feature_used".into(),
        distinct_id: "d".into(),
        timestamp_ms: n as u64,
        properties: Default::default(),
    }
}

fn store(dir: &tempfile::TempDir) -> DiskQueue {
    DiskQueue::new(dir.path().join("q.jsonl"))
}

/// 失败退避与离线补发。
///
/// 数据构造：队列 3 条；sink 第 1 次返回 Retry，之后 Sent。首次 tick 在 t=30_000。
/// 退避 = 30_000 << 1 = 60_000，故 t=89_999 仍在退避期，t=90_000 重试成功。
/// 同时验证失败后队列已落盘，新 Pipeline 能读回 3 条（模拟重启）。
///
/// 预期结果：
///   - 正断言：退避期内不发送；到期后 3 条按序送达；落盘文件在失败后可读回
///   - 负断言：退避期内 sent 为空
#[test]
fn retry_backs_off_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let sink = FakeSink::scripted(&[SendOutcome::Retry]);
    let mut p = Pipeline::new(sink.clone(), store(&dir), true);
    (1..=3).for_each(|n| p.push(item(n)));
    p.tick(30_000);
    assert!(sink.sent().is_empty(), "首次发送失败，不应有送达记录");
    assert_eq!(store(&dir).load().len(), 3, "失败后事件应落盘");

    let mut restarted = Pipeline::new(sink.clone(), store(&dir), true);
    assert_eq!(restarted.len(), 3, "重启后应读回 3 条");
    p.tick(89_999);
    assert!(sink.sent().is_empty(), "退避期内不应发送");
    p.tick(90_000);
    assert_eq!(sink.sent(), ["u1", "u2", "u3"], "退避结束后按序送达");
    restarted.set_enabled(false);
}

/// 永久失败（Drop）会移除该批，队列不卡死。
#[test]
fn dropped_batch_does_not_block_queue() {
    let dir = tempfile::tempdir().unwrap();
    let sink = FakeSink::scripted(&[SendOutcome::Drop]);
    let mut p = Pipeline::new(sink.clone(), store(&dir), true);
    p.push(item(1));
    p.tick(30_000);
    assert!(p.is_empty(), "4xx 的批次应被丢弃");
    assert!(sink.sent().is_empty(), "丢弃不代表送达");
    p.push(item(2));
    p.tick(60_000);
    assert_eq!(sink.sent(), ["u2"], "后续事件仍可发送");
}

/// 达到批量阈值立即发送，且一批最多 20 条。
///
/// 数值推导：push 25 条；tick(1) 时 len>=20 触发，只发前 20 条，剩 5 条。
#[test]
fn batch_threshold_sends_twenty() {
    let dir = tempfile::tempdir().unwrap();
    let sink = FakeSink::default();
    let mut p = Pipeline::new(sink.clone(), store(&dir), true);
    (1..=25).for_each(|n| p.push(item(n)));
    p.tick(1);
    assert_eq!(sink.sent().len(), BATCH_SIZE, "一批最多 20 条");
    assert_eq!(p.len(), 5, "剩余 5 条等待下一次");
}

/// 队列封顶：超出 MAX_QUEUED 淘汰最旧。
#[test]
fn queue_evicts_oldest_over_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Pipeline::new(FakeSink::default(), store(&dir), true);
    (0..MAX_QUEUED + 5).for_each(|n| p.push(item(n)));
    assert_eq!(p.len(), MAX_QUEUED, "队列长度封顶 1000");
}

/// 关闭开关：内存与磁盘队列清空，之后 push 无效。
#[test]
fn disabling_clears_memory_and_disk() {
    let dir = tempfile::tempdir().unwrap();
    let sink = FakeSink::scripted(&[SendOutcome::Retry]);
    let mut p = Pipeline::new(sink.clone(), store(&dir), true);
    p.push(item(1));
    p.tick(30_000);
    assert_eq!(store(&dir).load().len(), 1, "前置：失败后已落盘");
    p.set_enabled(false);
    assert!(p.is_empty(), "关闭后内存队列清空");
    assert!(store(&dir).load().is_empty(), "关闭后磁盘队列清空");
    p.push(item(2));
    assert!(p.is_empty(), "关闭期间 push 被忽略");
    assert!(
        Pipeline::new(FakeSink::default(), store(&dir), false).is_empty(),
        "以关闭状态启动不读取旧队列"
    );
}

/// 身份：匿名 ID 持久；identify 产生稳定哈希且不泄露原文；reset 换新匿名 ID。
///
/// 预期结果：
///   - 正断言：二次 load 匿名 ID 相同；identify 后 distinct_id == hash_user_id(raw, salt)（64 位 hex）
///   - 负断言：distinct_id 不含原文；同一原文再次 identify 返回 None；reset 后匿名 ID 改变且不再是已识别用户
#[test]
fn identity_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = Identity::load(dir.path());
    let anon = a.anonymous_id().to_owned();
    assert_eq!(
        Identity::load(dir.path()).anonymous_id(),
        anon,
        "匿名 ID 应持久化"
    );
    assert_eq!(a.distinct_id(), anon, "未识别时 distinct_id 为匿名 ID");

    let hashed = a
        .identify("license-key-123", "salt")
        .expect("首次 identify 应返回新 ID");
    assert_eq!(
        hashed,
        hash_user_id("license-key-123", "salt"),
        "应为盐哈希"
    );
    assert_eq!(hashed.len(), 64, "HMAC-SHA256 hex 为 64 字符");
    assert!(!hashed.contains("license"), "不得包含原文");
    assert_eq!(
        a.identify("license-key-123", "salt"),
        None,
        "重复 identify 不再产生变化"
    );
    assert_eq!(
        Identity::load(dir.path()).distinct_id(),
        hashed,
        "已识别身份应跨启动保留"
    );
    assert_ne!(
        hash_user_id("license-key-123", "other"),
        hashed,
        "盐不同结果不同"
    );

    a.reset();
    assert_ne!(a.anonymous_id(), anon, "reset 后应换新匿名 ID");
    assert_eq!(
        a.distinct_id(),
        a.anonymous_id(),
        "reset 后不再是已识别用户"
    );
}

/// PostHog 请求体结构。
#[test]
fn batch_body_shape() {
    let body = batch_body("phc_x", &[item(1)]);
    assert_eq!(body["api_key"], "phc_x", "api_key");
    assert_eq!(body["batch"][0]["event"], "feature_used", "事件名");
    assert_eq!(body["batch"][0]["uuid"], "u1", "uuid 用于重试去重");
    assert_eq!(
        body["batch"][0]["timestamp"], "1970-01-01T00:00:00.001Z",
        "ISO 时间"
    );
}

/// 端到端句柄：track → 后台线程 → flush → Sink。
///
/// 数据构造：同一功能 feature_used 连续 3 次（去抖应只留 1 条）+ 1 次 BrowserCardCreated。
/// 随后关闭开关：应额外送出 analytics_opted_out，之后 track 不再入队。
#[test]
fn handle_end_to_end_debounce_and_opt_out() {
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<String>>>);
    impl Sink for Capture {
        fn send(&mut self, batch: &[Queued]) -> SendOutcome {
            self.0
                .lock()
                .unwrap()
                .extend(batch.iter().map(|q| q.event.clone()));
            SendOutcome::Sent
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let capture = Capture::default();
    let config = Config {
        enabled: true,
        host: DEFAULT_HOST.into(),
        project_key: "phc_test".into(),
        data_dir: dir.path().to_path_buf(),
        id_salt: "s".into(),
        app_version: "0.0.0".into(),
        os_version: "test".into(),
    };
    let a = Analytics::start_with_sink(config, capture.clone());
    for _ in 0..3 {
        a.track(Event::FeatureUsed {
            feature: Feature::Browser,
            success: true,
        });
    }
    a.track(Event::BrowserCardCreated);
    a.flush_blocking(std::time::Duration::from_secs(5));
    assert_eq!(
        *capture.0.lock().unwrap(),
        ["feature_used", "browser_card_created"],
        "3 次重复只留 1 条 feature_used"
    );

    a.set_enabled(false);
    a.flush_blocking(std::time::Duration::from_secs(5));
    a.track(Event::IdeaCreated);
    a.flush_blocking(std::time::Duration::from_secs(5));
    let sent = capture.0.lock().unwrap().clone();
    assert_eq!(
        sent.last().map(String::as_str),
        Some("analytics_opted_out"),
        "关闭时应送出退出事件"
    );
    assert!(
        !sent.contains(&"idea_created".to_owned()),
        "关闭后 track 不得入队"
    );
    assert!(
        opt_out_marker(dir.path()).exists(),
        "关闭后应写入退出标记，下次启动保持关闭"
    );
    assert!(
        !Config::from_build_env(dir.path().to_path_buf(), "t".into()).enabled,
        "有退出标记时重启应保持关闭"
    );
    a.set_enabled(true);
    a.flush_blocking(std::time::Duration::from_secs(5));
    assert!(
        !opt_out_marker(dir.path()).exists(),
        "重新开启应移除退出标记"
    );
}

/// PostHogSink 真实 HTTP：本地假服务验证路径、请求体与状态映射。
///
/// 数据构造：127.0.0.1 随机端口起一个只应答一次的 TcpListener，返回 `status`。
/// 预期结果：请求行为 `POST /batch/`，正文含 api_key 与事件 uuid；200→Sent，503→Retry，401→Drop；
/// 连接被拒（端口已关）→ Retry。
#[test]
fn posthog_sink_http_roundtrip() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve_once(status: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_owned();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            let mut total = 0;
            // 读到请求头结束且正文齐全（Content-Length）为止。
            loop {
                let n = stream.read(&mut buf[total..]).unwrap();
                total += n;
                let text = String::from_utf8_lossy(&buf[..total]).to_string();
                if let Some(idx) = text.find("\r\n\r\n") {
                    let len = text
                        .to_lowercase()
                        .split("content-length:")
                        .nth(1)
                        .and_then(|s| {
                            s.trim_start()
                                .split("\r\n")
                                .next()?
                                .trim()
                                .parse::<usize>()
                                .ok()
                        })
                        .unwrap_or(0);
                    if total >= idx + 4 + len || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let reply =
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&buf[..total]).to_string()
        });
        (host, handle)
    }

    for (status, expect) in [
        ("200 OK", SendOutcome::Sent),
        ("503 Service Unavailable", SendOutcome::Retry),
        ("401 Unauthorized", SendOutcome::Drop),
    ] {
        let (host, handle) = serve_once(status);
        let mut sink = PostHogSink::new(&host, "phc_key");
        assert_eq!(sink.send(&[item(7)]), expect, "状态 {status} 的映射");
        let request = handle.join().unwrap();
        assert!(
            request.starts_with("POST /batch/ "),
            "请求行应为 POST /batch/，实际：{}",
            request.lines().next().unwrap_or("")
        );
        assert!(
            request.contains("phc_key") && request.contains("\"u7\""),
            "正文应含 api_key 与事件 uuid"
        );
    }

    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    assert_eq!(
        PostHogSink::new(&closed, "k").send(&[item(1)]),
        SendOutcome::Retry,
        "连接失败应重试"
    );
}
