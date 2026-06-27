//! macOS-only: read a process's current working directory via proc_pidinfo.
//! The non-macOS stub always returns None — this project is macOS-only today,
//! but the stub keeps CI on other platforms compilable.
//!
//! TODO(Phase 6): verify proc_pidinfo works under App Sandbox before notarization.

#[cfg(target_os = "macos")]
pub(crate) fn process_cwd(pid: u32) -> Option<String> {
    use std::ffi::CStr;

    // proc_vnodepathinfo layout (verified via sizeof/offsetof on macOS 14):
    //   pvi_cdir: vnode_info_path {
    //       vip_vi:   vnode_info    = 152 bytes
    //       vip_path: [u8; 1024]
    //   }  → 1176 bytes total
    //   pvi_rdir: vnode_info_path  = 1176 bytes (not used)
    // Total: 2352 bytes
    // vip_path offset within pvi_cdir = 152 (offsetof verified)
    const PROC_PIDVNODEPATHINFO: libc::c_int = 9;
    const VNODE_INFO_SIZE: usize = 152;
    const MAXPATHLEN: usize = 1024;
    const BUF_SIZE: usize = (VNODE_INFO_SIZE + MAXPATHLEN) * 2;

    let mut buf = [0u8; BUF_SIZE];
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::pid_t,
            PROC_PIDVNODEPATHINFO,
            0,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            BUF_SIZE as libc::c_int,
        )
    };

    // proc_pidinfo returns the number of bytes written; a short or negative
    // result means permission denied, no such process, or wrong buffer size.
    if ret as usize != BUF_SIZE {
        return None;
    }

    // cwd path immediately follows the 152-byte vnode_info header of pvi_cdir
    let path_bytes = &buf[VNODE_INFO_SIZE..VNODE_INFO_SIZE + MAXPATHLEN];
    // SAFETY: path_bytes is NUL-terminated (the buffer was zero-initialized and
    // proc_pidinfo writes a NUL-terminated C string into vip_path).
    let cstr = unsafe { CStr::from_ptr(path_bytes.as_ptr().cast::<libc::c_char>()) };
    let s = cstr.to_string_lossy();
    if s.is_empty() {
        None
    } else {
        Some(s.into_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn process_cwd(_pid: u32) -> Option<String> {
    None
}

#[cfg(test)]
#[path = "../tests/unit/macos_proc_tests.rs"]
mod tests;
