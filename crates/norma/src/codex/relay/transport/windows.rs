//! WinSock readiness for AF_UNIX, with the same bounded cancellation and backpressure budget.
use super::super::Socket;
use std::os::windows::io::{AsRawSocket, BorrowedSocket, OwnedSocket};
use windows_sys::Win32::Networking::WinSock::{
    WSAGetLastError, WSAPoll, POLLERR, POLLHUP, POLLNVAL, POLLRDNORM, POLLWRNORM, WSAEINTR,
    WSAPOLLFD,
};

fn wait_ready(
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

// Subscribe to close events; Windows AF_UNIX may defer them until unread data is drained.
use std::{
    ffi::c_void,
    ptr::null_mut,
    time::{Duration, Instant},
};
use windows_sys::Win32::Networking::WinSock::{
    SOCK_NOTIFY_EVENT_ERR, SOCK_NOTIFY_EVENT_HANGUP, SOCK_NOTIFY_EVENT_REMOVE,
    SOCK_NOTIFY_OP_ENABLE, SOCK_NOTIFY_OP_REMOVE, SOCK_NOTIFY_REGISTER_EVENT_HANGUP,
    SOCK_NOTIFY_REGISTRATION, SOCK_NOTIFY_TRIGGER_LEVEL, SOCK_NOTIFY_TRIGGER_PERSISTENT,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::IO::{CreateIoCompletionPort, OVERLAPPED_ENTRY},
};

type NotifySockets = unsafe extern "system" fn(
    HANDLE,
    u32,
    *mut SOCK_NOTIFY_REGISTRATION,
    u32,
    u32,
    *mut OVERLAPPED_ENTRY,
    *mut u32,
) -> u32;

fn notification_api() -> Option<NotifySockets> {
    static API: std::sync::OnceLock<Option<NotifySockets>> = std::sync::OnceLock::new();
    *API.get_or_init(|| {
        // WSAPoll is a static import, so ws2_32 remains loaded for the process lifetime.
        let module = unsafe { GetModuleHandleA(c"ws2_32.dll".as_ptr().cast()) };
        if module.is_null() {
            return None;
        }
        let symbol =
            unsafe { GetProcAddress(module, c"ProcessSocketNotifications".as_ptr().cast()) }?;
        // The SDK's documented WINAPI signature matches NotifySockets exactly.
        Some(unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, NotifySockets>(symbol)
        })
    })
}

pub(crate) struct Readiness {
    notify: Option<NotifySockets>,
    port: HANDLE,
    sockets: [Option<OwnedSocket>; 2],
    registered: [bool; 2],
    closed: [bool; 2],
    last_probe: [Instant; 2],
}

impl Readiness {
    pub(crate) fn new(native: &Socket, server: &Socket) -> Result<Self, String> {
        let this = Self::with_api(native, server, notification_api())?;
        #[cfg(test)]
        eprintln!(
            "auto WinSock readiness: notifications={}, registered={:?}",
            this.notify.is_some(),
            this.registered
        );
        Ok(this)
    }

    #[cfg(test)]
    pub(crate) fn without_socket_notifications(
        native: &Socket,
        server: &Socket,
    ) -> Result<Self, String> {
        let this = Self::with_api(native, server, None)?;
        assert!(this.notify.is_none(), "测试必须真实选择缺API路径");
        assert!(this.port.is_null(), "缺API路径不能创建IOCP");
        assert!(
            this.sockets.iter().all(Option::is_none),
            "缺API路径不能持有额外socket句柄"
        );
        Ok(this)
    }

    fn with_api(
        native: &Socket,
        server: &Socket,
        notify: Option<NotifySockets>,
    ) -> Result<Self, String> {
        let mut this = Self {
            notify,
            port: null_mut(),
            sockets: [None, None],
            registered: [false; 2],
            closed: [false; 2],
            last_probe: [Instant::now(); 2],
        };
        // Older systems retain bounded WSAPoll and the existing guarded Ping probes.
        if this.notify.is_none() {
            return Ok(this);
        }
        let clone = |socket: &Socket| {
            unsafe { BorrowedSocket::borrow_raw(socket.get_ref().as_raw_socket()) }
                .try_clone_to_owned()
                .map(Some)
                .map_err(|error| error.to_string())
        };
        this.sockets = [clone(native)?, clone(server)?];
        this.port = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 1) };
        if this.port.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        this.register(SOCK_NOTIFY_OP_ENABLE)?;
        Ok(this)
    }

    fn register(&mut self, operation: u32) -> Result<(), String> {
        let Some(notify) = self.notify else {
            return Ok(());
        };
        let indices: Vec<_> = (0..2)
            .filter(|i| operation == SOCK_NOTIFY_OP_ENABLE || self.registered[*i])
            .collect();
        let mut registrations: Vec<_> = indices
            .iter()
            .map(|i| SOCK_NOTIFY_REGISTRATION {
                socket: self.sockets[*i]
                    .as_ref()
                    .expect("registered socket remains owned")
                    .as_raw_socket() as _,
                completionKey: (i + 1) as *mut c_void,
                eventFilter: SOCK_NOTIFY_REGISTER_EVENT_HANGUP as u16,
                operation: operation as u8,
                triggerFlags: (SOCK_NOTIFY_TRIGGER_LEVEL | SOCK_NOTIFY_TRIGGER_PERSISTENT) as u8,
                registrationResult: 0,
            })
            .collect();
        let code = unsafe {
            notify(
                self.port,
                registrations.len() as u32,
                registrations.as_mut_ptr(),
                0,
                0,
                null_mut(),
                null_mut(),
            )
        };
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code as i32).to_string());
        }
        let mut error = None;
        for (i, registration) in indices.into_iter().zip(registrations) {
            if registration.registrationResult != 0 {
                error.get_or_insert_with(|| {
                    std::io::Error::from_raw_os_error(registration.registrationResult as i32)
                        .to_string()
                });
            } else if operation == SOCK_NOTIFY_OP_ENABLE {
                self.registered[i] = true;
            }
        }
        error.map_or(Ok(()), Err)
    }

    fn notifications(&mut self, timeout: u32) -> Result<(), String> {
        let Some(notify) = self.notify else {
            return Ok(());
        };
        let mut entries = [OVERLAPPED_ENTRY::default(); 8];
        let mut count = 0;
        let code = unsafe {
            notify(
                self.port,
                0,
                null_mut(),
                timeout,
                entries.len() as u32,
                entries.as_mut_ptr(),
                &mut count,
            )
        };
        if code == 258 || code == 1460 {
            return Ok(());
        }
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code as i32).to_string());
        }
        for entry in entries.iter().take(count as usize) {
            let Some(i) = entry.lpCompletionKey.checked_sub(1).filter(|i| *i < 2) else {
                return Err("invalid socket notification identity".into());
            };
            let events = entry.dwNumberOfBytesTransferred;
            if events & SOCK_NOTIFY_EVENT_REMOVE != 0 {
                self.registered[i] = false;
            }
            if events & SOCK_NOTIFY_EVENT_ERR != 0 {
                return Err("codex_native_connection_closed".into());
            }
            if events & SOCK_NOTIFY_EVENT_HANGUP != 0 {
                self.closed[i] = true;
            }
        }
        Ok(())
    }

    pub(crate) fn wait_ready(
        &mut self,
        native: &Socket,
        server: &Socket,
        native_ready: bool,
        server_ready: bool,
        read_server: bool,
    ) -> Result<(), String> {
        self.notifications(0)?;
        wait_ready(native, server, native_ready, server_ready, read_server)?;
        self.notifications(0)?;
        if self
            .closed
            .iter()
            .zip([server_ready, native_ready && read_server])
            .any(|(closed, reading)| *closed && !reading)
        {
            return Err("codex_native_connection_closed".into());
        }
        Ok(())
    }

    pub(crate) fn probe_backpressure(
        &mut self,
        native: &mut Socket,
        server: &mut Socket,
        native_ready: &mut bool,
        server_ready: &mut bool,
        answer_pending: bool,
    ) -> Result<(), String> {
        // AF_UNIX defers HUP behind unread queued sends. A legal control frame exercises
        // the reverse write path without consuming data or growing a read-ahead queue.
        // Never flush an in-flight answer outside its cancellation/ticket lock.
        let blocked = [!*server_ready, !*native_ready && !answer_pending];
        for (index, socket, writable) in [(0, native, native_ready), (1, server, server_ready)] {
            if blocked[index]
                && *writable
                && self.last_probe[index].elapsed() >= Duration::from_millis(250)
            {
                *writable = super::send(socket, tungstenite::Message::Ping(Vec::new().into()))?;
                self.last_probe[index] = Instant::now();
            }
        }
        Ok(())
    }
}

impl Drop for Readiness {
    fn drop(&mut self) {
        if self.registered.iter().any(|registered| *registered) {
            if let Err(error) = self.register(SOCK_NOTIFY_OP_REMOVE) {
                tracing::warn!(%error, "socket notification removal failed");
            }
            let deadline = Instant::now() + Duration::from_millis(100);
            while self.registered.iter().any(|registered| *registered) && Instant::now() < deadline
            {
                if self.notifications(5).is_err() {
                    break;
                }
            }
        }
        // The registrations own duplicated handles. Closing them deregisters even on REMOVE error.
        // Completion keys are integer tags, so queued notifications never refer to freed memory.
        self.sockets.iter_mut().for_each(|socket| {
            drop(socket.take());
        });
        if !self.port.is_null() {
            unsafe {
                CloseHandle(self.port);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/windows_readiness_compat.rs"]
mod tests;
