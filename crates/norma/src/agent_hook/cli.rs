use super::{
    default_install_paths, forward_hook, hook_forward_env_from_process, install_hook_config,
    parse_hook_source, parse_hook_target, HookSource, HookTarget,
};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "multisoul-agent-hook")]
#[command(about = "Bridge Claude/Codex/OpenCode lifecycle hooks back to Norma OS")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Forward {
        #[arg(long, value_parser = parse_source_arg)]
        source: HookSource,
        #[arg(long)]
        request_approval: bool,
    },
    Install {
        #[arg(long, value_parser = parse_target_arg)]
        target: HookTarget,
        #[arg(long)]
        command_path: Option<PathBuf>,
    },
}

pub fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Forward {
            source,
            request_approval,
        } => {
            let _ = forward_from_stdin(source, request_approval);
            Ok(())
        }
        Command::Install {
            target,
            command_path,
        } => {
            let command_path = command_path.unwrap_or(std::env::current_exe()?);
            install_hook_config(target, &command_path, &default_install_paths())?;
            println!(
                "installed Norma OS agent hooks using {}",
                command_path.display()
            );
            Ok(())
        }
    }
}

fn forward_from_stdin(source: HookSource, request_approval: bool) -> super::HookForwardOutcome {
    let mut input = String::new();
    if std::io::stdin()
        .take(1_048_577)
        .read_to_string(&mut input)
        .is_err()
        || input.len() > 1_048_576
    {
        return super::HookForwardOutcome::Noop;
    }
    let Ok(mut raw_json) = serde_json::from_str::<Value>(&input) else {
        return super::HookForwardOutcome::Noop;
    };
    if let Some(object) = raw_json.as_object_mut() {
        object.remove("_norma_control_run_id");
        if let Ok(run_id) = std::env::var(super::ENV_CONTROL_RUN_ID) {
            object.insert("_norma_control_run_id".into(), Value::String(run_id));
        }
    }
    if request_approval {
        let Some(env) = hook_forward_env_from_process() else {
            return super::HookForwardOutcome::Noop;
        };
        let Ok(run_id) = std::env::var(super::ENV_CONTROL_RUN_ID) else {
            return super::HookForwardOutcome::Noop;
        };
        match super::forward_permission(source, raw_json, env, run_id) {
            Ok(Some(output)) => println!("{output}"),
            Ok(None) => {}
            Err(error) => {
                eprintln!("multisoul-agent-hook: permission bridge unavailable: {error}");
                return super::HookForwardOutcome::FailedButIgnored;
            }
        }
        return super::HookForwardOutcome::Posted;
    }
    let env = hook_forward_env_from_process();
    let mut claim = claim_session_once;
    if let Some(output) = super::session_context_output(
        source,
        &raw_json,
        env.is_some() && super::path_convention_enabled_from_env(),
        &mut claim,
    ) {
        println!("{output}");
    }
    forward_hook(source, raw_json, env)
}

/// 以临时目录下的标记文件保证“每个会话只注入一次”；任何 IO 失败按未注入处理（宁可少注入）。
fn claim_session_once(session_id: &str) -> bool {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(96)
        .collect();
    if safe.is_empty() {
        return false;
    }
    let dir = std::env::temp_dir().join("norma-path-convention");
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(safe))
        .is_ok()
}

fn parse_source_arg(value: &str) -> Result<HookSource, String> {
    parse_hook_source(value).ok_or_else(|| {
        "expected claude, codex, cursor, opencode, pi, kimi, hermes, or zcode".to_string()
    })
}

fn parse_target_arg(value: &str) -> Result<HookTarget, String> {
    parse_hook_target(value).ok_or_else(|| {
        "expected claude, codex, cursor, opencode, pi, kimi, hermes, zcode, or all".to_string()
    })
}
