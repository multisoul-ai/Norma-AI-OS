pub use asr::{
    build_online_recognizer_config, build_online_recognizer_config_with_hotwords, AsrHotwordConfig,
};

#[cfg(test)]
#[path = "../tests/unit/recognizer_config_tests.rs"]
mod recognizer_config_tests;

#[cfg(test)]
#[path = "../tests/unit/recognizer_config_hotword_tests.rs"]
mod recognizer_config_hotword_tests;
