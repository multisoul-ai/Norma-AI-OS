//! Window-level transient visual effects rendered above Canvas cards and below workspace chrome.

pub(crate) mod dispatch_receipt;

#[cfg(test)]
#[path = "../../tests/unit/dispatch_receipt_effect_geometry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/dispatch_receipt_idea_geometry_tests.rs"]
mod idea_geometry_tests;
