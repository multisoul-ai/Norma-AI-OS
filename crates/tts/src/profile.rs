use crate::{TtsModelId, TtsProviderId, TtsVoiceId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSelectionProvenance {
    Catalog,
    Manual,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TtsProviderProfile {
    provider_id: TtsProviderId,
    model_id: TtsModelId,
    voice_id: TtsVoiceId,
    voice_provenance: VoiceSelectionProvenance,
}

impl TtsProviderProfile {
    pub fn new(
        provider_id: TtsProviderId,
        model_id: TtsModelId,
        voice_id: TtsVoiceId,
        voice_provenance: VoiceSelectionProvenance,
    ) -> Self {
        Self {
            provider_id,
            model_id,
            voice_id,
            voice_provenance,
        }
    }

    pub fn provider_id(&self) -> &TtsProviderId {
        &self.provider_id
    }

    pub fn model_id(&self) -> &TtsModelId {
        &self.model_id
    }

    pub fn voice_id(&self) -> &TtsVoiceId {
        &self.voice_id
    }

    pub fn voice_provenance(&self) -> VoiceSelectionProvenance {
        self.voice_provenance
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteAccountStatus {
    Unchecked,
    Checking,
    Ready,
    TemporarilyUnavailable,
    InvalidCredential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteTtsReadiness {
    Unconfigured,
    Configured,
    Checking,
    Ready,
    TemporarilyUnavailable,
    InvalidCredential,
    VoiceUnavailable,
    VoiceValidationRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TtsValidatedSelection {
    credential_ref: String,
    model_id: TtsModelId,
    voice_id: TtsVoiceId,
}

impl TtsValidatedSelection {
    pub fn from_profile(credential_ref: impl Into<String>, profile: &TtsProviderProfile) -> Self {
        Self {
            credential_ref: credential_ref.into(),
            model_id: profile.model_id().clone(),
            voice_id: profile.voice_id().clone(),
        }
    }

    fn matches(&self, credential_ref: &str, profile: &TtsProviderProfile) -> bool {
        self.credential_ref == credential_ref
            && self.model_id == *profile.model_id()
            && self.voice_id == *profile.voice_id()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteTtsValidation {
    account_status: RemoteAccountStatus,
    credential_ref: String,
    catalog_voice_ids: Vec<TtsVoiceId>,
    manually_validated: Option<TtsValidatedSelection>,
}

impl RemoteTtsValidation {
    pub fn new(
        account_status: RemoteAccountStatus,
        credential_ref: impl Into<String>,
        catalog_voice_ids: Vec<TtsVoiceId>,
        manually_validated: Option<TtsValidatedSelection>,
    ) -> Self {
        Self {
            account_status,
            credential_ref: credential_ref.into(),
            catalog_voice_ids,
            manually_validated,
        }
    }
}

pub fn evaluate_remote_readiness(
    profile: Option<&TtsProviderProfile>,
    validation: &RemoteTtsValidation,
) -> RemoteTtsReadiness {
    let Some(profile) = profile else {
        return RemoteTtsReadiness::Unconfigured;
    };
    match validation.account_status {
        RemoteAccountStatus::Unchecked => RemoteTtsReadiness::Configured,
        RemoteAccountStatus::Checking => RemoteTtsReadiness::Checking,
        RemoteAccountStatus::TemporarilyUnavailable => RemoteTtsReadiness::TemporarilyUnavailable,
        RemoteAccountStatus::InvalidCredential => RemoteTtsReadiness::InvalidCredential,
        RemoteAccountStatus::Ready => {
            if validation
                .catalog_voice_ids
                .iter()
                .any(|voice_id| voice_id == profile.voice_id())
            {
                return RemoteTtsReadiness::Ready;
            }
            if validation
                .manually_validated
                .as_ref()
                .is_some_and(|proof| proof.matches(&validation.credential_ref, profile))
            {
                return RemoteTtsReadiness::Ready;
            }
            match profile.voice_provenance() {
                VoiceSelectionProvenance::Catalog => RemoteTtsReadiness::VoiceUnavailable,
                VoiceSelectionProvenance::Manual => RemoteTtsReadiness::VoiceValidationRequired,
            }
        }
    }
}
