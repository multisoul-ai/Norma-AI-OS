use super::*;

pub(super) fn spawn_turn(
    runtime: &TurnRuntime,
    input: String,
    input_source: AgentInputSource,
    canvas: Option<agent::CurrentCanvasContext>,
    desktop_context: Option<agent::DesktopContext>,
    roster: Vec<agent::AgentRosterEntry>,
    response: async_channel::Sender<Result<Vec<AgentEvent>, String>>,
    turn_generation: u64,
) {
    let runtime = runtime.clone();
    shared_tokio_runtime()
        .expect("Norma Tokio runtime was initialized before spawning a turn")
        .spawn(async move {
            let mut session = tokio::select! {
                biased;
                _ = runtime.cancelled.cancelled() => {
                    let _ = response.send(Err("Norma Session 已关闭".into())).await;
                    return;
                }
                session = runtime.session.lock() => session,
            };
            if runtime.generation.load(Ordering::Acquire) != turn_generation {
                let _ = response
                    .send(Err("Norma 回合已被新输入取代".to_string()))
                    .await;
                return;
            }
            if let Some(canvas) = canvas {
                session.update_environment(canvas, roster);
            }
            session.update_desktop_context(desktop_context);
            let cancel = session.cancel_handle();
            let dialogue_cancelled = Arc::clone(&runtime.dialogue_cancelled);
            let watcher = tokio::spawn(async move {
                while !dialogue_cancelled.load(Ordering::Acquire) {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                cancel.cancel();
            });
            let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel(32);
            let produced_deltas = Arc::new(Mutex::new(Vec::new()));
            let collected_deltas = Arc::clone(&produced_deltas);
            let live_events = runtime.events.clone();
            let stopped = runtime.cancelled.clone();
            let delta_forwarder = tokio::spawn(async move {
                while let Some(text) = delta_rx.recv().await {
                    let event = AgentEvent::DialogueTextDelta { text };
                    collected_deltas
                        .lock()
                        .expect("Norma streamed event lock should not be poisoned")
                        .push(event.clone());
                    tokio::select! {
                        biased;
                        _ = stopped.cancelled() => break,
                        _ = live_events.send(event) => {}
                    }
                }
            });
            let result = tokio::select! {
                biased;
                _ = runtime.cancelled.cancelled() => {
                    session.cancel_handle().cancel();
                    Err(agent::NormaSessionError::Cancelled)
                }
                result = session.turn_streaming(&input, delta_tx.clone(), input_source) => result,
            };
            drop(delta_tx);
            let _ = delta_forwarder.await;
            watcher.abort();
            if runtime.cancelled.is_cancelled() {
                let _ = response.send(Err("Norma Session 已关闭".into())).await;
                return;
            }
            match result {
                Ok(_result) => {
                    let mut produced = produced_deltas
                        .lock()
                        .expect("Norma streamed event lock should not be poisoned")
                        .clone();
                    let finished = AgentEvent::DialogueStreamFinished;
                    produced.push(finished);
                    tokio::select! {
                        biased;
                        _ = runtime.cancelled.cancelled() => {
                            let _ = response.send(Err("Norma Session 已关闭".into())).await;
                            return;
                        }
                        _ = runtime.events.send(AgentEvent::DialogueStreamFinished) => {}
                    }
                    let _ = response.send(Ok(produced)).await;
                }
                Err(error) => {
                    let message = error.to_string();
                    tokio::select! {
                        biased;
                        _ = runtime.cancelled.cancelled() => {},
                        _ = runtime.events.send(AgentEvent::ProviderError { message: message.clone() }) => {}
                    }
                    let _ = response.send(Err(message)).await;
                }
            }
        });
}
