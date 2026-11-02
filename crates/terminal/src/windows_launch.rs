//! Keep existing Agent launch intent while executing native Windows commands through PowerShell.
use crate::{PtyCommand, PtyError};

pub(super) fn prepare(mut command: PtyCommand) -> Result<PtyCommand, PtyError> {
    if command.program != "/bin/zsh" {
        return Ok(command);
    }
    command.program = "powershell.exe".into();
    if command.args.is_empty() {
        return Ok(command);
    }
    if command.args.len() != 2 || command.args[0] != "-lic" {
        return Err(PtyError::Spawn(
            "unsupported Windows shell invocation".into(),
        ));
    }
    let script = command.args[1]
        .strip_prefix("exec ")
        .ok_or_else(|| PtyError::Spawn("unsupported POSIX launch script on Windows".into()))?;
    let mut script = script.to_string();
    for (variable, default) in [("OPENCODE_BIN", "opencode"), ("INFCODE_BIN", "infcode")] {
        let value = std::env::var(variable).unwrap_or_else(|_| default.into());
        script = script.replace(
            &format!("\"${{{variable}:-{default}}}\""),
            &shell_words::quote(&value),
        );
    }
    let words = shell_words::split(&script).map_err(|error| PtyError::Spawn(error.to_string()))?;
    let program = words
        .first()
        .ok_or_else(|| PtyError::Spawn("empty Windows Agent command".into()))?;
    let known = [
        "claude",
        "codex",
        "cursor-agent",
        "grok",
        "opencode",
        "infcode",
        "kimi",
        "zcode",
        "pi",
        "hermes",
    ]
    .contains(&program.as_str())
        || ["OPENCODE_BIN", "INFCODE_BIN"]
            .iter()
            .any(|key| std::env::var(key).is_ok_and(|value| value == *program));
    if !known
        || words
            .iter()
            .any(|word| ["&&", "||", ";", "|", ">", "<"].contains(&word.as_str()))
    {
        return Err(PtyError::Spawn(
            "unsupported compound Agent launch script on Windows".into(),
        ));
    }
    let invocation = words
        .iter()
        .map(|word| format!("'{}'", word.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(" ");
    command.args = vec![
        "-NoLogo".into(),
        "-NoProfile".into(),
        "-Command".into(),
        format!("& {invocation}"),
    ];
    Ok(command)
}

#[cfg(test)]
#[path = "../tests/unit/windows_launch_tests.rs"]
mod tests;
