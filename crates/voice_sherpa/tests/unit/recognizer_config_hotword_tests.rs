use crate::{build_online_recognizer_config_with_hotwords, AsrHotwordConfig};
use asr::{builtin_model_catalog, validate_installation, InstallationManifest};
use std::fs;

fn available_model(model_id: &str) -> (tempfile::TempDir, asr::AvailableAsrModel) {
    let temp = tempfile::tempdir().expect("应可创建热词约束测试临时目录");
    let model_id = asr::AsrModelId::parse(model_id).expect("内置测试模型 ID 应有效");
    let descriptor = builtin_model_catalog()
        .find(&model_id)
        .expect("受控模型库应包含指定测试模型")
        .clone();
    for file in descriptor.artifact().required_files() {
        fs::write(temp.path().join(file), [1_u8]).expect("应可写入测试模型占位文件");
    }
    InstallationManifest::for_descriptor(&descriptor)
        .write_to(temp.path())
        .expect("应可写入测试模型安装清单");
    let available =
        validate_installation(temp.path(), &descriptor).expect("完整测试模型安装应转为可用模型");
    (temp, available)
}

/// SHERPA-HOTWORD-1：调用方可以覆盖默认开发者热词、分数与搜索宽度。
///
/// 数据构造（含关键数值的推导过程）：
///   hotword phrases  = 3，`JAVA` + `KOTLIN` + `SPRING BOOT`；
///   score            = 2.5，用不同于默认 2.0 的值证明参数真正透传；
///   max active paths = 12，用不同于默认 8 的值证明搜索宽度真正透传；
///   model files      = 5，四个 Transducer 工件加热词所需 `bpe.vocab`。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造可用的双语 Zipformer 模型；
///   2. 创建自定义 `AsrHotwordConfig` 并把 paths 覆盖为 12；
///   3. 通过公开 factory 生成 Sherpa OnlineRecognizerConfig。
///
/// 预期结果：
///   - 正断言：文本、score 2.5、paths 12 和 BPE 解码参数全部精确透传；
///   - 负断言：不得静默退回默认 `JAVA/KOTLIN + 2.0 + 8`。
#[test]
fn test_custom_hotword_parameters_are_forwarded_to_sherpa() {
    let (temp, available) = available_model(asr::ZIPFORMER_BILINGUAL_MODEL_ID);
    let hotwords =
        AsrHotwordConfig::new("JAVA\nKOTLIN\nSPRING BOOT\n", 2.5).with_max_active_paths(12);

    let config = build_online_recognizer_config_with_hotwords(&available, Some(&hotwords))
        .expect("有效的自定义热词参数应生成 Sherpa 配置");

    assert_eq!(
        config.hotwords_buf.as_deref(),
        Some("JAVA\nKOTLIN\nSPRING BOOT\n".as_bytes()),
        "自定义热词文本必须逐字节透传给 Sherpa"
    );
    assert_eq!(
        config.hotwords_score, 2.5,
        "自定义热词分数不得被默认 2.0 覆盖"
    );
    assert_eq!(
        config.max_active_paths, 12,
        "自定义搜索宽度不得被默认 8 覆盖"
    );
    assert_eq!(
        config.decoding_method.as_deref(),
        Some("modified_beam_search"),
        "自定义热词仍必须使用 modified_beam_search"
    );
    assert_eq!(
        config.model_config.modeling_unit.as_deref(),
        Some("cjkchar+bpe"),
        "自定义中英混合热词必须启用 cjkchar+bpe 分词"
    );
    assert_eq!(
        config.model_config.bpe_vocab.as_deref(),
        Some(temp.path().join("bpe.vocab").to_string_lossy().as_ref()),
        "自定义热词必须使用当前受管模型内已校验的 bpe.vocab"
    );
    assert_ne!(
        config.hotwords_buf.as_deref(),
        Some("JAVA\nKOTLIN\n".as_bytes()),
        "自定义热词不得静默退回默认 JAVA/KOTLIN 列表"
    );
}

/// SHERPA-HOTWORD-2：不支持热词的模型必须明确拒绝配置，不能静默忽略偏置。
///
/// 数据构造（含关键数值的推导过程）：
///   model            = Paraformer Bilingual，支持热词分支数为 0；
///   hotword phrases  = 1，仅 `JAVA`；
///   score            = 2.0；max active paths = 默认 8。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造完整 Paraformer 占位安装并通过校验；
///   2. 向显式 recognizer builder 传入有效热词配置；
///   3. 检查返回错误包含模型 ID 与 unsupported 语义。
///
/// 预期结果：
///   - 正断言：返回明确“不支持 configured hotwords”的 Recognizer 错误；
///   - 负断言：不得成功构造一个实际忽略 JAVA 的 Paraformer 配置。
#[test]
fn test_non_transducer_model_rejects_hotwords_explicitly() {
    let (_temp, available) = available_model(asr::PARAFORMER_MODEL_ID);
    let hotwords = AsrHotwordConfig::new("JAVA\n", 2.0);

    let result = build_online_recognizer_config_with_hotwords(&available, Some(&hotwords));
    let message = result
        .as_ref()
        .expect_err("Paraformer 收到热词配置时必须失败")
        .to_string();

    assert!(
        message.contains(asr::PARAFORMER_MODEL_ID) && message.contains("does not support"),
        "错误必须指出具体模型不支持热词，实际消息: {message}"
    );
    assert!(result.is_err(), "不支持热词的模型不得静默返回成功配置");
}

/// SHERPA-HOTWORD-3：双语 Zipformer 必须拒绝空文本、非法分数和零搜索宽度。
///
/// 数据构造（含关键数值的推导过程）：
///   invalid text count = 1（仅空白）；
///   invalid scores     = 2（0.0 与 NaN，分别覆盖非正与非有限）；
///   invalid paths      = 1（0）；valid control = JAVA / 2.0 / 8。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造完整双语 Zipformer 占位安装；
///   2. 分别提交空文本、0 分、NaN 分数和 0 paths；
///   3. 对每个独立结果检查明确错误，并用有效控制证明模型本身可配置。
///
/// 预期结果：
///   - 正断言：四个非法配置逐一失败，有效 JAVA/2.0/8 成功；
///   - 负断言：NaN、零值或空白热词不得进入 Sherpa 配置。
#[test]
fn test_bilingual_hotwords_reject_each_invalid_boundary() {
    let (_temp, available) = available_model(asr::ZIPFORMER_BILINGUAL_MODEL_ID);
    let empty = AsrHotwordConfig::new("   \n", 2.0);
    let zero_score = AsrHotwordConfig::new("JAVA\n", 0.0);
    let nan_score = AsrHotwordConfig::new("JAVA\n", f32::NAN);
    let zero_paths = AsrHotwordConfig::new("JAVA\n", 2.0).with_max_active_paths(0);
    let valid = AsrHotwordConfig::new("JAVA\n", 2.0).with_max_active_paths(8);

    let empty_result = build_online_recognizer_config_with_hotwords(&available, Some(&empty));
    let zero_score_result =
        build_online_recognizer_config_with_hotwords(&available, Some(&zero_score));
    let nan_score_result =
        build_online_recognizer_config_with_hotwords(&available, Some(&nan_score));
    let zero_paths_result =
        build_online_recognizer_config_with_hotwords(&available, Some(&zero_paths));
    let valid_result = build_online_recognizer_config_with_hotwords(&available, Some(&valid));

    let empty_message = empty_result
        .as_ref()
        .expect_err("只含空白的热词文本必须被拒绝")
        .to_string();
    let zero_score_message = zero_score_result
        .as_ref()
        .expect_err("score=0.0 的非正偏置必须被拒绝")
        .to_string();
    let nan_score_message = nan_score_result
        .as_ref()
        .expect_err("score=NaN 的非有限偏置必须被拒绝")
        .to_string();
    let zero_paths_message = zero_paths_result
        .as_ref()
        .expect_err("max_active_paths=0 的空搜索空间必须被拒绝")
        .to_string();

    assert!(
        empty_message.contains("hotwords require non-empty text"),
        "空热词必须命中明确的 hotword 参数校验，实际消息: {empty_message}"
    );
    assert!(
        zero_score_message.contains("positive finite score"),
        "零分必须命中明确的正有限分数校验，实际消息: {zero_score_message}"
    );
    assert!(
        nan_score_message.contains("positive finite score"),
        "NaN 必须命中明确的正有限分数校验，实际消息: {nan_score_message}"
    );
    assert!(
        zero_paths_message.contains("positive max active paths"),
        "零搜索宽度必须命中明确的 paths 校验，实际消息: {zero_paths_message}"
    );
    assert!(
        valid_result.is_ok(),
        "JAVA/2.0/8 有效控制必须成功，避免把模型安装错误误判为边界校验"
    );
}
