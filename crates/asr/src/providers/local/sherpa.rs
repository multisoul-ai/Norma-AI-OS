use crate::{
    AsrError, AsrProviderCapabilities, AvailableAsrModel, Recognizer, RecognizerFactory,
    PARAFORMER_MODEL_ID, ZIPFORMER_BILINGUAL_MODEL_ID, ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID,
    ZIPFORMER_STREAMING_ZH_MODEL_ID,
};

const DEFAULT_HOTWORDS_SCORE: f32 = 2.0;
const DEFAULT_MAX_ACTIVE_PATHS: i32 = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct AsrHotwordConfig {
    text: String,
    score: f32,
    max_active_paths: i32,
}

#[derive(Clone)]
pub struct SherpaRecognizerFactory {
    model: AvailableAsrModel,
}

impl SherpaRecognizerFactory {
    pub fn new(model: AvailableAsrModel) -> Self {
        Self { model }
    }
}

impl RecognizerFactory for SherpaRecognizerFactory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(true, None)
    }

    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        SherpaRecognizer::from_available_model(&self.model)
            .map(|recognizer| Box::new(recognizer) as Box<dyn Recognizer>)
    }
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
) -> Result<sherpa_onnx::OnlineRecognizerConfig, AsrError> {
    if model.descriptor().id().as_str() == ZIPFORMER_BILINGUAL_MODEL_ID {
        let hotwords = AsrHotwordConfig::developer_defaults();
        return build_online_recognizer_config_with_hotwords(model, Some(&hotwords));
    }
    build_online_recognizer_config_with_hotwords(model, None)
}

pub fn build_online_recognizer_config_with_hotwords(
    model: &AvailableAsrModel,
    hotwords: Option<&AsrHotwordConfig>,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, AsrError> {
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
        | ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID => Err(AsrError::Provider(format!(
            "ASR model {} does not support configured hotwords",
            model.descriptor().id().as_str()
        ))),
        other => Err(AsrError::Provider(format!(
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
) -> Result<sherpa_onnx::OnlineRecognizerConfig, AsrError> {
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
            return Err(AsrError::Provider(
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
) -> Result<sherpa_onnx::OnlineRecognizerConfig, AsrError> {
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
) -> Result<sherpa_onnx::OnlineRecognizerConfig, AsrError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineParaformerModelConfig, OnlineRecognizerConfig};

    let encoder = find_model_file(root, &["encoder.int8.onnx", "encoder.onnx"])?;
    let decoder = find_model_file(root, &["decoder.int8.onnx", "decoder.onnx"])?;
    let tokens = root.join("tokens.txt");
    if !tokens.is_file() {
        return Err(AsrError::Provider(tokens.display().to_string()));
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
) -> Result<std::path::PathBuf, AsrError> {
    for name in candidates {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(AsrError::Provider(format!(
        "none of {:?} found in {}",
        candidates,
        dir.display()
    )))
}

fn required_model_file(root: &std::path::Path, name: &str) -> Result<std::path::PathBuf, AsrError> {
    let path = root.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(AsrError::Provider(path.display().to_string()))
    }
}

fn path_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

pub struct SherpaRecognizer {
    recognizer: sherpa_onnx::OnlineRecognizer,
    stream: sherpa_onnx::OnlineStream,
    last_partial: String,
}

impl SherpaRecognizer {
    pub fn from_available_model(model: &AvailableAsrModel) -> Result<Self, AsrError> {
        let config = build_online_recognizer_config(model)?;
        let recognizer = sherpa_onnx::OnlineRecognizer::create(&config)
            .ok_or_else(|| AsrError::Provider("failed to create OnlineRecognizer".to_string()))?;
        let stream = recognizer.create_stream();
        Ok(Self {
            recognizer,
            stream,
            last_partial: String::new(),
        })
    }

    pub fn reset(&mut self) {
        self.stream = self.recognizer.create_stream();
        self.last_partial.clear();
    }
}

impl crate::Recognizer for SherpaRecognizer {
    fn accept_pcm(&mut self, chunk: &audio::PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        let samples = pcm16_chunk_to_f32_samples(chunk)?;
        self.stream.accept_waveform(16_000, &samples);
        let mut partials = Vec::new();
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
            let text = self
                .recognizer
                .get_result(&self.stream)
                .map(|result| result.text.trim().to_string())
                .unwrap_or_default();
            if !text.is_empty() && text != self.last_partial {
                self.last_partial = text.clone();
                partials.push(text);
            }
        }
        Ok(partials)
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        self.stream.input_finished();
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
        let text = self
            .recognizer
            .get_result(&self.stream)
            .map(|result| result.text.trim().to_string())
            .unwrap_or_default();
        Ok((!text.is_empty()).then_some(text))
    }
}

fn pcm16_chunk_to_f32_samples(chunk: &audio::PcmAudioChunk) -> Result<Vec<f32>, AsrError> {
    if chunk.format != audio::PcmFormat::Pcm16
        || chunk.sample_rate_hz != 16_000
        || chunk.channels != 1
        || !chunk.bytes.len().is_multiple_of(2)
    {
        return Err(AsrError::InvalidPcm);
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
