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
    let binaries = [
        ("OPENCODE_BIN", "opencode"),
        ("INFCODE_BIN", "infcode"),
        ("CURSOR_AGENT_BIN", "cursor-agent"),
    ]
    .map(|(variable, default)| {
        let value = command
            .env
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(variable))
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(variable).ok())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.to_owned());
        (variable, default, value)
    });
    for (variable, default, value) in &binaries {
        let executable = format!("\"${{{variable}:-{default}}}\"");
        if let Some(arguments) = script
            .trim_start_matches([' ', '\t', '\n'])
            .strip_prefix(&executable)
        {
            if arguments.chars().next().is_none_or(char::is_whitespace) {
                script = format!("{}{arguments}", shell_words::quote(value));
                break;
            }
        }
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
        || binaries.iter().any(|(_, _, value)| value == program);
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
        .map(|word| powershell_literal(word))
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

fn powershell_literal(word: &str) -> String {
    let mut literal = String::from("'");
    for ch in word.chars() {
        literal.push(ch);
        // PowerShell's tokenizer treats these Unicode characters as single quotes too.
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            literal.push(ch);
        }
    }
    literal.push('\'');
    literal
}

#[cfg(test)]
#[path = "../tests/unit/windows_launch_tests.rs"]
mod tests;
