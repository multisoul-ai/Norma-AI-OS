use std::sync::{Mutex, OnceLock};

pub(super) fn endpoint_override() -> &'static Mutex<Option<String>> {
    static ENDPOINT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ENDPOINT.get_or_init(|| Mutex::new(None))
}

pub struct AzureSpeechEndpointOverrideGuard {
    previous: Option<String>,
}

impl Drop for AzureSpeechEndpointOverrideGuard {
    fn drop(&mut self) {
        *endpoint_override()
            .lock()
            .expect("Azure test endpoint override lock must not be poisoned") =
            self.previous.take();
    }
}

pub fn install_azure_speech_endpoint_override_for_tests(
    endpoint: impl Into<String>,
) -> AzureSpeechEndpointOverrideGuard {
    let mut current = endpoint_override()
        .lock()
        .expect("Azure test endpoint override lock must not be poisoned");
    let previous = current.replace(endpoint.into());
    AzureSpeechEndpointOverrideGuard { previous }
}
