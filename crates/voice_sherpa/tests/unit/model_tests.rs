/// MODEL-1: When the env var is set, resolve_model_dir() must return that path
/// without checking whether the directory actually exists.
///
/// Data construction:
///   MULTISOUL_PARAFORMER_MODEL_DIR = "/tmp/test-model-dir" (arbitrary non-existent path)
///
/// Execution:
///   1. Set env var to a path that does NOT exist on disk.
///   2. Call voice_sherpa::resolve_model_dir_for_test().
///   3. Remove env var so subsequent tests are not affected.
///
/// Expected result:
///   - Ok(PathBuf::from("/tmp/test-model-dir"))  — env var path returned as-is.
#[test]
fn test_resolve_model_dir_from_env() {
    // Safety: test runs in a single-threaded context as far as this env var is
    // concerned; remove_var cleans up before any other test can observe it.
    std::env::set_var("MULTISOUL_PARAFORMER_MODEL_DIR", "/tmp/test-model-dir");
    let result = crate::resolve_model_dir_for_test();
    std::env::remove_var("MULTISOUL_PARAFORMER_MODEL_DIR");

    assert_eq!(
        result,
        Ok(std::path::PathBuf::from("/tmp/test-model-dir")),
        "resolve_model_dir() should return the env var path without checking existence"
    );
}
