//! Hard license gate: boot phase, activation shell, and Keygate client adapter.

mod activate_http;
mod boot;
mod config;
mod gate_root;
mod machine_id;
mod resolve;
mod token_store;

pub use gate_root::open_gated_window;

#[cfg(feature = "e2e-test-support")]
pub use gate_root::build_activation_root_e2e;

#[cfg(test)]
#[path = "../../tests/unit/license_gate_boot_tests.rs"]
mod license_gate_boot_tests;

#[cfg(test)]
#[path = "../../tests/unit/license_gate_activate_http_tests.rs"]
mod license_gate_activate_http_tests;

#[cfg(test)]
#[path = "../../tests/unit/license_gate_background_tests.rs"]
mod license_gate_background_tests;

#[cfg(test)]
#[path = "../../tests/unit/license_gate_input_tests.rs"]
mod license_gate_input_tests;
