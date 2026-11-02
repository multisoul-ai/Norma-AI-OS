//! Confirm the actual system AEC effect, rather than assuming a category enables it.
use super::audio_error;
use crate::VoiceInputError;
use ::windows::Win32::{
    Media::{
        Audio::{IAudioClient2, IAudioEffectsManager, AUDIO_EFFECT, AUDIO_EFFECT_STATE_ON},
        KernelStreaming::AUDIO_EFFECT_TYPE_ACOUSTIC_ECHO_CANCELLATION,
    },
    System::Com::CoTaskMemFree,
};

pub(super) struct EchoCancellation(IAudioEffectsManager);

impl EchoCancellation {
    pub(super) fn enable(client: &IAudioClient2) -> Result<Self, VoiceInputError> {
        let manager = Self(unsafe { client.GetService() }.map_err(audio_error)?);
        let effect = manager.effect()?;
        if effect.state != AUDIO_EFFECT_STATE_ON && effect.canSetState.as_bool() {
            unsafe {
                manager
                    .0
                    .SetAudioEffectState(effect.id, AUDIO_EFFECT_STATE_ON)
            }
            .map_err(audio_error)?;
        }
        manager.check()?;
        Ok(manager)
    }

    pub(super) fn check(&self) -> Result<(), VoiceInputError> {
        if self.effect()?.state != AUDIO_EFFECT_STATE_ON {
            return Err(unavailable());
        }
        Ok(())
    }

    fn effect(&self) -> Result<AUDIO_EFFECT, VoiceInputError> {
        let mut data = std::ptr::null_mut();
        let mut count = 0;
        unsafe { self.0.GetAudioEffects(&mut data, &mut count) }.map_err(audio_error)?;
        // A zero-length list may be null; never form a slice from a null pointer.
        let effect = if data.is_null() || count == 0 {
            None
        } else {
            unsafe { std::slice::from_raw_parts(data, count as usize) }
                .iter()
                .find(|effect| effect.id == AUDIO_EFFECT_TYPE_ACOUSTIC_ECHO_CANCELLATION)
                .copied()
        };
        unsafe { CoTaskMemFree(Some(data.cast())) };
        effect.ok_or_else(unavailable)
    }
}

fn unavailable() -> VoiceInputError {
    VoiceInputError::Audio(
        "acoustic echo cancellation is unavailable on the selected microphone".into(),
    )
}
