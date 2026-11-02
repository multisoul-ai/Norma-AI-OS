//! A private, non-inheritable Job owns one PTY client and all of its descendants.
use filedescriptor::OwnedHandle;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::{mem, ptr, thread, time::Duration};
use winapi::um::jobapi2::{
    AssignProcessToJobObject, CreateJobObjectW, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
use winapi::um::winnt::{
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[derive(Debug)]
pub(super) struct Job(OwnedHandle);

impl Job {
    pub(super) fn new() -> io::Result<Self> {
        // Null security attributes make this handle non-inheritable; there is no shared job name.
        let raw = unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(unsafe { OwnedHandle::from_raw_handle(raw as _) });
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        // No BREAKAWAY flags: CREATE_NO_WINDOW/DETACHED_PROCESS cannot escape this ownership.
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle() as _,
                JobObjectExtendedLimitInformation,
                &mut limits as *mut _ as _,
                mem::size_of_val(&limits) as _,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub(super) fn assign(&self, process: &OwnedHandle) -> io::Result<()> {
        if unsafe {
            AssignProcessToJobObject(self.0.as_raw_handle() as _, process.as_raw_handle() as _)
        } == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.0.as_raw_handle() as _, 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn is_empty(&self) -> io::Result<bool> {
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle() as _,
                JobObjectBasicAccountingInformation,
                &mut accounting as *mut _ as _,
                mem::size_of_val(&accounting) as _,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(accounting.ActiveProcesses == 0)
    }

    // Only child.wait/Future's waiter thread uses this; kill/try_wait remain nonblocking.
    pub(super) fn finish(&self) -> io::Result<()> {
        self.terminate()?;
        while !self.is_empty()? {
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
}
