//! 按行分隔的 JSON-RPC over stdio。

use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

#[derive(Debug)]
pub enum Incoming {
    Response {
        id: u64,
        result: Result<Value, Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

pub struct Connection {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Incoming>,
    next_id: u64,
    _adapter: Option<super::codex_compat::Adapter>,
}

impl Connection {
    pub fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
        kind: crate::backend::BackendKind,
    ) -> io::Result<Self> {
        let adapter = (kind == crate::backend::BackendKind::Codex)
            .then(|| super::codex_compat::Adapter::prepare(program, args))
            .transpose()?;
        let (program, args) = adapter.as_ref().map_or((program, args), |adapter| {
            (adapter.program.as_path(), adapter.args.as_slice())
        });
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(adapter_stderr());
        super::claude_auth::configure(&mut command, kind)?;
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Some(message) = parse(&line) else {
                    continue;
                };
                if tx.send(message).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            next_id: 0,
            _adapter: adapter,
        })
    }

    fn write(&mut self, value: &Value) -> io::Result<()> {
        let mut line = value.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()
    }

    pub fn send_request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        Ok(id)
    }

    pub fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub fn respond(&mut self, id: &Value, result: Value) -> io::Result<()> {
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    pub fn respond_error(&mut self, id: &Value, code: i64, message: &str) -> io::Result<()> {
        self.write(
            &json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        )
    }

    pub fn recv(&self, timeout: Duration) -> Result<Incoming, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.kill();
    }
}

/// 排查用：设置 `NORMA_BRAIN_ADAPTER_LOG` 后，适配器的 stderr 追加写入该文件，否则丢弃。
fn adapter_stderr() -> Stdio {
    std::env::var_os("NORMA_BRAIN_ADAPTER_LOG")
        .and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        })
        .map_or_else(Stdio::null, Stdio::from)
}

fn parse(line: &str) -> Option<Incoming> {
    let value: Value = serde_json::from_str(line).ok()?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    let id = value.get("id").cloned();
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    match (method, id) {
        (Some(method), Some(id)) => Some(Incoming::Request { id, method, params }),
        (Some(method), None) => Some(Incoming::Notification { method, params }),
        (None, Some(id)) => {
            let result = match value.get("error") {
                Some(error) => Err(error.clone()),
                None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
            };
            Some(Incoming::Response {
                id: id.as_u64()?,
                result,
            })
        }
        (None, None) => None,
    }
}
