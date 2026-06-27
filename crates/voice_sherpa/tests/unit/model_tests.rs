use std::sync::Mutex;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// MODEL-1: resolve_model_dir() returns the path from env var MULTISOUL_PARAFORMER_MODEL_DIR.
///
/// Data:
///   MULTISOUL_PARAFORMER_MODEL_DIR = "/tmp/test-model-dir" (non-existent path)
///   from_env_or_default() calls resolve_model_dir() which reads this env var
///
/// Execution:
///   1. Lock ENV_MUTEX to serialize concurrent env mutation
///   2. Set env var → resolve_model_dir() → returns Ok(PathBuf::from("/tmp/test-model-dir"))
///   3. Clear env var in finally-style cleanup
///
/// Expected:
///   - Positive: Ok(PathBuf::from("/tmp/test-model-dir")) is returned
///   - Negative: a non-existent path is still returned as Ok (no existence check)
#[test]
fn test_resolve_model_dir_from_env() {
    let _lock = ENV_MUTEX.lock().unwrap();
    std::env::set_var("MULTISOUL_PARAFORMER_MODEL_DIR", "/tmp/test-model-dir");
    let result = crate::resolve_model_dir_for_test();
    std::env::remove_var("MULTISOUL_PARAFORMER_MODEL_DIR");
    assert_eq!(
        result,
        Ok(std::path::PathBuf::from("/tmp/test-model-dir")),
        "resolve_model_dir should return the env var path without checking existence"
    );
}
