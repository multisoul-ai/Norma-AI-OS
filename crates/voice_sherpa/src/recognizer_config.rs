use crate::SpeechError;
use asr::{
    AvailableAsrModel, PARAFORMER_MODEL_ID, ZIPFORMER_BILINGUAL_MODEL_ID,
    ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID, ZIPFORMER_STREAMING_ZH_MODEL_ID,
};

const DEFAULT_HOTWORDS_SCORE: f32 = 2.0;
const DEFAULT_MAX_ACTIVE_PATHS: i32 = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct AsrHotwordConfig {
    text: String,
    score: f32,
    max_active_paths: i32,
}

impl AsrHotwordConfig {
    pub fn new(text: impl Into<String>, score: f32) -> Self {
        Self {
            text: text.into(),
            score,
            max_active_paths: DEFAULT_MAX_ACTIVE_PATHS,
        }
    }

    pub fn with_max_active_paths(mut self, max_active_paths: i32) -> Self {
        self.max_active_paths = max_active_paths;
        self
    }

    fn developer_defaults() -> Self {
        Self::new("JAVA\nKOTLIN\n", DEFAULT_HOTWORDS_SCORE)
    }
}

pub fn build_online_recognizer_config(
    model: &AvailableAsrModel,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    if model.descriptor().id().as_str() == ZIPFORMER_BILINGUAL_MODEL_ID {
        let hotwords = AsrHotwordConfig::developer_defaults();
        return build_online_recognizer_config_with_hotwords(model, Some(&hotwords));
    }
    build_online_recognizer_config_with_hotwords(model, None)
}

pub fn build_online_recognizer_config_with_hotwords(
    model: &AvailableAsrModel,
    hotwords: Option<&AsrHotwordConfig>,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    match model.descriptor().id().as_str() {
        PARAFORMER_MODEL_ID if hotwords.is_none() => build_paraformer_config(model.root()),
        ZIPFORMER_BILINGUAL_MODEL_ID => build_transducer_config(
            model.root(),
            "encoder-epoch-99-avg-1.int8.onnx",
            "decoder-epoch-99-avg-1.onnx",
            "joiner-epoch-99-avg-1.int8.onnx",
            hotwords,
        ),
        ZIPFORMER_STREAMING_ZH_MODEL_ID if hotwords.is_none() => build_transducer_config(
            model.root(),
            "encoder.int8.onnx",
            "decoder.onnx",
            "joiner.int8.onnx",
            None,
        ),
        ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID if hotwords.is_none() => {
            build_zipformer2_ctc_config(model.root())
        }
        PARAFORMER_MODEL_ID
        | ZIPFORMER_STREAMING_ZH_MODEL_ID
        | ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID => Err(SpeechError::Recognizer(format!(
            "ASR model {} does not support configured hotwords",
            model.descriptor().id().as_str()
        ))),
        other => Err(SpeechError::Recognizer(format!(
            "unsupported ASR model: {other}"
        ))),
    }
}

fn build_transducer_config(
    root: &std::path::Path,
    encoder_name: &str,
    decoder_name: &str,
    joiner_name: &str,
    hotwords: Option<&AsrHotwordConfig>,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineRecognizerConfig, OnlineTransducerModelConfig};

    let encoder = required_model_file(root, encoder_name)?;
    let decoder = required_model_file(root, decoder_name)?;
    let joiner = required_model_file(root, joiner_name)?;
    let tokens = required_model_file(root, "tokens.txt")?;
    let model_config = OnlineModelConfig {
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
    };
    let mut config = OnlineRecognizerConfig {
        model_config,
        ..Default::default()
    };
    if let Some(hotwords) = hotwords {
        if hotwords.text.trim().is_empty()
            || !hotwords.score.is_finite()
            || hotwords.score <= 0.0
            || hotwords.max_active_paths <= 0
        {
            return Err(SpeechError::Recognizer(
                "hotwords require non-empty text, a positive finite score, and positive max active paths"
                    .to_string(),
            ));
        }
        let bpe_vocab = required_model_file(root, "bpe.vocab")?;
        config.model_config.modeling_unit = Some("cjkchar+bpe".to_string());
        config.model_config.bpe_vocab = Some(path_string(&bpe_vocab));
        config.decoding_method = Some("modified_beam_search".to_string());
        config.max_active_paths = hotwords.max_active_paths;
        config.hotwords_score = hotwords.score;
        config.hotwords_buf = Some(hotwords.text.as_bytes().to_vec());
    }
    Ok(config)
}

fn build_zipformer2_ctc_config(
    root: &std::path::Path,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineRecognizerConfig, OnlineZipformer2CtcModelConfig};

    let model = required_model_file(root, "model.int8.onnx")?;
    let tokens = required_model_file(root, "tokens.txt")?;
    Ok(OnlineRecognizerConfig {
        model_config: OnlineModelConfig {
            zipformer2_ctc: OnlineZipformer2CtcModelConfig {
                model: Some(path_string(&model)),
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

#[cfg(test)]
#[path = "../tests/unit/recognizer_config_hotword_tests.rs"]
mod recognizer_config_hotword_tests;
