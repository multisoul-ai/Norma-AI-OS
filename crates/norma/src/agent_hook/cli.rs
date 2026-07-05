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
#[command(about = "Bridge Claude/Codex lifecycle hooks back to Multisoul Desktop")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Forward {
        #[arg(long, value_parser = parse_source_arg)]
        source: HookSource,
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
        Command::Forward { source } => {
            let _ = forward_from_stdin(source);
            Ok(())
        }
        Command::Install {
            target,
            command_path,
        } => {
            let command_path = command_path.unwrap_or(std::env::current_exe()?);
            install_hook_config(target, &command_path, &default_install_paths())?;
            println!(
                "installed Multisoul agent hooks using {}",
                command_path.display()
            );
            Ok(())
        }
    }
}

fn forward_from_stdin(source: HookSource) -> super::HookForwardOutcome {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return super::HookForwardOutcome::Noop;
    }
    let Ok(raw_json) = serde_json::from_str::<Value>(&input) else {
        return super::HookForwardOutcome::Noop;
    };
    forward_hook(source, raw_json, hook_forward_env_from_process())
}

fn parse_source_arg(value: &str) -> Result<HookSource, String> {
    parse_hook_source(value).ok_or_else(|| "expected claude or codex".to_string())
}

fn parse_target_arg(value: &str) -> Result<HookTarget, String> {
    parse_hook_target(value).ok_or_else(|| "expected claude, codex, or all".to_string())
}
