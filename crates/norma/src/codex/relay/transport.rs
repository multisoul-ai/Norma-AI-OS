//! Bounded socket IO and readiness waits, independent of question observation.
use super::Socket;
use std::os::fd::AsRawFd;
use tungstenite::{Error, Message};

pub(super) fn wait_ready(
    native: &Socket,
    server: &Socket,
    native_ready: bool,
    server_ready: bool,
    read_server: bool,
) -> Result<(), String> {
    // Darwin ignores descriptors whose event mask is zero, including their hangup.
    // Keep HUP enabled while backpressure intentionally suspends data reads.
    let mut descriptors = [
        libc::pollfd {
            fd: native.get_ref().as_raw_fd(),
            events: libc::POLLHUP
                | (if server_ready { libc::POLLIN } else { 0 })
                | (if !native_ready { libc::POLLOUT } else { 0 }),
            revents: 0,
        },
        libc::pollfd {
            fd: server.get_ref().as_raw_fd(),
            events: libc::POLLHUP
                | (if native_ready && read_server {
                    libc::POLLIN
                } else {
                    0
                })
                | (if !server_ready { libc::POLLOUT } else { 0 }),
            revents: 0,
        },
    ];
    // Both sockets outlive this call, and poll receives the exact mutable array length.
    // The 5 ms bound also services cancellation and queued answers without a socket wakeup.
    let result = unsafe {
        libc::poll(
            descriptors.as_mut_ptr(),
            descriptors.len() as libc::nfds_t,
            5,
        )
    };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.to_string());
        }
    }
    for (descriptor, reading) in descriptors
        .iter()
        .zip([server_ready, native_ready && read_server])
    {
        if descriptor.revents & (libc::POLLERR | libc::POLLNVAL) != 0
            || (!reading && descriptor.revents & libc::POLLHUP != 0)
        {
            return Err("codex_native_connection_closed".into());
        }
    }
    Ok(())
}

pub(super) enum ReadFrame {
    Data(Message),
    Control,
    Pending,
}

pub(super) fn receive(socket: &mut Socket) -> Result<ReadFrame, String> {
    match socket.read() {
        Ok(Message::Ping(_) | Message::Pong(_)) => Ok(ReadFrame::Control),
        Ok(Message::Close(_)) => Err("codex_native_connection_closed".into()),
        Ok(frame) => Ok(ReadFrame::Data(frame)),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Ok(ReadFrame::Pending)
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn send(socket: &mut Socket, frame: Message) -> Result<bool, String> {
    writable(socket.send(frame))
}

pub(super) fn flush(socket: &mut Socket) -> Result<bool, String> {
    writable(socket.flush())
}

fn writable(result: Result<(), Error>) -> Result<bool, String> {
    match result {
        Ok(()) => Ok(true),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}
