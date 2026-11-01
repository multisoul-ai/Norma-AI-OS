//! WinSock readiness for AF_UNIX, with the same bounded cancellation and backpressure budget.
use super::super::Socket;
use std::os::windows::io::AsRawSocket;
use windows_sys::Win32::Networking::WinSock::{
    WSAGetLastError, WSAPoll, POLLERR, POLLHUP, POLLNVAL, POLLRDNORM, POLLWRNORM, WSAEINTR,
    WSAPOLLFD,
};

pub(crate) fn wait_ready(
    native: &Socket,
    server: &Socket,
    native_ready: bool,
    server_ready: bool,
    read_server: bool,
) -> Result<(), String> {
    let reading = [server_ready, native_ready && read_server];
    let mut descriptors = [
        WSAPOLLFD {
            fd: native.get_ref().as_raw_socket() as _,
            events: (if reading[0] { POLLRDNORM } else { 0 })
                | (if !native_ready { POLLWRNORM } else { 0 }),
            revents: 0,
        },
        WSAPOLLFD {
            fd: server.get_ref().as_raw_socket() as _,
            events: (if reading[1] { POLLRDNORM } else { 0 })
                | (if !server_ready { POLLWRNORM } else { 0 }),
            revents: 0,
        },
    ];
    // HUP/ERR are output-only flags on Windows, including when backpressure suspends reads.
    // Both live sockets outlast the call; 5ms also bounds cancellation and queued-answer service.
    if unsafe { WSAPoll(descriptors.as_mut_ptr(), descriptors.len() as u32, 5) } < 0 {
        let error = unsafe { WSAGetLastError() };
        if error != WSAEINTR {
            return Err(std::io::Error::from_raw_os_error(error).to_string());
        }
    }
    for (descriptor, reading) in descriptors.iter().zip(reading) {
        if descriptor.revents & (POLLERR | POLLNVAL) != 0
            || (!reading && descriptor.revents & POLLHUP != 0)
        {
            return Err("codex_native_connection_closed".into());
        }
    }
    Ok(())
}
