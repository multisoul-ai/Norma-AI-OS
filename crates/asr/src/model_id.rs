use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AsrModelId(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AsrModelIdError {
    #[error("ASR model id must not be blank")]
    Blank,
}

impl AsrModelId {
    pub fn parse(value: impl Into<String>) -> Result<Self, AsrModelIdError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(AsrModelIdError::Blank);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AsrModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}
