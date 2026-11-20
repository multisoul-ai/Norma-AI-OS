use crate::{TtsModelId, TtsVoiceId};
use model_store::{ModelArtifact, VerifiedFile};

pub const MATCHA_ZH_MODEL_ID: &str = "matcha-icefall-zh-baker";
pub const MATCHA_ZH_EN_MODEL_ID: &str = "matcha-icefall-zh-en";
pub const KOKORO_MULTI_LANG_MODEL_ID: &str = "kokoro-int8-multi-lang-v1_1";
pub const ZIPVOICE_DISTILL_INT8_MODEL_ID: &str = "zipvoice-distill-int8-zh-en-emilia";
pub const QWEN3_TTS_MODEL_ID: &str = "qwen3-tts-0.6b-customvoice-8bit";
const QWEN3_TTS_BASE_URL: &str = "https://huggingface.co/mlx-community/Qwen3-TTS-12Hz-0.6B-CustomVoice-8bit/resolve/049ef77fe8816b536193c0c25f9a214d17921282";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsBackend {
    Matcha,
    Kokoro,
    ZipVoice,
    /// MLX model run by the managed A1 Python engine, not by Sherpa.
    Qwen3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsReleaseGate {
    M4PerformancePending,
    EmiliaDistributionAuditPending,
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
    requires_voice_reference: bool,
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
    base_url: String,
    verified_files: Vec<VerifiedFile>,
}

impl TtsModelPackage {
    pub fn new(archive: ModelArtifact, support_artifacts: Vec<TtsSupportArtifact>) -> Self {
        Self {
            archive,
            support_artifacts,
            base_url: String::new(),
            verified_files: Vec::new(),
        }
    }

    /// Individually downloaded files; `archive` is the manifest `install_verified_files` writes.
    pub fn from_verified_files(base_url: &str, files: Vec<VerifiedFile>) -> Self {
        Self {
            archive: model_store::verified_files_artifact(&files)
                .expect("built-in verified TTS file list must be valid"),
            support_artifacts: Vec::new(),
            base_url: base_url.to_string(),
            verified_files: files,
        }
    }

    /// Empty for archive packages.
    pub fn verified_files(&self) -> &[VerifiedFile] {
        &self.verified_files
    }

    pub fn file_url(&self, file: &VerifiedFile) -> String {
        format!("{}/{}", self.base_url, file.path)
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
            requires_voice_reference: false,
        }
    }

    pub fn with_voice_reference(mut self) -> Self {
        self.requires_voice_reference = true;
        self
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    pub fn requires_voice_reference(&self) -> bool {
        self.requires_voice_reference
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
    release_gates: Vec<TtsReleaseGate>,
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
            release_gates: Vec::new(),
        }
    }

    pub fn with_release_gates(mut self, release_gates: Vec<TtsReleaseGate>) -> Self {
        self.release_gates = release_gates;
        self
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

    pub fn experimental(&self) -> bool {
        !self.release_gates.is_empty()
    }

    pub fn release_gates(&self) -> &[TtsReleaseGate] {
        &self.release_gates
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
    let cloned = voice("cloned_reference", "Cloned Reference Voice", 0);
    let qwen3_voices: Vec<_> = [
        ("vivian", "Vivian"),
        ("serena", "Serena"),
        ("uncle_fu", "Uncle Fu"),
        ("dylan", "Dylan"),
        ("eric", "Eric"),
        ("ryan", "Ryan"),
        ("aiden", "Aiden"),
        ("ono_anna", "Ono Anna"),
        ("sohee", "Sohee"),
    ]
    .into_iter()
    .zip(0..)
    .map(|((id, name), index)| voice(id, name, index))
    .collect();
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
                            "espeak-ng-data/phondata",
                            "espeak-ng-data/phonindex",
                            "espeak-ng-data/en_dict",
                            "espeak-ng-data/lang/gmw/en",
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
            descriptor(
                ZIPVOICE_DISTILL_INT8_MODEL_ID,
                "ZipVoice-Distill INT8",
                TtsBackend::ZipVoice,
                TtsCapabilities::new(vec!["zh", "en"], 24_000).with_voice_reference(),
                TtsModelPackage::new(
                    ModelArtifact::new(
                        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2",
                        "77219c8b40f4ee8d73a7f902305ff6c1128ef9b54461c41b4ca6ed890b6c2803",
                        109_162_785,
                        vec![
                            "encoder.int8.onnx",
                            "decoder.int8.onnx",
                            "tokens.txt",
                            "lexicon.txt",
                            "espeak-ng-data/phontab",
                            "espeak-ng-data/phondata",
                            "espeak-ng-data/phonindex",
                        ],
                    ),
                    vec![TtsSupportArtifact::new(
                        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx",
                        "bcb3b970e384161c4d634f0bb9e999ff1c471b34c9bc0b1049a5014065ed3cc0",
                        54_157_409,
                        "vocos_24khz.onnx",
                    )],
                ),
                vec![cloned.clone()],
                TtsModelRecommendation::new(cloned.id().clone(), false),
            )
            .with_release_gates(vec![
                TtsReleaseGate::M4PerformancePending,
                TtsReleaseGate::EmiliaDistributionAuditPending,
            ]),
            descriptor(
                QWEN3_TTS_MODEL_ID,
                "Qwen3-TTS 0.6B",
                TtsBackend::Qwen3,
                TtsCapabilities::new(vec!["zh", "en"], 24_000),
                TtsModelPackage::from_verified_files(QWEN3_TTS_BASE_URL, crate::qwen3_files::qwen3_tts_files()),
                qwen3_voices.clone(),
                TtsModelRecommendation::new(qwen3_voices[0].id().clone(), false),
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
