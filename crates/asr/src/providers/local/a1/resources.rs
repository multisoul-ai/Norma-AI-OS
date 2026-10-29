use super::host::A1EngineHost;
use std::sync::Weak;

pub(super) fn normal_pressure() -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut level: i32 = 0;
        let mut size = std::mem::size_of_val(&level);
        // Kernel-defined NORMAL=1; WARN/CRITICAL cause release, not a guessed RSS budget.
        let result = unsafe {
            libc::sysctlbyname(
                c"kern.memorystatus_vm_pressure_level".as_ptr(),
                (&mut level as *mut i32).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        result == 0 && level == 1
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub(super) fn watch(host: Weak<A1EngineHost>) {
    let _ = std::thread::Builder::new()
        .name("a1_memory_pressure".into())
        .spawn(move || {
            while let Some(host) = host.upgrade() {
                if !normal_pressure() {
                    host.shutdown();
                }
                drop(host);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
}
