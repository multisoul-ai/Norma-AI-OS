mod config;
mod transport;
pub use config::A1Config;
mod host;

pub use host::{
    begin_shared_a1_shutdown, shared_a1_ready, shutdown_shared_a1, A1EngineHost,
    A1RecognizerFactory, A1Release,
};

mod resources;

#[cfg(test)]
#[path = "../../../../tests/support/a1_python.rs"]
mod test_python;
