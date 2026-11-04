use asr::{
    a1::{A1Config, A1RecognizerFactory},
    RecognizerFactory,
};

/// 场景：默认平台必须导出真实 A1 工厂；数据为预取消会话及未安装模型配置。
/// 执行：构造真实工厂并查询能力，再请求已取消会话。
/// 预期：保留 A1 能力描述，取消立即失败，不启动 Python 或模型进程。
#[test]
fn default_factory_preserves_capability_and_rejects_cancelled_start() {
    let factory = A1RecognizerFactory::new(A1Config {
        protocol: 1,
        python: std::env::current_exe().expect("测试二进制"),
        python_sha256: String::new(),
        zipformer: std::env::temp_dir(),
        qwen: std::env::temp_dir(),
    });
    assert_eq!(
        factory.capabilities(),
        asr::AsrProviderCapabilities::new(true, Some(60)),
        "不能通过移除 A1 工厂隐藏编译失败"
    );
    let cancellation = asr::RecognitionCancellation::new();
    cancellation.cancel();
    assert!(
        matches!(
            factory.create_with_cancellation(cancellation),
            Err(asr::AsrError::SessionClosed)
        ),
        "已取消会话不能占用宿主或启动模型"
    );
}
