use super::{
    default_install_paths, forward_hook, hook_forward_env_from_process, install_hook_config,
    parse_hook_source, parse_hook_target, HookSource, HookTarget,
};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::io::{Read, Write};
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
    let mut claim = super::session_claim::claim;
    if let Some(output) = super::session_context_output(
        source,
        &raw_json,
        env.is_some() && super::path_convention_enabled_from_env(),
        &mut claim,
    ) {
        // 写 stdout 失败（Agent 已关管道）不能 panic：事件转发仍要继续，并撤销“已注入”标记。
        let mut stdout = std::io::stdout().lock();
        if writeln!(stdout, "{output}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            if let Some(id) = raw_json.get("session_id").and_then(Value::as_str) {
                super::session_claim::release(id);
            }
        }
    }
    forward_hook(source, raw_json, env)
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
