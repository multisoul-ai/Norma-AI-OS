use audio::{PcmAudioChunk, PcmFormat};
use std::path::Path;

pub(crate) fn pcm16_chunk_to_f32_samples(chunk: &PcmAudioChunk) -> Result<Vec<f32>, String> {
    if chunk.format != PcmFormat::Pcm16 {
        return Err("expected PCM16 chunk".to_string());
    }
    if chunk.sample_rate_hz != 16_000 {
        return Err(format!(
            "expected 16_000Hz chunk, got {}",
            chunk.sample_rate_hz
        ));
    }
    if chunk.channels != 1 {
        return Err(format!(
            "expected mono chunk, got {} channels",
            chunk.channels
        ));
    }
    if !chunk.bytes.len().is_multiple_of(2) {
        return Err("PCM16 byte length must be even".to_string());
    }
    Ok(chunk
        .bytes
        .chunks_exact(2)
        .map(|pair| {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            f32::from(sample) / 32_768.0
        })
        .collect())
}

pub(crate) fn find_vad_model_file(dir: &Path) -> Result<std::path::PathBuf, crate::SpeechError> {
    for name in ["silero_vad.onnx", "silero_vad.int8.onnx"] {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(crate::SpeechError::ModelNotFound(format!(
        "no silero VAD model found in {}",
        dir.display()
    )))
}

pub(crate) fn resolve_vad_model_dir(
    asr_model_dir: &Path,
) -> Result<std::path::PathBuf, crate::SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_SILERO_VAD_MODEL_DIR") {
        let dir = std::path::PathBuf::from(path);
        if dir.exists() {
            return Ok(dir);
        }
        return Err(crate::SpeechError::ModelNotFound(dir.display().to_string()));
    }
    if let Ok(path) = std::env::var("MULTISOUL_SILERO_VAD_MODEL") {
        let file = std::path::PathBuf::from(path);
        if file.exists() {
            return file
                .parent()
                .map(std::path::Path::to_path_buf)
                .ok_or_else(|| {
                    crate::SpeechError::ModelNotFound("VAD model path has no parent".to_string())
                });
        }
        return Err(crate::SpeechError::ModelNotFound(
            file.display().to_string(),
        ));
    }

    let sibling = asr_model_dir
        .parent()
        .map(|parent| parent.join("silero_vad"));
    if let Some(dir) = sibling {
        if dir.exists() {
            return Ok(dir);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let repo_root = exe.ancestors().find(|p| p.join("Cargo.toml").exists());
        if let Some(root) = repo_root {
            let dev_path = root.join("assets/models/silero_vad");
            if dev_path.exists() {
                return Ok(dev_path);
            }
        }
    }

    Err(crate::SpeechError::ModelNotFound(
        "no silero VAD model found; set MULTISOUL_SILERO_VAD_MODEL_DIR".to_string(),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/samples_tests.rs"]
mod samples_tests;
