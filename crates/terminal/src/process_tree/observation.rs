use super::{ManagedProcess, ManagedProcessIdentity};

/// Bounded read-only observation of new direct children of an existing PTY process.
pub struct ChildProcessObserver {
    root: ManagedProcessIdentity,
    launcher: Option<ManagedProcessIdentity>,
    baseline: Option<Vec<ManagedProcessIdentity>>,
}

impl ChildProcessObserver {
    pub(super) fn from_managed(process: ManagedProcess) -> Option<Self> {
        Some(Self {
            root: process.identity?,
            launcher: None,
            baseline: None,
        })
    }
    /// Call on a worker before sending the bound input.
    pub fn prepare(&mut self) -> bool {
        self.baseline = self.children();
        self.baseline.is_some()
    }
    /// Follow one uniquely identified native child of a CLI launcher, keeping both births bound.
    /// No matching child keeps the original root; ambiguity disables this observation.
    pub fn prepare_with_child(&mut self, matches: impl Fn(&[String]) -> bool) -> bool {
        if self.baseline.is_some() || self.launcher.is_some() {
            return false;
        }
        let Some(children) = self.children() else {
            return false;
        };
        let mut candidates = children.into_iter().filter(|id| {
            process_argv(id.pid).is_some_and(|argv| self.current_child(*id) && matches(&argv))
        });
        let child = candidates.next();
        if candidates.next().is_some() {
            return false;
        }
        if let Some(child) = child {
            self.launcher = Some(self.root);
            self.root = child;
        }
        self.prepare()
    }
    /// Reads argv only; never returns the process environment.
    pub fn matches_new_child(&self, matches: impl Fn(&[String]) -> bool) -> bool {
        let Some(baseline) = &self.baseline else {
            return false;
        };
        let Some(children) = self.children() else {
            return false;
        };
        children
            .into_iter()
            .filter(|id| !baseline.contains(id))
            .any(|id| {
                let Some(argv) = process_argv(id.pid) else {
                    return false;
                };
                // Recheck both births and the direct-parent edge after reading argv: PIDs can be reused.
                self.current_child(id) && matches(&argv)
            })
    }

    fn current_child(&self, child: ManagedProcessIdentity) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.current_root()
                && super::macos::process_info(child.pid) == Some((child, self.root.pid))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (self.root, child);
            false
        }
    }

    #[cfg(target_os = "macos")]
    fn current_root(&self) -> bool {
        super::macos::process_identity(self.root.pid) == Some(self.root)
            && self.launcher.is_none_or(|parent| {
                super::macos::process_identity(parent.pid) == Some(parent)
                    && super::macos::process_info(self.root.pid) == Some((self.root, parent.pid))
            })
    }

    fn children(&self) -> Option<Vec<ManagedProcessIdentity>> {
        #[cfg(target_os = "macos")]
        {
            use super::macos::{checked_child_pids, process_info};
            if !self.current_root() {
                return None;
            }
            let pids = checked_child_pids(i32::try_from(self.root.pid).ok()?)?;
            // ponytail: observe at most 64 direct children. Larger/unknown trees use transcript evidence.
            if pids.len() > 64 {
                return None;
            }
            let children: Option<Vec<_>> = pids
                .into_iter()
                .map(|pid| {
                    let (id, parent) = process_info(u32::try_from(pid).ok()?)?;
                    (parent == self.root.pid).then_some(id)
                })
                .collect();
            if !self.current_root() {
                return None;
            }
            children
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn process_argv(pid: u32) -> Option<Vec<String>> {
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROCARGS2,
        i32::try_from(pid).ok()?,
    ];
    // macOS ARG_MAX is bounded here at 1MiB, including the kernel buffer's environment section.
    // Parse only argc arguments, then immediately drop the raw buffer; never log it.
    let mut bytes = vec![0u8; 1024 * 1024];
    let mut len = bytes.len();
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || len > bytes.len() {
        return None;
    }
    parse_procargs(&bytes[..len])
}

#[cfg(not(target_os = "macos"))]
fn process_argv(_pid: u32) -> Option<Vec<String>> {
    None
}

#[cfg(target_os = "macos")]
pub(super) fn parse_procargs(bytes: &[u8]) -> Option<Vec<String>> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if !(1..=4096).contains(&argc) {
        return None;
    }
    let bytes = bytes.get(4..)?;
    let executable_end = bytes.iter().position(|b| *b == 0)?;
    if executable_end == 0 {
        return None;
    }
    let mut rest = bytes.get(executable_end + 1..)?;
    let first = rest.iter().position(|b| *b != 0)?;
    rest = &rest[first..];
    let mut argv = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        let end = rest.iter().position(|b| *b == 0)?;
        argv.push(std::str::from_utf8(&rest[..end]).ok()?.to_owned());
        rest = &rest[end + 1..];
    }
    Some(argv)
}
