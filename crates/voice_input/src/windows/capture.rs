//! Shared-mode WASAPI capture. All COM interfaces stay on their owning worker.
use super::{audio_error, effects::EchoCancellation, Apartment};
use crate::{AudioQueueStats, PcmEvent, VoiceInputError};
use ::windows::{
    core::HSTRING,
    Win32::{
        Foundation::E_ACCESSDENIED,
        Media::{
            Audio::{
                AudioCategory_Communications, AudioCategory_Speech, AudioClientProperties,
                IAudioCaptureClient, IAudioClient2, IMMDeviceEnumerator, MMDeviceEnumerator,
                AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                WAVEFORMATEX,
            },
            Multimedia::WAVE_FORMAT_IEEE_FLOAT,
        },
        System::Com::{CoCreateInstance, CLSCTX_ALL},
    },
};
use crossbeam_channel::Sender;
use std::{
    sync::{mpsc, Arc},
    thread::JoinHandle,
    time::Duration,
};

pub(crate) struct WindowsAudioCapture {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<Result<(), VoiceInputError>>>,
}

impl WindowsAudioCapture {
    pub(crate) fn start(
        sender: Sender<PcmEvent>,
        stats: Arc<AudioQueueStats>,
        selected: Option<&str>,
        rate: u32,
        voice_processing: bool,
    ) -> Result<Self, VoiceInputError> {
        if rate == 0 || rate > 384_000 {
            return Err(VoiceInputError::Audio(format!(
                "invalid capture sample rate: {rate}"
            )));
        }
        let selected = selected.map(str::to_owned);
        let (stop, stopping) = mpsc::channel();
        let (started, ready) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("voice_input_wasapi".into())
            .spawn(move || {
                let mut capture =
                    match CaptureClient::open(selected.as_deref(), rate, voice_processing) {
                        Ok(capture) => capture,
                        Err(error) => {
                            let _ = started.send(Err(error));
                            return Ok(());
                        }
                    };
                if started.send(Ok(())).is_err() {
                    return capture.stop();
                }
                let result = capture.run(&stopping, &sender, &stats);
                if let Err(error) = &result {
                    tracing::error!(%error, "Windows microphone capture failed");
                }
                result
            })
            .map_err(|error| {
                VoiceInputError::Audio(format!("could not spawn Windows audio capture: {error}"))
            })?;
        let mut capture = Self {
            stop,
            worker: Some(worker),
        };
        match ready.recv() {
            Ok(Ok(())) => Ok(capture),
            Ok(Err(error)) => {
                let _ = capture.finish();
                Err(error)
            }
            Err(_) => capture.finish().and(Err(VoiceInputError::Audio(
                "Windows capture startup closed before reporting status".into(),
            ))),
        }
    }

    pub(crate) fn stop(mut self) -> Result<(), VoiceInputError> {
        self.finish()
    }

    fn finish(&mut self) -> Result<(), VoiceInputError> {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| VoiceInputError::Audio("Windows capture worker panicked".into()))??;
        }
        Ok(())
    }
}

impl Drop for WindowsAudioCapture {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            tracing::error!(%error, "Windows capture cleanup failed");
        }
    }
}

struct CaptureClient {
    input: IAudioCaptureClient,
    client: IAudioClient2,
    aec: Option<EchoCancellation>,
    frames: u32,
    started: bool,
    _apartment: Apartment,
}

impl CaptureClient {
    fn open(
        selected: Option<&str>,
        rate: u32,
        voice_processing: bool,
    ) -> Result<Self, VoiceInputError> {
        let apartment = Apartment::new()?;
        let devices = crate::list_audio_input_devices()?;
        let selected = crate::resolve_audio_input_device_selection(selected, &devices)?
            .ok_or_else(|| {
                VoiceInputError::Audio("no active microphone endpoint is available".into())
            })?;
        crate::ensure_microphone_authorized()?;
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(audio_error)?;
            let device = enumerator
                .GetDevice(&HSTRING::from(selected.id))
                .map_err(audio_error)?;
            let client: IAudioClient2 = device.Activate(CLSCTX_ALL, None).map_err(capture_error)?;
            client
                .SetClientProperties(&AudioClientProperties {
                    cbSize: std::mem::size_of::<AudioClientProperties>() as u32,
                    eCategory: if voice_processing {
                        AudioCategory_Communications
                    } else {
                        AudioCategory_Speech
                    },
                    ..Default::default()
                })
                .map_err(capture_error)?;
            // The shared audio engine converts the selected endpoint's native rate
            // and channel layout to mono f32, including state across packet boundaries.
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
                nChannels: 1,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * 4,
                nBlockAlign: 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    1_000_000, // 100 ms in 100 ns units; drain every 10 ms below.
                    0,
                    &format,
                    None,
                )
                .map_err(capture_error)?;
            let aec = if voice_processing {
                Some(EchoCancellation::enable(&client)?)
            } else {
                None
            };
            let input = client.GetService().map_err(capture_error)?;
            let frames = client.GetBufferSize().map_err(capture_error)?;
            client.Start().map_err(capture_error)?;
            Ok(Self {
                input,
                client,
                aec,
                frames,
                started: true,
                _apartment: apartment,
            })
        }
    }

    fn run(
        &mut self,
        stopping: &mpsc::Receiver<()>,
        sender: &Sender<PcmEvent>,
        stats: &AudioQueueStats,
    ) -> Result<(), VoiceInputError> {
        'capture: loop {
            match stopping.recv_timeout(Duration::from_millis(10)) {
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            while unsafe { self.input.GetNextPacketSize() }.map_err(capture_error)? > 0 {
                if !matches!(stopping.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                    break 'capture;
                }
                let samples = self.read_packet()?;
                if let Some(aec) = &self.aec {
                    aec.check()?;
                }
                if samples.is_empty() {
                    continue;
                }
                match sender.try_send(PcmEvent::Samples(samples)) {
                    Ok(()) => stats.record_sent_chunk(),
                    Err(crossbeam_channel::TrySendError::Full(_)) => stats.record_dropped_chunk(),
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => break 'capture,
                }
            }
        }
        self.stop()
    }

    fn read_packet(&self) -> Result<Vec<f32>, VoiceInputError> {
        let mut data = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        unsafe {
            self.input
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
        }
        .map_err(capture_error)?;
        let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
        let bytes = if frames <= self.frames && frames != 0 && !data.is_null() && !silent {
            // WASAPI owns this memory until ReleaseBuffer. Bound the slice by the
            // negotiated buffer size; do not assume f32 alignment of its pointer.
            Some(unsafe { std::slice::from_raw_parts(data, frames as usize * 4) })
        } else {
            None
        };
        let samples = decode_packet(bytes, frames, self.frames, silent);
        let released = unsafe { self.input.ReleaseBuffer(frames) }.map_err(capture_error);
        let samples = samples?;
        released?;
        Ok(samples)
    }

    fn stop(&mut self) -> Result<(), VoiceInputError> {
        if self.started {
            unsafe { self.client.Stop() }.map_err(capture_error)?;
            self.started = false;
        }
        Ok(())
    }
}

impl Drop for CaptureClient {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            tracing::error!(%error, "WASAPI stream stop failed");
        }
    }
}

fn capture_error(error: ::windows::core::Error) -> VoiceInputError {
    if error.code() == E_ACCESSDENIED {
        VoiceInputError::MicrophoneDenied
    } else {
        audio_error(error)
    }
}

fn decode_packet(
    bytes: Option<&[u8]>,
    frames: u32,
    capacity: u32,
    silent: bool,
) -> Result<Vec<f32>, VoiceInputError> {
    let invalid = || VoiceInputError::Audio("Windows capture returned an invalid packet".into());
    if frames > capacity {
        return Err(invalid());
    }
    if frames == 0 || silent {
        return Ok(vec![0.0; frames as usize]);
    }
    let bytes = bytes
        .filter(|bytes| bytes.len() == frames as usize * 4)
        .ok_or_else(invalid)?;
    Ok(bytes
        .chunks_exact(4)
        .map(|bytes| f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect())
}

#[cfg(test)]
#[path = "../../tests/unit/windows_capture_tests.rs"]
mod tests;
