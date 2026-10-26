use std::sync::OnceLock;
static TOKIO_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

pub(crate) fn shared_tokio_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    TOKIO_RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .thread_name("norma-agent-runtime")
                .enable_all()
                .build()
                .map_err(|error| format!("无法启动 Norma async runtime：{error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}
