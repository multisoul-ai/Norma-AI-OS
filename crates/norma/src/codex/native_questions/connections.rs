//! A picker connection must not replace the active question owner until it resumes a thread.
use super::*;

struct Auxiliary {
    relay: CodexQuestionRelay,
    // A new picker owns terminal input until it resumes a thread or closes.
    candidate: Arc<AtomicBool>,
}

impl Worker {
    pub(super) fn pump_connections(
        &mut self,
        connections: Receiver<CodexQuestionRelay>,
        input_pending: InputPending,
    ) {
        let mut auxiliary: Vec<Auxiliary> = Vec::new();
        while self.running.load(Ordering::Acquire) {
            for relay in connections.try_iter().take(8) {
                // Native TUI normally needs two connections; keep resource use bounded.
                if auxiliary.len() < 16 {
                    let candidate = Arc::new(AtomicBool::new(true));
                    input_pending
                        .lock()
                        .expect("native input owner poisoned")
                        .push(candidate.clone());
                    // Production connections are paused until their input guard is visible.
                    relay.start_forwarding();
                    auxiliary.push(Auxiliary { relay, candidate });
                }
            }
            self.poll();
            let mut index = 0;
            while index < auxiliary.len() {
                let mut attachment = None;
                for event in auxiliary[index].relay.events().try_iter().take(128) {
                    if let CodexRelayEvent::Session(session) = event {
                        if auxiliary[index].candidate.load(Ordering::Acquire) {
                            attachment = Some(session);
                            break;
                        }
                    }
                }
                if let Some(attachment) = attachment {
                    // Only a real start/resume response transfers ownership. Retired
                    // connections keep forwarding, but cannot reclaim the newer session.
                    let next = auxiliary.swap_remove(index);
                    let previous = std::mem::replace(&mut self.relay, next.relay);
                    {
                        let mut pending =
                            input_pending.lock().expect("native input owner poisoned");
                        pending[0] = self.relay.input_pending.clone();
                        next.candidate.store(false, Ordering::Release);
                    }
                    if previous.is_running() {
                        auxiliary.push(Auxiliary {
                            relay: previous,
                            candidate: next.candidate,
                        });
                    }
                    for waiting in self.questions.drain(..) {
                        waiting.ticket.cancel();
                    }
                    self.available
                        .store(self.relay.is_observing(), Ordering::Release);
                    self.observe(CodexRelayEvent::Session(attachment));
                    self.poll();
                } else {
                    index += 1;
                }
            }
            auxiliary.retain(|connection| connection.relay.is_running());
            let mut pending = vec![self.relay.input_pending.clone()];
            pending.extend(
                auxiliary
                    .iter()
                    .map(|connection| connection.candidate.clone()),
            );
            *input_pending.lock().expect("native input owner poisoned") = pending;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
