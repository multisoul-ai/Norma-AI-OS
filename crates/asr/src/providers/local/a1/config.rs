use crate::AsrError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Explicit local installation; no PATH search, remote repository or model fallback.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct A1Config {
    pub protocol: u32,
    pub python: PathBuf,
    pub python_sha256: String,
    pub zipformer: PathBuf,
    pub qwen: PathBuf,
}

#[derive(Deserialize)]
struct Artifact {
    role: String,
    name: String,
    bytes: u64,
    sha256: String,
}

pub(super) fn error(message: impl ToString) -> AsrError {
    AsrError::Provider(format!("A1: {}", message.to_string()))
}

impl A1Config {
    /// Metadata-only read. Full artifact validation occurs before worker launch.
    pub fn from_model_root(root: &Path) -> Result<Self, AsrError> {
        let path = root.join("a1/runtime.json");
        let bytes = std::fs::read(&path).map_err(|e| error(format!("{}: {e}", path.display())))?;
        if bytes.len() > 16_384 {
            return Err(error("runtime manifest too large"));
        }
        let config: Self = serde_json::from_slice(&bytes).map_err(error)?;
        if config.protocol != 1
            || !config.python.is_absolute()
            || !config.zipformer.is_absolute()
            || !config.qwen.is_absolute()
            || config.python_sha256.len() != 64
        {
            return Err(error("invalid local runtime manifest"));
        }
        Ok(config)
    }

    /// Blocking IO: callers must run on a background executor. Never loads models.
    pub fn validate(&self) -> Result<(), AsrError> {
        if self.protocol != 1
            || !self.python.is_absolute()
            || !self.zipformer.is_absolute()
            || !self.qwen.is_absolute()
        {
            return Err(error("invalid local runtime manifest"));
        }
        if file_sha256(&self.python)? != self.python_sha256 {
            return Err(error("Python executable integrity mismatch"));
        }
        let artifacts: Vec<Artifact> =
            serde_json::from_str(include_str!("../../../../runtime/a1/artifacts.json"))
                .map_err(error)?;
        for artifact in artifacts {
            let root = if artifact.role == "zipformer" {
                &self.zipformer
            } else {
                &self.qwen
            };
            let path = root.join(&artifact.name);
            if std::fs::metadata(&path).map_err(error)?.len() != artifact.bytes
                || file_sha256(&path)? != artifact.sha256
            {
                return Err(error(format!(
                    "artifact integrity mismatch: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
}

fn file_sha256(path: &Path) -> Result<String, AsrError> {
    let mut file = File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = file.read(&mut buffer).map_err(error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
