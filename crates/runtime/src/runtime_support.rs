use std::path::Path;

use storage::SessionCheckpoint;
use terminal::PtyCommand;

use crate::runtime::{DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS};

pub(crate) fn checkpoint_grid_size(checkpoint: Option<&SessionCheckpoint>) -> (u16, u16) {
    checkpoint
        .map(|checkpoint| (checkpoint.cols, checkpoint.rows))
        .unwrap_or((DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS))
}

pub(crate) fn usize_to_u16(value: usize) -> u16 {
    value.min(usize::from(u16::MAX)) as u16
}

pub(crate) fn terminal_shell_command(shell: &str) -> PtyCommand {
    let command = PtyCommand::new(shell);
    if is_zsh_shell(shell) {
        command.arg("-l")
    } else {
        command
    }
}

fn is_zsh_shell(shell: &str) -> bool {
    Path::new(shell).file_name().and_then(|name| name.to_str()) == Some("zsh")
}
