use crate::SpeechError;
use asr::{AvailableAsrModel, PARAFORMER_MODEL_ID, ZIPFORMER_STREAMING_ZH_MODEL_ID};

pub fn build_online_recognizer_config(
    model: &AvailableAsrModel,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    match model.descriptor().id().as_str() {
        PARAFORMER_MODEL_ID => build_paraformer_config(model.root()),
        ZIPFORMER_STREAMING_ZH_MODEL_ID => build_transducer_config(model.root()),
        other => Err(SpeechError::Recognizer(format!(
            "unsupported ASR model: {other}"
        ))),
    }
}

fn build_transducer_config(
    root: &std::path::Path,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineRecognizerConfig, OnlineTransducerModelConfig};

    let encoder = required_model_file(root, "encoder.int8.onnx")?;
    let decoder = required_model_file(root, "decoder.onnx")?;
    let joiner = required_model_file(root, "joiner.int8.onnx")?;
    let tokens = required_model_file(root, "tokens.txt")?;
    Ok(OnlineRecognizerConfig {
        model_config: OnlineModelConfig {
            transducer: OnlineTransducerModelConfig {
                encoder: Some(path_string(&encoder)),
                decoder: Some(path_string(&decoder)),
                joiner: Some(path_string(&joiner)),
            },
            tokens: Some(path_string(&tokens)),
            num_threads: 2,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn build_paraformer_config(
    root: &std::path::Path,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineParaformerModelConfig, OnlineRecognizerConfig};

    let encoder = find_model_file(root, &["encoder.int8.onnx", "encoder.onnx"])?;
    let decoder = find_model_file(root, &["decoder.int8.onnx", "decoder.onnx"])?;
    let tokens = root.join("tokens.txt");
    if !tokens.is_file() {
        return Err(SpeechError::ModelNotFound(tokens.display().to_string()));
    }
    Ok(OnlineRecognizerConfig {
        model_config: OnlineModelConfig {
            paraformer: OnlineParaformerModelConfig {
                encoder: Some(path_string(&encoder)),
                decoder: Some(path_string(&decoder)),
            },
            tokens: Some(path_string(&root.join("tokens.txt"))),
            num_threads: 2,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn find_model_file(
    dir: &std::path::Path,
    candidates: &[&str],
) -> Result<std::path::PathBuf, SpeechError> {
    for name in candidates {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(SpeechError::ModelNotFound(format!(
        "none of {:?} found in {}",
        candidates,
        dir.display()
    )))
}

fn required_model_file(
    root: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, SpeechError> {
    let path = root.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(SpeechError::ModelNotFound(path.display().to_string()))
    }
}

fn path_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
#[path = "../tests/unit/recognizer_config_tests.rs"]
mod recognizer_config_tests;
