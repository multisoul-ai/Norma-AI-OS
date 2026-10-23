//! 按命令名与参数判断单段 shell 命令的风险。

use super::path::{is_inside, is_secret};
use super::shell::{self, Segment};
use super::HighRiskReason as Reason;
use std::path::Path;

const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "fish"];
const WRAPPERS: [&str; 6] = ["command", "builtin", "exec", "nohup", "time", "env"];
const INTERPRETERS: [&str; 7] = ["python", "python3", "node", "perl", "ruby", "php", "deno"];
const INLINE_FLAGS: [&str; 4] = ["-c", "-e", "-p", "--eval"];
const DELETERS: [&str; 6] = ["rm", "rmdir", "unlink", "shred", "trash", "srm"];
const DEPLOYERS: [&str; 7] = [
    "vercel", "netlify", "firebase", "ssh", "scp", "sftp", "flyctl",
];
/// 会写入参数路径的命令；任何一个路径参数在项目外都算高风险。
const WRITERS: [&str; 10] = [
    "cp", "mv", "tee", "touch", "mkdir", "chmod", "chown", "ln", "install", "truncate",
];

pub(super) fn classify(command: &str, root: &Path) -> Option<Reason> {
    let Some(segments) = shell::parse(command) else {
        return Some(Reason::UnparseableCommand);
    };
    segments
        .iter()
        .find_map(|segment| segment_risk(segment, root))
}

fn segment_risk(segment: &Segment, root: &Path) -> Option<Reason> {
    for target in &segment.redirects {
        if is_secret(target) {
            return Some(Reason::Secrets);
        }
        if !is_inside(root, target) {
            return Some(Reason::OutsideProject);
        }
    }
    let argv = strip_prefixes(&segment.argv);
    let name = Path::new(argv.first()?).file_name()?.to_str()?;
    let args = &argv[1..];
    if SHELLS.contains(&name) {
        return Some(if segment.piped {
            Reason::RemoteScript
        } else {
            Reason::UnparseableCommand
        });
    }
    if let Some(reason) = by_name(name, args) {
        return Some(reason);
    }
    if args.iter().any(|a| is_secret(a)) {
        return Some(Reason::Secrets);
    }
    writes_outside(name, args, root).then_some(Reason::OutsideProject)
}

/// 去掉 `KEY=VAL` 前缀和 `env`、`nohup` 等包装。
fn strip_prefixes(argv: &[String]) -> &[String] {
    let mut rest = argv;
    while let Some(first) = rest.first() {
        let is_assignment = first.contains('=') && !first.starts_with('=') && !first.contains('/');
        let is_wrapper = WRAPPERS.contains(&first.as_str());
        if !is_assignment && !is_wrapper {
            break;
        }
        rest = &rest[1..];
    }
    rest
}

fn by_name(name: &str, args: &[String]) -> Option<Reason> {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match name {
        "sudo" | "doas" | "su" => Some(Reason::Privilege),
        "eval" | "source" | "." | "xargs" => Some(Reason::UnparseableCommand),
        n if DELETERS.contains(&n) => Some(Reason::Delete),
        n if DEPLOYERS.contains(&n) => Some(Reason::Deploy),
        n if INTERPRETERS.contains(&n) && INLINE_FLAGS.iter().any(|f| has(f)) => {
            Some(Reason::UnparseableCommand)
        }
        "awk" | "gawk" | "mawk" if args.iter().any(|a| a.contains("system(")) => {
            Some(Reason::UnparseableCommand)
        }
        "find" if has("-delete") => Some(Reason::Delete),
        "find" if has("-exec") || has("-execdir") || has("-ok") => Some(Reason::UnparseableCommand),
        "git" => git_risk(args),
        "gh" => match first_two(args) {
            ("pr", "merge") => Some(Reason::GitPublish),
            ("release", _) => Some(Reason::Deploy),
            ("repo", "delete") => Some(Reason::Delete),
            _ => None,
        },
        "npm" | "yarn" | "pnpm" | "cargo" | "pip" | "twine" if has("publish") => {
            Some(Reason::Deploy)
        }
        "docker" if has("push") => Some(Reason::Deploy),
        "kubectl"
            if ["apply", "delete", "create", "replace", "patch"]
                .iter()
                .any(|v| has(v)) =>
        {
            Some(Reason::Deploy)
        }
        "terraform" | "tofu" if has("apply") || has("destroy") => Some(Reason::Deploy),
        "helm" if ["install", "upgrade", "uninstall"].iter().any(|v| has(v)) => {
            Some(Reason::Deploy)
        }
        "wrangler" | "serverless" | "sls" | "fly" if has("deploy") || has("publish") => {
            Some(Reason::Deploy)
        }
        "security" if args.first().is_some_and(|s| !s.starts_with("list")) => Some(Reason::Secrets),
        "pass" | "op" | "gpg" | "gpg2" => Some(Reason::Secrets),
        _ => None,
    }
}

fn first_two(args: &[String]) -> (&str, &str) {
    let mut iter = args.iter().map(String::as_str);
    (iter.next().unwrap_or(""), iter.next().unwrap_or(""))
}

fn git_risk(args: &[String]) -> Option<Reason> {
    let mut iter = args.iter();
    let sub = loop {
        let arg = iter.next()?;
        if ["-C", "-c", "--git-dir", "--work-tree", "--namespace"].contains(&arg.as_str()) {
            iter.next();
        } else if !arg.starts_with('-') {
            break arg.as_str();
        }
    };
    let rest: Vec<&str> = iter.map(String::as_str).collect();
    match sub {
        "push" | "merge" | "rebase" => Some(Reason::GitPublish),
        "clean" => Some(Reason::Delete),
        "reset" if rest.contains(&"--hard") => Some(Reason::Delete),
        "branch" if rest.contains(&"-D") => Some(Reason::Delete),
        "stash" if rest.first().is_some_and(|s| *s == "drop" || *s == "clear") => {
            Some(Reason::Delete)
        }
        _ => None,
    }
}

fn writes_outside(name: &str, args: &[String], root: &Path) -> bool {
    let in_place_sed = name == "sed" && args.iter().any(|a| a.starts_with("-i"));
    let dd_target = |a: &String| a.strip_prefix("of=").map(str::to_string);
    if !WRITERS.contains(&name) && !in_place_sed && name != "dd" {
        return false;
    }
    args.iter()
        .filter_map(|a| {
            if name == "dd" {
                dd_target(a)
            } else {
                (!a.starts_with('-')).then(|| a.clone())
            }
        })
        .any(|path| !is_inside(root, &path))
}
