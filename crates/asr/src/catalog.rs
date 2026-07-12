use crate::AsrModelId;

pub const PARAFORMER_MODEL_ID: &str = "paraformer-bilingual-zh-en-int8";
pub const PARAFORMER_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-paraformer-bilingual-zh-en.tar.bz2";
pub const ZIPFORMER_STREAMING_ZH_MODEL_ID: &str = "zipformer-streaming-zh-int8-2025-06-30";
pub const ZIPFORMER_STREAMING_ZH_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30.tar.bz2";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsrCapabilities {
    languages: Vec<String>,
    streaming_partials: bool,
}

impl AsrCapabilities {
    pub fn new(languages: Vec<&str>, streaming_partials: bool) -> Self {
        Self {
            languages: languages.into_iter().map(str::to_string).collect(),
            streaming_partials,
        }
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    pub fn streaming_partials(&self) -> bool {
        self.streaming_partials
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsrArtifact {
    url: String,
    sha256: String,
    archive_bytes: u64,
    required_files: Vec<String>,
}

impl AsrArtifact {
    pub fn new(
        url: impl Into<String>,
        sha256: impl Into<String>,
        archive_bytes: u64,
        required_files: Vec<&str>,
    ) -> Self {
        Self {
            url: url.into(),
            sha256: sha256.into(),
            archive_bytes,
            required_files: required_files.into_iter().map(str::to_string).collect(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn archive_bytes(&self) -> u64 {
        self.archive_bytes
    }

    pub fn required_files(&self) -> &[String] {
        &self.required_files
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsrModelDescriptor {
    id: AsrModelId,
    display_name: String,
    capabilities: AsrCapabilities,
    artifact: AsrArtifact,
    recommended: bool,
}

impl AsrModelDescriptor {
    pub fn new(
        id: AsrModelId,
        display_name: impl Into<String>,
        capabilities: AsrCapabilities,
        artifact: AsrArtifact,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            capabilities,
            artifact,
            recommended: false,
        }
    }

    pub fn id(&self) -> &AsrModelId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn capabilities(&self) -> &AsrCapabilities {
        &self.capabilities
    }

    pub fn artifact(&self) -> &AsrArtifact {
        &self.artifact
    }

    pub fn recommended(&self) -> bool {
        self.recommended
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsrModelCatalog {
    entries: Vec<AsrModelDescriptor>,
}

impl AsrModelCatalog {
    pub fn entries(&self) -> &[AsrModelDescriptor] {
        &self.entries
    }

    pub fn find(&self, id: &AsrModelId) -> Option<&AsrModelDescriptor> {
        self.entries.iter().find(|entry| entry.id() == id)
    }
}

pub fn builtin_model_catalog() -> AsrModelCatalog {
    let paraformer_id =
        AsrModelId::parse(PARAFORMER_MODEL_ID).expect("built-in Paraformer model id must be valid");
    let zipformer_id = AsrModelId::parse(ZIPFORMER_STREAMING_ZH_MODEL_ID)
        .expect("built-in Zipformer model id must be valid");
    AsrModelCatalog {
        entries: vec![
            AsrModelDescriptor::new(
                paraformer_id,
                "Paraformer Bilingual",
                AsrCapabilities::new(vec!["zh", "en"], true),
                AsrArtifact::new(
                    PARAFORMER_ARCHIVE_URL,
                    "5462a1fce42693deae572af1e8c4687124b12aa85fe61ff4d3168bb5280e205f",
                    1_047_319_737,
                    vec!["encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"],
                ),
            ),
            AsrModelDescriptor::new(
                zipformer_id,
                "Zipformer Streaming ZH",
                AsrCapabilities::new(vec!["zh"], true),
                AsrArtifact::new(
                    ZIPFORMER_STREAMING_ZH_ARCHIVE_URL,
                    "5a2832047ea1f97dd0dc595b816c230c4bafad65cfc0341fa57517cadc50afd0",
                    132_634_597,
                    vec![
                        "encoder.int8.onnx",
                        "decoder.onnx",
                        "joiner.int8.onnx",
                        "tokens.txt",
                    ],
                ),
            ),
        ],
    }
}
