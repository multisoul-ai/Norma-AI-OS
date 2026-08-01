use std::io;
use std::mem::{size_of, MaybeUninit};
use std::time::Duration;

use super::ProcessCounters;

pub(super) fn read_process_counters(
    observed_at: Duration,
    repaint_count: u64,
    render_count: u64,
) -> io::Result<ProcessCounters> {
    let pid = std::process::id() as libc::c_int;
    let mut task_info = MaybeUninit::<libc::proc_taskinfo>::uninit();
    // SAFETY: `task_info` points to writable storage of exactly the size passed to
    // libproc. A successful call initializes the entire `proc_taskinfo` value.
    let task_info_size = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            task_info.as_mut_ptr().cast(),
            size_of::<libc::proc_taskinfo>() as libc::c_int,
        )
    };
    if task_info_size != size_of::<libc::proc_taskinfo>() as libc::c_int {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the size check above confirms libproc filled a complete value.
    let task_info = unsafe { task_info.assume_init() };

    let mut resource_usage = MaybeUninit::<libc::rusage_info_v2>::uninit();
    // SAFETY: `resource_usage` is valid writable storage for RUSAGE_INFO_V2. A
    // zero return value means libproc initialized the full structure.
    let usage_result = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V2,
            resource_usage.as_mut_ptr().cast(),
        )
    };
    if usage_result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful return above guarantees initialization.
    let resource_usage = unsafe { resource_usage.assume_init() };

    Ok(ProcessCounters {
        observed_at,
        cpu_time_ns: task_info
            .pti_total_user
            .saturating_add(task_info.pti_total_system),
        phys_footprint_bytes: resource_usage.ri_phys_footprint,
        wakeup_count: resource_usage
            .ri_pkg_idle_wkups
            .saturating_add(resource_usage.ri_interrupt_wkups),
        thread_count: task_info.pti_threadnum.max(0) as u32,
        repaint_count,
        render_count,
    })
}
