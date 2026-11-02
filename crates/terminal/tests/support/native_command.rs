//! Native shell fixtures: Unix retains sh; Windows uses the installed native command interpreters.
use crate::PtyCommand;

pub(super) fn shell() -> &'static str {
    if cfg!(windows) {
        "cmd.exe"
    } else {
        "/bin/sh"
    }
}

pub(super) fn command(unix: &str, windows: &str) -> PtyCommand {
    if cfg!(windows) {
        PtyCommand::new("powershell.exe").args(["-NoLogo", "-NoProfile", "-Command", windows])
    } else {
        PtyCommand::new("/bin/sh").args(["-c", unix])
    }
}
