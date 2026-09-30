pub mod cloud;
pub mod local;

pub use cloud::*;
pub use local::*;

use crate::AsrProviderCapabilities;
use crate::{AsrError, AvailableAsrModel, RecognizerFactory};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsrProviderId {
    LocalSherpa,
    CloudAzure,
}

impl AsrProviderId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalSherpa => "local_sherpa",
            Self::CloudAzure => "cloud_azure",
        }
    }

    pub fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local_sherpa" => Some(Self::LocalSherpa),
            "cloud_azure" => Some(Self::CloudAzure),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsrProviderLocation {
    Local,
    Cloud,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsrProviderDescriptor {
    id: AsrProviderId,
    display_name: &'static str,
    location: AsrProviderLocation,
    capabilities: AsrProviderCapabilities,
}

impl AsrProviderDescriptor {
    pub const fn id(self) -> AsrProviderId {
        self.id
    }

    pub const fn display_name(self) -> &'static str {
        self.display_name
    }

    pub const fn location(self) -> AsrProviderLocation {
        self.location
    }

    pub const fn capabilities(self) -> AsrProviderCapabilities {
        self.capabilities
    }
}

const BUILTIN_PROVIDERS: [AsrProviderDescriptor; 2] = [
    AsrProviderDescriptor {
        id: AsrProviderId::LocalSherpa,
        display_name: "On-device (Sherpa)",
        location: AsrProviderLocation::Local,
        capabilities: AsrProviderCapabilities::new(true, None),
    },
    AsrProviderDescriptor {
        id: AsrProviderId::CloudAzure,
        display_name: "Azure Speech",
        location: AsrProviderLocation::Cloud,
        capabilities: AsrProviderCapabilities::new(false, Some(60)),
    },
];

pub fn builtin_asr_providers() -> &'static [AsrProviderDescriptor] {
    &BUILTIN_PROVIDERS
}

pub fn asr_provider_descriptor(id: AsrProviderId) -> &'static AsrProviderDescriptor {
    BUILTIN_PROVIDERS
        .iter()
        .find(|descriptor| descriptor.id == id)
        .expect("every typed ASR Provider ID must have a built-in descriptor")
}

pub enum BuiltinAsrProfile {
    LocalSherpa(AvailableAsrModel),
    CloudAzure(AzureSpeechProfile),
}

pub fn builtin_recognizer_factory(
    profile: BuiltinAsrProfile,
    resolve_credential: impl FnOnce(&str) -> Result<Option<String>, AsrError>,
) -> Result<Box<dyn RecognizerFactory>, AsrError> {
    match profile {
        BuiltinAsrProfile::LocalSherpa(model) => Ok(Box::new(SherpaRecognizerFactory::new(model))),
        BuiltinAsrProfile::CloudAzure(profile) => {
            let secret = resolve_credential(&profile.credential_ref)?.ok_or_else(|| {
                AsrError::Provider("Azure Speech API Key is missing from local storage".to_string())
            })?;
            let config = AzureSpeechConfig::new(
                profile.region,
                profile.locale,
                AzureSpeechSecret::new(secret),
            )?;
            Ok(Box::new(AzureSpeechRecognizerFactory::new(config)))
        }
    }
}
