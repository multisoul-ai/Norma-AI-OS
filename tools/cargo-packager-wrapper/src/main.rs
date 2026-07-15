use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const NOTARY_ENV_VARS: &[&str] = &[
    "APPLE_API_KEY",
    "APPLE_API_ISSUER",
    "APPLE_API_KEY_PATH",
    "APPLE_KEYCHAIN_PROFILE",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
];

fn main() -> ExitCode {
    let mut notarize = false;
    let mut forwarded_args = Vec::new();

    for arg in env::args().skip(1) {
        if arg == "--notarize" {
            notarize = true;
        } else {
            forwarded_args.push(arg);
        }
    }

    let working_dir = match caller_working_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("failed to determine cargo-packager working directory: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut command = Command::new("cargo-packager");
    command.current_dir(working_dir);
    command.args(&forwarded_args);

    for name in NOTARY_ENV_VARS {
        command.env_remove(name);
    }

    if notarize {
        let key_id = match required_env("APP_STORE_CONNECT_API_KEY_ID") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let issuer = match required_env("APP_STORE_CONNECT_API_ISSUER_ID") {
            Ok(value) => value,
            Err(code) => return code,
        };
        let key_path = match required_env("APP_STORE_CONNECT_API_KEY_PATH") {
            Ok(value) => value,
            Err(code) => return code,
        };

        command.env("APPLE_API_KEY", key_id);
        command.env("APPLE_API_ISSUER", issuer);
        command.env("APPLE_API_KEY_PATH", key_path);
    }

    match command.status() {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("failed to run cargo-packager: {error}");
            ExitCode::FAILURE
        }
    }
}

fn caller_working_dir() -> Result<PathBuf, std::io::Error> {
    if let Some(path) = env::var_os("PWD").map(PathBuf::from) {
        if path.is_dir() {
            return Ok(path);
        }
    }

    env::current_dir()
}

fn required_env(name: &str) -> Result<String, ExitCode> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => {
            eprintln!("--notarize requires {name}");
            Err(ExitCode::from(2))
        }
    }
}
