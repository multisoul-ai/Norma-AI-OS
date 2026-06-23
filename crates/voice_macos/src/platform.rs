use crate::{
    microphone_authorization_from_raw, speech_authorization_from_raw, MicrophoneAuthorization,
    SessionEvent, SessionState, SpeechAuthorization, SpeechError, TranscriptAccumulator,
};
use block2::RcBlock;
use objc2::{rc::Retained, AnyThread};
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
use objc2_avf_audio::{AVAudioEngine, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRunLoop};
use objc2_foundation::{NSError, NSLocale, NSString};
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionRequest, SFSpeechRecognitionResult,
    SFSpeechRecognitionTask, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(60);
const AUTHORIZATION_POLL_INTERVAL: Duration = Duration::from_millis(20);
const FINAL_RESULT_TIMEOUT: Duration = Duration::from_millis(1_500);
const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 8_192;

type ResultBlock = RcBlock<dyn Fn(*mut SFSpeechRecognitionResult, *mut NSError)>;
type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;

pub(crate) struct PlatformSpeechSession {
    state: SessionState,
    engine: Retained<AVAudioEngine>,
    input_node: Retained<AVAudioInputNode>,
    request: Retained<SFSpeechAudioBufferRecognitionRequest>,
    task: Retained<SFSpeechRecognitionTask>,
    result_block: ResultBlock,
    tap_block: TapBlock,
    transcript: Arc<Mutex<TranscriptAccumulator>>,
    finished: Arc<(Mutex<bool>, Condvar)>,
    error: Arc<Mutex<Option<String>>>,
    tap_installed: bool,
}

impl PlatformSpeechSession {
    pub(crate) fn start(locale: String) -> Result<Self, SpeechError> {
        debug_log("requesting speech authorization");
        ensure_speech_authorized()?;
        debug_log("requesting microphone authorization");
        ensure_microphone_authorized()?;
        debug_log("creating recognizer");
        let recognizer = recognizer_for_locale(&locale)?;
        if unsafe { !recognizer.isAvailable() } {
            return Err(SpeechError::RecognizerUnavailable);
        }

        debug_log("creating audio buffer recognition request");
        let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
        unsafe { request.setShouldReportPartialResults(true) };

        let transcript = Arc::new(Mutex::new(TranscriptAccumulator::default()));
        let finished = Arc::new((Mutex::new(false), Condvar::new()));
        let error = Arc::new(Mutex::new(None));
        let result_block = result_handler(transcript.clone(), finished.clone(), error.clone());
        let speech_request: &SFSpeechRecognitionRequest = request.as_ref();
        debug_log("starting speech recognition task");
        let task = unsafe {
            recognizer.recognitionTaskWithRequest_resultHandler(speech_request, &result_block)
        };

        debug_log("creating audio engine");
        let engine = unsafe { AVAudioEngine::new() };
        let input_node = unsafe { engine.inputNode() };
        let format = unsafe { input_node.outputFormatForBus(INPUT_BUS) };
        let tap_block = audio_tap(request.clone());
        debug_log("installing audio tap and starting engine");
        unsafe {
            input_node.installTapOnBus_bufferSize_format_block(
                INPUT_BUS,
                TAP_BUFFER_SIZE,
                Some(&format),
                RcBlock::as_ptr(&tap_block),
            );
            engine.prepare();
            engine
                .startAndReturnError()
                .map_err(|error| SpeechError::Provider(error.localizedDescription().to_string()))?;
        }

        let mut state = SessionState::default();
        state.apply(SessionEvent::Start)?;
        debug_log("speech session started");
        Ok(Self {
            state,
            engine,
            input_node,
            request,
            task,
            result_block,
            tap_block,
            transcript,
            finished,
            error,
            tap_installed: true,
        })
    }

    pub(crate) fn finish(&mut self) -> Result<String, SpeechError> {
        self.state.apply(SessionEvent::Finish)?;
        self.stop_audio();
        unsafe {
            self.request.endAudio();
            self.task.finish();
        }
        wait_for_final(&self.finished);
        if let Some(error) = self.error.lock().map_err(lock_error)?.clone() {
            if self
                .transcript
                .lock()
                .map_err(lock_error)?
                .finish()
                .is_err()
            {
                return Err(SpeechError::Provider(error));
            }
        }
        self.transcript.lock().map_err(lock_error)?.finish()
    }

    pub(crate) fn cancel(&mut self) {
        let _ = self.state.apply(SessionEvent::Cancel);
        self.stop_audio();
        unsafe { self.task.cancel() };
        let (lock, condvar) = &*self.finished;
        if let Ok(mut done) = lock.lock() {
            *done = true;
            condvar.notify_all();
        }
    }

    fn stop_audio(&mut self) {
        if self.tap_installed {
            unsafe { self.input_node.removeTapOnBus(INPUT_BUS) };
            self.tap_installed = false;
        }
        unsafe { self.engine.stop() };
        let _ = &self.result_block;
        let _ = &self.tap_block;
    }
}

impl Drop for PlatformSpeechSession {
    fn drop(&mut self) {
        if self.tap_installed {
            self.cancel();
        }
    }
}

fn result_handler(
    transcript: Arc<Mutex<TranscriptAccumulator>>,
    finished: Arc<(Mutex<bool>, Condvar)>,
    error: Arc<Mutex<Option<String>>>,
) -> ResultBlock {
    RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, ns_error: *mut NSError| {
            if let Some(result) = NonNull::new(result) {
                let result = unsafe { result.as_ref() };
                let text = unsafe { result.bestTranscription().formattedString().to_string() };
                if unsafe { result.isFinal() } {
                    if let Ok(mut transcript) = transcript.lock() {
                        transcript.push_final(text);
                    }
                    signal_finished(&finished);
                } else if let Ok(mut transcript) = transcript.lock() {
                    transcript.push_partial(text);
                }
            }
            if let Some(ns_error) = NonNull::new(ns_error) {
                let message = unsafe { ns_error.as_ref().localizedDescription().to_string() };
                if let Ok(mut error) = error.lock() {
                    *error = Some(message);
                }
                signal_finished(&finished);
            }
        },
    )
}

fn audio_tap(request: Retained<SFSpeechAudioBufferRecognitionRequest>) -> TapBlock {
    RcBlock::new(
        move |buffer: NonNull<AVAudioPCMBuffer>, _: NonNull<AVAudioTime>| {
            unsafe { request.appendAudioPCMBuffer(buffer.as_ref()) };
        },
    )
}

fn ensure_speech_authorized() -> Result<(), SpeechError> {
    debug_log("reading speech authorization status");
    let status = speech_authorization_from_raw(unsafe {
        SFSpeechRecognizer::authorizationStatus().0 as i64
    })
    .ok_or_else(|| SpeechError::Provider("unknown speech authorization status".to_string()))?;
    match status {
        SpeechAuthorization::Authorized => Ok(()),
        SpeechAuthorization::Denied => Err(SpeechError::SpeechDenied),
        SpeechAuthorization::Restricted => Err(SpeechError::SpeechRestricted),
        SpeechAuthorization::NotDetermined => request_speech_authorization(),
    }
}

fn request_speech_authorization() -> Result<(), SpeechError> {
    ensure_permission_prompt_can_run()?;
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
        let _ = tx.send(status.0 as i64);
    });
    debug_log("calling SFSpeechRecognizer::requestAuthorization");
    unsafe { SFSpeechRecognizer::requestAuthorization(&block) };
    debug_log("waiting for speech authorization callback");
    let raw = wait_for_authorization(rx, "speech authorization timed out")?;
    match speech_authorization_from_raw(raw) {
        Some(SpeechAuthorization::Authorized) => Ok(()),
        Some(SpeechAuthorization::Denied) => Err(SpeechError::SpeechDenied),
        Some(SpeechAuthorization::Restricted) => Err(SpeechError::SpeechRestricted),
        Some(SpeechAuthorization::NotDetermined) => Err(SpeechError::Provider(
            "speech authorization was not determined".to_string(),
        )),
        None => Err(SpeechError::Provider(
            "unknown speech authorization status".to_string(),
        )),
    }
}

fn ensure_microphone_authorized() -> Result<(), SpeechError> {
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| SpeechError::Provider("audio media type is unavailable".to_string()))?;
    let status = microphone_authorization_from_raw(unsafe {
        AVCaptureDevice::authorizationStatusForMediaType(media_type).0 as i64
    })
    .ok_or_else(|| SpeechError::Provider("unknown microphone authorization status".to_string()))?;
    match status {
        MicrophoneAuthorization::Authorized => Ok(()),
        MicrophoneAuthorization::Denied => Err(SpeechError::MicrophoneDenied),
        MicrophoneAuthorization::Restricted => Err(SpeechError::MicrophoneRestricted),
        MicrophoneAuthorization::NotDetermined => request_microphone_authorization(media_type),
    }
}

fn request_microphone_authorization(
    media_type: &'static objc2_av_foundation::AVMediaType,
) -> Result<(), SpeechError> {
    ensure_permission_prompt_can_run()?;
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block) };
    if wait_for_authorization(rx, "microphone authorization timed out")? {
        Ok(())
    } else {
        Err(SpeechError::MicrophoneDenied)
    }
}

fn recognizer_for_locale(locale: &str) -> Result<Retained<SFSpeechRecognizer>, SpeechError> {
    let locale_id = NSString::from_str(locale);
    let locale = NSLocale::initWithLocaleIdentifier(NSLocale::alloc(), &locale_id);
    unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &locale) }
        .or_else(|| unsafe { SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) })
        .ok_or(SpeechError::RecognizerUnavailable)
}

fn wait_for_final(finished: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, condvar) = &**finished;
    if let Ok(done) = lock.lock() {
        let _ = condvar.wait_timeout_while(done, FINAL_RESULT_TIMEOUT, |done| !*done);
    }
}

fn signal_finished(finished: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, condvar) = &**finished;
    if let Ok(mut done) = lock.lock() {
        *done = true;
        condvar.notify_all();
    }
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> SpeechError {
    SpeechError::Provider("speech session state lock was poisoned".to_string())
}

fn wait_for_authorization<T>(
    rx: mpsc::Receiver<T>,
    timeout_message: &'static str,
) -> Result<T, SpeechError> {
    let started = Instant::now();
    loop {
        match rx.try_recv() {
            Ok(value) => return Ok(value),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(SpeechError::Provider(
                    "authorization callback channel closed".to_string(),
                ));
            }
            Err(mpsc::TryRecvError::Empty) if started.elapsed() >= AUTHORIZATION_TIMEOUT => {
                return Err(SpeechError::Provider(timeout_message.to_string()));
            }
            Err(mpsc::TryRecvError::Empty) => pump_run_loop_once(),
        }
    }
}

fn pump_run_loop_once() {
    if let Some(mode) = unsafe { kCFRunLoopDefaultMode } {
        CFRunLoop::run_in_mode(Some(mode), AUTHORIZATION_POLL_INTERVAL.as_secs_f64(), true);
    } else {
        std::thread::sleep(AUTHORIZATION_POLL_INTERVAL);
    }
}

fn ensure_permission_prompt_can_run() -> Result<(), SpeechError> {
    let exe = std::env::current_exe().map_err(|error| {
        SpeechError::Provider(format!("could not inspect current executable: {error}"))
    })?;
    if is_inside_app_bundle_path(&exe) {
        Ok(())
    } else {
        Err(SpeechError::Provider(
            "Apple Speech permissions must be requested from the packaged .app; run cargo packager --release for first-time permission setup".to_string(),
        ))
    }
}

fn is_inside_app_bundle_path(path: &Path) -> bool {
    let parts: Vec<_> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect();
    parts.windows(3).any(|window| {
        window[0].ends_with(".app") && window[1] == "Contents" && window[2] == "MacOS"
    })
}

fn debug_log(message: &str) {
    if std::env::var_os("MULTISOUL_SPEECH_DEBUG").is_some() {
        eprintln!("voice_macos: {message}");
    }
}

#[cfg(test)]
#[path = "../tests/unit/platform_tests.rs"]
mod platform_tests;
