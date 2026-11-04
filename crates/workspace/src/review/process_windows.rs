use super::ReviewError;
use std::{
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Child, Command},
    ptr::null,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
    },
};

#[link(name = "ntdll")]
extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

pub(super) struct Job(OwnedHandle);
impl Job {
    pub(super) fn terminate(&self) {
        unsafe {
            TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

pub(super) fn spawn(command: &mut Command) -> Result<(Child, Job), ReviewError> {
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return Err(ReviewError::Unavailable);
    }
    let job = Job(unsafe { OwnedHandle::from_raw_handle(handle) });
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.0.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(ReviewError::Unavailable);
    }
    // No process executes before assignment, so even a launcher that exits immediately cannot escape.
    let mut child = command
        .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| ReviewError::Unavailable)?;
    let assigned =
        unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), child.as_raw_handle()) } != 0;
    if !assigned || unsafe { NtResumeProcess(child.as_raw_handle()) } < 0 {
        job.terminate();
        let _ = child.kill();
        let _ = child.wait();
        return Err(ReviewError::Unavailable);
    }
    Ok((child, job))
}
