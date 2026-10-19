use super::*;

pub(super) async fn transmit(
    mut socket: impl Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    session: Arc<Mutex<LiveSession>>,
    mut audio: mpsc::Receiver<AudioFrame>,
    mut commands: mpsc::Receiver<String>,
    input_gate: Arc<Mutex<InputGate>>,
    cancellation: CancellationToken,
) -> Result<(), LiveError> {
    let origin = Instant::now();
    let mut pacer = AudioPacer::default();
    let mut generation = 0;
    let mut pending: Option<AudioFrame> = None;
    let mut send_at = origin;
    let mut samples_sent = 0_u64;
    loop {
        let current = input_gate.lock().expect("Live input gate lock").clone();
        if current.generation != generation {
            // No PCM captured before a mute boundary may reappear after unmute.
            pending = None;
            pacer = AudioPacer::default();
            generation = current.generation;
        }
        let message;
        let held;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                message = session.lock().expect("Live session lock").close_message()?;
                timeout(WRITE_TIMEOUT, socket.send(Message::Text(message.into()))).await.map_err(|_| LiveError::Timeout)?
                    .map_err(|_| LiveError::Connection)?;
                return Ok(());
            }
            command = commands.recv() => {
                message = command.ok_or(LiveError::Connection)?;
                held = None;
            }
            _ = tokio::time::sleep_until(send_at), if pending.is_some() => {
                let frame = pending.take().expect("scheduled audio owns its frame");
                let current = input_gate.lock().expect("Live input gate lock").clone();
                if (current.muted && !frame.synthetic_silence) || frame.generation != current.generation {
                    continue;
                }
                message = session.lock().expect("Live session lock").audio_message(&frame.pcm)?;
                held = Some(frame);
            }
            frame = audio.recv(), if pending.is_none() => {
                let frame = frame.ok_or(LiveError::Connection)?;
                let current = input_gate.lock().expect("Live input gate lock").clone();
                if (current.muted && !frame.synthetic_silence) || frame.generation != current.generation {
                    continue;
                }
                if current.generation != generation {
                    pacer = AudioPacer::default();
                    generation = current.generation;
                }
                send_at = origin + pacer.reserve(origin.elapsed(), &frame.pcm)?;
                pending = Some(frame);
                continue;
            }
        }
        timeout(WRITE_TIMEOUT, socket.send(Message::Text(message.into())))
            .await
            .map_err(|_| LiveError::Timeout)?
            .map_err(|_| LiveError::Connection)?;
        if let Some(frame) = held {
            let start_sample = samples_sent;
            samples_sent += frame.pcm.bytes.len() as u64 / 2;
            let observer = input_gate
                .lock()
                .expect("Live input gate lock")
                .observer
                .clone();
            if let Some(observer) = observer {
                let notified = observer
                    .try_send(UploadedAudio {
                        pcm: frame.pcm,
                        start_sample,
                        input_generation: frame.generation,
                        _permit: frame._permit,
                    })
                    .is_ok();
                if !notified && !cancellation.is_cancelled() {
                    return Err(LiveError::Capacity);
                }
            }
        }
    }
}
