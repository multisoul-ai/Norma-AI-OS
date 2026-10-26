use audio::PcmAudioChunk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CaptureMode {
    pub generation: u64,
    pub muted: bool,
}

pub(super) enum CaptureEvent {
    Stopped {
        generation: u64,
    },
    Ready {
        generation: u64,
    },
    Pcm {
        generation: u64,
        chunk: PcmAudioChunk,
    },
    Failed {
        generation: u64,
        message: String,
    },
}

pub(super) enum CallCommand {
    SetMuted(bool),
    SetInputEnabled {
        enabled: bool,
        revision: u64,
    },
    SetOutputBlocked(bool),
    AppendResult {
        event_id: String,
        kind: live_voice::AppendKind,
        delegation_id: Option<String>,
        content: String,
    },
}

#[derive(Debug)]
pub(super) enum CallEvent {
    InputReleased(u64),
    ToolsUnavailable(String),
    VadReady(bool),
    AppendAcknowledged(String),
    OutputBlocked(bool),
    Connected,
    Transcript(live_voice::TranscriptFragment),
    Usage(live_voice::LiveUsage),
    // Local upload gate changed; this is not a service ACK or a hardware shutdown receipt.
    Muted(bool),
    Delegation(live_voice::DelegationNotice),
    SpeechActivity(voice_input::VadObservation),
}

pub(super) struct CaptureInput {
    pub captured: async_channel::Receiver<CaptureEvent>,
    pub capture_mode: tokio::sync::watch::Sender<CaptureMode>,
    pub vad: Option<voice_input::SherpaVadClient>,
}

pub(super) struct CallEnd {
    pub usage: live_voice::LiveUsage,
    pub error: Option<String>,
}

#[derive(Debug)]
enum LoopError {
    Live(live_voice::LiveError),
    Other(String),
}
impl LoopError {
    fn message(self) -> String {
        match self {
            Self::Live(error) => error.to_string(),
            Self::Other(message) => message,
        }
    }
}
impl From<live_voice::LiveError> for LoopError {
    fn from(error: live_voice::LiveError) -> Self {
        Self::Live(error)
    }
}
impl From<String> for LoopError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}
impl From<&str> for LoopError {
    fn from(message: &str) -> Self {
        Self::Other(message.into())
    }
}

fn local_call_error(
    result: Result<(), LoopError>,
    requested_close: bool,
    outcome: &live_voice::LiveOutcome,
) -> Option<String> {
    if requested_close
        && outcome.usage.finalized
        && outcome.error.is_none()
        && matches!(
            &result,
            Err(LoopError::Live(live_voice::LiveError::NotReady))
        )
    {
        // A ready control branch can race the transport entering Closing on another thread.
        None
    } else {
        result.err().map(LoopError::message)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_audio_loop_tests.rs"]
mod tests;

pub(super) async fn run_audio_loop(
    mut connection: live_voice::LiveConnection,
    capture: CaptureInput,
    playback: crate::app::doubao::playback::PlaybackQueue,
    commands: async_channel::Receiver<CallCommand>,
    report_input: async_channel::Receiver<super::native_report::NativeReport>,
    events: async_channel::Sender<CallEvent>,
    cancellation: tokio_util::sync::CancellationToken,
) -> CallEnd {
    let control = connection.control();
    let _ = events.try_send(CallEvent::VadReady(capture.vad.is_some()));
    let CaptureInput {
        captured,
        capture_mode,
        mut vad,
    } = capture;
    let uploaded = control.observe_uploaded_audio();
    let mut microphone = std::collections::VecDeque::new();
    let mut output_blocked = false;
    let mut reports = super::native_report::NativeReports::default();
    let mut report_input_open = true;
    let mut mode = *capture_mode.borrow();
    let mut input_enabled = !mode.muted;
    let mut user_muted = false;
    let mut draining_generations = std::collections::BTreeSet::new();
    let mut release_pending = None;
    let mut mute_sequence = 0_u64;
    let mut submitted_samples = 0_u64;
    let mut uploaded_samples = 0_u64;
    let mut last_real_sample = None;
    let mut clock = tokio::time::interval(std::time::Duration::from_millis(20));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let send_event = |event| {
        events
            .try_send(event)
            .map_err(|_| "Live UI event queue is full or closed".to_owned())
    };
    let result: Result<(), LoopError> = async {
        let mut uploaded = uploaded?;
        if mode.muted { send_event(CallEvent::Connected)?; }
        loop {
            // Later protocol silence must not delay the fence for the last real microphone frame.
            if draining_generations.is_empty() && microphone.is_empty() && last_real_sample.is_none_or(|last| uploaded_samples >= last) {
                if let Some(revision) = release_pending.take() { send_event(CallEvent::InputReleased(revision))?; }
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok(()),
                command = commands.recv() => {
                    let muted = match command.map_err(|_| "Live call controls closed")? {
                        CallCommand::SetMuted(muted) => {
                            user_muted = muted;
                            mute_sequence += 1;
                            control.send_command(&live_voice::LiveCommand::Mute { event_id: format!("desktop-mute-{mute_sequence}"), muted })?;
                            submitted_samples = 0;
                            uploaded_samples = 0;
                            last_real_sample = None;
                            if muted { microphone.clear(); draining_generations.clear(); }
                            send_event(CallEvent::Muted(muted))?;
                            user_muted || !input_enabled
                        }
                        CallCommand::SetInputEnabled { enabled, revision } => {
                            release_pending = (!enabled).then_some(revision);
                            if !enabled && !mode.muted { draining_generations.insert(mode.generation); }
                            input_enabled = enabled;
                            user_muted || !input_enabled
                        }
                        CallCommand::SetOutputBlocked(blocked) => {
                            output_blocked = blocked;
                            if blocked {
                                reports.clear();
                                // Controls have priority over the separate report ingress.
                                // Discard backlog here, before a queued unblock can run.
                                while let Ok(report) = report_input.try_recv() { drop(report); }
                            }
                            super::output::set_blocked(blocked, &playback, &control, &events)?;
                            continue;
                        }
                        CallCommand::AppendResult{event_id,kind,delegation_id,content}=> {
                            control.send_command(&live_voice::LiveCommand::Append{event_id,kind,delegation_id,content})?;
                            continue;
                        }
                    };
                    if muted == mode.muted { continue; }
                    // Closing a hold keeps the stream generation until its captured tail is drained.
                    let generation = mode.generation + u64::from(!muted || user_muted);
                    mode = CaptureMode { generation, muted };
                    capture_mode.send_replace(mode);
                }
                _ = clock.tick() => {
                    reports.pump(|command, scope| control.send_scoped_append(command, scope))?;
                    if user_muted || microphone.is_empty() { control.send_silence(320)?; submitted_samples += 320; }
                    else {
                        let mut bytes = vec![0;640];
                        for byte in &mut bytes { if let Some(sample_byte) = microphone.pop_front() { *byte = sample_byte; } else { break; } }
                        let pcm = PcmAudioChunk::new(audio::PcmFormat::Pcm16,16_000,1,bytes).map_err(|error| format!("Invalid microphone frame: {error:?}"))?;
                        control.send_audio(pcm)?;
                        submitted_samples += 320;
                        last_real_sample = Some(submitted_samples);
                    }
                },
                observation = uploaded.recv() => {
                    let observation = observation.ok_or("Live upload observation ended")?;
                    if observation.input_generation == mute_sequence {
                        uploaded_samples += observation.pcm.bytes.len() as u64 / 2;
                    }
                    if let Some(vad) = vad.as_mut() {
                        let observed = vad.observe_pcm(&observation.pcm).map_err(|error| error.to_string())?;
                        if !observed.events.is_empty() || !observed.completed_spans.is_empty() {
                            send_event(CallEvent::SpeechActivity(observed))?;
                        }
                    }
                }
                input = captured.recv() => match input.map_err(|_| "Microphone capture ended")? {
                    CaptureEvent::Ready { generation } if generation == mode.generation && !mode.muted => send_event(CallEvent::Connected)?,
                    CaptureEvent::Pcm { generation, chunk } if !user_muted && ((generation == mode.generation && !mode.muted) || draining_generations.contains(&generation)) => {
                        if chunk.format != audio::PcmFormat::Pcm16 || chunk.sample_rate_hz != 16_000 || chunk.channels != 1 || !chunk.bytes.len().is_multiple_of(2) {
                            return Err("Microphone returned invalid PCM".into());
                        }
                        // 16000 samples/s × 2B × 0.2s = 6400B of local microphone backlog.
                        if microphone.len() + chunk.bytes.len() > 6400 { return Err("Microphone audio exceeded the 200ms buffer".into()); }
                        microphone.extend(chunk.bytes);
                    }
                    CaptureEvent::Stopped { generation } => draining_generations.retain(|pending| *pending > generation),
                    CaptureEvent::Failed { generation, message } if generation == mode.generation => return Err(message.into()),
                    _ => {},
                },
                error = playback.errors.recv() => return Err(error.map(|error| error.to_string()).unwrap_or_else(|_| "Live playback ended".into()).into()),
                event = connection.next_event() => match event {
                    Some(live_voice::LiveEvent::OutputAudio(pcm)) => {
                        if !output_blocked { playback.enqueue_for(playback.generation(), pcm).map_err(|error| error.to_string())?; }
                        tracing::trace!(buffered_ms=playback.buffered_duration().as_millis(), "Live playback backlog");
                    },
                    Some(live_voice::LiveEvent::Delegation(notice)) => send_event(CallEvent::Delegation(notice))?,
                    Some(live_voice::LiveEvent::Usage(usage)) => send_event(CallEvent::Usage(usage))?,
                    Some(live_voice::LiveEvent::Transcript(part)) => {
                        send_event(CallEvent::Transcript(part))?;
                    },
                    Some(live_voice::LiveEvent::Closed { .. }) | None => return Ok(()),
                    Some(live_voice::LiveEvent::Acknowledged {client_event_id, kind:live_voice::Acknowledgement::Appended(kind)} ) => {
                        reports.acknowledge(&client_event_id, kind);
                        send_event(CallEvent::AppendAcknowledged(client_event_id))?;
                    },
                    Some(live_voice::LiveEvent::Rejected { .. }) => return Err("Live service rejected a session command".into()),
                    _ => {},
                },
                report = report_input.recv(), if report_input_open => match report {
                    Ok(report) => { if !output_blocked { reports.push(report); } }
                    Err(_) => report_input_open = false,
                },
            }
        }
    }.await;
    // Stop local sound immediately; final cloud usage may take up to the protocol close deadline.
    let requested_close = cancellation.is_cancelled();
    cancellation.cancel();
    reports.clear();
    report_input.close();
    while let Ok(report) = report_input.try_recv() {
        drop(report);
    }
    commands.close();
    while let Ok(command) = commands.try_recv() {
        drop(command);
    }
    playback.end();
    control.close();
    let outcome = connection.finish().await;
    playback.wait_stopped().await;
    let local_error = local_call_error(result, requested_close, &outcome);
    CallEnd {
        usage: outcome.usage,
        error: local_error.or_else(|| outcome.error.map(|error| error.to_string())),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_output_audio_tests.rs"]
mod output_audio_tests;

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_native_report_audio_tests.rs"]
mod native_report_audio_tests;
