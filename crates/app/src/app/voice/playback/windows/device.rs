//! WASAPI objects are created, used and released only by the playback worker.
use super::OutputDevice;
use ::windows::{
    core::HRESULT,
    Win32::{
        Foundation::ERROR_NOT_FOUND,
        Media::Audio::{
            eCommunications, eRender, AudioCategory_Communications, AudioClientProperties,
            IAudioClient2, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, WAVEFORMATEX,
        },
        System::{
            Com::{CoCreateInstance, CLSCTX_ALL},
            WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        },
    },
};

pub(super) struct Apartment;
impl Apartment {
    pub(super) fn new() -> Result<Self, String> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(audio_error)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub(super) struct NativeOutput {
    render: IAudioRenderClient,
    client: IAudioClient2,
    pub(super) rate: u32,
    pub(super) frames: u32,
    pub(super) written: u64,
    started: bool,
}

impl NativeOutput {
    pub(super) fn open(rate: u32) -> Result<Self, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(audio_error)?;
            let endpoint = match enumerator.GetDefaultAudioEndpoint(eRender, eCommunications) {
                Ok(endpoint) => endpoint,
                Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => {
                    let endpoints = enumerator
                        .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                        .map_err(audio_error)?;
                    if endpoints.GetCount().map_err(audio_error)? == 0 {
                        return Err("Windows audio: no active output endpoint is available".into());
                    }
                    endpoints.Item(0).map_err(audio_error)?
                }
                Err(error) => return Err(audio_error(error)),
            };
            let client: IAudioClient2 = endpoint.Activate(CLSCTX_ALL, None).map_err(audio_error)?;
            client
                .SetClientProperties(&AudioClientProperties {
                    cbSize: std::mem::size_of::<AudioClientProperties>() as u32,
                    eCategory: AudioCategory_Communications,
                    ..Default::default()
                })
                .map_err(audio_error)?;
            let format = WAVEFORMATEX {
                wFormatTag: 1, // WAVE_FORMAT_PCM
                nChannels: 1,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    1_000_000,
                    0,
                    &format,
                    None,
                )
                .map_err(audio_error)?;
            let frames = client.GetBufferSize().map_err(audio_error)?;
            if frames == 0 {
                return Err("Windows audio: output buffer has no frames".into());
            }
            let render = client.GetService().map_err(audio_error)?;
            Ok(Self {
                render,
                client,
                rate,
                frames,
                written: 0,
                started: false,
            })
        }
    }

    pub(super) fn padding(&self) -> Result<u32, String> {
        let padding = unsafe { self.client.GetCurrentPadding() }.map_err(audio_error)?;
        if padding > self.frames || u64::from(padding) > self.written {
            return Err("Windows audio: invalid output buffer padding".into());
        }
        Ok(padding)
    }

    pub(super) fn write(&mut self, bytes: &[u8], frames: u32) -> Result<(), String> {
        if frames == 0 || bytes.len() != frames as usize * 2 {
            return Err("Windows audio: invalid output frame buffer".into());
        }
        unsafe {
            let destination = self.render.GetBuffer(frames).map_err(audio_error)?;
            if destination.is_null() {
                let _ = self.render.ReleaseBuffer(0, 0);
                return Err("Windows audio: output buffer has no memory".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
            self.render.ReleaseBuffer(frames, 0).map_err(audio_error)?;
            if !self.started {
                self.client.Start().map_err(audio_error)?;
                self.started = true;
            }
        }
        self.written += u64::from(frames);
        Ok(())
    }

    pub(super) fn stop(&mut self) -> Result<(), String> {
        if self.started {
            unsafe { self.client.Stop() }.map_err(audio_error)?;
            self.started = false;
        }
        unsafe { self.client.Reset() }.map_err(audio_error)?;
        self.written = 0;
        Ok(())
    }
}

impl Drop for NativeOutput {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            tracing::warn!(%error, "Windows output cleanup failed");
        }
    }
}

impl OutputDevice for NativeOutput {
    fn rate(&self) -> u32 {
        self.rate
    }
    fn capacity(&self) -> u32 {
        self.frames
    }
    fn frames_written(&self) -> u64 {
        self.written
    }
    fn padding(&self) -> Result<u32, String> {
        NativeOutput::padding(self)
    }
    fn write(&mut self, bytes: &[u8], frames: u32) -> Result<(), String> {
        NativeOutput::write(self, bytes, frames)
    }
    fn stop(&mut self) -> Result<(), String> {
        NativeOutput::stop(self)
    }
}

fn audio_error(error: ::windows::core::Error) -> String {
    format!("Windows audio: {error}")
}
