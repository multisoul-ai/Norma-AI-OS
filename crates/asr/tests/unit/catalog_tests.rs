use crate::{
    builtin_model_catalog, AsrModelId, PARAFORMER_MODEL_ID, ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID,
    ZIPFORMER_STREAMING_ZH_MODEL_ID,
};

/// ASR-CAT-1：受控模型库必须按不可变 ID 返回 Paraformer 的用户能力描述。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entry count = 3，Paraformer + Zipformer Streaming ZH + Zipformer Streaming CTC ZH
///   language count      = 2，中文 zh + 英文 en
///   required files      = 3，encoder + decoder + tokens
///   unknown id          = "unknown-asr-model-2099"，不在受控模型库
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 获取内置模型库 → 包含 Paraformer、Zipformer Streaming ZH 与 Zipformer Streaming CTC ZH
///   2. 按 PARAFORMER_MODEL_ID 查询 → 返回显示信息与流式能力
///   3. 按 unknown id 查询 → 返回 None
///
/// 预期结果：
///   - 正断言：Paraformer 存在且明确支持 zh/en 与流式 partial
///   - 负断言：未知 ID 不能被伪造成可用条目
#[test]
fn test_builtin_catalog_finds_paraformer_and_rejects_unknown_id() {
    let catalog = builtin_model_catalog();
    let paraformer_id =
        AsrModelId::parse(PARAFORMER_MODEL_ID).expect("内置 Paraformer ID 应是有效的不可变 ID");
    let paraformer = catalog
        .find(&paraformer_id)
        .expect("受控模型库应包含 Paraformer");

    assert_eq!(
        catalog.entries().len(),
        3,
        "受控模型库应包含 Paraformer、Zipformer Streaming ZH 与 Zipformer Streaming CTC ZH"
    );
    assert_eq!(
        paraformer.display_name(),
        "Paraformer Bilingual",
        "模型库应向 Settings 暴露友好名称而不是文件夹"
    );
    assert_eq!(
        paraformer.capabilities().languages(),
        &["zh", "en"],
        "Paraformer 能力必须明确包含中文与英文"
    );
    assert!(
        paraformer.capabilities().streaming_partials(),
        "Paraformer 必须标记支持流式 partial，否则不能共用于两种语音交互模式"
    );
    assert_eq!(
        paraformer.artifact().required_files().len(),
        3,
        "Paraformer 条目应声明 encoder、decoder 和 tokens 三类必要文件"
    );
    assert_eq!(
        paraformer.artifact().sha256(),
        "5462a1fce42693deae572af1e8c4687124b12aa85fe61ff4d3168bb5280e205f",
        "Paraformer 归档也必须固定完整 SHA-256，不能保留占位值"
    );
    assert_eq!(
        paraformer.artifact().archive_bytes(),
        1_047_319_737,
        "Paraformer 下载大小必须匹配官方 release 资产元数据"
    );

    let unknown = AsrModelId::parse("unknown-asr-model-2099").expect("非空未知 ID 本身应可表示");
    assert!(
        catalog.find(&unknown).is_none(),
        "未知模型 ID 不应出现在受控模型库中"
    );
}

/// ASR-CAT-2：Zipformer 条目必须绑定唯一官方资产及 Transducer 所需文件。
///
/// 数据构造（含关键数值的推导过程）：
///   language count      = 1，仅中文 zh
///   required files      = 4，encoder + decoder + joiner + tokens
///   archive bytes       = 132,634,597，来自官方 GitHub release 资产元数据
///   pinned SHA-256      = 64 个十六进制字符
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按不可变 Zipformer ID 查询受控模型库
///   2. 读取能力、归档大小、完整性摘要和必要文件
///   3. 检查发布推荐标记 → 质量门禁前保持 false
///
/// 预期结果：
///   - 正断言：Zipformer 支持中文流式识别并要求 joiner
///   - 负断言：未经质量/性能门禁不得标记为推荐
#[test]
fn test_zipformer_catalog_entry_pins_official_artifact_without_recommendation() {
    let catalog = builtin_model_catalog();
    let id = AsrModelId::parse(ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("内置 Zipformer ID 应有效");
    let zipformer = catalog.find(&id).expect("受控模型库应包含 Zipformer");

    assert_eq!(
        zipformer.display_name(),
        "Zipformer Streaming ZH",
        "Settings 应显示用户可理解的 Zipformer 名称"
    );
    assert_eq!(
        zipformer.capabilities().languages(),
        &["zh"],
        "Zipformer ZH 条目只能声明已知的中文能力"
    );
    assert!(
        zipformer.capabilities().streaming_partials(),
        "Zipformer Streaming ZH 必须声明支持流式 partial"
    );
    assert_eq!(
        zipformer.artifact().archive_bytes(),
        132_634_597,
        "下载大小必须匹配固定官方资产，而不是营销参数量"
    );
    assert_eq!(
        zipformer.artifact().sha256(),
        "5a2832047ea1f97dd0dc595b816c230c4bafad65cfc0341fa57517cadc50afd0",
        "Zipformer 官方归档 SHA-256 必须固定，防止 ID 指向不同字节"
    );
    assert!(
        zipformer
            .artifact()
            .required_files()
            .iter()
            .any(|file| file == "joiner.int8.onnx"),
        "Transducer 安装必须显式要求 joiner.int8.onnx"
    );
    assert!(
        !zipformer.recommended(),
        "未完成中文质量与 Apple Silicon 性能门禁前不得标记 Zipformer 为推荐"
    );
}

/// ASR-CAT-3：Zipformer Streaming CTC ZH 必须固定官方 CTC 归档与必要文件。
///
/// 数据构造（含关键数值的推导过程）：
///   language count = 1，仅中文 zh
///   required files = 2，model.int8.onnx + tokens.txt（无 encoder/decoder/joiner）
///   archive bytes  = 127,965,713，来自官方 GitHub release 资产元数据
///   pinned SHA-256 = f2ab7a5deb02717801f6a5b26c751b42f8a2db891b07f5b095e6da7442081448
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按不可变 Zipformer CTC ID 查询受控模型库
///   2. 读取显示名、语言、流式能力、归档 URL/大小/哈希与必要文件
///   3. 检查推荐标记 → 质量门禁前保持 false
///
/// 预期结果：
///   - 正断言：条目指向 streaming-zipformer-ctc 官方资产且要求 model.int8.onnx
///   - 负断言：不得要求 Transducer 的 joiner；未门禁前不得推荐
#[test]
fn test_zipformer_ctc_catalog_entry_pins_official_ctc_artifact() {
    let catalog = builtin_model_catalog();
    let id = AsrModelId::parse(ZIPFORMER_STREAMING_CTC_ZH_MODEL_ID)
        .expect("内置 Zipformer CTC ID 应有效");
    let zipformer_ctc = catalog
        .find(&id)
        .expect("受控模型库应包含 Zipformer Streaming CTC ZH");

    assert_eq!(
        zipformer_ctc.display_name(),
        "Zipformer Streaming CTC ZH",
        "Settings 应显示可与 Transducer Zipformer 区分的友好名称"
    );
    assert_eq!(
        zipformer_ctc.capabilities().languages(),
        &["zh"],
        "Zipformer CTC ZH 只能声明已知的中文能力"
    );
    assert!(
        zipformer_ctc.capabilities().streaming_partials(),
        "Zipformer Streaming CTC ZH 必须声明支持流式 partial"
    );
    assert_eq!(
        zipformer_ctc.artifact().url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-ctc-zh-int8-2025-06-30.tar.bz2",
        "下载 URL 必须钉死官方 streaming-zipformer-ctc 归档"
    );
    assert_eq!(
        zipformer_ctc.artifact().archive_bytes(),
        127_965_713,
        "下载大小必须匹配固定官方 CTC 归档字节数"
    );
    assert_eq!(
        zipformer_ctc.artifact().sha256(),
        "f2ab7a5deb02717801f6a5b26c751b42f8a2db891b07f5b095e6da7442081448",
        "Zipformer CTC 官方归档 SHA-256 必须固定"
    );
    assert_eq!(
        zipformer_ctc.artifact().required_files(),
        &["model.int8.onnx".to_string(), "tokens.txt".to_string()],
        "CTC 安装只应要求 model.int8.onnx 与 tokens.txt"
    );
    assert!(
        !zipformer_ctc
            .artifact()
            .required_files()
            .iter()
            .any(|file| file.contains("joiner")),
        "CTC 条目不得要求 Transducer joiner"
    );
    assert!(
        !zipformer_ctc.recommended(),
        "未完成中文质量与 Apple Silicon 性能门禁前不得标记 Zipformer CTC 为推荐"
    );
}
