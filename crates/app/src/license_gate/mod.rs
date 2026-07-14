//! Hard license gate: boot phase, activation shell, and Keygate client adapter.

mod activate_http;
mod boot;
mod config;
mod gate_root;
mod machine_id;
mod resolve;
mod token_store;

pub use gate_root::open_gated_window;

#[cfg(test)]
#[path = "../../tests/unit/license_gate_boot_tests.rs"]
mod license_gate_boot_tests;
