use super::ProcessControl;
use std::{
    io::{self, Read, Write},
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Command, Stdio},
    ptr::null,
    sync::{
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    thread::JoinHandle,
};
use windows_sys::Win32::{
    Foundation::{RtlNtStatusToDosError, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
    },
};

#[link(name = "ntdll")]
extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

type WriteRequest = (Vec<u8>, SyncSender<io::Result<usize>>);
pub(super) struct Pipe {
    output: Option<Receiver<io::Result<Vec<u8>>>>,
    input: Option<SyncSender<WriteRequest>>,
    pending_write: Option<Receiver<io::Result<usize>>>,
    buffered: Vec<u8>,
    offset: usize,
    threads: Vec<JoinHandle<()>>,
}
fn would_block() -> io::Error {
    io::Error::from(io::ErrorKind::WouldBlock)
}

pub(super) fn spawn(mut command: Command) -> io::Result<(ProcessControl, Pipe)> {
    // Suspend before assignment: no worker or descendant can escape the owned kill-on-close job.
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
        .spawn()?;
    let mut child = child;
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        let error = io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let job = unsafe { OwnedHandle::from_raw_handle(job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let initialized = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        ) != 0
            && AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) != 0
    };
    if !initialized {
        let error = io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let control = ProcessControl {
        child: Arc::new(Mutex::new(child)),
        job: Arc::new(job),
    };
    let pipe = match Pipe::new(&control) {
        Ok(pipe) => pipe,
        Err(error) => {
            control.terminate();
            return Err(error);
        }
    };
    let code = unsafe {
        NtResumeProcess(
            control
                .child
                .lock()
                .map_err(|_| io::Error::other("worker lock poisoned"))?
                .as_raw_handle(),
        )
    };
    if code < 0 {
        control.terminate();
        drop(pipe);
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(code) } as i32,
        ));
    }
    Ok((control, pipe))
}
impl Pipe {
    fn new(control: &ProcessControl) -> io::Result<Self> {
        let (mut stdin, mut stdout) = {
            let mut child = control
                .child
                .lock()
                .map_err(|_| io::Error::other("worker lock poisoned"))?;
            (
                child
                    .stdin
                    .take()
                    .ok_or_else(|| io::Error::other("missing worker stdin"))?,
                child
                    .stdout
                    .take()
                    .ok_or_else(|| io::Error::other("missing worker stdout"))?,
            )
        };
        let (send_write, receive_write) = mpsc::sync_channel::<WriteRequest>(1);
        let writer = std::thread::Builder::new()
            .name("a1_worker_input".into())
            .spawn(move || {
                while let Ok((bytes, reply)) = receive_write.recv() {
                    let result = stdin.write_all(&bytes).map(|_| bytes.len());
                    let failed = result.is_err();
                    let _ = reply.send(result);
                    if failed {
                        break;
                    }
                }
            })?;
        let (send_read, receive_read) = mpsc::sync_channel(2);
        let reader = match std::thread::Builder::new()
            .name("a1_worker_output".into())
            .spawn(move || loop {
                let mut bytes = vec![0; 4096];
                let result = stdout.read(&mut bytes).map(|length| {
                    bytes.truncate(length);
                    bytes
                });
                let terminal = result.as_ref().map_or(true, |bytes| bytes.is_empty());
                if send_read.send(result).is_err() || terminal {
                    break;
                }
            }) {
            Ok(reader) => reader,
            Err(error) => {
                drop(send_write);
                control.terminate();
                let _ = writer.join();
                return Err(error);
            }
        };
        Ok(Self {
            output: Some(receive_read),
            input: Some(send_write),
            pending_write: None,
            buffered: vec![],
            offset: 0,
            threads: vec![reader, writer],
        })
    }
}
impl Read for Pipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.offset == self.buffered.len() {
            self.buffered = match self
                .output
                .as_ref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?
                .try_recv()
            {
                Ok(value) => value?,
                Err(TryRecvError::Empty) => return Err(would_block()),
                Err(TryRecvError::Disconnected) => return Ok(0),
            };
            self.offset = 0;
        }
        let count = buffer.len().min(self.buffered.len() - self.offset);
        buffer[..count].copy_from_slice(&self.buffered[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.pending_write.is_none() {
            let (sender, receiver) = mpsc::sync_channel(1);
            self.input
                .as_ref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?
                .send((bytes.to_vec(), sender))
                .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
            self.pending_write = Some(receiver);
        }
        match self
            .pending_write
            .as_ref()
            .expect("pending write installed")
            .try_recv()
        {
            Ok(result) => {
                self.pending_write = None;
                result
            }
            Err(TryRecvError::Empty) => Err(would_block()),
            Err(TryRecvError::Disconnected) => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.pending_write.is_some() {
            Err(would_block())
        } else {
            Ok(())
        }
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        // Transport kills/waits the owned job before dropping its pipe endpoints and joining IO.
        drop(self.output.take());
        drop(self.input.take());
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
