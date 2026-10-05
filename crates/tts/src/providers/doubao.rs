//! Seed TTS 2.0 system voices, including the HTTP-only foreign voices.
//! Source: https://www.volcengine.com/docs/6561/1257544 (2026-10-04).
//! The Plan endpoint uses the same `seed-tts-2.0` resource; private cloned
//! voices use `seed-icl-2.0` and are intentionally outside this catalog.

use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::LazyLock;

pub const RESOURCE_ID: &str = "seed-tts-2.0";
pub const DEFAULT_VOICE_ID: &str = "zh_female_vv_uranus_bigtts";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DoubaoVoice {
    pub id: String,
    pub name: String,
    pub language: String,
}

static VOICES: LazyLock<Vec<DoubaoVoice>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("doubao/voices.json"))
        .expect("bundled Seed TTS 2.0 voice catalog must be valid")
});

pub fn voices() -> &'static [DoubaoVoice] {
    &VOICES
}

pub fn find_voice(id: &str) -> Option<&'static DoubaoVoice> {
    voices().iter().find(|voice| voice.id == id.trim())
}

pub fn default_voice() -> &'static DoubaoVoice {
    find_voice(DEFAULT_VOICE_ID).expect("bundled catalog must contain Vivi 2.0")
}

pub fn request_body(text: &str, voice: &DoubaoVoice) -> Value {
    json!({"req_params": {
        "text": text, "speaker": voice.id,
        "audio_params": {"format": "pcm", "sample_rate": 24000}
    }})
}

#[cfg(test)]
#[path = "../../tests/unit/doubao_voice_tests.rs"]
mod tests;
