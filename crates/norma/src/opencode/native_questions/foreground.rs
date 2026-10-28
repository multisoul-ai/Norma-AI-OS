//! Foreground route changes revoke eligibility, never erase once-only send ownership.
use super::*;
impl Worker {
    pub(super) fn reconnect(&mut self) {
        self.retire_waiting();
        if self.sent.len() >= 4096 {
            return;
        }
        let session = self.client.session_id().to_owned();
        if self.client.follow_session(&session).is_err() {
            self.disable();
            return;
        }
        *self.current.lock().expect("foreground poisoned") =
            Some((session.clone(), self.client.connection_id().into()));
        self.recovering = false;
        self.available.store(true, Ordering::Release);
        // New connection leases require their own successful pending GET before input opens.
        self.blocked.store(true, Ordering::Release);
        self.emit(OpenCodeNativeEvent::ForegroundChanged {
            session: Some(session),
            connection: self.client.connection_id().into(),
        });
    }

    pub(super) fn remember_sends(&mut self) {
        for waiting in &self.waiting {
            if waiting.ticket.native_send_status() != crate::agent_hook::NativeSendStatus::Unsent {
                self.sent.insert((
                    waiting.lease.question().session_id().into(),
                    waiting.lease.question().request_id().into(),
                ));
            }
        }
        if self.sent.len() >= 4096 {
            self.disable();
        }
    }
    pub(super) fn sync_foreground(&mut self) -> bool {
        let Some(foreground) = &mut self.foreground else {
            return true;
        };
        let next = foreground.read();
        if next.is_none() {
            if foreground.current.take().is_some() {
                self.retire_waiting();
                *self.current.lock().expect("foreground poisoned") = None;
                self.emit(OpenCodeNativeEvent::Unavailable);
            }
            self.blocked.store(true, Ordering::Release);
            return false;
        }
        let unchanged = match (&next, &foreground.current) {
            (Some(a), Some(b)) => a.same_position(b),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            if next.as_ref().is_some_and(|r| r.session.is_none()) {
                self.blocked.store(false, Ordering::Release);
            }
            return next.is_some_and(|r| r.session.is_some());
        }
        foreground.current = next.clone();
        self.retire_waiting();
        let session = next.and_then(|r| r.session);
        if let Some(session) = &session {
            if self.client.follow_session(session).is_err() {
                self.disable();
                return false;
            }
        }
        *self.current.lock().expect("foreground poisoned") = session
            .clone()
            .map(|s| (s, self.client.connection_id().into()));
        self.blocked.store(session.is_some(), Ordering::Release);
        self.emit(OpenCodeNativeEvent::ForegroundChanged {
            session: session.clone(),
            connection: self.client.connection_id().into(),
        });
        session.is_some()
    }
}
