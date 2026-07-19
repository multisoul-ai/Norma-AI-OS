use crate::ZIPVOICE_DISTILL_INT8_MODEL_ID;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const REFERENCE_WAV: &str = "reference.wav";
const REFERENCE_TRANSCRIPT: &str = "reference.txt";
const REQUIRED_SAMPLE_RATE_HZ: u32 = 16_000;
const REQUIRED_CHANNELS: u16 = 1;
const REQUIRED_BITS_PER_SAMPLE: u16 = 16;
const MIN_DURATION_SECONDS: usize = 3;
const MAX_DURATION_SECONDS: usize = 15;
const STAGING_SUFFIX: &str = ".importing";
const BACKUP_SUFFIX: &str = ".backup";
const REMOVING_SUFFIX: &str = ".removing";

static REFERENCE_IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq)]
pub struct ZipVoiceReference {
    audio_path: PathBuf,
    transcript: String,
    samples: Vec<f32>,
    sample_rate_hz: u32,
}

impl ZipVoiceReference {
    pub fn audio_path(&self) -> &Path {
        &self.audio_path
    }

    pub fn transcript(&self) -> &str {
        &self.transcript
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ZipVoiceReferenceError {
    #[error("reference transcript must not be empty")]
    EmptyTranscript,
    #[error("ZipVoice reference is not configured at {0}")]
    NotConfigured(PathBuf),
    #[error("failed to access ZipVoice reference at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to decode ZipVoice reference WAV at {path}: {source}")]
    Wav {
        path: PathBuf,
        #[source]
        source: hound::Error,
    },
    #[error(
        "unsupported reference WAV format: expected 16 kHz mono 16-bit PCM, got {sample_rate_hz} Hz, {channels} channels, {bits_per_sample}-bit {sample_format}"
    )]
    UnsupportedFormat {
        sample_rate_hz: u32,
        channels: u16,
        bits_per_sample: u16,
        sample_format: &'static str,
    },
    #[error("reference WAV duration must be 3-15 seconds, got {duration_millis} ms")]
    UnsupportedDuration { duration_millis: u64 },
}

pub fn zipvoice_reference_root(model_root: &Path) -> PathBuf {
    model_root
        .join("voice-references")
        .join(ZIPVOICE_DISTILL_INT8_MODEL_ID)
}

pub fn import_zipvoice_reference(
    source: &Path,
    transcript: &str,
    model_root: &Path,
) -> Result<ZipVoiceReference, ZipVoiceReferenceError> {
    let transcript = normalized_transcript(transcript)?;
    let decoded = decode_reference(source, transcript.clone())?;
    let _guard = lock_reference_io();
    let root = zipvoice_reference_root(model_root);
    recover_interrupted_commit(&root)?;
    let staging = sibling_with_suffix(&root, STAGING_SUFFIX);
    let backup = sibling_with_suffix(&root, BACKUP_SUFFIX);
    remove_directory_if_present(&staging)?;
    remove_directory_if_present(&backup)?;
    fs::create_dir_all(&staging).map_err(|source| io_error(&staging, source))?;
    let staging_audio = staging.join(REFERENCE_WAV);
    let staging_transcript = staging.join(REFERENCE_TRANSCRIPT);
    if let Err(error) = fs::copy(source, &staging_audio)
        .map_err(|source| io_error(&staging_audio, source))
        .and_then(|_| {
            fs::write(&staging_transcript, transcript.as_bytes())
                .map_err(|source| io_error(&staging_transcript, source))
        })
    {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if root.exists() {
        fs::rename(&root, &backup).map_err(|source| io_error(&backup, source))?;
    }
    if let Err(source) = fs::rename(&staging, &root) {
        if backup.exists() && !root.exists() {
            let _ = fs::rename(&backup, &root);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(io_error(&root, source));
    }
    let _ = remove_directory_if_present(&backup);
    let audio_path = root.join(REFERENCE_WAV);
    Ok(ZipVoiceReference {
        audio_path,
        transcript,
        samples: decoded.samples,
        sample_rate_hz: decoded.sample_rate_hz,
    })
}

pub fn load_zipvoice_reference(
    model_root: &Path,
) -> Result<ZipVoiceReference, ZipVoiceReferenceError> {
    let _guard = lock_reference_io();
    let root = zipvoice_reference_root(model_root);
    recover_interrupted_commit(&root)?;
    let audio_path = root.join(REFERENCE_WAV);
    let transcript_path = root.join(REFERENCE_TRANSCRIPT);
    if !audio_path.is_file() || !transcript_path.is_file() {
        return Err(ZipVoiceReferenceError::NotConfigured(root));
    }
    let transcript = fs::read_to_string(&transcript_path)
        .map_err(|source| io_error(&transcript_path, source))?;
    decode_reference(&audio_path, normalized_transcript(&transcript)?)
}

pub fn remove_zipvoice_reference(model_root: &Path) -> Result<(), ZipVoiceReferenceError> {
    let _guard = lock_reference_io();
    let root = zipvoice_reference_root(model_root);
    recover_interrupted_commit(&root)?;
    let removing = sibling_with_suffix(&root, REMOVING_SUFFIX);
    remove_directory_if_present(&removing)?;
    if root.exists() {
        fs::rename(&root, &removing).map_err(|source| io_error(&removing, source))?;
    }
    let _ = remove_directory_if_present(&removing);
    let _ = remove_directory_if_present(&sibling_with_suffix(&root, STAGING_SUFFIX));
    let _ = remove_directory_if_present(&sibling_with_suffix(&root, BACKUP_SUFFIX));
    Ok(())
}

fn normalized_transcript(transcript: &str) -> Result<String, ZipVoiceReferenceError> {
    let transcript = transcript.trim();
    if transcript.is_empty() {
        Err(ZipVoiceReferenceError::EmptyTranscript)
    } else {
        Ok(transcript.to_string())
    }
}

fn decode_reference(
    path: &Path,
    transcript: String,
) -> Result<ZipVoiceReference, ZipVoiceReferenceError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|source| ZipVoiceReferenceError::Wav {
            path: path.to_path_buf(),
            source,
        })?;
    let spec = reader.spec();
    if spec.sample_rate != REQUIRED_SAMPLE_RATE_HZ
        || spec.channels != REQUIRED_CHANNELS
        || spec.bits_per_sample != REQUIRED_BITS_PER_SAMPLE
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(ZipVoiceReferenceError::UnsupportedFormat {
            sample_rate_hz: spec.sample_rate,
            channels: spec.channels,
            bits_per_sample: spec.bits_per_sample,
            sample_format: match spec.sample_format {
                hound::SampleFormat::Int => "PCM",
                hound::SampleFormat::Float => "float",
            },
        });
    }
    let declared_samples = reader.duration() as usize;
    validate_duration(declared_samples)?;
    let samples = reader
        .samples::<i16>()
        .map(|sample| {
            sample
                .map(|value| f32::from(value) / 32_768.0)
                .map_err(|source| ZipVoiceReferenceError::Wav {
                    path: path.to_path_buf(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_duration(samples.len())?;
    Ok(ZipVoiceReference {
        audio_path: path.to_path_buf(),
        transcript,
        samples,
        sample_rate_hz: REQUIRED_SAMPLE_RATE_HZ,
    })
}

fn validate_duration(samples: usize) -> Result<(), ZipVoiceReferenceError> {
    let min_samples = REQUIRED_SAMPLE_RATE_HZ as usize * MIN_DURATION_SECONDS;
    let max_samples = REQUIRED_SAMPLE_RATE_HZ as usize * MAX_DURATION_SECONDS;
    if !(min_samples..=max_samples).contains(&samples) {
        let duration_millis = samples as u64 * 1_000 / u64::from(REQUIRED_SAMPLE_RATE_HZ);
        return Err(ZipVoiceReferenceError::UnsupportedDuration { duration_millis });
    }
    Ok(())
}

fn lock_reference_io() -> MutexGuard<'static, ()> {
    REFERENCE_IO_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn sibling_with_suffix(root: &Path, suffix: &str) -> PathBuf {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(ZIPVOICE_DISTILL_INT8_MODEL_ID);
    root.with_file_name(format!("{name}{suffix}"))
}

fn recover_interrupted_commit(root: &Path) -> Result<(), ZipVoiceReferenceError> {
    let removing = sibling_with_suffix(root, REMOVING_SUFFIX);
    let _ = remove_directory_if_present(&removing);
    let backup = sibling_with_suffix(root, BACKUP_SUFFIX);
    if !root.exists() && backup.exists() {
        fs::rename(&backup, root).map_err(|source| io_error(root, source))?;
    } else if root.exists() && backup.exists() {
        remove_directory_if_present(&backup)?;
    }
    Ok(())
}

fn remove_directory_if_present(path: &Path) -> Result<(), ZipVoiceReferenceError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error(path, source)),
    }
}

fn io_error(path: &Path, source: std::io::Error) -> ZipVoiceReferenceError {
    ZipVoiceReferenceError::Io {
        path: path.to_path_buf(),
        source,
    }
}
