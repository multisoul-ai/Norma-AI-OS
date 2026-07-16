use crate::{builtin_model_catalog, TtsBackend};

/// TTS-CATALOG-1：内置轻量模型必须声明 Matcha 中英双语包的完整安装契约。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models   = 2（Matcha zh-en + Kokoro multi-language）
///   archive bytes    = 79,033,838（已下载发布包的 Content-Length）
///   vocoder bytes    = 53,882,848（已下载 16 kHz Vocos 的 Content-Length）
///   download total   = 79,033,838 + 53,882,848 = 132,916,686 bytes
///   sample rate      = 16,000 Hz（模型发布页与实际推理输出一致）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过公共目录 API 查找 `matcha-icefall-zh-en`；
///   2. 读取语言、采样率、归档文件与辅助声码器元数据；
///   3. 检查旧的 `matcha-icefall-zh-baker` 不再暴露给用户选择。
///
/// 预期结果：
///   - 正断言：zh-en 模型及其中英能力、16 kHz Vocos 和 eSpeak 数据契约完整存在；
///   - 负断言：旧中文 Baker 模型和它的 dict 文件契约不得继续出现在产品目录中。
#[test]
fn test_builtin_catalog_declares_matcha_zh_en_installation_contract() {
    let catalog = builtin_model_catalog();
    let matcha = catalog
        .find_by_str("matcha-icefall-zh-en")
        .expect("内置目录应包含 Matcha zh-en 中英双语模型");
    let archive = matcha.package().archive();
    let support = matcha
        .package()
        .support_artifacts()
        .first()
        .expect("Matcha zh-en 必须声明 16 kHz Vocos 辅助文件");

    assert_eq!(
        catalog.entries().len(),
        2,
        "产品目录应保持一个推荐轻量模型和一个多 Voice 模型，共 2 项"
    );
    assert_eq!(
        matcha.display_name(),
        "Matcha Chinese-English",
        "Matcha zh-en 应显示为中英双语模型"
    );
    assert_eq!(
        matcha.backend(),
        TtsBackend::Matcha,
        "Matcha zh-en 必须路由到 Matcha 后端"
    );
    assert_eq!(
        matcha.capabilities().languages(),
        &["zh".to_string(), "en".to_string()],
        "Matcha zh-en 必须明确声明中文和英文能力"
    );
    assert_eq!(
        matcha.capabilities().sample_rate_hz(),
        16_000,
        "Matcha zh-en 的输出采样率必须是 16 kHz"
    );
    assert!(matcha.recommended(), "Matcha zh-en 应继续作为推荐轻量模型");
    assert_eq!(
        archive.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/matcha-icefall-zh-en.tar.bz2",
        "Matcha zh-en 归档必须来自 sherpa-onnx 官方发布地址"
    );
    assert_eq!(
        archive.sha256(),
        "271b804af570400d3bcdcb53bf6e53cc9f75180ee763b9f13eb5eaf2b0d086ef",
        "Matcha zh-en 归档哈希必须匹配已验证下载"
    );
    assert_eq!(
        archive.archive_bytes(),
        79_033_838,
        "Matcha zh-en 归档大小必须匹配已验证下载"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "model-steps-3.onnx"),
        "安装校验必须要求 Matcha ONNX 模型"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "lexicon.txt"),
        "安装校验必须要求 Matcha 词典"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "tokens.txt"),
        "安装校验必须要求 Matcha token 表"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "phone-zh.fst"),
        "安装校验必须要求中文 phone FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "date-zh.fst"),
        "安装校验必须要求中文 date FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "number-zh.fst"),
        "安装校验必须要求中文 number FST"
    );
    assert!(
        archive
            .required_files()
            .iter()
            .any(|path| path == "espeak-ng-data/phontab"),
        "安装校验必须要求英文前端所需的 eSpeak 数据"
    );
    assert!(
        !archive
            .required_files()
            .iter()
            .any(|path| path == "dict/stop_words.utf8"),
        "Matcha zh-en 不得沿用 Baker 模型的 dict 文件契约"
    );
    assert_eq!(
        matcha.package().support_artifacts().len(),
        1,
        "Matcha zh-en 应且仅应附带一个 Vocos 辅助文件"
    );
    assert_eq!(
        support.url(),
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos-16khz-univ.onnx",
        "Matcha zh-en 必须使用 16 kHz Vocos 下载地址"
    );
    assert_eq!(
        support.sha256(),
        "b599142a1fb8ff03de3e84ac35ff537c619e56f4267a6fe894851a42844acf9e",
        "16 kHz Vocos 哈希必须匹配已验证下载"
    );
    assert_eq!(
        support.bytes(),
        53_882_848,
        "16 kHz Vocos 大小必须匹配已验证下载"
    );
    assert_eq!(
        support.destination(),
        "vocos-16khz-univ.onnx",
        "16 kHz Vocos 必须安装到模型根目录的固定文件名"
    );
    assert_eq!(
        matcha.package().download_bytes(),
        132_916_686,
        "下载总量必须等于归档与 Vocos 文件大小之和"
    );
    assert!(
        catalog.find_by_str("matcha-icefall-zh-baker").is_none(),
        "旧 Matcha Baker 模型不得继续出现在产品目录中"
    );
}

/// TTS-CATALOG-2：切换到不包含当前 Voice 的模型时必须回退到该模型推荐 Voice。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models = 2（中英双语 Matcha + 多 Voice Kokoro）
///   current voice  = "af_heart"（只属于 Kokoro）
///   Matcha voices  = 1（"default"）
///   fallback count = 1（不兼容 Voice → 推荐 Voice）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从内置目录查找 Matcha 模型；
///   2. 请求保留仅属于 Kokoro 的 "af_heart"；
///   3. 通过目录公共 API 解析切换后的有效 Voice。
///
/// 预期结果：
///   - 正断言：结果为 Matcha 的 "default" 推荐 Voice；
///   - 负断言：结果不得保留不存在的 "af_heart"。
#[test]
fn test_model_switch_falls_back_to_recommended_voice() {
    let catalog = builtin_model_catalog();
    let matcha = catalog
        .find_by_str("matcha-icefall-zh-en")
        .expect("内置目录应包含轻量 Matcha 中英双语模型");

    let resolved = matcha.resolve_voice_id(Some("af_heart"));

    assert_eq!(
        resolved.as_str(),
        "default",
        "不属于 Matcha 的 Voice 必须回退到其推荐 default Voice"
    );
    assert_ne!(
        resolved.as_str(),
        "af_heart",
        "切换模型后不得保留新模型不存在的旧 Voice"
    );
}
