use super::*;

pub(super) async fn transmit(
    mut socket: impl Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    session: Arc<Mutex<LiveSession>>,
    mut audio: mpsc::Receiver<AudioFrame>,
    mut commands: mpsc::Receiver<CommandFrame>,
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
        let command_scope;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                message = session.lock().expect("Live session lock").close_message()?;
                timeout(WRITE_TIMEOUT, socket.send(Message::Text(message.into()))).await.map_err(|_| LiveError::Timeout)?
                    .map_err(|_| LiveError::Connection)?;
                return Ok(());
            }
            command = commands.recv() => {
                let command = command.ok_or(LiveError::Connection)?;
                if command.scope.as_ref().is_some_and(CancellationToken::is_cancelled) { continue; }
                command_scope = command.scope;
                message = command.message;
                held = None;
            }
            _ = tokio::time::sleep_until(send_at), if pending.is_some() => {
                let frame = pending.take().expect("scheduled audio owns its frame");
                let current = input_gate.lock().expect("Live input gate lock").clone();
                if (current.muted && !frame.synthetic_silence) || frame.generation != current.generation {
                    continue;
                }
                message = session.lock().expect("Live session lock").audio_message(&frame.pcm)?;
                command_scope = None;
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
        let written = timeout(
            WRITE_TIMEOUT,
            write_message(&mut socket, message, command_scope.as_ref()),
        )
        .await
        .map_err(|_| LiveError::Timeout)?
        .map_err(|_| LiveError::Connection)?;
        if !written {
            continue;
        }
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

async fn write_message(
    socket: &mut (impl Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
    message: String,
    scope: Option<&CancellationToken>,
) -> Result<bool, tokio_tungstenite::tungstenite::Error> {
    if let Some(scope) = scope {
        tokio::select! {
            biased;
            _ = scope.cancelled() => return Ok(false),
            ready = futures::future::poll_fn(|cx| std::pin::Pin::new(&mut *socket).poll_ready(cx)) => ready?,
        }
        if scope.is_cancelled() {
            return Ok(false);
        }
        std::pin::Pin::new(&mut *socket).start_send(Message::Text(message.into()))?;
        // Once start_send accepts the frame, finish flushing to preserve framing.
        // Cancellation cannot retract bytes already handed to the socket.
        socket.flush().await?;
    } else {
        socket.send(Message::Text(message.into())).await?;
    }
    Ok(true)
}
