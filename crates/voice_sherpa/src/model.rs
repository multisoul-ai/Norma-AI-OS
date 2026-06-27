use crate::SpeechError;
use std::path::PathBuf;

#[cfg(test)]
#[path = "../tests/unit/model_tests.rs"]
mod tests;

pub fn resolve_model_dir() -> Result<PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR") {
        return Ok(PathBuf::from(path));
    }
    // Dev: repo root assets/models/paraformer/
    if let Ok(exe) = std::env::current_exe() {
        let repo_root = exe.ancestors().find(|p| p.join("Cargo.toml").exists());
        if let Some(root) = repo_root {
            let dev_path = root.join("assets/models/paraformer");
            if dev_path.exists() {
                return Ok(dev_path);
            }
        }
    }
    // Release: Contents/Resources/models/paraformer/
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent() // MacOS/
            .and_then(|p| p.parent()) // Contents/
            .map(|p| p.join("Resources/models/paraformer"));
        if let Some(path) = bundle_path {
            if path.exists() {
                return Ok(path);
            }
        }
    }
    Err(SpeechError::ModelNotFound(
        "no paraformer model found; run scripts/download-paraformer-model.sh".to_string(),
    ))
}
