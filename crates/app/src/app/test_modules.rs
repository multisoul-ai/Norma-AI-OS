use super::*;

#[cfg(test)]
#[path = "../../tests/unit/app_cursor_animation_tests.rs"]
mod cursor_animation_tests;
#[cfg(test)]
#[path = "../../tests/unit/database_isolation_support.rs"]
mod database_isolation_support;
#[cfg(test)]
#[path = "../../tests/unit/database_isolation_tests.rs"]
mod database_isolation_tests;
#[cfg(test)]
#[path = "../../tests/unit/packager_production_data_tests.rs"]
mod packager_production_data_tests;
#[cfg(test)]
#[path = "../../tests/unit/app_tests.rs"]
mod tests;
#[cfg(all(test, target_os = "macos"))]
#[path = "../../tests/unit/video_decoder_tests.rs"]
mod video_decoder_tests;
