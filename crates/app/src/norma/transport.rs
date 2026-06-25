use crossbeam_channel::{unbounded, Sender};
use norma::client::{decode_request_line, encode_response_line};
use norma::protocol::{CardCommandError, WireRequest, WireResponse};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct NormaServerRequest {
    pub(crate) request: WireRequest,
    pub(crate) response: Sender<WireResponse>,
}

pub(crate) fn bind_socket(path: PathBuf, sender: Sender<NormaServerRequest>) -> Option<PathBuf> {
    let _ = fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to bind Norma socket"
            );
            return None;
        }
    };

    let thread_path = path.clone();
    thread::Builder::new()
        .name("norma-server".to_string())
        .spawn(move || accept_loop(listener, sender))
        .map_err(|error| {
            tracing::warn!(
                path = %thread_path.display(),
                %error,
                "failed to spawn Norma server thread"
            );
        })
        .ok()?;

    Some(path)
}

fn accept_loop(listener: UnixListener, sender: Sender<NormaServerRequest>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let sender = sender.clone();
                let _ = thread::Builder::new()
                    .name("norma-client".to_string())
                    .spawn(move || handle_stream(stream, sender));
            }
            Err(error) => tracing::warn!(%error, "Norma socket accept failed"),
        }
    }
}

fn handle_stream(mut stream: UnixStream, sender: Sender<NormaServerRequest>) {
    let mut line = String::new();
    let read_result = BufReader::new(&stream).read_line(&mut line);
    let response = match read_result {
        Ok(0) => response_for_error(0, "empty Norma request"),
        Ok(_) => match decode_request_line(&line) {
            Ok(request) => dispatch_request(request, sender),
            Err(error) => response_for_error(0, &format!("invalid Norma request: {error}")),
        },
        Err(error) => response_for_error(0, &format!("could not read Norma request: {error}")),
    };

    if let Ok(line) = encode_response_line(&response) {
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
    }
}

fn dispatch_request(request: WireRequest, sender: Sender<NormaServerRequest>) -> WireResponse {
    let request_id = request.request_id;
    let (response_sender, response_receiver) = unbounded();
    if sender
        .send(NormaServerRequest {
            request,
            response: response_sender,
        })
        .is_err()
    {
        return response_for_error(request_id, "Norma app receiver is closed");
    }

    response_receiver
        .recv_timeout(RESPONSE_TIMEOUT)
        .unwrap_or_else(|_| response_for_error(request_id, "Norma request timed out"))
}

fn response_for_error(request_id: u64, message: &str) -> WireResponse {
    WireResponse {
        request_id,
        result: Err(CardCommandError::Runtime {
            message: message.to_string(),
        }),
    }
}
