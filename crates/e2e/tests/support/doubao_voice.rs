use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

/// Loopback peer speaks the companion gateway contract; no external model or secret is used.
pub struct Fixture {
    pub origin: String,
    pub stats: Arc<Mutex<Value>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Fixture {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("isolated voice listener");
        let origin = format!("http://{}", listener.local_addr().expect("voice address"));
        listener.set_nonblocking(true).expect("nonblocking fixture");
        let stopped = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(
            json!({"sessions":0,"closed":0,"active":0,"frames":0,
            "invalid_frames":0,"interrupts":0,"played_ms":0,"playback_done":0,"mute":0,"unmute":0,"overlap":0}),
        ));
        let (stop, report, address) = (stopped.clone(), stats.clone(), origin.clone());
        let worker = thread::spawn(move || {
            let mut peers = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (stop, report, address) =
                            (stop.clone(), report.clone(), address.clone());
                        peers.push(thread::spawn(move || {
                            serve(stream, &address, &report, &stop)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        // 10ms polling is below one 20ms capture frame, solely for fixture shutdown.
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
            for peer in peers {
                peer.join().expect("voice peer must not panic");
            }
        });
        Self {
            origin,
            stats,
            stopped,
            worker: Some(worker),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("voice fixture shutdown");
        }
    }
}

fn increment(stats: &Mutex<Value>, key: &str) {
    let mut stats = stats.lock().expect("voice stats");
    stats[key] = json!(stats[key].as_u64().unwrap_or(0) + 1);
}

fn serve(mut stream: TcpStream, origin: &str, stats: &Mutex<Value>, stopped: &AtomicBool) {
    // 100ms reads bound teardown even when capture is muted; 2s writes bound a stalled simulator.
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("read deadline");
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("write deadline");
    let mut peek = [0; 16 * 1024];
    let header = loop {
        if stopped.load(Ordering::Relaxed) {
            return;
        }
        match stream.peek(&mut peek) {
            Ok(0) => return,
            Ok(n) => {
                let text = String::from_utf8_lossy(&peek[..n]);
                if text.contains("\r\n\r\n") {
                    break text.into_owned();
                }
                if n == peek.len() {
                    return;
                }
                thread::sleep(Duration::from_millis(1));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(_) => return,
        }
    };
    if header.starts_with("GET /v1/voice ") {
        let mut ws =
            match tungstenite::accept_hdr(stream, |request: &Request, response: Response| {
                if request
                    .headers()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    == Some("Bearer local-one-use-ticket")
                {
                    Ok(response)
                } else {
                    let mut denied = tungstenite::handshake::server::ErrorResponse::new(None);
                    *denied.status_mut() = tungstenite::http::StatusCode::UNAUTHORIZED;
                    Err(denied)
                }
            }) {
                Ok(ws) => ws,
                Err(_) => return,
            };
        increment(stats, "sessions");
        increment(stats, "active");
        let _ = ws.send(Message::Text(
            json!({"type":"session.created"}).to_string().into(),
        ));
        let mut replied = false;
        while !stopped.load(Ordering::Relaxed) {
            let text = match ws.read() {
                Ok(Message::Text(text)) => text,
                Ok(Message::Close(_)) => break,
                Ok(_) => continue,
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(_) => break,
            };
            let event: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            match event["type"].as_str().unwrap_or("") {
                "input_audio_buffer.append" => {
                    increment(stats, "frames");
                    // 640 PCM bytes become 4 × ceil(640/3) = 856 base64 bytes.
                    if event["audio"].as_str().map(str::len) != Some(856) {
                        increment(stats, "invalid_frames");
                    }
                    if !replied
                        && stats.lock().expect("stats")["frames"].as_u64().unwrap_or(0) >= 10
                    {
                        replied = true;
                        for event in reply("first", "本地豆包回复", 96_000) {
                            if ws.send(Message::Text(event.to_string().into())).is_err() {
                                break;
                            }
                        }
                    }
                }
                "gateway.interrupt" => {
                    increment(stats, "interrupts");
                    stats.lock().expect("stats")["played_ms"] = event["played_ms"].clone();
                    let late = json!({"type":"response.output_text.delta","response_id":"first","delta":"迟到旧字幕"});
                    let _ = ws.send(Message::Text(late.to_string().into()));
                    for event in reply("second", "插话后的新回复", 4_800) {
                        let _ = ws.send(Message::Text(event.to_string().into()));
                    }
                }
                "gateway.playback.done" => increment(stats, "playback_done"),
                "input_audio_mute.commit" => increment(stats, "mute"),
                "input_audio_unmute.commit" => increment(stats, "unmute"),
                "session.close" => {
                    // Delay ACK by five capture frames to expose overlapping replacement connections.
                    thread::sleep(Duration::from_millis(100));
                    stats.lock().expect("stats")["active"] = json!(0);
                    let _ = ws.send(Message::Text(
                        json!({"type":"session.closed"}).to_string().into(),
                    ));
                    break;
                }
                _ => {}
            }
        }
        stats.lock().expect("stats")["active"] = json!(0);
        increment(stats, "closed");
        let _ = ws.close(None);
        return;
    }
    let end = header.find("\r\n\r\n").expect("complete header") + 4;
    let length = header[..end]
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    if length > 4096 {
        return;
    }
    let mut request = vec![0; end + length];
    if stream.read_exact(&mut request).is_err() {
        return;
    }
    let (status, body) = if header.starts_with("GET /stats ") {
        (200, stats.lock().expect("stats").clone())
    } else if header.starts_with("POST /v1/session ") {
        let body: Value = serde_json::from_slice(&request[end..]).unwrap_or(Value::Null);
        if !header
            .to_ascii_lowercase()
            .contains("authorization: bearer local-doubao-test-passphrase-24chars")
            || body["provider"] != "doubao"
        {
            (401, json!({"error":"invalid_request"}))
        } else if stats.lock().expect("stats")["active"] != 0 {
            increment(stats, "overlap");
            (429, json!({"error":"session_active"}))
        } else {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_secs();
            (
                200,
                json!({"provider":"doubao","transport":"websocket","token":"local-one-use-ticket",
                "expiresAt":now + 30,"url":origin.replace("http:","ws:") + "/v1/voice","inputRate":16000,"outputRate":24000}),
            )
        }
    } else {
        (404, Value::Null)
    };
    let body = body.to_string();
    let _ = write!(stream, "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

fn reply(id: &str, text: &str, bytes: usize) -> Vec<Value> {
    vec![
        json!({"type":"conversation.item.input_audio_transcription.started"}),
        json!({"type":"conversation.item.input_audio_transcription.completed","transcript":"本机输入"}),
        json!({"type":"response.output_text.delta","response_id":id,"delta":text}),
        json!({"type":"response.output_audio.started","response_id":id}),
        json!({"type":"response.output_audio.delta","response_id":id,"delta":"AAAA".repeat(bytes / 3)}),
        json!({"type":"response.output_audio.done","response_id":id}),
    ]
}
