mod client;
mod config;
mod protocol;
mod transport;
mod voice;

pub use client::MiniMaxTtsClient;
pub use config::MiniMaxTtsConfig;
pub(super) use transport::{MiniMaxConnection, MiniMaxTransport, ProductionMiniMaxTransport};
pub use voice::MiniMaxProvider;

#[cfg(test)]
#[path = "../../../tests/unit/minimax_stream_tests.rs"]
mod minimax_stream_tests;

#[cfg(test)]
#[path = "../../../tests/unit/minimax_failure_tests.rs"]
mod minimax_failure_tests;

#[cfg(test)]
#[path = "../../../tests/unit/minimax_catalog_tests.rs"]
mod minimax_catalog_tests;

#[cfg(test)]
#[path = "../../../tests/unit/minimax_live_smoke_tests.rs"]
mod minimax_live_smoke_tests;
