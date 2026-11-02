//! Observe the production WinSock notification registration without consuming stream data.
use std::io;
use std::os::windows::io::{
    AsRawHandle, AsRawSocket, BorrowedSocket, FromRawHandle, OwnedHandle, OwnedSocket,
};
use std::{
    ptr,
    time::{Duration, Instant},
};
use uds_windows::UnixStream;
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Networking::WinSock::{
        ProcessSocketNotifications, SOCK_NOTIFY_EVENT_REMOVE, SOCK_NOTIFY_OP_ENABLE,
        SOCK_NOTIFY_OP_REMOVE, SOCK_NOTIFY_REGISTER_EVENT_HANGUP, SOCK_NOTIFY_REGISTRATION,
        SOCK_NOTIFY_TRIGGER_LEVEL, SOCK_NOTIFY_TRIGGER_PERSISTENT,
    },
    System::IO::{CreateIoCompletionPort, OVERLAPPED_ENTRY},
};

pub struct CloseNotifications {
    // Closing the registered socket precedes the port, even if explicit deregistration fails.
    socket: OwnedSocket,
    port: OwnedHandle,
    registered: bool,
}

impl CloseNotifications {
    pub fn new(stream: &UnixStream) -> io::Result<Self> {
        let socket =
            unsafe { BorrowedSocket::borrow_raw(stream.as_raw_socket()) }.try_clone_to_owned()?;
        let port = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, ptr::null_mut(), 0, 1) };
        if port.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut this = Self {
            socket,
            port: unsafe { OwnedHandle::from_raw_handle(port) },
            registered: false,
        };
        this.configure(SOCK_NOTIFY_OP_ENABLE)?;
        this.registered = true;
        Ok(this)
    }

    fn configure(&self, operation: u32) -> io::Result<()> {
        let mut registration = SOCK_NOTIFY_REGISTRATION {
            socket: self.socket.as_raw_socket() as _,
            completionKey: ptr::without_provenance_mut(1),
            eventFilter: SOCK_NOTIFY_REGISTER_EVENT_HANGUP as _,
            operation: operation as _,
            triggerFlags: (SOCK_NOTIFY_TRIGGER_LEVEL | SOCK_NOTIFY_TRIGGER_PERSISTENT) as _,
            registrationResult: 0,
        };
        let code = unsafe {
            ProcessSocketNotifications(
                self.port.as_raw_handle(),
                1,
                &mut registration,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        eprintln!("socket notification configuration: operation={operation}, return={code}, registration={}", registration.registrationResult);
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as _));
        }
        if registration.registrationResult != 0 {
            return Err(io::Error::from_raw_os_error(
                registration.registrationResult as _,
            ));
        }
        Ok(())
    }

    pub fn poll(&self) -> io::Result<u32> {
        let mut entries = [OVERLAPPED_ENTRY::default(); 8];
        let mut count = 0;
        let code = unsafe {
            ProcessSocketNotifications(
                self.port.as_raw_handle(),
                0,
                ptr::null_mut(),
                0,
                entries.len() as _,
                entries.as_mut_ptr(),
                &mut count,
            )
        };
        if code == 258 || code == 1460 {
            return Ok(0);
        }
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as _));
        }
        let mut events = 0;
        for entry in entries.iter().take(count as usize) {
            if entry.lpCompletionKey != 1 {
                return Err(io::Error::other(
                    "Unexpected private IOCP completion identity",
                ));
            }
            events |= entry.dwNumberOfBytesTransferred;
        }
        Ok(events)
    }
}

impl Drop for CloseNotifications {
    fn drop(&mut self) {
        if !self.registered {
            return;
        }
        if let Err(error) = self.configure(SOCK_NOTIFY_OP_REMOVE) {
            eprintln!("socket notification removal failed: {error}");
            return;
        }
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            match self.poll() {
                Ok(events) if events & SOCK_NOTIFY_EVENT_REMOVE != 0 => return,
                Ok(_) => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    eprintln!("socket notification removal completion failed: {error}");
                    return;
                }
            }
        }
        // Queued completion keys are integer tags, never pointers into this fixture.
        eprintln!("socket notification removal completion timed out; owned socket will deregister on close");
    }
}
