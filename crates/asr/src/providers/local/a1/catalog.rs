use crate::AsrError;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct A1ModelArtifact {
    pub role: String,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

impl A1ModelArtifact {
    pub fn relative_path(&self) -> String {
        format!("{}/{}", self.role, self.name)
    }

    pub fn download_url(&self) -> String {
        let repository = if self.role == "zipformer" {
            "csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20/resolve/98590b7ed6443e77b714204da2757d75e1a642f4"
        } else {
            "mlx-community/Qwen3-ASR-0.6B-8bit/resolve/89e96d92ba34aca20b3e29fb10cc284097d1219f"
        };
        format!("https://huggingface.co/{repository}/{}", self.name)
    }
}

pub fn a1_model_artifacts() -> Result<Vec<A1ModelArtifact>, AsrError> {
    serde_json::from_str(include_str!("../../../../runtime/a1/artifacts.json"))
        .map_err(super::config::error)
}

pub const A1_REQUIREMENTS: &str = include_str!("../../../../runtime/a1/requirements.lock");
