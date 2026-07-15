use crate::installation::sha256_file;
use crate::AsrInstallationError;
use std::fs;
use std::path::{Path, PathBuf};

pub const SILERO_VAD_ARCHIVE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
pub const SILERO_VAD_SHA256: &str =
    "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";
pub const SILERO_VAD_ARCHIVE_BYTES: u64 = 643_854;
pub const SILERO_VAD_DIRECTORY: &str = "silero_vad";
pub const SILERO_VAD_FILE: &str = "silero_vad.onnx";

pub fn validate_silero_vad_installation(
    model_root: &Path,
) -> Result<PathBuf, AsrInstallationError> {
    let path = model_root.join(SILERO_VAD_DIRECTORY).join(SILERO_VAD_FILE);
    if !path.is_file() {
        return Err(AsrInstallationError::MissingRequiredFile(path));
    }
    if sha256_file(&path)? != SILERO_VAD_SHA256 {
        return Err(AsrInstallationError::SupportArtifactHashMismatch);
    }
    Ok(path)
}

pub fn install_silero_vad_file(
    source: &Path,
    model_root: &Path,
) -> Result<PathBuf, AsrInstallationError> {
    if sha256_file(source)? != SILERO_VAD_SHA256 {
        return Err(AsrInstallationError::SupportArtifactHashMismatch);
    }
    fs::create_dir_all(model_root).map_err(|error| AsrInstallationError::Io {
        path: model_root.to_path_buf(),
        message: error.to_string(),
    })?;
    let final_root = model_root.join(SILERO_VAD_DIRECTORY);
    if final_root.exists() {
        return validate_silero_vad_installation(model_root);
    }
    let temp = tempfile::Builder::new()
        .prefix(".silero-vad-partial-")
        .tempdir_in(model_root)
        .map_err(|error| AsrInstallationError::Io {
            path: model_root.to_path_buf(),
            message: error.to_string(),
        })?;
    let temp_file = temp.path().join(SILERO_VAD_FILE);
    fs::copy(source, &temp_file).map_err(|error| AsrInstallationError::Io {
        path: temp_file,
        message: error.to_string(),
    })?;
    fs::rename(temp.path(), &final_root).map_err(|error| AsrInstallationError::Io {
        path: final_root,
        message: error.to_string(),
    })?;
    validate_silero_vad_installation(model_root)
}
