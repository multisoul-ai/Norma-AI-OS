use crate::build_online_recognizer_config;
use asr::{builtin_model_catalog, validate_installation, InstallationManifest};
use std::fs;

/// SHERPA-CONFIG-1：Paraformer 可用模型必须只填充 Paraformer 配置分支。
///
/// 数据构造（含关键数值的推导过程）：
///   required files       = 3，encoder.int8.onnx + decoder.int8.onnx + tokens.txt
///   Paraformer branches  = 1，只允许 paraformer 分支有路径
///   Transducer branches  = 0，encoder/decoder/joiner 均必须为 None
///   recognizer threads   = 2，保留现有本地 CPU 配置
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用受控 Paraformer descriptor 构造完整安装
///   2. validate_installation → AvailableAsrModel
///   3. build_online_recognizer_config → Sherpa OnlineRecognizerConfig
///
/// 预期结果：
///   - 正断言：Paraformer encoder/decoder/tokens 是已校验根目录下的精确路径
///   - 负断言：Transducer 不得残留任何路径，避免同时配置两种模型
#[test]
fn test_paraformer_available_model_builds_only_paraformer_config() {
    let temp = tempfile::tempdir().expect("应可创建 Paraformer 配置临时目录");
    let descriptor = builtin_model_catalog()
        .entries()
        .first()
        .expect("受控模型库应包含 Paraformer")
        .clone();
    for file in descriptor.artifact().required_files() {
        fs::write(temp.path().join(file), [1_u8]).expect("应可写入模型占位文件");
    }
    InstallationManifest::for_descriptor(&descriptor)
        .write_to(temp.path())
        .expect("应可写入 Paraformer 安装清单");
    let available = validate_installation(temp.path(), &descriptor)
        .expect("完整 Paraformer 安装应转为可用模型");

    let config =
        build_online_recognizer_config(&available).expect("可用 Paraformer 应能生成 Sherpa 配置");

    assert_eq!(
        config.model_config.paraformer.encoder.as_deref(),
        Some(
            temp.path()
                .join("encoder.int8.onnx")
                .to_string_lossy()
                .as_ref()
        ),
        "Paraformer encoder 必须来自已校验安装根目录"
    );
    assert_eq!(
        config.model_config.paraformer.decoder.as_deref(),
        Some(
            temp.path()
                .join("decoder.int8.onnx")
                .to_string_lossy()
                .as_ref()
        ),
        "Paraformer decoder 必须来自已校验安装根目录"
    );
    assert_eq!(
        config.model_config.tokens.as_deref(),
        Some(temp.path().join("tokens.txt").to_string_lossy().as_ref()),
        "tokens 必须来自已校验安装根目录"
    );
    assert!(
        config.model_config.transducer.encoder.is_none(),
        "Paraformer 配置不得填充 Transducer encoder"
    );
    assert!(
        config.model_config.transducer.decoder.is_none(),
        "Paraformer 配置不得填充 Transducer decoder"
    );
    assert!(
        config.model_config.transducer.joiner.is_none(),
        "Paraformer 配置不得填充 Transducer joiner"
    );
    assert_eq!(
        config.model_config.num_threads, 2,
        "统一 factory 应保留现有 2 线程 CPU 配置"
    );
}

/// SHERPA-CONFIG-2：Zipformer 可用模型必须只填充 Transducer 配置分支。
///
/// 数据构造（含关键数值的推导过程）：
///   required files       = 4，encoder.int8.onnx + decoder.onnx + joiner.int8.onnx + tokens.txt
///   Transducer branches  = 3，encoder/decoder/joiner 均有精确路径
///   Paraformer branches  = 0，encoder/decoder 均必须为 None
///   recognizer threads   = 2，与现有本地 CPU 配置一致
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从受控模型库定位 Zipformer descriptor 并构造完整安装
///   2. validate_installation → AvailableAsrModel
///   3. build_online_recognizer_config → Sherpa OnlineRecognizerConfig
///
/// 预期结果：
///   - 正断言：Transducer 三个模型路径及 tokens 都来自已校验根目录
///   - 负断言：Paraformer 分支保持为空，避免错误适配
#[test]
fn test_zipformer_available_model_builds_only_transducer_config() {
    let temp = tempfile::tempdir().expect("应可创建 Zipformer 配置临时目录");
    let zipformer_id = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID)
        .expect("内置 Zipformer ID 应有效");
    let descriptor = builtin_model_catalog()
        .find(&zipformer_id)
        .expect("受控模型库应包含 Zipformer")
        .clone();
    for file in descriptor.artifact().required_files() {
        fs::write(temp.path().join(file), [1_u8]).expect("应可写入模型占位文件");
    }
    InstallationManifest::for_descriptor(&descriptor)
        .write_to(temp.path())
        .expect("应可写入 Zipformer 安装清单");
    let available =
        validate_installation(temp.path(), &descriptor).expect("完整 Zipformer 安装应转为可用模型");

    let config = build_online_recognizer_config(&available)
        .expect("可用 Zipformer 应能生成 Sherpa Transducer 配置");

    assert_eq!(
        config.model_config.transducer.encoder.as_deref(),
        Some(
            temp.path()
                .join("encoder.int8.onnx")
                .to_string_lossy()
                .as_ref()
        ),
        "Zipformer encoder 必须来自已校验安装根目录"
    );
    assert_eq!(
        config.model_config.transducer.decoder.as_deref(),
        Some(temp.path().join("decoder.onnx").to_string_lossy().as_ref()),
        "Zipformer decoder 必须来自已校验安装根目录"
    );
    assert_eq!(
        config.model_config.transducer.joiner.as_deref(),
        Some(
            temp.path()
                .join("joiner.int8.onnx")
                .to_string_lossy()
                .as_ref()
        ),
        "Zipformer joiner 必须来自已校验安装根目录"
    );
    assert!(
        config.model_config.paraformer.encoder.is_none(),
        "Zipformer 配置不得填充 Paraformer encoder"
    );
    assert!(
        config.model_config.paraformer.decoder.is_none(),
        "Zipformer 配置不得填充 Paraformer decoder"
    );
    assert_eq!(
        config.model_config.num_threads, 2,
        "统一 factory 应让 Zipformer 使用 2 线程 CPU 配置"
    );
}
