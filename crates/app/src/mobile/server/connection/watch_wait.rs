use super::Shared;
use norma::mobile::MobileSocket;
use std::{
    io::ErrorKind,
    net::TcpStream,
    os::fd::AsFd,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tokio::io::{unix::AsyncFd, Interest};
use tungstenite::{Error, Message};

/// Wait for either project changes or peer control frames, using the existing shared reactor.
/// One registration per watch; no additional per-peer threads, periodic socket polling or larger limits.
pub(super) struct WatchWait(AsyncFd<TcpStream>);

impl WatchWait {
    pub(super) fn new(socket: &MobileSocket) -> Result<Self, &'static str> {
        let runtime = crate::app::shared_tokio_runtime().map_err(|_| "unavailable")?;
        let _entered = runtime.enter();
        let fd = socket
            .get_ref()
            .sock
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| "unavailable")?;
        AsyncFd::with_interest(TcpStream::from(fd), Interest::READABLE)
            .map(Self)
            .map_err(|_| "unavailable")
    }

    pub(super) fn wait(
        &self,
        socket: &mut MobileSocket,
        shared: &Shared,
        revision: u64,
        sent_at: Instant,
    ) -> Result<(), &'static str> {
        let checked_at = Instant::now();
        if shared.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        // Also drain frames already buffered by TLS/WebSocket before registering readability.
        self.control(socket)?;
        let runtime = crate::app::shared_tokio_runtime().map_err(|_| "unavailable")?;
        runtime.block_on(async {
            let changed = crate::mobile::project_events::wait(
                revision,
                Duration::from_secs(2).saturating_sub(sent_at.elapsed()),
            );
            tokio::pin!(changed);
            loop {
                tokio::select! {
                    ready = self.0.readable() => {
                        let mut ready = ready.map_err(|_| "unavailable")?;
                        self.control(socket)?;
                        // control drains through WouldBlock, including partial TLS/WS frames.
                        ready.clear_ready();
                    }
                    _ = &mut changed => break,
                }
            }
            Ok::<_, &'static str>(())
        })?;
        // Retain the existing 1000/50=20 reads/s coalescing bound; wire control is separate.
        let remaining = Duration::from_millis(50).saturating_sub(checked_at.elapsed());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
        if shared.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        Ok(())
    }

    fn control(&self, socket: &mut MobileSocket) -> Result<(), &'static str> {
        // The duplicated descriptor shares flags with MobileTcp. Only this peer thread owns
        // protocol IO; restore blocking mode before ordinary deadline-bounded reads/writes.
        self.0
            .get_ref()
            .set_nonblocking(true)
            .map_err(|_| "unavailable")?;
        socket
            .get_mut()
            .sock
            .set_deadline(Instant::now() + Duration::from_secs(3));
        let result = (|| {
            // A peer sending >32 control frames in one drain is closed, not allowed to spin.
            for _ in 0..32 {
                match socket.read() {
                    Ok(Message::Ping(_) | Message::Pong(_)) => {}
                    Ok(Message::Close(_)) => {
                        let _ = socket.flush();
                        return Err("unavailable");
                    }
                    Err(Error::Io(error)) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                    Err(_) => return Err("unavailable"),
                    Ok(_) => return Err("invalid_request"),
                }
            }
            Err("invalid_request")
        })();
        self.0
            .get_ref()
            .set_nonblocking(false)
            .map_err(|_| "unavailable")?;
        result
    }
}
