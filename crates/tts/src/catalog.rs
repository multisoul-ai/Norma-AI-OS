use crate::{TtsModelId, TtsVoiceId};
use model_store::ModelArtifact;

pub const MATCHA_ZH_MODEL_ID: &str = "matcha-icefall-zh-baker";
pub const MATCHA_ZH_EN_MODEL_ID: &str = "matcha-icefall-zh-en";
pub const KOKORO_MULTI_LANG_MODEL_ID: &str = "kokoro-int8-multi-lang-v1_1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsBackend {
    Matcha,
    Kokoro,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsVoiceDescriptor {
    id: TtsVoiceId,
    display_name: String,
    speaker_id: i32,
}

impl TtsVoiceDescriptor {
    pub fn new(id: TtsVoiceId, display_name: impl Into<String>, speaker_id: i32) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            speaker_id,
        }
    }

    pub fn id(&self) -> &TtsVoiceId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn speaker_id(&self) -> i32 {
        self.speaker_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsCapabilities {
    languages: Vec<String>,
    sample_rate_hz: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsSupportArtifact {
    url: String,
    sha256: String,
    bytes: u64,
    destination: String,
}

impl TtsSupportArtifact {
    pub fn new(
        url: impl Into<String>,
        sha256: impl Into<String>,
        bytes: u64,
        destination: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            sha256: sha256.into(),
            bytes,
            destination: destination.into(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn destination(&self) -> &str {
        &self.destination
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsModelPackage {
    archive: ModelArtifact,
    support_artifacts: Vec<TtsSupportArtifact>,
}

impl TtsModelPackage {
    pub fn new(archive: ModelArtifact, support_artifacts: Vec<TtsSupportArtifact>) -> Self {
        Self {
            archive,
            support_artifacts,
        }
    }

    pub fn archive(&self) -> &ModelArtifact {
        &self.archive
    }
    pub fn support_artifacts(&self) -> &[TtsSupportArtifact] {
        &self.support_artifacts
    }
    pub fn download_bytes(&self) -> u64 {
        self.archive.archive_bytes()
            + self
                .support_artifacts
                .iter()
                .map(TtsSupportArtifact::bytes)
                .sum::<u64>()
    }
}

impl TtsCapabilities {
    pub fn new(languages: Vec<&str>, sample_rate_hz: u32) -> Self {
        Self {
            languages: languages.into_iter().map(str::to_string).collect(),
            sample_rate_hz,
        }
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsModelRecommendation {
    recommended_voice_id: TtsVoiceId,
    model_recommended: bool,
}

impl TtsModelRecommendation {
    pub fn new(recommended_voice_id: TtsVoiceId, model_recommended: bool) -> Self {
        Self {
            recommended_voice_id,
            model_recommended,
        }
    }

    pub fn recommended_voice_id(&self) -> &TtsVoiceId {
        &self.recommended_voice_id
    }

    pub fn model_recommended(&self) -> bool {
        self.model_recommended
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsModelDescriptor {
    id: TtsModelId,
    display_name: String,
    backend: TtsBackend,
    capabilities: TtsCapabilities,
    package: TtsModelPackage,
    voices: Vec<TtsVoiceDescriptor>,
    recommendation: TtsModelRecommendation,
}

impl TtsModelDescriptor {
    pub fn new(
        id: TtsModelId,
        display_name: impl Into<String>,
        backend: TtsBackend,
        capabilities: TtsCapabilities,
        package: TtsModelPackage,
        voices: Vec<TtsVoiceDescriptor>,
        recommendation: TtsModelRecommendation,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            backend,
            capabilities,
            package,
            voices,
            recommendation,
        }
    }

    pub fn id(&self) -> &TtsModelId {
        &self.id
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub fn backend(&self) -> TtsBackend {
        self.backend
    }
    pub fn capabilities(&self) -> &TtsCapabilities {
        &self.capabilities
    }
    pub fn package(&self) -> &TtsModelPackage {
        &self.package
    }
    pub fn voices(&self) -> &[TtsVoiceDescriptor] {
        &self.voices
    }
    pub fn recommended(&self) -> bool {
        self.recommendation.model_recommended()
    }

    pub fn resolve_voice_id(&self, requested: Option<&str>) -> &TtsVoiceId {
        requested
            .and_then(|requested| {
                self.voices
                    .iter()
                    .find(|voice| voice.id().as_str() == requested)
            })
            .map(TtsVoiceDescriptor::id)
            .unwrap_or_else(|| self.recommendation.recommended_voice_id())
    }

    pub fn voice(&self, id: &TtsVoiceId) -> Option<&TtsVoiceDescriptor> {
        self.voices.iter().find(|voice| voice.id() == id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsModelCatalog {
    entries: Vec<TtsModelDescriptor>,
}

impl TtsModelCatalog {
    pub fn entries(&self) -> &[TtsModelDescriptor] {
        &self.entries
    }

    pub fn find(&self, id: &TtsModelId) -> Option<&TtsModelDescriptor> {
        self.entries.iter().find(|entry| entry.id() == id)
    }

    pub fn find_by_str(&self, id: &str) -> Option<&TtsModelDescriptor> {
        self.entries.iter().find(|entry| entry.id().as_str() == id)
    }
}

pub fn builtin_model_catalog() -> TtsModelCatalog {
    let default = voice("default", "Default Bilingual Voice", 0);
    let heart = voice("af_heart", "Heart", 0);
    let bella = voice("af_bella", "Bella", 1);
    TtsModelCatalog {
        entries: vec![
            descriptor(
                MATCHA_ZH_EN_MODEL_ID,
                "Matcha Chinese-English",
                TtsBackend::Matcha,
                TtsCapabilities::new(vec!["zh", "en"], 16_000),
                TtsModelPackage::new(
                    ModelArtifact::new(
                        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/matcha-icefall-zh-en.tar.bz2",
                        "271b804af570400d3bcdcb53bf6e53cc9f75180ee763b9f13eb5eaf2b0d086ef",
                        79_033_838,
                        vec![
                            "model-steps-3.onnx",
                            "lexicon.txt",
                            "tokens.txt",
                            "phone-zh.fst",
                            "date-zh.fst",
                            "number-zh.fst",
                            "espeak-ng-data/phontab",
                        ],
                    ),
                    vec![TtsSupportArtifact::new(
                        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos-16khz-univ.onnx",
                        "b599142a1fb8ff03de3e84ac35ff537c619e56f4267a6fe894851a42844acf9e",
                        53_882_848,
                        "vocos-16khz-univ.onnx",
                    )],
                ),
                vec![default.clone()],
                TtsModelRecommendation::new(default.id().clone(), true),
            ),
            descriptor(
                KOKORO_MULTI_LANG_MODEL_ID,
                "Kokoro Multi-language",
                TtsBackend::Kokoro,
                TtsCapabilities::new(vec!["zh", "en"], 24_000),
                TtsModelPackage::new(
                    ModelArtifact::new(
                        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2",
                        "a1e94694776049035c4f2c6529f003aaece993c76aae9a78995831c3c4dcafc6",
                        147_031_220,
                        vec!["model.int8.onnx", "voices.bin", "tokens.txt", "lexicon-zh.txt"],
                    ),
                    Vec::new(),
                ),
                vec![heart.clone(), bella],
                TtsModelRecommendation::new(heart.id().clone(), false),
            ),
        ],
    }
}

fn descriptor(
    id: &str,
    name: &str,
    backend: TtsBackend,
    capabilities: TtsCapabilities,
    package: TtsModelPackage,
    voices: Vec<TtsVoiceDescriptor>,
    recommendation: TtsModelRecommendation,
) -> TtsModelDescriptor {
    TtsModelDescriptor::new(
        TtsModelId::parse(id).expect("built-in TTS model id must be valid"),
        name,
        backend,
        capabilities,
        package,
        voices,
        recommendation,
    )
}

fn voice(id: &str, name: &str, speaker_id: i32) -> TtsVoiceDescriptor {
    TtsVoiceDescriptor::new(
        TtsVoiceId::parse(id).expect("built-in TTS voice id must be valid"),
        name,
        speaker_id,
    )
}
