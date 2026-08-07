use crate::{
    install_silero_vad_file, validate_silero_vad_installation, SILERO_VAD_ARCHIVE_BYTES,
    SILERO_VAD_ARCHIVE_URL, SILERO_VAD_DIRECTORY, SILERO_VAD_FILE, SILERO_VAD_SHA256,
};

/// ASR-SUPPORT-1：共享 Silero VAD 必须固定官方工件身份。
///
/// 数据构造（含关键数值的推导过程）：
///   file count = 1（silero_vad.onnx）
///   archive bytes = 643,854
///   SHA-256 length = 64 个十六进制字符
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取编译期固定 URL、文件名、字节数与 SHA-256
///   2. 检查共享目录不依赖具体 ASR 模型 ID
///
/// 预期结果：
///   - 正断言：元数据精确匹配官方 Silero VAD 工件
///   - 负断言：不得把 VAD 塞进某个 Paraformer/Zipformer 私有目录
#[test]
fn test_silero_vad_support_artifact_is_pinned_and_shared() {
    assert_eq!(
        SILERO_VAD_ARCHIVE_URL,
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
        "Silero VAD 必须从固定官方 release URL 下载"
    );
    assert_eq!(
        SILERO_VAD_SHA256, "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
        "Silero VAD SHA-256 必须固定官方 643,854 字节工件"
    );
    assert_eq!(
        SILERO_VAD_ARCHIVE_BYTES, 643_854,
        "Silero VAD 下载进度应使用真实官方字节数"
    );
    assert_eq!(SILERO_VAD_DIRECTORY, "silero_vad", "VAD 应位于共享目录");
    assert_eq!(
        SILERO_VAD_FILE, "silero_vad.onnx",
        "共享目录应使用 Sherpa 可识别的固定文件名"
    );
    assert_ne!(
        SILERO_VAD_DIRECTORY,
        crate::ZIPFORMER_STREAMING_ZH_MODEL_ID,
        "共享 VAD 不得绑定 Zipformer 私有模型 ID"
    );
}

/// ASR-SUPPORT-2：错误哈希的 VAD 文件不得进入共享 Ready 目录。
///
/// 数据构造（含关键数值的推导过程）：
///   fake source bytes = 3（`bad`）
///   expected official bytes = 643,854
///   byte deficit = 643,854 - 3 = 643,851
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入 3 字节伪 VAD 文件
///   2. install_silero_vad_file 校验固定 SHA-256
///   3. 检查共享目录与 validate 结果
///
/// 预期结果：
///   - 正断言：安装返回哈希错误
///   - 负断言：不得创建共享 VAD 文件或通过 validate
#[test]
fn test_silero_vad_install_rejects_wrong_hash_without_ready_file() {
    let temp = tempfile::tempdir().expect("应创建 VAD 安装临时目录");
    let source = temp.path().join("bad-vad.onnx");
    std::fs::write(&source, b"bad").expect("应写入 3 字节伪 VAD");
    let model_root = temp.path().join("models");

    let result = install_silero_vad_file(&source, &model_root);

    assert!(result.is_err(), "错误哈希 VAD 安装必须返回错误");
    assert!(
        !model_root
            .join(SILERO_VAD_DIRECTORY)
            .join(SILERO_VAD_FILE)
            .exists(),
        "错误哈希不得产生共享 VAD 文件"
    );
    assert!(
        validate_silero_vad_installation(&model_root).is_err(),
        "错误哈希安装后共享 VAD 不得验证为可用"
    );
}
