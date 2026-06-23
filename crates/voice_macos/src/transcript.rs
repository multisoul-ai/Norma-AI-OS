use crate::SpeechError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TranscriptAccumulator {
    partial: Option<String>,
    final_text: Option<String>,
}

impl TranscriptAccumulator {
    pub fn push_partial(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref().trim();
        if !text.is_empty() {
            self.partial = Some(text.to_string());
        }
    }

    pub fn push_final(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref().trim();
        if !text.is_empty() {
            self.final_text = Some(text.to_string());
        }
    }

    pub fn finish(&self) -> Result<String, SpeechError> {
        self.final_text
            .as_ref()
            .or(self.partial.as_ref())
            .cloned()
            .ok_or(SpeechError::EmptyTranscript)
    }
}

#[cfg(test)]
#[path = "../tests/unit/transcript_tests.rs"]
mod tests;
