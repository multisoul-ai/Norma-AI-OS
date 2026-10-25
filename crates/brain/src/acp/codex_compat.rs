//! codex-acp 2.2.2 的权限展示丢失了 shell 参数边界。只在模块加载时保留原始命令，
//! 不改用户安装包；格式或版本不匹配则拒绝启动，绝不降级为猜测引号。

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const NEEDLE: &str = "command: stripShellPrefix(params.command)";

pub(super) struct Adapter {
    pub program: PathBuf,
    pub args: Vec<String>,
    _loader: NamedTempFile,
}

impl Adapter {
    pub fn prepare(program: &Path, args: &[String]) -> io::Result<Self> {
        let entry = program.canonicalize()?;
        let package = entry
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| io::Error::other("Codex 适配器缺少包目录"))?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(package.join("package.json"))?)?;
        let source = std::fs::read_to_string(&entry)?;
        if manifest["name"] != "@agentclientprotocol/codex-acp"
            || manifest["version"] != "2.2.2"
            || source.matches(NEEDLE).count() != 1
        {
            return Err(io::Error::other(
                "Codex 权限适配只支持已验证的 codex-acp 2.2.2，请重新安装适配器",
            ));
        }
        let mut loader = tempfile::Builder::new()
            .prefix("norma-codex-")
            .suffix(".mjs")
            .tempfile()?;
        loader.write_all(include_bytes!("codex_loader.mjs"))?;
        let mut launch_args = vec![
            loader.path().to_string_lossy().into_owned(),
            entry.to_string_lossy().into_owned(),
        ];
        launch_args.extend_from_slice(args);
        Ok(Self {
            program: "node".into(),
            args: launch_args,
            _loader: loader,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/codex_compat_tests.rs"]
mod tests;
