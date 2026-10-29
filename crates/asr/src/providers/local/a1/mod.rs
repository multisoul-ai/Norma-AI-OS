mod config;
#[cfg(unix)]
mod transport;
pub use config::A1Config;
#[cfg(unix)]
mod host;

#[cfg(unix)]
pub use host::{
    begin_shared_a1_shutdown, shared_a1_ready, shutdown_shared_a1, A1EngineHost,
    A1RecognizerFactory, A1Release,
};

#[cfg(unix)]
mod resources;
